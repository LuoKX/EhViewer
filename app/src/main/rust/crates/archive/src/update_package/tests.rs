use super::*;
use std::io::Cursor;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn member(name: &str) -> ZipMember {
    ZipMember {
        index: 0,
        name: name.into(),
        size: 3,
        regular: true,
        encrypted: false,
    }
}
fn fixture(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn selects_unique_regular_apk_and_rejects_ambiguous_or_unsafe_members() {
    let entries = vec![member("readme.txt"), member("nested/APP.APK")];
    assert_eq!(select_apk(&entries).unwrap().name, "nested/APP.APK");
    assert!(matches!(select_apk(&[]), Err(UpdatePackageError::Missing)));
    assert!(matches!(
        select_apk(&[member("readme")]),
        Err(UpdatePackageError::Missing)
    ));
    assert!(matches!(
        select_apk(&[member("one.apk"), member("two.apk")]),
        Err(UpdatePackageError::Ambiguous)
    ));
    assert!(matches!(
        select_apk(&vec![member("readme"); MAX_ENTRIES + 1]),
        Err(UpdatePackageError::Limit)
    ));
    for invalid in [
        ZipMember {
            encrypted: true,
            ..member("x.apk")
        },
        ZipMember {
            regular: false,
            ..member("x.apk")
        },
        ZipMember {
            size: 0,
            ..member("x.apk")
        },
        member("/x.apk"),
        member("../x.apk"),
        member("a/./x.apk"),
        member("a//x.apk"),
        member("a\\x.apk"),
        member("C:x.apk"),
        member("nul\0.apk"),
    ] {
        assert!(matches!(
            select_apk(&[invalid]),
            Err(UpdatePackageError::InvalidEntry)
        ));
    }
    assert!(matches!(
        select_apk(&[ZipMember {
            size: MAX_PACKAGE_BYTES + 1,
            ..member("x.apk")
        }]),
        Err(UpdatePackageError::Limit)
    ));
    assert!(
        select_apk(&[ZipMember {
            size: MAX_PACKAGE_BYTES,
            ..member("x.apk")
        }])
        .is_ok()
    );
}

#[test]
fn extracts_the_apk_after_metadata_and_preserves_source_offset() {
    let bytes = fixture(&[("README", b"ignore"), ("nested/update.apk", b"apk-payload")]);
    let mut source = Cursor::new(bytes);
    source.set_position(7);
    let mut output = Vec::new();
    assert_eq!(extract_apk(&mut source, &mut output).unwrap(), 11);
    assert_eq!(output, b"apk-payload");
    assert_eq!(source.position(), 7);
    for entries in [
        vec![],
        vec![("one.txt", b"a".as_slice())],
        vec![("one.apk", b"a".as_slice()), ("two.apk", b"b".as_slice())],
    ] {
        let mut source = Cursor::new(fixture(&entries));
        let mut output = Vec::new();
        assert!(extract_apk(&mut source, &mut output).is_err());
        assert!(output.is_empty());
    }
}

struct BrokenOutput;
impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn integrity_and_output_errors_propagate_and_restore_source() {
    let bytes = fixture(&[("update.apk", b"apk-payload")]);
    let mut source = Cursor::new(bytes.clone());
    source.set_position(2);
    assert!(extract_apk(&mut source, &mut BrokenOutput).is_err());
    assert_eq!(source.position(), 2);
    let mut damaged = bytes;
    let offset = damaged
        .windows(11)
        .position(|b| b == b"apk-payload")
        .unwrap();
    damaged[offset] ^= 1;
    assert!(extract_apk(&mut Cursor::new(damaged), &mut Vec::new()).is_err());
    assert!(extract_apk(&mut Cursor::new(b"not zip"), &mut Vec::new()).is_err());
    assert!(matches!(
        extract_apk(&mut Cursor::new(Vec::<u8>::new()), &mut Vec::new()),
        Err(UpdatePackageError::Limit)
    ));
}

struct SeekProbe {
    position: u64,
    size: u64,
    failure: Option<u8>,
}
impl Read for SeekProbe {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}
impl Seek for SeekProbe {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let (stage, position) = match from {
            SeekFrom::Current(0) => (0, self.position),
            SeekFrom::End(0) => (1, self.size),
            SeekFrom::Start(value) => (2, value),
            _ => panic!("unexpected seek"),
        };
        if self.failure == Some(stage) {
            return Err(io::ErrorKind::Other.into());
        }
        self.position = position;
        Ok(position)
    }
}

#[test]
fn input_size_and_seek_failures_are_bounded() {
    for failure in [None, Some(0), Some(1), Some(2)] {
        let mut source = SeekProbe {
            position: 3,
            size: MAX_PACKAGE_BYTES + 1,
            failure,
        };
        assert!(extract_apk(&mut source, &mut Vec::new()).is_err());
        if failure != Some(2) {
            assert_eq!(source.position, 3);
        }
    }
}
