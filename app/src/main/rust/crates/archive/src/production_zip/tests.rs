use std::io::{Cursor, Seek, SeekFrom, Write};

use zip::write::SimpleFileOptions;
use zip::{AesMode, CompressionMethod, ZipWriter};

use super::*;

struct BrokenOutput;
impl Write for BrokenOutput {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const PAGE: &[u8] = b"archive-v2-page\n";
const ZIP_CRYPTO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../androidTest/assets/archive-v2/encrypted-images.zip"
));

fn members(bytes: &[u8]) -> Vec<ZipMember> {
    let ZipScan::Complete(members) = scan(&mut Cursor::new(bytes), None, false).unwrap() else {
        panic!("header scan must not need a password");
    };
    members
}

fn fixture(method: CompressionMethod, password: Option<&str>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let mut options = SimpleFileOptions::default().compression_method(method);
    if let Some(password) = password {
        options = options.with_aes_encryption(AesMode::Aes256, password);
    }
    writer.start_file("路径/猫2.png", options).unwrap();
    writer.write_all(PAGE).unwrap();
    writer.finish().unwrap().into_inner()
}

#[test]
fn round_trips_stored_deflated_bzip_xz_and_ppmd() {
    for method in [
        CompressionMethod::Stored,
        CompressionMethod::Deflated,
        CompressionMethod::Bzip2,
        CompressionMethod::Xz,
        CompressionMethod::Ppmd,
    ] {
        let bytes = fixture(method, None);
        let entries = members(&bytes);
        assert_eq!(entries[0].name, "路径/猫2.png");
        assert_eq!(entries[0].size, PAGE.len() as u64);
        let mut output = Vec::new();
        extract(&mut Cursor::new(&bytes), None, &entries[0], &mut output).unwrap();
        assert_eq!(output, PAGE, "{method:?}");
    }
}

#[test]
fn reads_lzma_zip_fixture() {
    let bytes = include_bytes!("test_data/lzma.zip");
    let entries = members(bytes);
    let mut output = Vec::new();
    extract(&mut Cursor::new(bytes), None, &entries[0], &mut output).unwrap();
    assert_eq!(output, PAGE);
}

#[test]
fn validates_zipcrypto_and_aes_passwords_before_acceptance() {
    for bytes in [
        ZIP_CRYPTO.to_vec(),
        fixture(CompressionMethod::Deflated, Some("secret")),
    ] {
        let entries = members(&bytes);
        assert!(entries[0].encrypted);
        assert!(matches!(
            scan(&mut Cursor::new(&bytes), None, true).unwrap(),
            ZipScan::PasswordRequired
        ));
        assert!(matches!(
            scan(&mut Cursor::new(&bytes), Some(b"wrong"), true).unwrap(),
            ZipScan::PasswordRequired
        ));
        assert!(matches!(
            scan(&mut Cursor::new(&bytes), Some(b"secret"), true).unwrap(),
            ZipScan::Complete(_)
        ));
        assert!(extract(&mut Cursor::new(&bytes), None, &entries[0], &mut Vec::new()).is_err());
        let mut output = Vec::new();
        extract(
            &mut Cursor::new(&bytes),
            Some(b"secret"),
            &entries[0],
            &mut output,
        )
        .unwrap();
        assert_eq!(output, PAGE);
    }
}

#[test]
fn preserves_position_success_errors_and_self_extracting_prefixes() {
    let mut bytes = b"MZ-not-a-real-executable".to_vec();
    bytes.extend(fixture(CompressionMethod::Deflated, None));
    let mut source = Cursor::new(bytes);
    source.seek(SeekFrom::Start(7)).unwrap();
    assert!(recognizes(&mut source).unwrap());
    assert_eq!(source.position(), 7);
    let ZipScan::Complete(entries) = scan(&mut source, None, false).unwrap() else {
        panic!()
    };
    assert_eq!(source.position(), 7);
    let mut output = Vec::new();
    extract(&mut source, None, &entries[0], &mut output).unwrap();
    assert_eq!(source.position(), 7);
    assert_eq!(output, PAGE);
    let mut changed = entries[0].clone();
    changed.name.push('x');
    assert!(matches!(
        extract(&mut source, None, &changed, &mut Vec::new()),
        Err(ProductionZipError::ChangedMember)
    ));
    assert_eq!(source.position(), 7);
    let mut invalid = Cursor::new(b"not a zip".to_vec());
    invalid.set_position(3);
    assert!(!recognizes(&mut invalid).unwrap());
    assert!(scan(&mut invalid, None, false).is_err());
    assert_eq!(invalid.position(), 3);
}

#[test]
fn retains_duplicate_names_and_original_source_order() {
    let bytes = raw_stored_zip(&[(b"same.png", b"first"), (b"same.png", b"second")]);
    let entries = members(&bytes);
    assert_eq!(entries.len(), 2);
    for (index, expected) in [b"first".as_slice(), b"second".as_slice()]
        .into_iter()
        .enumerate()
    {
        assert_eq!(entries[index].index, index);
        let mut output = Vec::new();
        extract(&mut Cursor::new(&bytes), None, &entries[index], &mut output).unwrap();
        assert_eq!(output, expected);
    }
}

#[test]
fn keeps_bionic_utf8_default_hidden_names_and_null_termination() {
    let bytes = raw_stored_zip(&[
        ("猫.png".as_bytes(), PAGE),
        (b".ehviewer", PAGE),
        (b"prefix\0.png", PAGE),
    ]);
    let entries = members(&bytes);
    assert_eq!(entries[0].name, "猫.png");
    assert_eq!(entries[1].name, ".ehviewer");
    assert_eq!(entries[2].name, "prefix");
}

#[test]
fn verifies_aes_authentication_tag_before_password_acceptance() {
    let mut bytes = fixture(CompressionMethod::Deflated, Some("secret"));
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    let compressed = u32::from_le_bytes(bytes[central + 20..central + 24].try_into().unwrap());
    let end = data_offset(&bytes) + usize::try_from(compressed).unwrap();
    bytes[end - 1] ^= 0x80;
    let entry = members(&bytes).remove(0);
    assert!(matches!(
        scan(&mut Cursor::new(&bytes), Some(b"secret"), true).unwrap(),
        ZipScan::PasswordRequired
    ));
    assert!(
        extract(
            &mut Cursor::new(bytes),
            Some(b"secret"),
            &entry,
            &mut Vec::new()
        )
        .is_err()
    );
}

#[test]
fn honors_crc_valid_infozip_unicode_path_extra() {
    let name = b"legacy.png";
    let mut bytes = raw_stored_zip(&[(name, PAGE)]);
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    let footer = bytes.len() - 22;
    let old_size = u32::from_le_bytes(bytes[footer + 12..footer + 16].try_into().unwrap());
    let path = "中文/猫.png".as_bytes();
    let mut extra = Vec::new();
    extra.extend_from_slice(&0x7075_u16.to_le_bytes());
    extra.extend_from_slice(&u16::try_from(5 + path.len()).unwrap().to_le_bytes());
    extra.push(1);
    extra.extend_from_slice(&crc32fast::hash(name).to_le_bytes());
    extra.extend_from_slice(path);
    bytes[central + 30..central + 32]
        .copy_from_slice(&u16::try_from(extra.len()).unwrap().to_le_bytes());
    bytes[footer + 12..footer + 16]
        .copy_from_slice(&(old_size + u32::try_from(extra.len()).unwrap()).to_le_bytes());
    bytes.splice(central + 46 + name.len()..central + 46 + name.len(), extra);
    let entry = members(&bytes).remove(0);
    assert_eq!(entry.name, "中文/猫.png");
    let mut output = Vec::new();
    extract(&mut Cursor::new(bytes), None, &entry, &mut output).unwrap();
    assert_eq!(output, PAGE);
}

#[test]
fn rejects_corrupted_crc_and_output_errors() {
    let mut bytes = fixture(CompressionMethod::Stored, None);
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    bytes[central + 16] ^= 0x80;
    let entries = members(&bytes);
    assert!(extract(&mut Cursor::new(&bytes), None, &entries[0], &mut Vec::new()).is_err());

    let bytes = fixture(CompressionMethod::Stored, None);
    let entries = members(&bytes);
    assert!(
        extract(
            &mut Cursor::new(&bytes),
            None,
            &entries[0],
            &mut BrokenOutput
        )
        .is_err()
    );
}

#[test]
fn empty_directory_and_metadata_limits_fail_before_codec_allocation() {
    assert!(members(&raw_stored_zip(&[])).is_empty());
    let mut bytes = raw_stored_zip(&[(b"page.png", PAGE)]);
    let footer = bytes.len() - 22;
    let offset = u32::from_le_bytes(bytes[footer + 16..footer + 20].try_into().unwrap());
    let directory_size = u32::from_le_bytes(bytes[footer + 12..footer + 16].try_into().unwrap());
    let mut zip64 = Vec::new();
    zip64.extend_from_slice(b"PK\x06\x06");
    zip64.extend_from_slice(&44_u64.to_le_bytes());
    for value in [45_u16, 45] {
        zip64.extend_from_slice(&value.to_le_bytes());
    }
    for value in [0_u32, 0] {
        zip64.extend_from_slice(&value.to_le_bytes());
    }
    for value in [
        MAX_ENTRIES + 1,
        MAX_ENTRIES + 1,
        u64::from(directory_size),
        u64::from(offset),
    ] {
        zip64.extend_from_slice(&value.to_le_bytes());
    }
    zip64.extend_from_slice(b"PK\x06\x07");
    zip64.extend_from_slice(&0_u32.to_le_bytes());
    zip64.extend_from_slice(&u64::try_from(footer).unwrap().to_le_bytes());
    zip64.extend_from_slice(&1_u32.to_le_bytes());
    bytes[footer + 8..footer + 12].fill(0xff);
    bytes.splice(footer..footer, zip64);
    assert!(matches!(
        scan(&mut Cursor::new(bytes), None, false),
        Err(ProductionZipError::Limit)
    ));

    let mut bytes = raw_stored_zip(&[(b"page.png", PAGE)]);
    let footer = bytes.len() - 22;
    let directory_size = u32::from_le_bytes(bytes[footer + 12..footer + 16].try_into().unwrap());
    let gap = u32::try_from(MAX_DIRECTORY_BYTES).unwrap();
    bytes[footer + 12..footer + 16].copy_from_slice(&(directory_size + gap).to_le_bytes());
    bytes.splice(footer..footer, std::iter::repeat_n(0, gap as usize));
    assert!(matches!(
        scan(&mut Cursor::new(bytes), None, false),
        Err(ProductionZipError::Limit)
    ));
}

#[test]
fn corrupt_encrypted_data_requires_password_but_unsupported_codec_is_an_error() {
    let mut bytes = ZIP_CRYPTO.to_vec();
    let start = data_offset(&bytes);
    bytes[start + 12] ^= 0x80;
    assert!(matches!(
        scan(&mut Cursor::new(bytes), Some(b"secret"), true).unwrap(),
        ZipScan::PasswordRequired
    ));

    let mut bytes = ZIP_CRYPTO.to_vec();
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    bytes[8..10].copy_from_slice(&113_u16.to_le_bytes());
    bytes[central + 10..central + 12].copy_from_slice(&113_u16.to_le_bytes());
    assert!(matches!(
        scan(&mut Cursor::new(bytes), Some(b"secret"), true),
        Err(ProductionZipError::Codec(_))
    ));
}

#[test]
fn rejects_large_dictionary_and_ppmd_model_before_reservation() {
    for method in [
        CompressionMethod::Lzma,
        CompressionMethod::Xz,
        CompressionMethod::Ppmd,
    ] {
        let mut bytes = if method == CompressionMethod::Lzma {
            include_bytes!("test_data/lzma.zip").to_vec()
        } else {
            fixture(method, None)
        };
        let start = data_offset(&bytes);
        if method == CompressionMethod::Lzma {
            bytes[start + 5..start + 9].copy_from_slice(&(256_u32 * 1024 * 1024).to_le_bytes());
            let central = bytes
                .windows(4)
                .position(|bytes| bytes == b"PK\x01\x02")
                .unwrap();
            bytes[central + 24..central + 28]
                .copy_from_slice(&(128_u32 * 1024 * 1024).to_le_bytes());
        } else if method == CompressionMethod::Xz {
            let header = start + 12;
            let size = (usize::from(bytes[header]) + 1) * 4;
            bytes[header + 4] = 40;
            let crc = crc32fast::hash(&bytes[header..header + size - 4]);
            bytes[header + size - 4..header + size].copy_from_slice(&crc.to_le_bytes());
        } else {
            let mut parameters = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
            parameters |= 0xff0;
            bytes[start..start + 2].copy_from_slice(&parameters.to_le_bytes());
        }
        let entry = members(&bytes).remove(0);
        let error = extract(&mut Cursor::new(bytes), None, &entry, &mut Vec::new()).unwrap_err();
        assert!(
            matches!(error, ProductionZipError::Io(ref error) if error.kind() == io::ErrorKind::OutOfMemory),
            "{method:?}: {error:?}"
        );
    }
}

#[test]
fn password_validation_preserves_resource_errors_instead_of_retrying_passwords() {
    let bytes = fixture(CompressionMethod::Ppmd, Some("secret"));
    let result = lzma_rust2::limits::with_decoder_limits(1024, MAX_ENTRIES, || {
        scan(&mut Cursor::new(bytes), Some(b"secret"), true)
    });
    assert!(
        matches!(result, Err(ProductionZipError::Io(error)) if error.kind() == io::ErrorKind::OutOfMemory)
    );
}

#[test]
fn excludes_directory_links_and_special_unix_types() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory("dir/", SimpleFileOptions::default())
        .unwrap();
    writer
        .add_symlink("link.png", "page.png", SimpleFileOptions::default())
        .unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    assert!(members(&bytes).iter().all(|member| !member.regular));
    let mut bytes = raw_stored_zip(&[(b"device.png", PAGE)]);
    let central = bytes
        .windows(4)
        .position(|bytes| bytes == b"PK\x01\x02")
        .unwrap();
    bytes[central + 4..central + 6].copy_from_slice(&0x0314_u16.to_le_bytes());
    bytes[central + 38..central + 42].copy_from_slice(&(0o020_600_u32 << 16).to_le_bytes());
    let entry = members(&bytes).remove(0);
    assert!(!entry.regular);
    assert!(matches!(
        extract(&mut Cursor::new(bytes), None, &entry, &mut Vec::new()),
        Err(ProductionZipError::ChangedMember)
    ));
}

#[test]
fn reads_zip64_local_headers_and_stream_data_descriptors() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("page.png", SimpleFileOptions::default().large_file(true))
        .unwrap();
    writer.write_all(PAGE).unwrap();
    let zip64 = writer.finish().unwrap().into_inner();
    let mut writer = ZipWriter::new_stream(Vec::new());
    writer
        .start_file(
            "page.png",
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
    writer.write_all(PAGE).unwrap();
    let streamed = writer.finish().unwrap().into_inner();
    assert_ne!(streamed[6] & 8, 0);
    for bytes in [zip64, streamed] {
        let entry = members(&bytes).remove(0);
        let mut output = Vec::new();
        extract(&mut Cursor::new(bytes), None, &entry, &mut output).unwrap();
        assert_eq!(output, PAGE);
    }
}

fn data_offset(bytes: &[u8]) -> usize {
    30 + usize::from(u16::from_le_bytes(bytes[26..28].try_into().unwrap()))
        + usize::from(u16::from_le_bytes(bytes[28..30].try_into().unwrap()))
}

#[test]
fn dictionary_and_index_limits_restore_and_do_not_change_other_decoders() {
    assert!(lzma_rust2::limits::check_dictionary(usize::MAX).is_ok());
    lzma_rust2::limits::with_decoder_limits(1024, 3, || {
        assert!(lzma_rust2::limits::check_dictionary(1025).is_err());
        assert!(lzma_rust2::limits::check_index(4).is_err());
        lzma_rust2::limits::with_decoder_limits(2048, 6, || {
            assert!(lzma_rust2::limits::check_dictionary(1025).is_err());
        });
    });
    assert!(lzma_rust2::limits::check_dictionary(usize::MAX).is_ok());
    assert!(lzma_rust2::limits::check_index(u64::MAX).is_ok());
    let panic = std::panic::catch_unwind(|| {
        lzma_rust2::limits::with_decoder_limits(0, 0, || panic!("test"))
    });
    assert!(panic.is_err());
    assert!(lzma_rust2::limits::check_dictionary(1).is_ok());
}

#[test]
fn caps_decoded_lengths_before_copy_and_checks_truncation() {
    assert!(matches!(
        copy_exact(
            &mut Cursor::new(PAGE),
            &mut Vec::new(),
            MAX_MEMBER_BYTES + 1
        ),
        Err(ProductionZipError::Limit)
    ));
    for expected in [0, PAGE.len() as u64 - 1, PAGE.len() as u64 + 1] {
        assert!(matches!(
            copy_exact(&mut Cursor::new(PAGE), &mut Vec::new(), expected),
            Err(ProductionZipError::SizeMismatch)
        ));
    }
}

fn raw_stored_zip(entries: &[(&[u8], &[u8])]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut central = Vec::new();
    for (name, contents) in entries {
        let offset = u32::try_from(bytes.len()).unwrap();
        let len = u32::try_from(contents.len()).unwrap();
        let name_len = u16::try_from(name.len()).unwrap();
        let crc = crc32fast::hash(contents);
        bytes.extend_from_slice(b"PK\x03\x04");
        for value in [20_u16, 0, 0, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, len, len] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&name_len.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(name);
        bytes.extend_from_slice(contents);
        central.extend_from_slice(b"PK\x01\x02");
        for value in [20_u16, 20, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [crc, len, len] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        for value in [name_len, 0, 0, 0, 0] {
            central.extend_from_slice(&value.to_le_bytes());
        }
        central.extend_from_slice(&0_u32.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let offset = u32::try_from(bytes.len()).unwrap();
    let len = u32::try_from(central.len()).unwrap();
    bytes.extend(central);
    bytes.extend_from_slice(b"PK\x05\x06");
    for value in [
        0_u16,
        0,
        u16::try_from(entries.len()).unwrap(),
        u16::try_from(entries.len()).unwrap(),
    ] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(&offset.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}
