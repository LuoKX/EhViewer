//! Rust-owned multi-format archive lifecycle with format-specific codec adapters.

#![allow(
    unsafe_code,
    reason = "libarchive exposes a C ABI; all raw pointers are contained by NativeArchive"
)]

use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
#[cfg(target_os = "android")]
use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
#[cfg(target_os = "android")]
use std::ptr::NonNull;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
#[cfg(target_os = "android")]
use libc::{c_char, c_int, c_void, mode_t, size_t, ssize_t};
use rars::{ArchiveReadOptions, ArchiveReader as RarArchiveReader};
use sevenz_rust2::{
    Archive as SevenZipArchive, EncoderMethod, Error as SevenZipError,
    Password as SevenZipPassword, decompress_with_extract_fn_and_password,
};
use zeroize::Zeroize;

use super::duplicate_file_descriptor;

pub(super) static ARCHIVE_READER_REGISTRY: LazyLock<ArchiveReaderRegistry> =
    LazyLock::new(ArchiveReaderRegistry::default);

#[cfg(target_os = "android")]
const ARCHIVE_OK: c_int = 0;
#[cfg(target_os = "android")]
const ARCHIVE_EOF: c_int = 1;
#[cfg(target_os = "android")]
const AE_IFREG: mode_t = 0o100_000;
#[cfg(target_os = "android")]
const ARCHIVE_BLOCK_SIZE: size_t = 64 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RECOVERY_METADATA_BYTES: u64 = 512 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;
const RAR4_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x00";
const RAR5_SIGNATURE: &[u8] = b"Rar!\x1a\x07\x01\x00";
const SEVEN_ZIP_SIGNATURE: &[u8] = b"7z\xbc\xaf\x27\x1c";
const PLAYABLE_EXTENSIONS: [&str; 11] = [
    "jpeg", "jpg", "png", "gif", "webp", "bmp", "ico", "wbmp", "heic", "heif", "avif",
];

#[derive(Clone, Copy, Debug)]
enum ArchiveKind {
    Libarchive,
    Rar,
    SevenZip,
}

#[derive(Clone, Debug)]
enum EntryLocator {
    #[cfg(target_os = "android")]
    Libarchive(usize),
    Rar(usize),
    SevenZip(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryMetadataKind {
    SpiderInfo,
    ComicInfo,
}

#[derive(Clone, Debug)]
struct ArchiveEntry {
    locator: EntryLocator,
    name: String,
    extension: String,
    size: u64,
    metadata_kind: Option<RecoveryMetadataKind>,
}

struct ArchivePassword {
    nul_terminated_bytes: Vec<u8>,
}

impl ArchivePassword {
    fn new(mut bytes: Vec<u8>) -> Result<Self> {
        ensure!(!bytes.contains(&0), "Archive password contains a NUL byte");
        bytes.push(0);
        Ok(Self {
            nul_terminated_bytes: bytes,
        })
    }

    fn bytes(&self) -> &[u8] {
        &self.nul_terminated_bytes[..self.nul_terminated_bytes.len() - 1]
    }

    #[cfg(target_os = "android")]
    fn as_ptr(&self) -> *const c_char {
        self.nul_terminated_bytes.as_ptr().cast()
    }
}

impl Drop for ArchivePassword {
    fn drop(&mut self) {
        self.nul_terminated_bytes.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArchiveReaderState {
    Ready,
    PasswordRequired,
    Closed,
}

struct ArchiveReader {
    source: File,
    kind: ArchiveKind,
    sort_entries: bool,
    password: Option<ArchivePassword>,
    state: ArchiveReaderState,
    entries: Vec<ArchiveEntry>,
    metadata_entries: Vec<ArchiveEntry>,
    unsafe_path: bool,
}

impl ArchiveReader {
    fn open(mut source: File, sort_entries: bool) -> Result<Self> {
        let kind = detect_archive_kind(&mut source)?;
        let outcome = scan_archive_kind(&mut source, kind, None, sort_entries, false)?;
        let (entries, metadata_entries, needs_password, unsafe_path) = match outcome {
            ScanOutcome::Complete {
                entries,
                metadata_entries,
                encrypted,
                unsafe_path,
            } => (entries, metadata_entries, encrypted, unsafe_path),
            ScanOutcome::PasswordRequired => (Vec::new(), Vec::new(), true, false),
        };
        ensure!(
            needs_password || !entries.is_empty(),
            "Rust archive reader found no playable image entries"
        );
        Ok(Self {
            source,
            kind,
            sort_entries,
            password: None,
            state: if needs_password {
                ArchiveReaderState::PasswordRequired
            } else {
                ArchiveReaderState::Ready
            },
            entries,
            metadata_entries,
            unsafe_path,
        })
    }

    fn provide_password(&mut self, password: Vec<u8>) -> Result<bool> {
        ensure!(
            self.state != ArchiveReaderState::Closed,
            "Rust archive reader is closed"
        );
        if self.state != ArchiveReaderState::PasswordRequired {
            return Ok(true);
        }
        let password = ArchivePassword::new(password)?;
        match scan_archive_kind(
            &mut self.source,
            self.kind,
            Some(&password),
            self.sort_entries,
            true,
        )? {
            ScanOutcome::PasswordRequired => Ok(false),
            ScanOutcome::Complete {
                entries,
                metadata_entries,
                unsafe_path,
                ..
            } => {
                ensure!(
                    !entries.is_empty(),
                    "Rust archive reader found no playable image entries after password validation"
                );
                self.password = Some(password);
                self.state = ArchiveReaderState::Ready;
                self.entries = entries;
                self.metadata_entries = metadata_entries;
                self.unsafe_path = unsafe_path;
                Ok(true)
            }
        }
    }

    fn entry_count(&self) -> Result<usize> {
        self.ensure_ready()?;
        Ok(self.entries.len())
    }

    fn entry_size(&self, index: usize) -> Result<u64> {
        Ok(self.entry(index)?.size)
    }

    fn extension(&self, index: usize) -> Result<String> {
        Ok(self.entry(index)?.extension.clone())
    }

    fn entry_name(&self, index: usize) -> Result<String> {
        Ok(self.entry(index)?.name.clone())
    }

    fn has_unsafe_path(&self) -> Result<bool> {
        self.ensure_ready()?;
        Ok(self.unsafe_path)
    }

    fn verify_entry(&mut self, index: usize) -> Result<bool> {
        let mut sink = std::io::sink();
        self.extract_to(index, &mut sink).map(|_| true)
    }

    fn recovery_metadata(&mut self, kind: RecoveryMetadataKind) -> Result<Option<Vec<u8>>> {
        self.ensure_ready()?;
        let target = self
            .metadata_entries
            .iter()
            .filter(|entry| entry.metadata_kind == Some(kind))
            .min_by(|left, right| natural_compare(&left.name, &right.name))
            .cloned();
        let Some(target) = target else {
            return Ok(None);
        };
        let capacity = usize::try_from(target.size)
            .context("Recovery metadata size exceeds the platform range")?;
        let mut output = Vec::with_capacity(capacity);
        let password = self.password.as_ref();
        let archive_kind = self.kind;
        with_source_position_restored(&mut self.source, |source| {
            extract_entry(source, archive_kind, password, &target, &mut output)
        })?;
        ensure!(
            output.len() == capacity,
            "Recovery metadata size does not match archive metadata"
        );
        Ok(Some(output))
    }

    fn extract_to(&mut self, index: usize, output: &mut (dyn Write + Send)) -> Result<u64> {
        let entry = self.entry(index)?.clone();
        let password = self.password.as_ref();
        let kind = self.kind;
        with_source_position_restored(&mut self.source, |source| {
            extract_entry(source, kind, password, &entry, output)
        })
    }

    fn ensure_ready(&self) -> Result<()> {
        ensure!(
            self.state != ArchiveReaderState::Closed,
            "Rust archive reader is closed"
        );
        ensure!(
            self.state != ArchiveReaderState::PasswordRequired,
            "Rust archive reader requires a password"
        );
        Ok(())
    }

    fn entry(&self, index: usize) -> Result<&ArchiveEntry> {
        self.ensure_ready()?;
        self.entries
            .get(index)
            .context("Rust archive image index is outside the reader")
    }
}

#[derive(Default)]
pub(super) struct ArchiveReaderRegistry {
    next_handle: AtomicI64,
    readers: Mutex<HashMap<i64, Arc<Mutex<ArchiveReader>>>>,
}

impl ArchiveReaderRegistry {
    pub(super) fn open(&self, source: File, sort_entries: bool) -> Result<i64> {
        let reader = ArchiveReader::open(source, sort_entries)?;
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed) + 1;
        ensure!(handle > 0, "Rust archive reader handle space is exhausted");
        let previous = self
            .readers
            .lock()
            .map_err(|_| anyhow!("Rust archive reader registry lock is poisoned"))?
            .insert(handle, Arc::new(Mutex::new(reader)));
        ensure!(previous.is_none(), "Rust archive reader handle collision");
        Ok(handle)
    }

    pub(super) fn needs_password(&self, handle: i64) -> Result<bool> {
        self.with_reader(handle, |reader| {
            Ok(reader.state == ArchiveReaderState::PasswordRequired)
        })
    }

    pub(super) fn provide_password(&self, handle: i64, password: Vec<u8>) -> Result<bool> {
        self.with_reader(handle, |reader| reader.provide_password(password))
    }

    pub(super) fn entry_count(&self, handle: i64) -> Result<usize> {
        self.with_reader(handle, |reader| reader.entry_count())
    }

    pub(super) fn entry_size(&self, handle: i64, index: usize) -> Result<u64> {
        self.with_reader(handle, |reader| reader.entry_size(index))
    }

    pub(super) fn extension(&self, handle: i64, index: usize) -> Result<String> {
        self.with_reader(handle, |reader| reader.extension(index))
    }

    pub(super) fn entry_name(&self, handle: i64, index: usize) -> Result<String> {
        self.with_reader(handle, |reader| reader.entry_name(index))
    }

    pub(super) fn has_unsafe_path(&self, handle: i64) -> Result<bool> {
        self.with_reader(handle, |reader| reader.has_unsafe_path())
    }

    pub(super) fn verify_entry(&self, handle: i64, index: usize) -> Result<bool> {
        self.with_reader(handle, |reader| reader.verify_entry(index))
    }

    pub(super) fn recovery_metadata(
        &self,
        handle: i64,
        kind: RecoveryMetadataKind,
    ) -> Result<Option<Vec<u8>>> {
        self.with_reader(handle, |reader| reader.recovery_metadata(kind))
    }

    pub(super) fn extract_to(
        &self,
        handle: i64,
        index: usize,
        output: &mut (dyn Write + Send),
    ) -> Result<u64> {
        self.with_reader(handle, |reader| reader.extract_to(index, output))
    }

    pub(super) fn close(&self, handle: i64) -> Result<bool> {
        let reader = self
            .readers
            .lock()
            .map_err(|_| anyhow!("Rust archive reader registry lock is poisoned"))?
            .remove(&handle);
        let Some(reader) = reader else {
            return Ok(false);
        };
        let mut reader = reader
            .lock()
            .map_err(|_| anyhow!("Rust archive reader lock is poisoned"))?;
        reader.state = ArchiveReaderState::Closed;
        Ok(true)
    }

    fn with_reader<T>(
        &self,
        handle: i64,
        operation: impl FnOnce(&mut ArchiveReader) -> Result<T>,
    ) -> Result<T> {
        ensure!(handle > 0, "Rust archive reader handle is invalid");
        let reader = self
            .readers
            .lock()
            .map_err(|_| anyhow!("Rust archive reader registry lock is poisoned"))?
            .get(&handle)
            .cloned()
            .context("Rust archive reader handle is unknown or closed")?;
        let mut reader = reader
            .lock()
            .map_err(|_| anyhow!("Rust archive reader lock is poisoned"))?;
        ensure!(
            reader.state != ArchiveReaderState::Closed,
            "Rust archive reader is closed"
        );
        operation(&mut reader)
    }
}

enum ScanOutcome {
    Complete {
        entries: Vec<ArchiveEntry>,
        metadata_entries: Vec<ArchiveEntry>,
        encrypted: bool,
        unsafe_path: bool,
    },
    PasswordRequired,
}

fn detect_archive_kind(source: &mut File) -> Result<ArchiveKind> {
    with_source_position_restored(source, |source| {
        let mut signature = [0u8; 8];
        let mut limited = source.take(signature.len() as u64);
        let read = limited
            .read(&mut signature)
            .context("Failed to read archive signature")?;
        let signature = &signature[..read];
        Ok(
            if signature.starts_with(RAR4_SIGNATURE) || signature.starts_with(RAR5_SIGNATURE) {
                ArchiveKind::Rar
            } else if signature.starts_with(SEVEN_ZIP_SIGNATURE) {
                ArchiveKind::SevenZip
            } else {
                ArchiveKind::Libarchive
            },
        )
    })
}

fn scan_archive_kind(
    source: &mut File,
    kind: ArchiveKind,
    password: Option<&ArchivePassword>,
    sort_entries: bool,
    validate_password: bool,
) -> Result<ScanOutcome> {
    with_source_position_restored(source, |source| match kind {
        ArchiveKind::Rar => scan_rar(source, password, sort_entries, validate_password),
        ArchiveKind::SevenZip => scan_seven_zip(source, password, sort_entries, validate_password),
        ArchiveKind::Libarchive => {
            #[cfg(target_os = "android")]
            {
                scan_libarchive(source, password, sort_entries, validate_password)
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (source, password, sort_entries, validate_password);
                bail!("libarchive-backed formats are available only in Android builds")
            }
        }
    })
}

fn scan_rar(
    source: &File,
    password: Option<&ArchivePassword>,
    sort_entries: bool,
    validate_password: bool,
) -> Result<ScanOutcome> {
    let options = rar_read_options(password);
    let archive = match RarArchiveReader::read_path_with_options(rar_source_path(source), options) {
        Ok(archive) => archive,
        Err(error) if rar_password_failure(&error) => return Ok(ScanOutcome::PasswordRequired),
        Err(error) => return Err(anyhow!(error)).context("Failed to parse RAR archive"),
    };

    let mut entries = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut encrypted = false;
    let mut unsafe_path = false;
    for (source_index, member) in archive.members().enumerate() {
        ensure!(
            source_index < MAX_ARCHIVE_ENTRIES,
            "Archive contains too many entries"
        );
        encrypted |= member.meta.is_encrypted;
        if member.meta.is_directory {
            continue;
        }
        let name = member.meta.name_lossy();
        unsafe_path |= is_unsafe_archive_path(&name);
        if let Some(extension) = playable_extension(&name).map(str::to_owned) {
            validate_image_size(member.meta.unpacked_size)?;
            entries.push(ArchiveEntry {
                locator: EntryLocator::Rar(source_index),
                name,
                extension,
                size: member.meta.unpacked_size,
                metadata_kind: None,
            });
        } else if let Some(metadata_kind) = recovery_metadata_kind(&name) {
            validate_recovery_metadata_size(member.meta.unpacked_size)?;
            metadata_entries.push(ArchiveEntry {
                locator: EntryLocator::Rar(source_index),
                name,
                extension: String::new(),
                size: member.meta.unpacked_size,
                metadata_kind: Some(metadata_kind),
            });
        }
    }

    if validate_password {
        if password.is_none() {
            return Ok(ScanOutcome::PasswordRequired);
        }
        match archive.extract_to_with_options(options, |_| Ok(Box::new(std::io::sink()))) {
            Ok(()) => {}
            Err(error) if rar_password_failure(&error) => {
                return Ok(ScanOutcome::PasswordRequired);
            }
            Err(error) => return Err(anyhow!(error)).context("Failed to validate RAR password"),
        }
    }

    sort_archive_entries(&mut entries, sort_entries);
    Ok(ScanOutcome::Complete {
        entries,
        metadata_entries,
        encrypted,
        unsafe_path,
    })
}

fn rar_read_options(password: Option<&ArchivePassword>) -> ArchiveReadOptions<'_> {
    ArchiveReadOptions::with_optional_password(password.map(ArchivePassword::bytes))
        .with_rar50_buffered_decode_limit(MAX_IMAGE_BYTES)
}

fn rar_source_path(source: &File) -> String {
    format!("/proc/self/fd/{}", source.as_raw_fd())
}

fn rar_password_failure(error: &rars::Error) -> bool {
    match error {
        rars::Error::NeedPassword | rars::Error::WrongPasswordOrCorruptData => true,
        rars::Error::AtArchiveOffset { source, .. } | rars::Error::AtEntry { source, .. } => {
            rar_password_failure(source)
        }
        _ => false,
    }
}

fn scan_seven_zip(
    source: &File,
    password: Option<&ArchivePassword>,
    sort_entries: bool,
    validate_password: bool,
) -> Result<ScanOutcome> {
    let codec_password = seven_zip_password(password)?;
    let mut input = duplicate_file_descriptor(source.as_raw_fd())
        .context("Failed to duplicate 7z source descriptor")?;
    let archive = match SevenZipArchive::read(&mut input, &codec_password) {
        Ok(archive) => archive,
        Err(error) if seven_zip_password_failure(&error) => {
            return Ok(ScanOutcome::PasswordRequired);
        }
        Err(error) => return Err(anyhow!(error)).context("Failed to parse 7z archive"),
    };
    let encrypted = archive.blocks.iter().any(|block| {
        block
            .coders
            .iter()
            .any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
    });
    let mut entries = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut unsafe_path = false;
    for (source_index, entry) in archive.files.iter().enumerate() {
        ensure!(
            source_index < MAX_ARCHIVE_ENTRIES,
            "Archive contains too many entries"
        );
        if entry.is_directory || !entry.has_stream {
            continue;
        }
        unsafe_path |= is_unsafe_archive_path(&entry.name);
        if let Some(extension) = playable_extension(&entry.name) {
            validate_image_size(entry.size)?;
            entries.push(ArchiveEntry {
                locator: EntryLocator::SevenZip(source_index),
                name: entry.name.clone(),
                extension: extension.to_owned(),
                size: entry.size,
                metadata_kind: None,
            });
        } else if let Some(metadata_kind) = recovery_metadata_kind(&entry.name) {
            validate_recovery_metadata_size(entry.size)?;
            metadata_entries.push(ArchiveEntry {
                locator: EntryLocator::SevenZip(source_index),
                name: entry.name.clone(),
                extension: String::new(),
                size: entry.size,
                metadata_kind: Some(metadata_kind),
            });
        }
    }

    if validate_password {
        let validation = validate_seven_zip(source, codec_password);
        match validation {
            Ok(()) => {}
            Err(error) if seven_zip_password_failure(&error) => {
                return Ok(ScanOutcome::PasswordRequired);
            }
            Err(error) => return Err(anyhow!(error)).context("Failed to validate 7z password"),
        }
    }

    sort_archive_entries(&mut entries, sort_entries);
    Ok(ScanOutcome::Complete {
        entries,
        metadata_entries,
        encrypted,
        unsafe_path,
    })
}

fn seven_zip_password(password: Option<&ArchivePassword>) -> Result<SevenZipPassword> {
    password.map_or_else(
        || Ok(SevenZipPassword::empty()),
        |password| {
            let password =
                std::str::from_utf8(password.bytes()).context("7z password is not valid UTF-8")?;
            Ok(SevenZipPassword::new(password))
        },
    )
}

fn validate_seven_zip(source: &File, password: SevenZipPassword) -> Result<(), SevenZipError> {
    let input = duplicate_file_descriptor(source.as_raw_fd())
        .map_err(|error| SevenZipError::Other(error.to_string().into()))?;
    decompress_with_extract_fn_and_password(input, Path::new("."), password, |_, reader, _| {
        let mut sink = std::io::sink();
        std::io::copy(reader, &mut sink)?;
        Ok(true)
    })
}

fn seven_zip_password_failure(error: &SevenZipError) -> bool {
    matches!(
        error,
        SevenZipError::PasswordRequired | SevenZipError::MaybeBadPassword(_)
    ) || error.to_string().to_ascii_lowercase().contains("password")
}

#[cfg(target_os = "android")]
fn scan_libarchive(
    source: &File,
    password: Option<&ArchivePassword>,
    sort_entries: bool,
    validate_password: bool,
) -> Result<ScanOutcome> {
    let password_supplied = password.is_some();
    let mut archive = match NativeArchive::open(source, password) {
        Ok(archive) => archive,
        Err(error) if libarchive_password_failure(&error, password_supplied) => {
            return Ok(ScanOutcome::PasswordRequired);
        }
        Err(error) => return Err(error),
    };
    let mut entries = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut total_entries = 0usize;
    let mut encrypted = false;
    let mut unsafe_path = false;
    let mut password_validated = false;

    loop {
        let entry = match archive.next_header() {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(error) if libarchive_password_failure(&error, password_supplied) => {
                return Ok(ScanOutcome::PasswordRequired);
            }
            Err(error) => return Err(error),
        };
        total_entries = total_entries
            .checked_add(1)
            .context("Archive entry count overflow")?;
        ensure!(
            total_entries <= MAX_ARCHIVE_ENTRIES,
            "Archive contains too many entries"
        );
        let is_regular = native_entry_is_regular(entry);
        let is_encrypted = native_entry_is_encrypted(entry);
        encrypted |= is_encrypted;
        let name = native_entry_name(entry)?;
        unsafe_path |= is_unsafe_archive_path(&name);

        if is_regular {
            let size = native_entry_size(entry)?;
            if let Some(extension) = playable_extension(&name).map(str::to_owned) {
                validate_image_size(size)?;
                entries.push(ArchiveEntry {
                    locator: EntryLocator::Libarchive(total_entries - 1),
                    name,
                    extension,
                    size,
                    metadata_kind: None,
                });
            } else if let Some(metadata_kind) = recovery_metadata_kind(&name) {
                validate_recovery_metadata_size(size)?;
                metadata_entries.push(ArchiveEntry {
                    locator: EntryLocator::Libarchive(total_entries - 1),
                    name,
                    extension: String::new(),
                    size,
                    metadata_kind: Some(metadata_kind),
                });
            }

            if validate_password && is_encrypted && !password_validated {
                if let Err(error) = archive.drain_current_entry(size) {
                    if libarchive_unsupported_encryption(&error) {
                        return Err(error);
                    }
                    return Ok(ScanOutcome::PasswordRequired);
                }
                password_validated = true;
            }
        }
    }

    encrypted |= archive.has_encrypted_entries() == 1;
    sort_archive_entries(&mut entries, sort_entries);
    Ok(ScanOutcome::Complete {
        entries,
        metadata_entries,
        encrypted,
        unsafe_path,
    })
}

fn validate_image_size(size: u64) -> Result<()> {
    ensure!(size > 0, "Archive image entry is empty");
    ensure!(
        size <= MAX_IMAGE_BYTES,
        "Archive image entry exceeds the supported size limit"
    );
    Ok(())
}

fn validate_recovery_metadata_size(size: u64) -> Result<()> {
    ensure!(size > 0, "Recovery metadata entry is empty");
    ensure!(
        size <= MAX_RECOVERY_METADATA_BYTES,
        "Recovery metadata entry exceeds the supported size limit"
    );
    Ok(())
}

fn sort_archive_entries(entries: &mut [ArchiveEntry], sort_entries: bool) {
    if sort_entries {
        entries.sort_by(|left, right| natural_compare(&left.name, &right.name));
    }
}

fn extract_entry(
    source: &File,
    kind: ArchiveKind,
    password: Option<&ArchivePassword>,
    target: &ArchiveEntry,
    output: &mut (dyn Write + Send),
) -> Result<u64> {
    match kind {
        ArchiveKind::Rar => extract_rar(source, password, target, output),
        ArchiveKind::SevenZip => extract_seven_zip(source, password, target, output),
        ArchiveKind::Libarchive => {
            #[cfg(target_os = "android")]
            {
                extract_libarchive(source, password, target, output)
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = (source, password, target, output);
                bail!("libarchive-backed formats are available only in Android builds")
            }
        }
    }
}

fn extract_rar(
    source: &File,
    password: Option<&ArchivePassword>,
    target: &ArchiveEntry,
    output: &mut (dyn Write + Send),
) -> Result<u64> {
    let EntryLocator::Rar(target_index) = target.locator else {
        bail!("RAR entry has a mismatched codec locator");
    };
    let options = rar_read_options(password);
    let archive = RarArchiveReader::read_path_with_options(rar_source_path(source), options)
        .context("Failed to reopen RAR archive for extraction")?;
    let (mut read_stream, write_stream) =
        UnixStream::pair().context("Failed to create RAR extraction stream")?;
    let mut write_stream = Some(write_stream);
    let mut source_index = 0usize;
    let mut found = false;

    thread::scope(|scope| {
        let copy_thread =
            scope.spawn(move || copy_exact_entry(&mut read_stream, output, target.size));
        let extraction = archive.extract_to_with_options(options, |_| {
            let current_index = source_index;
            source_index += 1;
            if current_index == target_index {
                found = true;
                Ok(Box::new(
                    write_stream
                        .take()
                        .expect("RAR target entry can be selected only once"),
                ))
            } else {
                Ok(Box::new(std::io::sink()))
            }
        });
        drop(write_stream);
        let copied = copy_thread
            .join()
            .map_err(|_| anyhow!("RAR extraction copy thread panicked"))??;
        extraction.context("Failed to extract RAR image entry")?;
        ensure!(found, "RAR image entry disappeared before extraction");
        Ok(copied)
    })
}

fn extract_seven_zip(
    source: &File,
    password: Option<&ArchivePassword>,
    target: &ArchiveEntry,
    output: &mut (dyn Write + Send),
) -> Result<u64> {
    let EntryLocator::SevenZip(target_index) = target.locator else {
        bail!("7z entry has a mismatched codec locator");
    };
    let input = duplicate_file_descriptor(source.as_raw_fd())
        .context("Failed to duplicate 7z source descriptor")?;
    let password = seven_zip_password(password)?;
    let mut source_index = 0usize;
    let mut copied = None;
    decompress_with_extract_fn_and_password(input, Path::new("."), password, |_, reader, _| {
        let current_index = source_index;
        source_index += 1;
        if current_index == target_index {
            copied = Some(
                copy_exact_entry(reader, output, target.size)
                    .map_err(|error| SevenZipError::Other(error.to_string().into()))?,
            );
            Ok(false)
        } else {
            let mut sink = std::io::sink();
            std::io::copy(reader, &mut sink)?;
            Ok(true)
        }
    })
    .context("Failed to extract 7z image entry")?;
    copied.context("7z image entry disappeared before extraction")
}

#[cfg(target_os = "android")]
fn extract_libarchive(
    source: &File,
    password: Option<&ArchivePassword>,
    target: &ArchiveEntry,
    output: &mut (dyn Write + Send),
) -> Result<u64> {
    let EntryLocator::Libarchive(target_index) = target.locator else {
        bail!("libarchive entry has a mismatched codec locator");
    };
    let mut archive = NativeArchive::open(source, password)?;
    let mut source_index = 0usize;
    loop {
        let entry = archive
            .next_header()?
            .context("Archive ended before the requested image entry")?;
        let current_index = source_index;
        source_index += 1;
        if current_index == target_index {
            let name = native_entry_name(entry)?;
            ensure!(
                native_entry_is_regular(entry) && name == target.name,
                "Archive entry changed between scan and extraction"
            );
            return archive.copy_current_entry(output, target.size);
        }
    }
}

fn copy_exact_entry(
    reader: &mut dyn Read,
    output: &mut (dyn Write + Send),
    expected_size: u64,
) -> Result<u64> {
    let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
    let mut total = 0u64;
    loop {
        let read = reader
            .read(&mut buffer)
            .context("Failed to decode archive image entry")?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(u64::try_from(read)?)
            .context("Archive image byte count overflow")?;
        ensure!(
            total <= expected_size && total <= MAX_IMAGE_BYTES,
            "Decoded archive image exceeds its declared size"
        );
        output
            .write_all(&buffer[..read])
            .context("Failed to write decoded archive image")?;
    }
    ensure!(
        total == expected_size,
        "Decoded archive image size does not match metadata"
    );
    Ok(total)
}

fn with_source_position_restored<T>(
    source: &mut File,
    operation: impl FnOnce(&File) -> Result<T>,
) -> Result<T> {
    let original_position = source
        .stream_position()
        .context("Failed to read archive descriptor position")?;
    source
        .seek(SeekFrom::Start(0))
        .context("Failed to seek archive descriptor to the start")?;
    let result = operation(source);
    source
        .seek(SeekFrom::Start(original_position))
        .context("Failed to restore archive descriptor position")?;
    result
}

fn playable_extension(name: &str) -> Option<&str> {
    let extension = name.rsplit_once('.')?.1;
    PLAYABLE_EXTENSIONS
        .iter()
        .copied()
        .find(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn recovery_metadata_kind(name: &str) -> Option<RecoveryMetadataKind> {
    let leaf = name.rsplit(['/', '\\']).next()?;
    if leaf.eq_ignore_ascii_case(".ehviewer") {
        Some(RecoveryMetadataKind::SpiderInfo)
    } else if leaf.eq_ignore_ascii_case("ComicInfo.xml") {
        Some(RecoveryMetadataKind::ComicInfo)
    } else {
        None
    }
}

fn is_unsafe_archive_path(name: &str) -> bool {
    let bytes = name.as_bytes();
    name.starts_with('/')
        || name.starts_with('\\')
        || (bytes.len() > 1 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
        || name.split(['/', '\\']).any(|component| component == "..")
}

fn natural_compare(left: &str, right: &str) -> CmpOrdering {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        if left[left_index].is_ascii_digit() && right[right_index].is_ascii_digit() {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let left_number = trim_digit_run(&left[left_index..left_end]);
            let right_number = trim_digit_run(&right[right_index..right_end]);
            let ordering = left_number
                .len()
                .cmp(&right_number.len())
                .then_with(|| left_number.cmp(right_number))
                .then_with(|| (left_end - left_index).cmp(&(right_end - right_index)));
            if ordering != CmpOrdering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
        } else {
            let ordering = left[left_index].cmp(&right[right_index]);
            if ordering != CmpOrdering::Equal {
                return ordering;
            }
            left_index += 1;
            right_index += 1;
        }
    }
    left.len().cmp(&right.len())
}

fn digit_run_end(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    index
}

fn trim_digit_run(bytes: &[u8]) -> &[u8] {
    let first_nonzero = bytes.iter().position(|byte| *byte != b'0');
    first_nonzero.map_or(&bytes[bytes.len() - 1..], |index| &bytes[index..])
}

#[cfg(target_os = "android")]
fn libarchive_password_failure(error: &anyhow::Error, password_supplied: bool) -> bool {
    if libarchive_unsupported_encryption(error) {
        return false;
    }
    let message = error.to_string().to_ascii_lowercase();
    message.contains("passphrase")
        || message.contains("password")
        || (!password_supplied && message.contains("encrypted"))
}

#[cfg(target_os = "android")]
fn libarchive_unsupported_encryption(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("encrypted data is not currently supported")
        || message.contains("unsupported encryption")
}

#[cfg(target_os = "android")]
struct NativeArchive {
    raw: NonNull<ArchiveOpaque>,
    source: File,
    opened: bool,
}

#[cfg(target_os = "android")]
impl NativeArchive {
    fn open(source: &File, password: Option<&ArchivePassword>) -> Result<Self> {
        let source = duplicate_file_descriptor(source.as_raw_fd())
            .context("Failed to duplicate archive source descriptor")?;
        // SAFETY: allocation has no preconditions and the returned pointer is checked below.
        let raw = NonNull::new(unsafe { archive_read_new() })
            .context("libarchive failed to allocate a reader")?;
        let mut archive = Self {
            raw,
            source,
            opened: false,
        };
        archive.enable_format(archive_read_support_format_tar, "tar")?;
        archive.enable_format(archive_read_support_format_zip, "zip")?;
        archive.enable_format(archive_read_support_filter_gzip, "gzip filter")?;
        archive.enable_format(archive_read_support_filter_xz, "xz filter")?;
        if let Some(password) = password {
            // SAFETY: both pointers remain valid for the duration of this call and libarchive copies
            // the passphrase into its internal list.
            let status = unsafe { archive_read_add_passphrase(raw.as_ptr(), password.as_ptr()) };
            archive.ensure_status(status, "Failed to add archive passphrase")?;
        }
        // SAFETY: the duplicated descriptor remains owned by `archive` until after close/free.
        let status = unsafe {
            archive_read_open_fd(raw.as_ptr(), archive.source.as_raw_fd(), ARCHIVE_BLOCK_SIZE)
        };
        archive.ensure_status(status, "Failed to open archive")?;
        archive.opened = true;
        Ok(archive)
    }

    fn enable_format(
        &mut self,
        enable: unsafe extern "C" fn(*mut ArchiveOpaque) -> c_int,
        name: &str,
    ) -> Result<()> {
        // SAFETY: `self.raw` is a live reader allocated by libarchive.
        let status = unsafe { enable(self.raw.as_ptr()) };
        self.ensure_status(status, &format!("Failed to enable {name}"))
    }

    fn next_header(&mut self) -> Result<Option<NonNull<ArchiveEntryOpaque>>> {
        let mut entry = std::ptr::null_mut();
        // SAFETY: the reader is open and `entry` is a valid out pointer.
        let status = unsafe { archive_read_next_header(self.raw.as_ptr(), &raw mut entry) };
        match status {
            ARCHIVE_OK => NonNull::new(entry)
                .map(Some)
                .context("libarchive returned a null entry header"),
            ARCHIVE_EOF => Ok(None),
            _ => Err(self.error("Failed to read archive entry header")),
        }
    }

    fn has_encrypted_entries(&self) -> c_int {
        // SAFETY: the reader remains live through this call.
        unsafe { archive_read_has_encrypted_entries(self.raw.as_ptr()) }
    }

    fn drain_current_entry(&mut self, expected_size: u64) -> Result<()> {
        self.copy_current_entry(&mut std::io::sink(), expected_size)
            .map(|_| ())
    }

    fn copy_current_entry(
        &mut self,
        output: &mut (dyn Write + Send),
        expected_size: u64,
    ) -> Result<u64> {
        let mut buffer = vec![0u8; COPY_BUFFER_BYTES];
        let mut total = 0u64;
        loop {
            // SAFETY: the buffer is writable for its declared length and the reader is positioned
            // at the current entry's data stream.
            let read = unsafe {
                archive_read_data(
                    self.raw.as_ptr(),
                    buffer.as_mut_ptr().cast::<c_void>(),
                    buffer.len(),
                )
            };
            if read < 0 {
                return Err(self.error("Failed to decode archive image entry"));
            }
            if read == 0 {
                break;
            }
            let read = usize::try_from(read).context("Archive read count exceeds Rust range")?;
            total = total
                .checked_add(u64::try_from(read)?)
                .context("Archive image byte count overflow")?;
            ensure!(
                total <= expected_size && total <= MAX_IMAGE_BYTES,
                "Decoded archive image exceeds its declared size"
            );
            output
                .write_all(&buffer[..read])
                .context("Failed to write decoded archive image")?;
        }
        ensure!(
            total == expected_size,
            "Decoded archive image size does not match metadata"
        );
        Ok(total)
    }

    fn ensure_status(&self, status: c_int, operation: &str) -> Result<()> {
        if status < ARCHIVE_OK {
            Err(self.error(operation))
        } else {
            Ok(())
        }
    }

    fn error(&self, operation: &str) -> anyhow::Error {
        // SAFETY: error strings are owned by the live reader and are NUL terminated when non-null.
        let message = unsafe {
            let message = archive_error_string(self.raw.as_ptr());
            (!message.is_null()).then(|| CStr::from_ptr(message).to_string_lossy().into_owned())
        }
        .unwrap_or_else(|| "unknown libarchive error".to_owned());
        anyhow!("{operation}: {message}")
    }
}

#[cfg(target_os = "android")]
impl Drop for NativeArchive {
    fn drop(&mut self) {
        // SAFETY: `raw` was allocated exactly once and is freed exactly once here. The descriptor
        // helper does not close `_source`, so Rust retains sole ownership of that duplicate.
        unsafe {
            if self.opened {
                archive_read_close(self.raw.as_ptr());
            }
            archive_read_free(self.raw.as_ptr());
        }
    }
}

#[cfg(target_os = "android")]
fn native_entry_name(entry: NonNull<ArchiveEntryOpaque>) -> Result<String> {
    // SAFETY: the entry belongs to the current live archive header.
    let name = unsafe { archive_entry_pathname(entry.as_ptr()) };
    ensure!(!name.is_null(), "Archive entry has no pathname");
    // SAFETY: libarchive pathnames are NUL-terminated for the lifetime of the current header.
    Ok(unsafe { CStr::from_ptr(name) }
        .to_string_lossy()
        .into_owned())
}

#[cfg(target_os = "android")]
fn native_entry_is_regular(entry: NonNull<ArchiveEntryOpaque>) -> bool {
    // SAFETY: the entry belongs to the current live archive header.
    unsafe { archive_entry_filetype(entry.as_ptr()) == AE_IFREG }
}

#[cfg(target_os = "android")]
fn native_entry_is_encrypted(entry: NonNull<ArchiveEntryOpaque>) -> bool {
    // SAFETY: the entry belongs to the current live archive header.
    unsafe { archive_entry_is_encrypted(entry.as_ptr()) == 1 }
}

#[cfg(target_os = "android")]
fn native_entry_size(entry: NonNull<ArchiveEntryOpaque>) -> Result<u64> {
    // SAFETY: the entry belongs to the current live archive header.
    let size = unsafe { archive_entry_size(entry.as_ptr()) };
    u64::try_from(size).context("Archive image entry has an unknown or negative size")
}

#[cfg(target_os = "android")]
#[repr(C)]
struct ArchiveOpaque {
    _private: [u8; 0],
}

#[cfg(target_os = "android")]
#[repr(C)]
struct ArchiveEntryOpaque {
    _private: [u8; 0],
}

#[cfg(target_os = "android")]
unsafe extern "C" {
    fn archive_read_new() -> *mut ArchiveOpaque;
    fn archive_read_support_format_tar(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_read_support_format_zip(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_read_support_filter_gzip(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_read_support_filter_xz(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_read_add_passphrase(archive: *mut ArchiveOpaque, passphrase: *const c_char)
    -> c_int;
    fn archive_read_open_fd(archive: *mut ArchiveOpaque, fd: c_int, block_size: size_t) -> c_int;
    fn archive_read_next_header(
        archive: *mut ArchiveOpaque,
        entry: *mut *mut ArchiveEntryOpaque,
    ) -> c_int;
    fn archive_read_has_encrypted_entries(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_read_data(archive: *mut ArchiveOpaque, buffer: *mut c_void, size: size_t)
    -> ssize_t;
    fn archive_read_close(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_read_free(archive: *mut ArchiveOpaque) -> c_int;
    fn archive_error_string(archive: *mut ArchiveOpaque) -> *const c_char;
    fn archive_entry_pathname(entry: *mut ArchiveEntryOpaque) -> *const c_char;
    fn archive_entry_filetype(entry: *mut ArchiveEntryOpaque) -> mode_t;
    fn archive_entry_is_encrypted(entry: *mut ArchiveEntryOpaque) -> c_int;
    fn archive_entry_size(entry: *mut ArchiveEntryOpaque) -> i64;
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);
    const RAR4: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../androidTest/assets/archive-v2/rar4-images.rar"
    ));
    const RAR5: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../androidTest/assets/archive-v2/rar5-images.rar"
    ));
    const RAR4_ENCRYPTED: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../androidTest/assets/archive-v2/rar4-encrypted-images.rar"
    ));
    const RAR5_ENCRYPTED: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../androidTest/assets/archive-v2/rar5-encrypted-images.rar"
    ));
    const SEVEN_ZIP: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../androidTest/assets/archive-v2/images.7z"
    ));
    const SEVEN_ZIP_ENCRYPTED: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../androidTest/assets/archive-v2/encrypted-images.7z"
    ));

    #[test]
    fn extracts_rar4_rar5_and_seven_zip_by_content() {
        assert_plain_fixture(RAR4, 2, b"test text document\r\n");
        assert_plain_fixture(RAR5, 1, b"hello libarchive test suite!\n");
        assert_plain_fixture(SEVEN_ZIP, 1, b"archive-v2-page\n");
    }

    #[test]
    fn rejects_wrong_passwords_and_extracts_encrypted_rar_and_seven_zip() {
        assert_encrypted_fixture(RAR4_ENCRYPTED, b"password", b"This is from a.txt");
        assert_encrypted_fixture(RAR5_ENCRYPTED, b"password", b"This is from a.txt");
        assert_encrypted_fixture(SEVEN_ZIP_ENCRYPTED, b"secret", b"archive-v2-page\n");
    }

    fn assert_plain_fixture(fixture: &[u8], count: usize, expected: &[u8]) {
        let registry = &ARCHIVE_READER_REGISTRY;
        let handle = registry.open(fixture_file(fixture), false).unwrap();
        assert!(!registry.needs_password(handle).unwrap());
        assert_eq!(registry.entry_count(handle).unwrap(), count);
        assert!(!registry.entry_name(handle, 0).unwrap().is_empty());
        assert!(!registry.has_unsafe_path(handle).unwrap());
        assert!(registry.verify_entry(handle, 0).unwrap());
        assert!(
            registry
                .recovery_metadata(handle, RecoveryMetadataKind::SpiderInfo)
                .unwrap()
                .is_none()
        );
        assert_eq!(registry.extension(handle, 0).unwrap(), "png");
        assert_eq!(
            registry.entry_size(handle, 0).unwrap(),
            expected.len() as u64
        );
        let mut output = Vec::new();
        assert_eq!(
            registry.extract_to(handle, 0, &mut output).unwrap(),
            expected.len() as u64
        );
        assert_eq!(output, expected);
        assert!(registry.close(handle).unwrap());
        assert!(!registry.close(handle).unwrap());
    }

    fn assert_encrypted_fixture(fixture: &[u8], password: &[u8], expected: &[u8]) {
        let registry = &ARCHIVE_READER_REGISTRY;
        let handle = registry.open(fixture_file(fixture), false).unwrap();
        assert!(registry.needs_password(handle).unwrap());
        assert!(
            !registry
                .provide_password(handle, b"wrong".to_vec())
                .unwrap()
        );
        assert!(
            registry
                .provide_password(handle, password.to_vec())
                .unwrap()
        );
        let mut output = Vec::new();
        registry.extract_to(handle, 0, &mut output).unwrap();
        assert_eq!(output, expected);
        assert!(registry.close(handle).unwrap());
    }

    fn fixture_file(bytes: &[u8]) -> File {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ehviewer-archive-v2-{}-{id}.misleading",
            std::process::id()
        ));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        std::fs::remove_file(path).unwrap();
        file
    }
}
