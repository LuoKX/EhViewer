//! Rust-owned multi-format archive lifecycle with format-specific codec adapters.

use std::cmp::Ordering as CmpOrdering;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

use anyhow::{Context as _, Result, anyhow, bail, ensure};
use eh_archive::archive_filters::DecodedSource;
use eh_archive::production_tar::{self, TarMember};
use eh_archive::production_zip::{self, ZipMember, ZipScan};
use rars::{ArchiveReadOptions, ArchiveReader as RarArchiveReader};
use sevenz_rust2::{
    Archive as SevenZipArchive, EncoderMethod, Error as SevenZipError,
    Password as SevenZipPassword, decompress_with_extract_fn_and_password,
};
use zeroize::Zeroize;

use super::duplicate_file_descriptor;

pub(super) static ARCHIVE_READER_REGISTRY: LazyLock<ArchiveReaderRegistry> =
    LazyLock::new(ArchiveReaderRegistry::default);

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
    Zip,
    Tar,
    Filtered,
    Rar,
    SevenZip,
}

#[derive(Clone, Debug)]
enum EntryLocator {
    Zip(ZipMember),
    Tar(TarMember),
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
    bytes: Vec<u8>,
}
impl ArchivePassword {
    fn new(bytes: Vec<u8>) -> Result<Self> {
        ensure!(!bytes.contains(&0), "Archive password contains a NUL byte");
        Ok(Self { bytes })
    }
    fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
impl Drop for ArchivePassword {
    fn drop(&mut self) {
        self.bytes.zeroize();
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
            } else if signature.starts_with(b"\x1f\x8b") || signature.starts_with(b"\xfd7zXZ\0") {
                ArchiveKind::Filtered
            } else if has_tar_header(source)? {
                ArchiveKind::Tar
            } else if signature.starts_with(b"PK\x03\x04")
                || signature.starts_with(b"PK\x05\x06")
                || signature.starts_with(b"PK\x07\x08")
                || production_zip::recognizes(&mut duplicate_file_descriptor(source.as_raw_fd())?)?
            {
                ArchiveKind::Zip
            } else {
                ArchiveKind::Tar
            },
        )
    })
}

fn has_tar_header(source: &File) -> Result<bool> {
    let mut input = duplicate_file_descriptor(source.as_raw_fd())?;
    with_source_position_restored(&mut input, |source| {
        let mut header = [0; 512];
        match source.take(512).read_exact(&mut header) {
            Ok(()) => Ok(production_tar::recognizes_header(&header)),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(false),
            Err(error) => Err(error.into()),
        }
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
        ArchiveKind::Zip => scan_zip(source, password, sort_entries, validate_password),
        ArchiveKind::Rar => scan_rar(source, password, sort_entries, validate_password),
        ArchiveKind::SevenZip => scan_seven_zip(source, password, sort_entries, validate_password),
        ArchiveKind::Tar => {
            let mut input = duplicate_file_descriptor(source.as_raw_fd())?;
            scan_tar_source(&mut input, sort_entries)
        }
        ArchiveKind::Filtered => {
            let mut input = DecodedSource::new(duplicate_file_descriptor(source.as_raw_fd())?)?;
            let budget = input.allocation_budget();
            budget.run(|| {
                let mut header = Vec::with_capacity(512);
                input.by_ref().take(512).read_to_end(&mut header)?;
                input.seek(SeekFrom::Start(0))?;
                if production_tar::recognizes_header(&header) {
                    scan_tar_source(&mut input, sort_entries)
                } else {
                    scan_zip_source(&mut input, password, sort_entries, validate_password)
                }
            })
        }
    })
}

fn scan_zip(
    source: &File,
    password: Option<&ArchivePassword>,
    sort_entries: bool,
    validate_password: bool,
) -> Result<ScanOutcome> {
    let mut input = duplicate_file_descriptor(source.as_raw_fd())?;
    scan_zip_source(&mut input, password, sort_entries, validate_password)
}

fn scan_zip_source<R: Read + Seek>(
    input: &mut R,
    password: Option<&ArchivePassword>,
    sort_entries: bool,
    validate_password: bool,
) -> Result<ScanOutcome> {
    let members = match production_zip::scan(
        input,
        password.map(ArchivePassword::bytes),
        validate_password,
    )? {
        ZipScan::Complete(members) => members,
        ZipScan::PasswordRequired => return Ok(ScanOutcome::PasswordRequired),
    };
    let mut entries = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut encrypted = false;
    let mut unsafe_path = false;
    for member in members {
        encrypted |= member.encrypted;
        unsafe_path |= is_unsafe_archive_path(&member.name);
        if !member.regular {
            continue;
        }
        let extension = playable_extension(&member.name).map(str::to_owned);
        let metadata_kind = recovery_metadata_kind(&member.name);
        if let Some(extension) = extension {
            validate_image_size(member.size)?;
            entries.push(ArchiveEntry {
                name: member.name.clone(),
                size: member.size,
                locator: EntryLocator::Zip(member),
                extension,
                metadata_kind: None,
            });
        } else if let Some(metadata_kind) = metadata_kind {
            validate_recovery_metadata_size(member.size)?;
            metadata_entries.push(ArchiveEntry {
                name: member.name.clone(),
                size: member.size,
                locator: EntryLocator::Zip(member),
                extension: String::new(),
                metadata_kind: Some(metadata_kind),
            });
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

fn scan_tar_source<R: Read + Seek>(input: &mut R, sort_entries: bool) -> Result<ScanOutcome> {
    let members = production_tar::scan(input)?;
    let mut entries = Vec::new();
    let mut metadata_entries = Vec::new();
    let mut unsafe_path = false;
    for member in members {
        unsafe_path |= is_unsafe_archive_path(&member.name);
        if !member.regular {
            continue;
        }
        let extension = playable_extension(&member.name).map(str::to_owned);
        let metadata_kind = recovery_metadata_kind(&member.name);
        if let Some(extension) = extension {
            validate_image_size(member.size)?;
            entries.push(ArchiveEntry {
                name: member.name.clone(),
                size: member.size,
                locator: EntryLocator::Tar(member),
                extension,
                metadata_kind: None,
            });
        } else if let Some(metadata_kind) = metadata_kind {
            validate_recovery_metadata_size(member.size)?;
            metadata_entries.push(ArchiveEntry {
                name: member.name.clone(),
                size: member.size,
                locator: EntryLocator::Tar(member),
                extension: String::new(),
                metadata_kind: Some(metadata_kind),
            });
        }
    }
    sort_archive_entries(&mut entries, sort_entries);
    Ok(ScanOutcome::Complete {
        entries,
        metadata_entries,
        encrypted: false,
        unsafe_path,
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
        ArchiveKind::Zip => {
            let EntryLocator::Zip(member) = &target.locator else {
                bail!("ZIP entry has a mismatched codec locator");
            };
            let mut input = duplicate_file_descriptor(source.as_raw_fd())?;
            Ok(production_zip::extract(
                &mut input,
                password.map(ArchivePassword::bytes),
                member,
                output,
            )?)
        }
        ArchiveKind::Rar => extract_rar(source, password, target, output),
        ArchiveKind::SevenZip => extract_seven_zip(source, password, target, output),
        ArchiveKind::Tar => {
            let EntryLocator::Tar(member) = &target.locator else {
                bail!("TAR entry has a mismatched codec locator");
            };
            let mut input = duplicate_file_descriptor(source.as_raw_fd())?;
            Ok(production_tar::extract(&mut input, member, output)?)
        }
        ArchiveKind::Filtered => {
            let mut input = DecodedSource::new(duplicate_file_descriptor(source.as_raw_fd())?)?;
            let budget = input.allocation_budget();
            budget.run(|| match &target.locator {
                EntryLocator::Zip(member) => Ok(production_zip::extract(
                    &mut input,
                    password.map(ArchivePassword::bytes),
                    member,
                    output,
                )?),
                EntryLocator::Tar(member) => {
                    Ok(production_tar::extract(&mut input, member, output)?)
                }
                _ => bail!("Filtered archive entry has a mismatched codec locator"),
            })
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
    fn outer_tar_and_filters_take_priority_over_nested_zip_footers() {
        let fixtures = [
            (
                include_bytes!("../../../../archive/src/production_zip/test_data/nested.tar")
                    .as_slice(),
                false,
            ),
            (
                include_bytes!("../../../../archive/src/production_zip/test_data/stored.zip.gz")
                    .as_slice(),
                true,
            ),
        ];
        for (bytes, filtered) in fixtures {
            let mut source = fixture_file(bytes);
            source.seek(SeekFrom::Start(7)).unwrap();
            let kind = detect_archive_kind(&mut source).unwrap();
            assert_eq!(matches!(kind, ArchiveKind::Filtered), filtered);
            assert_eq!(source.stream_position().unwrap(), 7);
        }
    }

    #[test]
    fn production_zip_registry_preserves_order_metadata_and_descriptor_position() {
        let bytes = include_bytes!("../../../../archive/src/production_zip/test_data/mixed.zip");
        let mut source = fixture_file(bytes);
        source.seek(SeekFrom::Start(7)).unwrap();
        let registry = &ARCHIVE_READER_REGISTRY;
        let handle = registry
            .open(duplicate_file_descriptor(source.as_raw_fd()).unwrap(), true)
            .unwrap();
        assert_eq!(source.stream_position().unwrap(), 7);
        assert_eq!(registry.entry_count(handle).unwrap(), 3);
        assert!(registry.has_unsafe_path(handle).unwrap());
        for (index, (name, expected)) in [
            ("PAGE0.PNG", b"upper".as_slice()),
            ("page2.png", b"two".as_slice()),
            ("page10.jpg", b"ten".as_slice()),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(registry.entry_name(handle, index).unwrap(), name);
            let mut output = Vec::new();
            registry.extract_to(handle, index, &mut output).unwrap();
            assert_eq!(output, expected);
            assert!(registry.verify_entry(handle, index).unwrap());
            assert_eq!(source.stream_position().unwrap(), 7);
        }
        assert_eq!(
            registry
                .recovery_metadata(handle, RecoveryMetadataKind::ComicInfo)
                .unwrap(),
            Some(b"<ComicInfo/>".to_vec())
        );
        assert_eq!(
            registry
                .recovery_metadata(handle, RecoveryMetadataKind::SpiderInfo)
                .unwrap(),
            Some(b"spider".to_vec())
        );
        assert_eq!(source.stream_position().unwrap(), 7);
        assert!(registry.close(handle).unwrap());
        assert!(registry.entry_count(handle).is_err());
    }

    #[test]
    fn production_tar_and_nested_filter_registry_restore_source_positions() {
        let plain = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../androidTest/assets/archive-v2/images.tar"
        ));
        for bytes in [
            plain.as_slice(),
            include_bytes!("../../../../archive/src/production_tar/test_data/images.tar.gz")
                .as_slice(),
            include_bytes!("../../../../archive/src/production_tar/test_data/images.tar.xz")
                .as_slice(),
            include_bytes!("../../../../archive/src/production_tar/test_data/images.tar.gz.xz")
                .as_slice(),
        ] {
            let mut source = fixture_file(bytes);
            source.seek(SeekFrom::Start(7)).unwrap();
            let handle = ARCHIVE_READER_REGISTRY
                .open(duplicate_file_descriptor(source.as_raw_fd()).unwrap(), true)
                .unwrap();
            assert_eq!(source.stream_position().unwrap(), 7);
            assert_eq!(ARCHIVE_READER_REGISTRY.entry_count(handle).unwrap(), 1);
            let mut out = Vec::new();
            ARCHIVE_READER_REGISTRY
                .extract_to(handle, 0, &mut out)
                .unwrap();
            assert_eq!(out, b"archive-v2-page\n");
            assert_eq!(source.stream_position().unwrap(), 7);
            assert!(ARCHIVE_READER_REGISTRY.close(handle).unwrap());
        }
    }

    #[test]
    fn filtered_zip_preserves_password_metadata_order_and_all_outer_integrity() {
        for bytes in [
            include_bytes!("../../../../archive/src/production_tar/test_data/mixed.zip.gz")
                .as_slice(),
            include_bytes!("../../../../archive/src/production_tar/test_data/mixed.zip.xz")
                .as_slice(),
        ] {
            let handle = ARCHIVE_READER_REGISTRY
                .open(fixture_file(bytes), true)
                .unwrap();
            assert_eq!(ARCHIVE_READER_REGISTRY.entry_count(handle).unwrap(), 3);
            assert_eq!(
                ARCHIVE_READER_REGISTRY.entry_name(handle, 1).unwrap(),
                "page2.png"
            );
            assert_eq!(
                ARCHIVE_READER_REGISTRY
                    .recovery_metadata(handle, RecoveryMetadataKind::SpiderInfo)
                    .unwrap(),
                Some(b"spider".to_vec())
            );
            assert!(ARCHIVE_READER_REGISTRY.verify_entry(handle, 1).unwrap());
            assert!(ARCHIVE_READER_REGISTRY.close(handle).unwrap());
        }
        for bytes in [
            include_bytes!("../../../../archive/src/production_tar/test_data/aes.zip.gz")
                .as_slice(),
            include_bytes!("../../../../archive/src/production_tar/test_data/aes.zip.xz")
                .as_slice(),
        ] {
            assert_encrypted_fixture(bytes, b"secret", b"archive-v2-page\n");
        }
        let mut bytes =
            include_bytes!("../../../../archive/src/production_tar/test_data/images.tar.gz")
                .to_vec();
        let end = bytes.len() - 5;
        bytes[end] ^= 1;
        assert!(
            ARCHIVE_READER_REGISTRY
                .open(fixture_file(&bytes), false)
                .is_err()
        );
    }

    #[test]
    fn production_zip_registry_validates_zipcrypto_and_lzma() {
        let encrypted = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../androidTest/assets/archive-v2/encrypted-images.zip"
        ));
        assert_encrypted_fixture(encrypted, b"secret", b"archive-v2-page\n");
        let plain = include_bytes!("../../../../archive/src/production_zip/test_data/lzma.zip");
        assert_plain_fixture(plain, 1, b"archive-v2-page\n");
    }

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
