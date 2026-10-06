use std::io::{Cursor, Seek, SeekFrom, Write};

use super::*;

struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn pax(pairs: &[(&str, &str)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (key, value) in pairs {
        let body = format!("{key}={value}\n");
        let mut size = body.len() + 2;
        loop {
            let next = body.len() + size.to_string().len() + 1;
            if next == size {
                break;
            }
            size = next;
        }
        bytes.extend(format!("{size} {body}").as_bytes());
    }
    bytes
}
fn raw(tag: u8, name: &str, data: &[u8], declared: Option<u64>) -> Vec<u8> {
    let mut header = Header::new_ustar();
    header.set_path(name).unwrap();
    header.set_entry_type(tar::EntryType::new(tag));
    header.set_size(declared.unwrap_or(data.len() as u64));
    header.set_mode(0o644);
    header.set_cksum();
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend(data);
    bytes.resize(bytes.len().div_ceil(512) * 512, 0);
    bytes
}

#[test]
fn pax_global_local_delete_size_and_legacy_metadata() {
    let mut bytes = raw(b'g', "global", &pax(&[("path", "global.png")]), None);
    bytes.extend(raw(
        b'x',
        "local",
        &pax(&[("path", "本地.png"), ("size", "4")]),
        None,
    ));
    bytes.extend(raw(b'0', "fallback.png", b"page", Some(0)));
    bytes.extend(raw(b'g', "delete", &pax(&[("path", "")]), None));
    bytes.extend(raw(b'K', "longlink", b"../target\0", None));
    bytes.extend(raw(b'0', "other.png", b"data", None));
    let mut source = Cursor::new(bytes);
    let members = scan(&mut source).unwrap();
    assert_eq!(members[0].name, "本地.png");
    assert_eq!(members[1].name, "other.png");
    let mut output = Vec::new();
    extract(&mut source, &members[0], &mut output).unwrap();
    assert_eq!(output, b"page");
}

#[test]
fn schily_realsize_and_solaris_in_place_holes() {
    let mut bytes = raw(b'x', "pax", &pax(&[("SCHILY.realsize", "8")]), None);
    bytes.extend(raw(b'0', "page.png", b"data", None));
    let mut source = Cursor::new(bytes);
    let member = scan(&mut source).unwrap().remove(0);
    let mut out = Vec::new();
    extract(&mut source, &member, &mut out).unwrap();
    assert_eq!(out, b"data\0\0\0\0");
    let mut bytes = raw(b'x', "pax", &pax(&[("SUN.holesdata", " 4 8 12")]), None);
    bytes.extend(raw(b'0', "page.png", b"JUNKdataJUNK", None));
    let mut source = Cursor::new(bytes);
    let member = scan(&mut source).unwrap().remove(0);
    let mut out = Vec::new();
    extract(&mut source, &member, &mut out).unwrap();
    assert_eq!(out, b"\0\0\0\0data\0\0\0\0");
    for map in ["1 2", " 4 3", " 4 13", " 4"] {
        assert!(sun_sparse_map(map.as_bytes(), 12).is_err());
    }
}

#[test]
fn extended_limits_pending_headers_and_sparse_failures() {
    assert!(!recognizes_header(&[0; 512]));
    for ending in [Vec::new(), vec![0; 512]] {
        let mut bytes = raw(b'L', "name", b"page.png\0", None);
        bytes.extend(ending);
        assert!(matches!(
            scan(&mut Cursor::new(bytes)),
            Err(TarError::Invalid)
        ));
    }
    assert!(matches!(
        scan(&mut Cursor::new(raw(
            b'x',
            "pax",
            b"",
            Some(MAX_EXTENSION_BYTES + 1)
        ))),
        Err(TarError::Limit)
    ));
    let mut ext = Extensions::default();
    let mut budget = MAX_METADATA_BYTES;
    assert!(
        ext.apply(b'x', pax(&[("path", "page.png")]), &mut budget)
            .is_err()
    );
    let malformed = [
        vec![("GNU.sparse.major", "2"), ("GNU.sparse.realsize", "4")],
        vec![("GNU.sparse.major", "1"), ("GNU.sparse.realsize", "4")],
        vec![("GNU.sparse.size", "4"), ("GNU.sparse.map", "1")],
        vec![("GNU.sparse.size", "4"), ("GNU.sparse.offset", "0")],
        vec![("GNU.sparse.size", "4"), ("GNU.sparse.numbytes", "4")],
        vec![
            ("GNU.sparse.size", "4"),
            ("GNU.sparse.offset", "0"),
            ("GNU.sparse.offset", "1"),
        ],
        vec![
            ("GNU.sparse.size", "4"),
            ("GNU.sparse.map", "0,4"),
            ("GNU.sparse.numblocks", "2"),
        ],
    ];
    for pairs in malformed {
        let mut bytes = raw(b'x', "pax", &pax(&pairs), None);
        bytes.extend(raw(b'0', "page.png", b"data", None));
        assert!(scan(&mut Cursor::new(bytes)).is_err());
    }
    let mut bytes = raw(
        b'x',
        "pax",
        &pax(&[("GNU.sparse.major", "1"), ("GNU.sparse.realsize", "4")]),
        None,
    );
    bytes.extend(raw(b'0', "page.png", b"100001\n", None));
    assert!(matches!(
        scan(&mut Cursor::new(bytes)),
        Err(TarError::Limit)
    ));
}

#[test]
fn ustar_prefix_and_sparse_in_band_padding_are_validated() {
    let name = format!("{}/page.png", "prefix".repeat(18));
    let members = scan(&mut Cursor::new(fixture(&[(&name, b"page")]))).unwrap();
    assert_eq!(members[0].name, name);
    let extensions = Extensions {
        local: vec![
            ("GNU.sparse.major".into(), b"1".to_vec()),
            ("GNU.sparse.size".into(), b"512".to_vec()),
        ],
        ..Extensions::default()
    };
    assert_eq!(extensions.stored_size().unwrap(), Some(512));
    for (stored, padding) in [(4, false), (516, true)] {
        let mut bytes = raw(
            b'x',
            "pax",
            &pax(&[("GNU.sparse.major", "1"), ("GNU.sparse.realsize", "4")]),
            None,
        );
        let mut data = b"1\n0\n4\n".to_vec();
        data.resize(512, 0);
        if padding {
            data[20] = 1;
        }
        data.extend(b"data");
        bytes.extend(raw(b'0', "page.png", &data, Some(stored)));
        assert!(scan(&mut Cursor::new(bytes)).is_err());
    }
}

#[test]
fn checksum_signed_octals_and_pax_key_bounds() {
    let mut header = Header::new_old();
    header.as_mut_bytes()[0] = 0xff;
    header.set_size(0);
    let sum = header.as_bytes()[..148]
        .iter()
        .chain(&header.as_bytes()[156..])
        .fold(256_i64, |sum, byte| {
            sum + i64::from(i8::from_ne_bytes([*byte]))
        });
    header.as_mut_bytes()[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
    assert!(recognizes_header(header.as_bytes()));
    let oversized = "x".repeat(1025);
    assert!(parse_pax(&pax(&[(&oversized, "value")])).is_err());
    let mut invalid = pax(&[("x", "y")]);
    let i = invalid.iter().position(|byte| *byte == b'x').unwrap();
    invalid[i] = 255;
    assert!(parse_pax(&invalid).is_err());
    assert!(copy_bytes(&mut Cursor::new(b"x"), &mut Vec::new(), 2).is_err());
}

fn fixture(names: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in names {
        let mut header = Header::new_ustar();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, *data).unwrap();
    }
    builder.finish().unwrap();
    builder.into_inner().unwrap()
}

#[test]
fn reads_headers_names_members_and_positions() {
    let bytes = fixture(&[
        ("路径/猫10.png", b"ten"),
        ("路径/猫2.png", b"two"),
        ("ComicInfo.xml", b"xml"),
    ]);
    let mut source = Cursor::new(bytes);
    source.set_position(7);
    let members = scan(&mut source).unwrap();
    assert_eq!(members.len(), 3);
    assert_eq!(source.position(), 7);
    for (member, expected) in members.iter().zip([b"ten", b"two", b"xml"]) {
        let mut output = Vec::new();
        extract(&mut source, member, &mut output).unwrap();
        assert_eq!(output, expected);
        assert_eq!(source.position(), 7);
    }
}

#[test]
fn reads_real_gnu_sparse_and_all_pax_sparse_versions() {
    for bytes in [
        include_bytes!("test_data/gnu-sparse.tar").as_slice(),
        include_bytes!("test_data/pax-sparse-00.tar").as_slice(),
        include_bytes!("test_data/pax-sparse-01.tar").as_slice(),
        include_bytes!("test_data/pax-sparse-10.tar").as_slice(),
    ] {
        let mut source = Cursor::new(bytes);
        let members = scan(&mut source).unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "page.png");
        let mut output = Vec::new();
        extract(&mut source, &members[0], &mut output).unwrap();
        let mut expected = vec![0; 13 * 8192 + 8192];
        for i in 0..13 {
            expected[i * 8192..i * 8192 + 4096].fill(u8::try_from(65 + i).unwrap());
        }
        assert_eq!(output, expected);
    }
}

#[test]
fn gnu_long_paths_links_and_changed_identity() {
    let name = format!("{}/page.png", "long".repeat(60));
    let mut b = tar::Builder::new(Vec::new());
    let mut h = Header::new_gnu();
    h.set_size(4);
    h.set_mode(0o644);
    h.set_cksum();
    b.append_data(&mut h, &name, b"page".as_slice()).unwrap();
    let mut h = Header::new_gnu();
    h.set_entry_type(tar::EntryType::Symlink);
    h.set_size(0);
    h.set_cksum();
    b.append_link(&mut h, "link.png", "../../outside").unwrap();
    let bytes = b.into_inner().unwrap();
    let mut source = Cursor::new(bytes);
    let mut members = scan(&mut source).unwrap();
    assert_eq!(members[0].name, name);
    assert!(!members[1].regular);
    members[0].name.push('x');
    assert!(matches!(
        extract(&mut source, &members[0], &mut Vec::new()),
        Err(TarError::ChangedMember)
    ));
    assert!(matches!(
        extract(&mut source, &members[1], &mut Vec::new()),
        Err(TarError::Limit)
    ));
}

#[test]
fn rejects_checksum_truncation_lengths_and_bad_sparse_maps() {
    assert!(!recognizes_header(b"short"));
    let mut bytes = fixture(&[("page.png", b"page")]);
    bytes[0] ^= 1;
    assert!(matches!(
        scan(&mut Cursor::new(bytes)),
        Err(TarError::Invalid)
    ));
    let bytes = fixture(&[("page.png", b"page")]);
    assert!(scan(&mut Cursor::new(&bytes[..513])).is_err());
    for (map, logical, stored) in [
        (vec![(1, 5), (2, 1)], 10, 6),
        (vec![(9, 2)], 10, 2),
        (vec![(u64::MAX, 1)], u64::MAX, 1),
        (vec![(0, 2)], 10, 3),
    ] {
        assert!(validate_map(&map, logical, stored).is_err());
    }
    assert!(padded(u64::MAX).is_err());
    assert!(bounded_end(0, 1024, 512).is_err());
    let mut total = MAX_METADATA_BYTES;
    assert!(account(&mut total, 1).is_err());
    for bytes in [b"".as_slice(), b"-1", b"9999999999999999999999999999999"] {
        assert!(decimal(bytes).is_err());
    }
    for bytes in [b"1 x=y\n".as_slice(), b"10 x", b"6 x\n", b"5 =\n"] {
        assert!(parse_pax(bytes).is_err());
    }
    assert!(
        read_decimal_line(
            &mut Cursor::new(b"12345678901234567890123456789012345\n"),
            0
        )
        .is_err()
    );
}

#[test]
fn zero_holes_empty_archives_and_broken_output() {
    assert!(scan(&mut Cursor::new(Vec::<u8>::new())).unwrap().is_empty());
    assert!(scan(&mut Cursor::new(vec![0; 512])).unwrap().is_empty());
    let mut output = Vec::new();
    fill_zeros(&mut output, 100_000, &vec![0; BUFFER]).unwrap();
    assert_eq!(output.len(), 100_000);
    let mut source = Cursor::new(fixture(&[("page.png", b"page")]));
    let member = scan(&mut source).unwrap().remove(0);
    source.seek(SeekFrom::Start(9)).unwrap();
    assert!(extract(&mut source, &member, &mut Broken).is_err());
    assert_eq!(source.position(), 9);
}
