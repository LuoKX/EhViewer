//! Safe ZIP/CBZ reading and opaque-handle lifecycle.
//!
//! The reader preserves the legacy image-extension filter and natural sorting
//! semantics while rejecting encrypted entries, unsupported compression, and
//! unbounded allocations. Every extraction reopens the ZIP view over the owned
//! seekable source, verifies CRC while copying, and restores the source's prior
//! seek position before returning.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use flate2::read::DeflateDecoder;
use rawzip::{
    CompressionMethod as RawCompressionMethod, Error as RawZipError,
    MAX_CENTRAL_DIRECTORY_RECORD_SIZE, ZipArchive as RawZipArchive, ZipArchiveEntryWayfinder,
};
use thiserror::Error;

/// Current JNI-facing ZIP reader protocol version.
pub const ZIP_READER_API_VERSION: u8 = 1;

/// Stable module discriminator for the ZIP reader boundary.
pub const ZIP_READER_MODULE: &str = "archiveReader";

/// Maximum number of central-directory entries accepted by the Rust reader.
pub const MAX_ZIP_ARCHIVE_ENTRIES: usize = 100_000;

const MAX_ZIP_ARCHIVE_ENTRIES_U64: u64 = 100_000;

/// Maximum uncompressed size of one image returned to Android.
pub const MAX_ZIP_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

const PLAYABLE_EXTENSIONS: [&str; 11] = [
    "jpeg", "jpg", "png", "gif", "webp", "bmp", "ico", "wbmp", "heic", "heif", "avif",
];

/// A deterministic ZIP reader, extraction, or registry failure.
#[derive(Debug, Error)]
pub enum ZipReaderError {
    /// Reading the source's current seek position failed.
    #[error("failed to read ZIP source position")]
    SourcePosition(#[source] io::Error),
    /// Seeking to the start of the ZIP source failed.
    #[error("failed to seek to ZIP source start")]
    SeekStart(#[source] io::Error),
    /// Restoring the source position after an operation failed.
    #[error("failed to restore ZIP source position")]
    RestorePosition(#[source] io::Error),
    /// The source is not a readable ZIP archive.
    #[error("ZIP central directory is invalid")]
    OpenArchive(#[source] RawZipError),
    /// The central directory exceeds the bounded reader policy.
    #[error("ZIP declares {actual} entries; maximum is {maximum}")]
    TooManyEntries {
        /// Maximum accepted central-directory entries.
        maximum: u64,
        /// Entries declared by the ZIP archive.
        actual: u64,
    },
    /// The EOCD declaration and streamed central-directory count disagree.
    #[error("ZIP entry count mismatch: declared {declared}, observed {actual}")]
    EntryCountMismatch {
        /// Entry count declared by the selected EOCD/ZIP64 record.
        declared: u64,
        /// Entry count observed by bounded central-directory iteration.
        actual: u64,
    },
    /// One central-directory entry could not be inspected or opened.
    #[error("ZIP archive entry {archive_index} is invalid")]
    OpenEntry {
        /// Physical zero-based central-directory position.
        archive_index: usize,
        /// ZIP parser failure.
        #[source]
        source: RawZipError,
    },
    /// A playable image entry is encrypted and must use the legacy password path.
    #[error("ZIP image entry {archive_index} is encrypted")]
    EncryptedEntry {
        /// Physical zero-based central-directory position.
        archive_index: usize,
    },
    /// A playable image uses a compression method outside Stored or Deflate.
    #[error("ZIP image entry {archive_index} uses unsupported compression")]
    UnsupportedCompression {
        /// Physical zero-based central-directory position.
        archive_index: usize,
    },
    /// A playable image exceeds the bounded Android allocation policy.
    #[error("ZIP image entry {archive_index} is {size} bytes; maximum is {maximum}")]
    EntryTooLarge {
        /// Physical zero-based central-directory position.
        archive_index: usize,
        /// Declared uncompressed image size.
        size: u64,
        /// Maximum accepted uncompressed image size.
        maximum: u64,
    },
    /// A playable image entry contains no bytes.
    #[error("ZIP image entry {archive_index} is empty")]
    EmptyEntry {
        /// Physical zero-based central-directory position.
        archive_index: usize,
    },
    /// The archive contains no regular file with a supported image extension.
    #[error("ZIP contains no playable image entries")]
    NoPlayableEntries,
    /// The Android-facing image index is outside the filtered entry list.
    #[error("ZIP image index {index} is outside entry count {entry_count}")]
    InvalidEntryIndex {
        /// Requested zero-based filtered image index.
        index: usize,
        /// Number of filtered playable images.
        entry_count: usize,
    },
    /// Copying an image payload failed, including ZIP CRC errors.
    #[error("ZIP image index {index} failed integrity extraction")]
    ExtractEntry {
        /// Requested zero-based filtered image index.
        index: usize,
        /// Read, write, decompression, or CRC failure.
        #[source]
        source: io::Error,
    },
    /// Extracted bytes differ from the central-directory size.
    #[error("ZIP image index {index} size mismatch: expected {expected}, observed {actual}")]
    EntrySizeMismatch {
        /// Requested zero-based filtered image index.
        index: usize,
        /// Declared uncompressed image size.
        expected: u64,
        /// Bytes observed before bounded extraction stopped.
        actual: u64,
    },
    /// The opaque-handle counter cannot issue another positive identifier.
    #[error("ZIP reader handle space is exhausted")]
    HandleExhausted,
    /// A generated handle unexpectedly already exists in the registry.
    #[error("ZIP reader handle collision")]
    HandleCollision,
    /// The requested positive opaque handle is absent or already closed.
    #[error("unknown ZIP reader handle: {handle}")]
    UnknownHandle {
        /// Requested opaque reader handle.
        handle: i64,
    },
    /// The global handle map was poisoned by a panic.
    #[error("ZIP reader registry is poisoned")]
    RegistryPoisoned,
    /// One reader lock was poisoned by a panic.
    #[error("ZIP reader state is poisoned")]
    ReaderPoisoned,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ImageEntry {
    archive_index: usize,
    wayfinder: ZipArchiveEntryWayfinder,
    name: Vec<u8>,
    extension: String,
    compression: RawCompressionMethod,
    size: u64,
}

/// One ZIP/CBZ source with filtered image metadata and bounded extraction.
pub struct ZipArchiveReader<R> {
    source: R,
    entries: Vec<ImageEntry>,
}

impl<R> ZipArchiveReader<R>
where
    R: Read + Seek,
{
    /// Opens and indexes one unencrypted ZIP/CBZ source.
    ///
    /// `sort_entries` applies the legacy natural comparator to complete entry
    /// names. Unsupported non-image entries are ignored.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for seek failures, malformed archives,
    /// excessive entry counts, encrypted or unsupported playable entries,
    /// oversized images, or archives without playable images.
    pub fn open(mut source: R, sort_entries: bool) -> Result<Self, ZipReaderError> {
        let entries = with_restored_source(&mut source, |source| {
            index_playable_entries(source, sort_entries, MAX_ZIP_ARCHIVE_ENTRIES_U64)
        })?;
        Ok(Self { source, entries })
    }

    /// Returns the number of filtered playable image entries.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Returns one filtered image entry's lowercase extension.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError::InvalidEntryIndex`] when `index` is outside
    /// [`Self::entry_count`].
    pub fn extension(&self, index: usize) -> Result<&str, ZipReaderError> {
        Ok(&self.entry(index)?.extension)
    }

    /// Returns one filtered image entry's declared uncompressed size.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError::InvalidEntryIndex`] when `index` is outside
    /// [`Self::entry_count`].
    pub fn entry_size(&self, index: usize) -> Result<u64, ZipReaderError> {
        Ok(self.entry(index)?.size)
    }

    /// Extracts one filtered image into Rust-owned bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for an invalid index, allocation conversion,
    /// malformed local entry, decompression, read, CRC, or size failure.
    pub fn extract(&mut self, index: usize) -> Result<Vec<u8>, ZipReaderError> {
        let metadata = self.entry(index)?.clone();
        if metadata.size > MAX_ZIP_IMAGE_BYTES {
            return Err(ZipReaderError::EntryTooLarge {
                archive_index: metadata.archive_index,
                size: metadata.size,
                maximum: MAX_ZIP_IMAGE_BYTES,
            });
        }
        let Ok(capacity) = usize::try_from(metadata.size) else {
            return Err(ZipReaderError::EntryTooLarge {
                archive_index: metadata.archive_index,
                size: metadata.size,
                maximum: MAX_ZIP_IMAGE_BYTES,
            });
        };
        let mut output = Vec::with_capacity(capacity);
        self.extract_to(index, &mut output)?;
        Ok(output)
    }

    /// Streams one filtered image to a caller-owned writer while verifying CRC.
    ///
    /// At most the declared uncompressed size is written. An additional byte is
    /// read only into a local probe so understated metadata cannot grow the
    /// caller's buffer or file before the size mismatch is reported.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for an invalid index, malformed local entry,
    /// decompression, read/write, CRC, seek restoration, or size failure.
    pub fn extract_to<W>(&mut self, index: usize, writer: &mut W) -> Result<u64, ZipReaderError>
    where
        W: Write,
    {
        let metadata = self.entry(index)?.clone();
        let copied = with_restored_source(&mut self.source, |source| {
            extract_entry(source, &metadata, index, writer)
        })?;
        Ok(copied)
    }

    fn entry(&self, index: usize) -> Result<&ImageEntry, ZipReaderError> {
        self.entries
            .get(index)
            .ok_or(ZipReaderError::InvalidEntryIndex {
                index,
                entry_count: self.entries.len(),
            })
    }
}

struct ManagedReader<R> {
    reader: Mutex<ZipArchiveReader<R>>,
    closed: AtomicBool,
}

type SharedReader<R> = Arc<ManagedReader<R>>;

/// Thread-safe positive opaque handles for independent ZIP readers.
pub struct ZipReaderRegistry<R> {
    next_handle: AtomicI64,
    readers: Mutex<HashMap<i64, SharedReader<R>>>,
}

impl<R> Default for ZipReaderRegistry<R> {
    fn default() -> Self {
        Self {
            next_handle: AtomicI64::new(1),
            readers: Mutex::new(HashMap::new()),
        }
    }
}

impl<R> ZipReaderRegistry<R>
where
    R: Read + Seek + Send,
{
    /// Opens a source, indexes it, and returns a new positive opaque handle.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] when opening/indexing fails, handle allocation
    /// is exhausted, a collision occurs, or the registry is poisoned.
    pub fn open(&self, source: R, sort_entries: bool) -> Result<i64, ZipReaderError> {
        let reader = Arc::new(ManagedReader {
            reader: Mutex::new(ZipArchiveReader::open(source, sort_entries)?),
            closed: AtomicBool::new(false),
        });
        let handle = self.next_handle()?;
        let mut readers = self
            .readers
            .lock()
            .map_err(|_| ZipReaderError::RegistryPoisoned)?;
        match readers.entry(handle) {
            Entry::Vacant(entry) => {
                entry.insert(reader);
                Ok(handle)
            }
            Entry::Occupied(_) => Err(ZipReaderError::HandleCollision),
        }
    }

    /// Returns the filtered image count for an open handle.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for an unknown handle or poisoned lock.
    pub fn entry_count(&self, handle: i64) -> Result<usize, ZipReaderError> {
        self.with_reader(handle, |reader| Ok(reader.entry_count()))
    }

    /// Returns one filtered image entry's extension as an owned string.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for an unknown handle, invalid index, or
    /// poisoned lock.
    pub fn extension(&self, handle: i64, index: usize) -> Result<String, ZipReaderError> {
        self.with_reader(handle, |reader| reader.extension(index).map(str::to_owned))
    }

    /// Returns one image entry's declared uncompressed size.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for an unknown handle, invalid index, or
    /// poisoned lock.
    pub fn entry_size(&self, handle: i64, index: usize) -> Result<u64, ZipReaderError> {
        self.with_reader(handle, |reader| reader.entry_size(index))
    }

    /// Extracts one image into owned bytes.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for handle, index, seek, ZIP, allocation, or
    /// CRC failures.
    pub fn extract(&self, handle: i64, index: usize) -> Result<Vec<u8>, ZipReaderError> {
        self.with_reader(handle, |reader| reader.extract(index))
    }

    /// Streams one image to a caller-owned writer.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] for handle, index, seek, ZIP, writer, or CRC
    /// failures.
    pub fn extract_to<W>(
        &self,
        handle: i64,
        index: usize,
        writer: &mut W,
    ) -> Result<u64, ZipReaderError>
    where
        W: Write,
    {
        self.with_reader(handle, |reader| reader.extract_to(index, writer))
    }

    /// Idempotently removes a handle and waits for any active extraction.
    ///
    /// Returns `true` only when this call removed an open reader. Non-positive or
    /// already-closed handles return `false`. The closed marker is published
    /// before waiting, so operations that cloned the handle but remain queued
    /// behind the active operation fail instead of starting after close begins.
    ///
    /// # Errors
    ///
    /// Returns [`ZipReaderError`] when the registry or reader lock is poisoned.
    pub fn close(&self, handle: i64) -> Result<bool, ZipReaderError> {
        if handle <= 0 {
            return Ok(false);
        }
        let reader = self
            .readers
            .lock()
            .map_err(|_| ZipReaderError::RegistryPoisoned)?
            .remove(&handle);
        let Some(reader) = reader else {
            return Ok(false);
        };
        reader.closed.store(true, AtomicOrdering::Release);
        drop(
            reader
                .reader
                .lock()
                .map_err(|_| ZipReaderError::ReaderPoisoned)?,
        );
        Ok(true)
    }

    fn next_handle(&self) -> Result<i64, ZipReaderError> {
        self.next_handle
            .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |handle| {
                handle.checked_add(1).filter(|next| *next > 0)
            })
            .map_err(|_| ZipReaderError::HandleExhausted)
    }

    fn with_reader<T, F>(&self, handle: i64, operation: F) -> Result<T, ZipReaderError>
    where
        F: FnOnce(&mut ZipArchiveReader<R>) -> Result<T, ZipReaderError>,
    {
        if handle <= 0 {
            return Err(ZipReaderError::UnknownHandle { handle });
        }
        let reader = self
            .readers
            .lock()
            .map_err(|_| ZipReaderError::RegistryPoisoned)?
            .get(&handle)
            .cloned()
            .ok_or(ZipReaderError::UnknownHandle { handle })?;
        let mut reader_state = reader
            .reader
            .lock()
            .map_err(|_| ZipReaderError::ReaderPoisoned)?;
        if reader.closed.load(AtomicOrdering::Acquire) {
            return Err(ZipReaderError::UnknownHandle { handle });
        }
        operation(&mut reader_state)
    }
}

fn with_restored_source<R, T, F>(source: &mut R, operation: F) -> Result<T, ZipReaderError>
where
    R: Read + Seek,
    F: FnOnce(&mut R) -> Result<T, ZipReaderError>,
{
    let original_position = source
        .stream_position()
        .map_err(ZipReaderError::SourcePosition)?;
    let result = (|| {
        source
            .seek(SeekFrom::Start(0))
            .map_err(ZipReaderError::SeekStart)?;
        operation(source)
    })();
    source
        .seek(SeekFrom::Start(original_position))
        .map_err(ZipReaderError::RestorePosition)?;
    result
}

fn index_playable_entries<R>(
    source: &mut R,
    sort_entries: bool,
    maximum_entries: u64,
) -> Result<Vec<ImageEntry>, ZipReaderError>
where
    R: Read + Seek,
{
    let mut buffer = vec![0_u8; MAX_CENTRAL_DIRECTORY_RECORD_SIZE];
    let archive = RawZipArchive::from_seekable(&mut *source, &mut buffer)
        .map_err(ZipReaderError::OpenArchive)?;
    let declared_entries = archive.entries_hint();
    enforce_entry_limit(declared_entries, maximum_entries)?;

    let mut entries = Vec::new();
    let mut actual_entries = 0_u64;
    let mut archive_entries = archive.entries(&mut buffer);
    loop {
        let Ok(archive_index) = usize::try_from(actual_entries) else {
            return Err(ZipReaderError::TooManyEntries {
                maximum: maximum_entries,
                actual: actual_entries,
            });
        };
        let next_entry = match archive_entries.next_entry() {
            Ok(next_entry) => next_entry,
            Err(source) => {
                return Err(ZipReaderError::OpenEntry {
                    archive_index,
                    source,
                });
            }
        };
        let Some(entry) = next_entry else {
            break;
        };
        actual_entries = actual_entries.saturating_add(1);
        enforce_entry_limit(actual_entries, maximum_entries)?;

        if entry.is_dir() {
            continue;
        }
        let path = entry.file_path();
        let name = path.as_ref();
        let Some(extension) = playable_extension(name) else {
            continue;
        };
        if entry.flags().is_encrypted() {
            return Err(ZipReaderError::EncryptedEntry { archive_index });
        }
        let compression = entry.compression_method();
        if compression != RawCompressionMethod::STORE
            && compression != RawCompressionMethod::DEFLATE
        {
            return Err(ZipReaderError::UnsupportedCompression { archive_index });
        }
        let size = entry.uncompressed_size_hint();
        if size == 0 {
            return Err(ZipReaderError::EmptyEntry { archive_index });
        }
        if size > MAX_ZIP_IMAGE_BYTES {
            return Err(ZipReaderError::EntryTooLarge {
                archive_index,
                size,
                maximum: MAX_ZIP_IMAGE_BYTES,
            });
        }
        entries.push(ImageEntry {
            archive_index,
            wayfinder: entry.wayfinder(),
            name: name.to_vec(),
            extension: extension.to_owned(),
            compression,
            size,
        });
    }

    if actual_entries != declared_entries {
        return Err(ZipReaderError::EntryCountMismatch {
            declared: declared_entries,
            actual: actual_entries,
        });
    }
    if entries.is_empty() {
        return Err(ZipReaderError::NoPlayableEntries);
    }
    if sort_entries {
        entries.sort_by(|left, right| natural_cmp(&left.name, &right.name));
    }
    Ok(entries)
}

fn enforce_entry_limit(actual: u64, maximum: u64) -> Result<(), ZipReaderError> {
    if actual > maximum {
        return Err(ZipReaderError::TooManyEntries { maximum, actual });
    }
    Ok(())
}

fn extract_entry<R>(
    source: &mut R,
    metadata: &ImageEntry,
    index: usize,
    writer: &mut dyn Write,
) -> Result<u64, ZipReaderError>
where
    R: Read + Seek,
{
    let mut buffer = vec![0_u8; MAX_CENTRAL_DIRECTORY_RECORD_SIZE];
    let archive = RawZipArchive::from_seekable(&mut *source, &mut buffer)
        .map_err(ZipReaderError::OpenArchive)?;
    enforce_entry_limit(archive.entries_hint(), MAX_ZIP_ARCHIVE_ENTRIES_U64)?;
    let entry =
        archive
            .get_entry(metadata.wayfinder)
            .map_err(|source| ZipReaderError::OpenEntry {
                archive_index: metadata.archive_index,
                source,
            })?;

    if metadata.compression == RawCompressionMethod::STORE {
        let compressed = entry.reader();
        let mut verified = entry.verifying_reader(compressed);
        return copy_verified(&mut verified, metadata, index, writer);
    }
    if metadata.compression == RawCompressionMethod::DEFLATE {
        let compressed = entry.reader();
        let decompressed = DeflateDecoder::new(compressed);
        let mut verified = entry.verifying_reader(decompressed);
        return copy_verified(&mut verified, metadata, index, writer);
    }
    Err(ZipReaderError::UnsupportedCompression {
        archive_index: metadata.archive_index,
    })
}

fn copy_verified(
    reader: &mut dyn Read,
    metadata: &ImageEntry,
    index: usize,
    writer: &mut dyn Write,
) -> Result<u64, ZipReaderError> {
    let copied = {
        let mut bounded = (&mut *reader).take(metadata.size);
        match io::copy(&mut bounded, writer) {
            Ok(copied) => copied,
            Err(source) => return Err(ZipReaderError::ExtractEntry { index, source }),
        }
    };
    let mut probe = [0_u8; 1];
    let trailing = match reader.read(&mut probe) {
        Ok(trailing) => trailing,
        Err(source) => return Err(ZipReaderError::ExtractEntry { index, source }),
    };
    if copied != metadata.size || trailing != 0 {
        let actual = if trailing == 0 {
            copied
        } else {
            copied.saturating_add(1)
        };
        return Err(ZipReaderError::EntrySizeMismatch {
            index,
            expected: metadata.size,
            actual,
        });
    }
    Ok(copied)
}

fn playable_extension(name: &[u8]) -> Option<&'static str> {
    let separator = name.iter().rposition(|byte| *byte == b'.')?;
    let extension = name.get(separator.checked_add(1)?..)?;
    PLAYABLE_EXTENSIONS
        .iter()
        .copied()
        .find(|candidate| candidate.as_bytes() == extension)
}

fn natural_cmp(left: &[u8], right: &[u8]) -> Ordering {
    let (mut left_index, mut right_index) = (0, 0);
    loop {
        while is_natural_space(left, left_index) {
            left_index += 1;
        }
        while is_natural_space(right, right_index) {
            right_index += 1;
        }

        if byte_at(left, left_index).is_ascii_digit()
            && byte_at(right, right_index).is_ascii_digit()
        {
            let ordering = compare_digit_runs(left, left_index, right, right_index);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }

        let ordering = byte_at(left, left_index).cmp(&byte_at(right, right_index));
        if ordering != Ordering::Equal {
            return ordering;
        }
        if byte_at(left, left_index) == 0 {
            return Ordering::Equal;
        }
        left_index += 1;
        right_index += 1;
    }
}

fn compare_digit_runs(
    left: &[u8],
    mut left_index: usize,
    right: &[u8],
    mut right_index: usize,
) -> Ordering {
    let mut bias = Ordering::Equal;
    loop {
        let (left_byte, right_byte) = (byte_at(left, left_index), byte_at(right, right_index));
        match (left_byte.is_ascii_digit(), right_byte.is_ascii_digit()) {
            (false, false) => return bias,
            (false, true) => return Ordering::Less,
            (true, false) => return Ordering::Greater,
            (true, true) => {
                if bias == Ordering::Equal {
                    bias = left_byte.cmp(&right_byte);
                }
            }
        }
        left_index += 1;
        right_index += 1;
    }
}

fn is_natural_space(bytes: &[u8], index: usize) -> bool {
    let byte = byte_at(bytes, index);
    byte.is_ascii_whitespace()
        || (byte == b'0' && byte_at(bytes, index.saturating_add(1)).is_ascii_digit())
}

fn byte_at(bytes: &[u8], index: usize) -> u8 {
    bytes.get(index).copied().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;
    use std::io::{self, Cursor, Write};
    use std::sync::Arc;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread;
    use std::time::{Duration, Instant};

    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::{
        MAX_ZIP_ARCHIVE_ENTRIES_U64, MAX_ZIP_IMAGE_BYTES, PLAYABLE_EXTENSIONS,
        RawCompressionMethod, ZipArchiveReader, ZipReaderError, ZipReaderRegistry,
        index_playable_entries, natural_cmp, playable_extension,
    };

    const IMAGE_TWO: &[u8] = b"second-image-payload";
    const IMAGE_TEN: &[u8] = b"tenth-image-payload";

    #[derive(Clone, Copy)]
    struct TestEntry<'a> {
        name: &'a str,
        contents: &'a [u8],
        compression: CompressionMethod,
    }

    fn zip_bytes(entries: &[TestEntry<'_>]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        for entry in entries {
            if entry.name.ends_with('/') {
                writer
                    .add_directory(entry.name, SimpleFileOptions::default())
                    .expect("test directory should write");
                continue;
            }
            writer
                .start_file(
                    entry.name,
                    SimpleFileOptions::default().compression_method(entry.compression),
                )
                .expect("test entry should start");
            writer
                .write_all(entry.contents)
                .expect("test entry should write");
        }
        writer
            .finish()
            .expect("test archive should finish")
            .into_inner()
    }

    fn valid_archive() -> Vec<u8> {
        zip_bytes(&[
            TestEntry {
                name: "folder/image10.jpg",
                contents: IMAGE_TEN,
                compression: CompressionMethod::Deflated,
            },
            TestEntry {
                name: "ComicInfo.xml",
                contents: b"metadata",
                compression: CompressionMethod::Stored,
            },
            TestEntry {
                name: "folder/",
                contents: b"",
                compression: CompressionMethod::Stored,
            },
            TestEntry {
                name: "folder/image2.png",
                contents: IMAGE_TWO,
                compression: CompressionMethod::Stored,
            },
            TestEntry {
                name: "folder/ignored.JPG",
                contents: b"uppercase",
                compression: CompressionMethod::Stored,
            },
        ])
    }

    #[test]
    fn indexes_filters_and_naturally_sorts_like_the_legacy_reader() {
        let mut cursor = Cursor::new(valid_archive());
        cursor.set_position(3);
        let mut sorted = ZipArchiveReader::open(cursor, true).expect("archive should open");
        assert_eq!(sorted.source.position(), 3);
        assert_eq!(sorted.entry_count(), 2);
        assert_eq!(sorted.extension(0).expect("first extension"), "png");
        assert_eq!(sorted.extension(1).expect("second extension"), "jpg");
        assert_eq!(
            sorted.entry_size(0).expect("first size"),
            u64::try_from(IMAGE_TWO.len()).expect("fixture size fits")
        );
        assert_eq!(sorted.extract(0).expect("stored image"), IMAGE_TWO);
        assert_eq!(sorted.source.position(), 3);
        assert_eq!(sorted.extract(1).expect("deflated image"), IMAGE_TEN);
        assert_eq!(sorted.source.position(), 3);

        let unsorted = ZipArchiveReader::open(Cursor::new(valid_archive()), false)
            .expect("archive should open");
        assert_eq!(unsorted.extension(0).expect("first extension"), "jpg");
        assert_eq!(unsorted.extension(1).expect("second extension"), "png");
    }

    #[test]
    fn bounds_declared_and_streamed_central_directory_counts() {
        let mut declared_over_limit = Cursor::new(valid_archive());
        assert!(matches!(
            index_playable_entries(&mut declared_over_limit, false, 4),
            Err(ZipReaderError::TooManyEntries {
                maximum: 4,
                actual: 5
            })
        ));

        let mut understated = valid_archive();
        patch_classic_entry_count(&mut understated, 2);
        assert!(matches!(
            index_playable_entries(&mut Cursor::new(understated), false, 2),
            Err(ZipReaderError::TooManyEntries {
                maximum: 2,
                actual: 3
            })
        ));
    }

    #[test]
    fn rejects_declared_entry_count_mismatches() {
        let mut overstated = valid_archive();
        patch_classic_entry_count(&mut overstated, 6);
        assert!(matches!(
            index_playable_entries(&mut Cursor::new(overstated), false, 10),
            Err(ZipReaderError::EntryCountMismatch {
                declared: 6,
                actual: 5
            })
        ));

        let mut understated = valid_archive();
        patch_classic_entry_count(&mut understated, 4);
        assert!(matches!(
            index_playable_entries(&mut Cursor::new(understated), false, 10),
            Err(ZipReaderError::EntryCountMismatch {
                declared: 4,
                actual: 5
            })
        ));
    }

    #[test]
    fn reads_zip64_metadata_and_applies_u64_entry_limits() {
        let mut reader =
            ZipArchiveReader::open(Cursor::new(zip64_archive(valid_archive(), 5)), true)
                .expect("ZIP64 archive should open");
        assert_eq!(reader.entry_count(), 2);
        assert_eq!(reader.extract(0).expect("stored ZIP64 image"), IMAGE_TWO);
        assert_eq!(reader.extract(1).expect("deflated ZIP64 image"), IMAGE_TEN);

        let mismatched = zip64_archive(valid_archive(), 6);
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(mismatched), false),
            Err(ZipReaderError::EntryCountMismatch {
                declared: 6,
                actual: 5
            })
        ));

        let over_limit = zip64_archive(valid_archive(), MAX_ZIP_ARCHIVE_ENTRIES_U64 + 1);
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(over_limit), false),
            Err(ZipReaderError::TooManyEntries {
                maximum: MAX_ZIP_ARCHIVE_ENTRIES_U64,
                actual
            }) if actual == MAX_ZIP_ARCHIVE_ENTRIES_U64 + 1
        ));
    }

    #[test]
    fn preserves_playable_extensions_and_legacy_natural_comparison() {
        for extension in PLAYABLE_EXTENSIONS {
            let name = format!("page.{extension}");
            assert_eq!(playable_extension(name.as_bytes()), Some(extension));
        }
        assert_eq!(playable_extension(b"page.JPG"), None);
        assert_eq!(playable_extension(b"page"), None);
        assert_eq!(natural_cmp(b"image2.jpg", b"image10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp(b"image02.jpg", b"image2.jpg"), Ordering::Equal);
        assert_eq!(natural_cmp(b" image 3.jpg", b"image10.jpg"), Ordering::Less);
        assert_eq!(
            natural_cmp(b"image10.jpg", b" image 3.jpg"),
            Ordering::Greater
        );
        assert_eq!(
            natural_cmp(b"image20.jpg", b"image2.jpg"),
            Ordering::Greater
        );
        assert_eq!(natural_cmp(b"same", b"same"), Ordering::Equal);
    }

    #[test]
    fn streams_images_and_rejects_invalid_indices_and_writer_failures() {
        let mut reader = ZipArchiveReader::open(Cursor::new(valid_archive()), true)
            .expect("archive should open");
        let mut output = Vec::new();
        assert_eq!(
            reader
                .extract_to(0, &mut output)
                .expect("image should copy"),
            u64::try_from(IMAGE_TWO.len()).expect("fixture size fits")
        );
        assert_eq!(output, IMAGE_TWO);
        output.clear();
        assert_eq!(
            reader
                .extract_to(1, &mut output)
                .expect("deflated image should copy"),
            u64::try_from(IMAGE_TEN.len()).expect("fixture size fits")
        );
        assert_eq!(output, IMAGE_TEN);
        assert!(matches!(
            reader.extension(2),
            Err(ZipReaderError::InvalidEntryIndex {
                index: 2,
                entry_count: 2
            })
        ));

        let mut writer = FailingWriter;
        assert!(matches!(
            reader.extract_to(0, &mut writer),
            Err(ZipReaderError::ExtractEntry { index: 0, .. })
        ));
        writer.flush().expect("flush is intentionally harmless");
    }

    #[test]
    fn rejects_internal_capacity_local_header_and_size_inconsistencies() {
        let mut impossible_capacity = ZipArchiveReader::open(Cursor::new(valid_archive()), true)
            .expect("archive should open");
        impossible_capacity.entries[0].size = u64::MAX;
        assert!(matches!(
            impossible_capacity.extract(0),
            Err(ZipReaderError::EntryTooLarge { .. })
        ));

        let mut mismatched_size = ZipArchiveReader::open(Cursor::new(valid_archive()), true)
            .expect("archive should open");
        mismatched_size.entries[0].size += 1;
        assert!(matches!(
            mismatched_size.extract_to(0, &mut Vec::new()),
            Err(ZipReaderError::EntrySizeMismatch { index: 0, .. })
        ));

        let mut understated_size = ZipArchiveReader::open(Cursor::new(valid_archive()), true)
            .expect("archive should open");
        understated_size.entries[0].size -= 1;
        let mut bounded_output = Vec::new();
        assert!(matches!(
            understated_size.extract_to(0, &mut bounded_output),
            Err(ZipReaderError::EntrySizeMismatch { index: 0, .. })
        ));
        assert_eq!(
            bounded_output.len(),
            IMAGE_TWO.len() - 1,
            "declared-size mismatch must not write the trailing byte"
        );

        let mut invalid_local_header = ZipArchiveReader::open(Cursor::new(valid_archive()), true)
            .expect("archive should open");
        let archive_index = invalid_local_header.entries[0].archive_index;
        let local_headers = signature_offsets(
            invalid_local_header.source.get_ref(),
            [0x50, 0x4b, 0x03, 0x04],
        );
        invalid_local_header.source.get_mut()[local_headers[archive_index]] = 0;
        assert!(matches!(
            invalid_local_header.extract_to(0, &mut Vec::new()),
            Err(ZipReaderError::OpenEntry { archive_index: index, .. }) if index == archive_index
        ));

        let mut invalid_cached_compression =
            ZipArchiveReader::open(Cursor::new(valid_archive()), true)
                .expect("archive should open");
        invalid_cached_compression.entries[0].compression = RawCompressionMethod::BZIP2;
        assert!(matches!(
            invalid_cached_compression.extract_to(0, &mut Vec::new()),
            Err(ZipReaderError::UnsupportedCompression { .. })
        ));
    }

    #[test]
    fn rejects_malformed_empty_encrypted_and_unsupported_archives() {
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(b"not-a-zip".to_vec()), false),
            Err(ZipReaderError::OpenArchive(_))
        ));
        let metadata_only = zip_bytes(&[TestEntry {
            name: "ComicInfo.xml",
            contents: b"metadata",
            compression: CompressionMethod::Stored,
        }]);
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(metadata_only), false),
            Err(ZipReaderError::NoPlayableEntries)
        ));

        let empty_image = zip_bytes(&[TestEntry {
            name: "page.jpg",
            contents: b"",
            compression: CompressionMethod::Stored,
        }]);
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(empty_image), false),
            Err(ZipReaderError::EmptyEntry { archive_index: 0 })
        ));

        let mut encrypted = zip_bytes(&[TestEntry {
            name: "page.jpg",
            contents: IMAGE_TWO,
            compression: CompressionMethod::Stored,
        }]);
        patch_first_entry_flags(&mut encrypted, 0x0001);
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(encrypted), false),
            Err(ZipReaderError::EncryptedEntry { archive_index: 0 })
        ));

        let mut unsupported = zip_bytes(&[TestEntry {
            name: "page.jpg",
            contents: IMAGE_TWO,
            compression: CompressionMethod::Stored,
        }]);
        patch_first_entry_compression(&mut unsupported, 12);
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(unsupported), false),
            Err(ZipReaderError::UnsupportedCompression { archive_index: 0 })
        ));
    }

    #[test]
    fn rejects_oversized_metadata_and_crc_corruption() {
        let mut oversized = zip_bytes(&[TestEntry {
            name: "page.jpg",
            contents: IMAGE_TWO,
            compression: CompressionMethod::Stored,
        }]);
        patch_first_entry_uncompressed_size(
            &mut oversized,
            u32::try_from(MAX_ZIP_IMAGE_BYTES + 1).expect("limit fits ZIP32"),
        );
        assert!(matches!(
            ZipArchiveReader::open(Cursor::new(oversized), false),
            Err(ZipReaderError::EntryTooLarge {
                archive_index: 0,
                ..
            })
        ));

        let mut corrupt = valid_archive();
        let payload = corrupt
            .windows(IMAGE_TWO.len())
            .position(|window| window == IMAGE_TWO)
            .expect("stored payload should exist");
        corrupt[payload] ^= 1;
        let mut reader =
            ZipArchiveReader::open(Cursor::new(corrupt), true).expect("metadata should open");
        assert!(matches!(
            reader.extract(0),
            Err(ZipReaderError::ExtractEntry { index: 0, .. })
        ));

        let mut corrupt_deflated_crc = valid_archive();
        patch_first_central_crc(&mut corrupt_deflated_crc);
        let mut reader = ZipArchiveReader::open(Cursor::new(corrupt_deflated_crc), true)
            .expect("metadata should open");
        assert!(matches!(
            reader.extract(1),
            Err(ZipReaderError::ExtractEntry { index: 1, .. })
        ));
    }

    #[test]
    fn registry_issues_unique_handles_extracts_and_closes_idempotently() {
        let registry = ZipReaderRegistry::default();
        let first = registry
            .open(Cursor::new(valid_archive()), true)
            .expect("first reader should open");
        let second = registry
            .open(Cursor::new(valid_archive()), false)
            .expect("second reader should open");
        assert!(first > 0);
        assert!(second > first);
        assert_eq!(registry.entry_count(first).expect("entry count"), 2);
        assert_eq!(registry.extension(first, 0).expect("extension"), "png");
        assert_eq!(
            registry.entry_size(first, 0).expect("entry size"),
            u64::try_from(IMAGE_TWO.len()).expect("fixture size fits")
        );
        assert_eq!(registry.extract(first, 0).expect("image"), IMAGE_TWO);
        let mut output = Vec::new();
        registry
            .extract_to(second, 0, &mut output)
            .expect("stream extraction");
        assert_eq!(output, IMAGE_TEN);
        assert!(registry.close(first).expect("first close"));
        assert!(!registry.close(first).expect("duplicate close"));
        assert!(!registry.close(0).expect("invalid close"));
        assert!(matches!(
            registry.entry_count(first),
            Err(ZipReaderError::UnknownHandle { handle }) if handle == first
        ));
        assert!(matches!(
            registry.entry_count(-1),
            Err(ZipReaderError::UnknownHandle { handle: -1 })
        ));
    }

    #[test]
    fn registry_serializes_concurrent_extraction_per_reader() {
        let registry = Arc::new(ZipReaderRegistry::default());
        let handle = registry
            .open(Cursor::new(valid_archive()), true)
            .expect("reader should open");
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let registry = Arc::clone(&registry);
                thread::spawn(move || registry.extract(handle, 0).expect("concurrent extraction"))
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().expect("thread should join"), IMAGE_TWO);
        }
        assert!(registry.close(handle).expect("reader should close"));
    }

    #[test]
    fn registry_close_waits_for_active_extraction() {
        let registry = Arc::new(ZipReaderRegistry::default());
        let handle = registry
            .open(Cursor::new(valid_archive()), true)
            .expect("reader should open");
        let (started_sender, started_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let extract_registry = Arc::clone(&registry);
        let extraction = thread::spawn(move || {
            let mut writer = BlockingWriter {
                started_sender: Some(started_sender),
                release_receiver,
            };
            let copied = extract_registry
                .extract_to(handle, 0, &mut writer)
                .expect("blocking extraction should finish");
            writer
                .flush()
                .expect("blocking writer flush should succeed");
            copied
        });
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("extraction should start");

        let (closed_sender, closed_receiver) = mpsc::channel();
        let close_registry = Arc::clone(&registry);
        let close = thread::spawn(move || {
            closed_sender
                .send(close_registry.close(handle))
                .expect("close result should send");
        });
        assert!(
            closed_receiver
                .recv_timeout(Duration::from_millis(20))
                .is_err()
        );
        release_sender.send(()).expect("extraction should release");
        assert_eq!(
            extraction.join().expect("extraction should join"),
            u64::try_from(IMAGE_TWO.len()).expect("fixture size fits")
        );
        assert!(
            closed_receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("close should finish after extraction")
                .expect("close should succeed")
        );
        close.join().expect("close thread should join");
    }

    #[test]
    fn registry_close_rejects_operations_queued_before_handle_removal() {
        let registry = Arc::new(ZipReaderRegistry::default());
        let handle = registry
            .open(Cursor::new(valid_archive()), true)
            .expect("reader should open");
        let reader = registry
            .readers
            .lock()
            .expect("registry should be healthy")
            .get(&handle)
            .cloned()
            .expect("reader should exist");
        let active_operation = reader.reader.lock().expect("reader should start healthy");

        let queued_registry = Arc::clone(&registry);
        let queued = thread::spawn(move || queued_registry.entry_count(handle));
        let queued_deadline = Instant::now() + Duration::from_secs(1);
        while Arc::strong_count(&reader) < 3 {
            assert!(
                Instant::now() < queued_deadline,
                "queued operation should clone the reader"
            );
            thread::yield_now();
        }

        let close_registry = Arc::clone(&registry);
        let close = thread::spawn(move || close_registry.close(handle));
        let close_deadline = Instant::now() + Duration::from_secs(1);
        while !reader.closed.load(super::AtomicOrdering::Acquire) {
            assert!(
                Instant::now() < close_deadline,
                "close should mark the reader before waiting"
            );
            thread::yield_now();
        }
        drop(active_operation);

        assert!(matches!(
            queued.join().expect("queued operation should join"),
            Err(ZipReaderError::UnknownHandle { handle: unknown }) if unknown == handle
        ));
        assert!(
            close
                .join()
                .expect("close operation should join")
                .expect("close should succeed")
        );
    }

    #[test]
    fn registry_reports_handle_exhaustion_collision_and_poisoning() {
        let exhausted = ZipReaderRegistry::default();
        exhausted
            .next_handle
            .store(i64::MAX, super::AtomicOrdering::Relaxed);
        assert!(matches!(
            exhausted.open(Cursor::new(valid_archive()), false),
            Err(ZipReaderError::HandleExhausted)
        ));

        let collision = ZipReaderRegistry::default();
        let handle = collision
            .open(Cursor::new(valid_archive()), false)
            .expect("reader should open");
        collision
            .next_handle
            .store(handle, super::AtomicOrdering::Relaxed);
        assert!(matches!(
            collision.open(Cursor::new(valid_archive()), false),
            Err(ZipReaderError::HandleCollision)
        ));

        let poisoned_registry = Arc::new(ZipReaderRegistry::<Cursor<Vec<u8>>>::default());
        let poison_target = Arc::clone(&poisoned_registry);
        assert!(
            thread::spawn(move || {
                let _guard = poison_target
                    .readers
                    .lock()
                    .expect("lock should start healthy");
                panic!("poison registry");
            })
            .join()
            .is_err()
        );
        assert!(matches!(
            poisoned_registry.open(Cursor::new(valid_archive()), false),
            Err(ZipReaderError::RegistryPoisoned)
        ));

        let poisoned_reader = ZipReaderRegistry::default();
        let handle = poisoned_reader
            .open(Cursor::new(valid_archive()), false)
            .expect("reader should open");
        let reader = poisoned_reader
            .readers
            .lock()
            .expect("registry should be healthy")
            .get(&handle)
            .cloned()
            .expect("reader should exist");
        assert!(
            thread::spawn(move || {
                let _guard = reader.reader.lock().expect("reader should start healthy");
                panic!("poison reader");
            })
            .join()
            .is_err()
        );
        assert!(matches!(
            poisoned_reader.entry_count(handle),
            Err(ZipReaderError::ReaderPoisoned)
        ));
        assert!(matches!(
            poisoned_reader.close(handle),
            Err(ZipReaderError::ReaderPoisoned)
        ));
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("expected writer failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct BlockingWriter {
        started_sender: Option<Sender<()>>,
        release_receiver: Receiver<()>,
    }

    impl Write for BlockingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if let Some(sender) = self.started_sender.take() {
                if sender.send(()).is_err() {
                    return Err(io::Error::other("failed to signal extraction start"));
                }
                if self.release_receiver.recv().is_err() {
                    return Err(io::Error::other("failed to await extraction release"));
                }
            }
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn patch_first_entry_flags(bytes: &mut [u8], flags: u16) {
        let local = signature_offset(bytes, [0x50, 0x4b, 0x03, 0x04]);
        let central = signature_offset(bytes, [0x50, 0x4b, 0x01, 0x02]);
        bytes[local + 6..local + 8].copy_from_slice(&flags.to_le_bytes());
        bytes[central + 8..central + 10].copy_from_slice(&flags.to_le_bytes());
    }

    fn patch_first_entry_compression(bytes: &mut [u8], method: u16) {
        let local = signature_offset(bytes, [0x50, 0x4b, 0x03, 0x04]);
        let central = signature_offset(bytes, [0x50, 0x4b, 0x01, 0x02]);
        bytes[local + 8..local + 10].copy_from_slice(&method.to_le_bytes());
        bytes[central + 10..central + 12].copy_from_slice(&method.to_le_bytes());
    }

    fn patch_first_entry_uncompressed_size(bytes: &mut [u8], size: u32) {
        let central = signature_offset(bytes, [0x50, 0x4b, 0x01, 0x02]);
        bytes[central + 24..central + 28].copy_from_slice(&size.to_le_bytes());
    }

    fn patch_first_central_crc(bytes: &mut [u8]) {
        let central = signature_offset(bytes, [0x50, 0x4b, 0x01, 0x02]);
        bytes[central + 16] ^= 0x80;
    }

    fn patch_classic_entry_count(bytes: &mut [u8], count: u16) {
        let eocd = signature_offset_from_end(bytes, [0x50, 0x4b, 0x05, 0x06]);
        bytes[eocd + 8..eocd + 10].copy_from_slice(&count.to_le_bytes());
        bytes[eocd + 10..eocd + 12].copy_from_slice(&count.to_le_bytes());
    }

    fn zip64_archive(mut bytes: Vec<u8>, declared_entries: u64) -> Vec<u8> {
        let eocd = signature_offset_from_end(&bytes, [0x50, 0x4b, 0x05, 0x06]);
        let central_directory_size = u32::from_le_bytes(
            bytes[eocd + 12..eocd + 16]
                .try_into()
                .expect("classic central-directory size should exist"),
        );
        let central_directory_offset = u32::from_le_bytes(
            bytes[eocd + 16..eocd + 20]
                .try_into()
                .expect("classic central-directory offset should exist"),
        );
        bytes[eocd + 8..eocd + 12].fill(0xff);
        bytes[eocd + 12..eocd + 20].fill(0xff);

        let mut zip64_record = Vec::with_capacity(56);
        zip64_record.extend_from_slice(&0x0606_4b50_u32.to_le_bytes());
        zip64_record.extend_from_slice(&44_u64.to_le_bytes());
        zip64_record.extend_from_slice(&45_u16.to_le_bytes());
        zip64_record.extend_from_slice(&45_u16.to_le_bytes());
        zip64_record.extend_from_slice(&0_u32.to_le_bytes());
        zip64_record.extend_from_slice(&0_u32.to_le_bytes());
        zip64_record.extend_from_slice(&declared_entries.to_le_bytes());
        zip64_record.extend_from_slice(&declared_entries.to_le_bytes());
        zip64_record.extend_from_slice(&u64::from(central_directory_size).to_le_bytes());
        zip64_record.extend_from_slice(&u64::from(central_directory_offset).to_le_bytes());

        let mut locator = Vec::with_capacity(20);
        locator.extend_from_slice(&0x0706_4b50_u32.to_le_bytes());
        locator.extend_from_slice(&0_u32.to_le_bytes());
        locator.extend_from_slice(
            &u64::try_from(eocd)
                .expect("fixture offset should fit u64")
                .to_le_bytes(),
        );
        locator.extend_from_slice(&1_u32.to_le_bytes());

        let mut output = Vec::with_capacity(bytes.len() + zip64_record.len() + locator.len());
        output.extend_from_slice(&bytes[..eocd]);
        output.extend_from_slice(&zip64_record);
        output.extend_from_slice(&locator);
        output.extend_from_slice(&bytes[eocd..]);
        output
    }

    fn signature_offset(bytes: &[u8], signature: [u8; 4]) -> usize {
        bytes
            .windows(signature.len())
            .position(|window| window == signature)
            .expect("ZIP signature should exist")
    }

    fn signature_offset_from_end(bytes: &[u8], signature: [u8; 4]) -> usize {
        bytes
            .windows(signature.len())
            .rposition(|window| window == signature)
            .expect("ZIP signature should exist")
    }

    fn signature_offsets(bytes: &[u8], signature: [u8; 4]) -> Vec<usize> {
        bytes
            .windows(signature.len())
            .enumerate()
            .filter_map(|(index, window)| (window == signature).then_some(index))
            .collect()
    }
}
