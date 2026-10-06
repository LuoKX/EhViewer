use super::*;
use flate2::{Compression, write::GzEncoder};
use lzma_rust2::{XzOptions, XzWriter};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut w = GzEncoder::new(Vec::new(), Compression::default());
    w.write_all(bytes).unwrap();
    w.finish().unwrap()
}
fn xz(bytes: &[u8]) -> Vec<u8> {
    let mut w = XzWriter::new(Vec::new(), XzOptions::default()).unwrap();
    w.write_all(bytes).unwrap();
    w.finish().unwrap()
}

#[test]
fn plain_nested_and_mixed_filters_are_seekable_without_spooling() {
    let payload = b"archive-page-payload";
    for bytes in [
        payload.to_vec(),
        gzip(payload),
        xz(payload),
        gzip(&xz(&gzip(payload))),
        xz(&gzip(payload)),
    ] {
        let mut source = DecodedSource::new(Cursor::new(bytes)).unwrap();
        assert_eq!(source.seek(SeekFrom::End(0)).unwrap(), payload.len() as u64);
        source.seek(SeekFrom::Start(3)).unwrap();
        let mut output = Vec::new();
        source.read_to_end(&mut output).unwrap();
        assert_eq!(output, &payload[3..]);
        source.seek(SeekFrom::Current(-4)).unwrap();
        let mut out = Vec::new();
        source.read_to_end(&mut out).unwrap();
        assert_eq!(out, &payload[payload.len() - 4..]);
        assert!(source.seek(SeekFrom::End(1)).is_err());
        assert!(source.seek(SeekFrom::Current(i64::MIN)).is_err());
    }
}

#[test]
fn checks_every_outer_trailer_before_exposing_inner_data() {
    for mut bytes in [gzip(b"payload"), xz(b"payload")] {
        let end = bytes.len() - 1;
        bytes[end] ^= 0x80;
        assert!(DecodedSource::new(Cursor::new(bytes)).is_err());
    }
    let mut bytes = gzip(b"payload");
    bytes.truncate(bytes.len() - 4);
    assert!(DecodedSource::new(Cursor::new(bytes)).is_err());
    let mut bytes = b"payload".to_vec();
    for _ in 0..=MAX_FILTERS {
        bytes = gzip(&bytes);
    }
    assert!(DecodedSource::new(Cursor::new(bytes)).is_err());
}

#[test]
fn concatenated_gzip_and_xz_streams_and_empty_source() {
    for wrap in [gzip as fn(&[u8]) -> Vec<u8>, xz] {
        let mut bytes = wrap(b"one");
        bytes.extend(wrap(b"two"));
        let mut source = DecodedSource::new(Cursor::new(bytes)).unwrap();
        let mut out = Vec::new();
        source.read_to_end(&mut out).unwrap();
        assert_eq!(out, b"onetwo");
    }
    let mut source = DecodedSource::new(Cursor::new(Vec::<u8>::new())).unwrap();
    let mut buf = [0; 1];
    assert_eq!(source.read(&mut buf).unwrap(), 0);
}

#[test]
fn short_reads_preserve_filter_recognition_and_shared_memory_credit() {
    struct Short(Cursor<Vec<u8>>);
    impl Read for Short {
        fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
            let n = b.len().min(1);
            self.0.read(&mut b[..n])
        }
    }
    impl Seek for Short {
        fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
            self.0.seek(from)
        }
    }
    let mut source = DecodedSource::new(Short(Cursor::new(gzip(&xz(b"page"))))).unwrap();
    source
        .allocation_budget()
        .run(|| assert!(eh_codec_limits::reserve_dictionary(MAX_DICTIONARY_BYTES + 1).is_err()));
    let mut out = Vec::new();
    source.read_to_end(&mut out).unwrap();
    assert_eq!(out, b"page");
    let mut source = DecodedSource::new(Cursor::new(b"x".to_vec())).unwrap();
    source.position = u64::MAX;
    assert!(source.read(&mut [0; 1]).is_err());
    for (decoded, work) in [
        (u64::MAX, 0),
        (MAX_DECODED_BYTES, 0),
        (0, u64::MAX),
        (0, MAX_REPLAY_BYTES),
    ] {
        let mut layer = Layer {
            reader: Box::new(Cursor::new(b"x")),
            decoded,
            work: Rc::new(Cell::new(work)),
        };
        assert!(layer.read(&mut [0; 1]).is_err());
    }
}

#[test]
fn replay_limits_fail_without_huge_allocation() {
    let mut source = DecodedSource::new(Cursor::new(gzip(b"page"))).unwrap();
    source.work.set(MAX_REPLAY_BYTES);
    assert!(source.read(&mut [0; 1]).is_err());
    let mut source = DecodedSource::new(Cursor::new(gzip(b"page"))).unwrap();
    source.position = MAX_DECODED_BYTES;
    assert!(source.read(&mut [0; 1]).is_err());
    let mut source = DecodedSource::new(Cursor::new(gzip(b"page"))).unwrap();
    source.length += 1;
    assert!(source.seek(SeekFrom::End(0)).is_err());
}
