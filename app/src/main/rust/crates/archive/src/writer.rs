//! Deterministic application-generated CBZ writing over sequential streams.
//!
//! The writer accepts no paths or platform handles. Callers retain ownership of
//! storage and lifecycle decisions while this module enforces the generated-CBZ
//! entry count, ordering, naming, permissions, compression, and non-empty data
//! contract before reporting success.

use std::io::{self, Read, Write};

use thiserror::Error;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::{COMIC_INFO_ENTRY_NAME, MAX_GENERATED_CBZ_IMAGES, is_generated_page_name};

/// One ordered input entry for an application-generated CBZ.
#[derive(Debug)]
pub struct CbzEntry<R> {
    /// Exact archive name, such as `00000001.jpg` or `ComicInfo.xml`.
    pub name: String,
    /// Stream read to EOF as this entry's contents.
    pub source: R,
}

impl<R> CbzEntry<R> {
    /// Creates an entry without reading or taking platform-specific ownership.
    pub fn new(name: impl Into<String>, source: R) -> Self {
        Self {
            name: name.into(),
            source,
        }
    }
}

/// Summary returned after the central directory is written successfully.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CbzWriteSummary {
    /// Number of sequential image entries written.
    pub image_count: usize,
    /// Total entry count, including [`COMIC_INFO_ENTRY_NAME`].
    pub entry_count: usize,
    /// Sum of every entry's uncompressed byte length.
    pub uncompressed_bytes: u64,
}

/// Deterministic failure produced while writing an application-generated CBZ.
#[derive(Debug, Error)]
pub enum CbzWriteError {
    /// The caller supplied zero images or exceeded the eight-digit page namespace.
    #[error("expected image count must be between 1 and {MAX_GENERATED_CBZ_IMAGES}")]
    InvalidExpectedImageCount,
    /// Inputs do not contain exactly the expected pages plus metadata.
    #[error("CBZ entry count mismatch: expected {expected}, found {actual}")]
    EntryCount {
        /// Expected page entries plus one metadata entry.
        expected: usize,
        /// Number of supplied entries.
        actual: usize,
    },
    /// Page names must be sequential and metadata must be the final entry.
    #[error("CBZ entry {index} has an unexpected name")]
    UnexpectedEntryName {
        /// Zero-based archive entry position.
        index: usize,
    },
    /// The ZIP local header for an entry could not be written.
    #[error("failed to start CBZ entry {index}")]
    StartEntry {
        /// Zero-based archive entry position.
        index: usize,
        /// ZIP writer failure.
        #[source]
        source: zip::result::ZipError,
    },
    /// Reading an input or writing its Stored payload failed.
    #[error("failed to copy CBZ entry {index}")]
    CopyEntry {
        /// Zero-based archive entry position.
        index: usize,
        /// Underlying input or output failure.
        #[source]
        source: io::Error,
    },
    /// A page or metadata input contains no bytes.
    #[error("CBZ entry {index} is empty")]
    EmptyEntry {
        /// Zero-based archive entry position.
        index: usize,
    },
    /// Accumulated uncompressed lengths exceed the summary representation.
    #[error("CBZ uncompressed size overflow")]
    UncompressedSizeOverflow,
    /// The final entry or central directory could not be written.
    #[error("failed to finish CBZ archive")]
    FinishArchive(#[source] zip::result::ZipError),
}

/// Writes one application-generated CBZ without requiring a seekable output.
///
/// `entries` must contain exactly `expected_images` sequential page names,
/// followed by one [`COMIC_INFO_ENTRY_NAME`]. Every input is copied to EOF as a
/// non-empty Stored entry with Unix mode `0644`. Names and counts are checked
/// before any output bytes are written. ZIP metadata is generated
/// deterministically instead of copying platform UID, GID, or timestamps.
///
/// The returned writer is positioned immediately after the completed central
/// directory. A failure may leave partial bytes in the output; the storage owner
/// must discard or truncate that output instead of retrying another writer on it.
///
/// # Errors
///
/// Returns [`CbzWriteError`] for invalid caller expectations, contract
/// mismatches, empty or unreadable inputs, output failures, size overflow, or
/// central-directory finalization failures.
pub fn write_generated_cbz<W, R>(
    output: W,
    mut entries: Vec<CbzEntry<R>>,
    expected_images: usize,
) -> Result<(W, CbzWriteSummary), CbzWriteError>
where
    W: Write,
    R: Read,
{
    validate_contract(&entries, expected_images)?;

    let entry_count = entries.len();
    let mut archive = ZipWriter::new_stream(output);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    let mut uncompressed_bytes = 0_u64;

    for (index, entry) in entries.iter_mut().enumerate() {
        archive
            .start_file(&entry.name, options)
            .map_err(|source| CbzWriteError::StartEntry { index, source })?;
        let entry_bytes = io::copy(&mut entry.source, &mut archive)
            .map_err(|source| CbzWriteError::CopyEntry { index, source })?;
        if entry_bytes == 0 {
            return Err(CbzWriteError::EmptyEntry { index });
        }
        uncompressed_bytes = add_uncompressed_bytes(uncompressed_bytes, entry_bytes)?;
    }

    let output = archive
        .finish()
        .map_err(CbzWriteError::FinishArchive)?
        .into_inner();
    Ok((
        output,
        CbzWriteSummary {
            image_count: expected_images,
            entry_count,
            uncompressed_bytes,
        },
    ))
}

fn validate_contract<R>(
    entries: &[CbzEntry<R>],
    expected_images: usize,
) -> Result<(), CbzWriteError> {
    if expected_images == 0 || expected_images > MAX_GENERATED_CBZ_IMAGES {
        return Err(CbzWriteError::InvalidExpectedImageCount);
    }
    let expected_entries = expected_images
        .checked_add(1)
        .ok_or(CbzWriteError::InvalidExpectedImageCount)?;
    if entries.len() != expected_entries {
        return Err(CbzWriteError::EntryCount {
            expected: expected_entries,
            actual: entries.len(),
        });
    }

    for (index, entry) in entries.iter().enumerate() {
        let has_expected_name = if index < expected_images {
            is_generated_page_name(&entry.name, index + 1)
        } else {
            entry.name == COMIC_INFO_ENTRY_NAME
        };
        if !has_expected_name {
            return Err(CbzWriteError::UnexpectedEntryName { index });
        }
    }
    Ok(())
}

fn add_uncompressed_bytes(total: u64, entry_bytes: u64) -> Result<u64, CbzWriteError> {
    total
        .checked_add(entry_bytes)
        .ok_or(CbzWriteError::UncompressedSizeOverflow)
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor, Read, Write};

    use zip::{CompressionMethod, ZipArchive};

    use super::{
        CbzEntry, CbzWriteError, CbzWriteSummary, add_uncompressed_bytes, write_generated_cbz,
    };
    use crate::{MAX_GENERATED_CBZ_IMAGES, validate_generated_cbz};

    const PAGE_ONE: &[u8] = b"page-one-unique-payload";
    const PAGE_TWO: &[u8] = b"page-two-unique-payload";
    const COMIC_INFO: &[u8] = b"<ComicInfo><PageCount>2</PageCount></ComicInfo>";

    fn entry(name: &str, contents: &[u8]) -> CbzEntry<Cursor<Vec<u8>>> {
        CbzEntry::new(name, Cursor::new(contents.to_vec()))
    }

    fn valid_entries() -> Vec<CbzEntry<Cursor<Vec<u8>>>> {
        vec![
            entry("00000001.jpg", PAGE_ONE),
            entry("00000002.webp", PAGE_TWO),
            entry("ComicInfo.xml", COMIC_INFO),
        ]
    }

    #[test]
    fn writes_streaming_stored_cbz_and_round_trips_validation() {
        let (bytes, summary) = write_generated_cbz(Vec::new(), valid_entries(), 2)
            .expect("generated archive should write");
        assert_eq!(
            summary,
            CbzWriteSummary {
                image_count: 2,
                entry_count: 3,
                uncompressed_bytes: u64::try_from(
                    PAGE_ONE.len() + PAGE_TWO.len() + COMIC_INFO.len()
                )
                .expect("fixture size fits u64"),
            }
        );
        assert_eq!(
            validate_generated_cbz(Cursor::new(bytes.clone()), 2)
                .expect("writer output should validate")
                .uncompressed_bytes,
            summary.uncompressed_bytes
        );

        let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("archive should open");
        for (index, expected_name) in ["00000001.jpg", "00000002.webp", "ComicInfo.xml"]
            .iter()
            .enumerate()
        {
            let entry = archive.by_index(index).expect("entry should open");
            assert_eq!(entry.name(), *expected_name);
            assert_eq!(entry.compression(), CompressionMethod::Stored);
            assert_eq!(entry.unix_mode(), Some(0o100_644));
        }
    }

    #[test]
    fn accepts_one_page_and_lowercase_alphanumeric_extensions() {
        for name in ["00000001.j", "00000001.avif", "00000001.j2k"] {
            let entries = vec![entry(name, PAGE_ONE), entry("ComicInfo.xml", COMIC_INFO)];
            let (_, summary) = write_generated_cbz(Vec::new(), entries, 1)
                .expect("supported extension should write");
            assert_eq!(summary.image_count, 1);
        }
    }

    #[test]
    fn rejects_invalid_expected_counts_before_writing() {
        let mut output = Vec::new();
        assert!(matches!(
            write_generated_cbz(&mut output, valid_entries(), 0),
            Err(CbzWriteError::InvalidExpectedImageCount)
        ));
        assert!(output.is_empty());

        assert!(matches!(
            write_generated_cbz(Vec::new(), valid_entries(), MAX_GENERATED_CBZ_IMAGES + 1,),
            Err(CbzWriteError::InvalidExpectedImageCount)
        ));
    }

    #[test]
    fn rejects_wrong_entry_counts_before_writing() {
        let too_few = vec![entry("00000001.jpg", PAGE_ONE)];
        assert!(matches!(
            write_generated_cbz(Vec::new(), too_few, 1),
            Err(CbzWriteError::EntryCount {
                expected: 2,
                actual: 1
            })
        ));

        let too_many = vec![
            entry("00000001.jpg", PAGE_ONE),
            entry("ComicInfo.xml", COMIC_INFO),
            entry("extra.xml", COMIC_INFO),
        ];
        assert!(matches!(
            write_generated_cbz(Vec::new(), too_many, 1),
            Err(CbzWriteError::EntryCount {
                expected: 2,
                actual: 3
            })
        ));
    }

    #[test]
    fn rejects_wrong_or_unsafe_names_before_writing() {
        for invalid_name in [
            "00000002.jpg",
            "../00000001.jpg",
            "00000001.JPG",
            "00000001.",
            "00000001.extension-is-too-long",
        ] {
            let entries = vec![
                entry(invalid_name, PAGE_ONE),
                entry("ComicInfo.xml", COMIC_INFO),
            ];
            assert!(matches!(
                write_generated_cbz(Vec::new(), entries, 1),
                Err(CbzWriteError::UnexpectedEntryName { index: 0 })
            ));
        }

        let wrong_metadata = vec![
            entry("00000001.jpg", PAGE_ONE),
            entry("metadata.xml", COMIC_INFO),
        ];
        assert!(matches!(
            write_generated_cbz(Vec::new(), wrong_metadata, 1),
            Err(CbzWriteError::UnexpectedEntryName { index: 1 })
        ));
    }

    #[test]
    fn rejects_empty_page_and_metadata_inputs() {
        let empty_page = vec![
            entry("00000001.jpg", b""),
            entry("ComicInfo.xml", COMIC_INFO),
        ];
        assert!(matches!(
            write_generated_cbz(Vec::new(), empty_page, 1),
            Err(CbzWriteError::EmptyEntry { index: 0 })
        ));

        let empty_metadata = vec![entry("00000001.jpg", PAGE_ONE), entry("ComicInfo.xml", b"")];
        assert!(matches!(
            write_generated_cbz(Vec::new(), empty_metadata, 1),
            Err(CbzWriteError::EmptyEntry { index: 1 })
        ));
    }

    #[test]
    fn reports_input_and_payload_output_failures() {
        let unreadable = vec![
            CbzEntry::new("00000001.jpg", FailingReader),
            CbzEntry::new("ComicInfo.xml", FailingReader),
        ];
        assert!(matches!(
            write_generated_cbz(Vec::new(), unreadable, 1),
            Err(CbzWriteError::CopyEntry { index: 0, .. })
        ));

        let entries = vec![
            entry("00000001.jpg", &[0xa5; 64]),
            entry("ComicInfo.xml", COMIC_INFO),
        ];
        let mut payload_failing = PayloadFailingWriter::default();
        payload_failing
            .flush()
            .expect("injected flush should succeed");
        assert!(matches!(
            write_generated_cbz(payload_failing, entries, 1),
            Err(CbzWriteError::CopyEntry { index: 0, .. })
        ));
    }

    #[test]
    fn reports_start_and_finish_output_failures() {
        let mut always_failing = AlwaysFailingWriter;
        always_failing
            .flush()
            .expect("injected flush should succeed");
        assert!(matches!(
            write_generated_cbz(always_failing, valid_entries(), 2),
            Err(CbzWriteError::StartEntry { index: 0, .. })
        ));

        let mut central_directory_failing = CentralDirectoryFailingWriter;
        central_directory_failing
            .flush()
            .expect("injected flush should succeed");
        assert!(matches!(
            write_generated_cbz(central_directory_failing, valid_entries(), 2),
            Err(CbzWriteError::FinishArchive(_))
        ));
    }

    #[test]
    fn reports_uncompressed_size_overflow() {
        assert_eq!(add_uncompressed_bytes(5, 7).expect("sum should fit"), 12);
        assert!(matches!(
            add_uncompressed_bytes(u64::MAX, 1),
            Err(CbzWriteError::UncompressedSizeOverflow)
        ));
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected input failure"))
        }
    }

    struct AlwaysFailingWriter;

    impl Write for AlwaysFailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("injected output failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct PayloadFailingWriter {
        bytes: Vec<u8>,
    }

    impl Write for PayloadFailingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if buffer.contains(&0xa5) {
                return Err(io::Error::other("injected payload failure"));
            }
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct CentralDirectoryFailingWriter;

    impl Write for CentralDirectoryFailingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if buffer.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
                return Err(io::Error::other("injected central directory failure"));
            }
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
