//! Production ZIP decoding without libarchive, filesystem paths or Android objects.
//! Central-directory limits are checked before the codec builds its metadata table.
//! Every member index, including duplicate names, remains stable. Password checking
//! drains one encrypted regular member through CRC/authentication before acceptance.

use std::io::{self, Read, Seek, SeekFrom, Write};

use rawzip::{MAX_CENTRAL_DIRECTORY_RECORD_SIZE, ZipArchive as RawZipArchive};
use thiserror::Error;
use zip::ZipArchive;
use zip::read::{Config, ZipFile};
use zip::result::ZipError;

/// Maximum number of physical central-directory records.
pub const MAX_ENTRIES: u64 = 100_000;
/// Bound on central-directory and ZIP64 auxiliary metadata before allocation.
pub const MAX_DIRECTORY_BYTES: u64 = 32 * 1024 * 1024;
/// Maximum uncompressed size of an extracted member.
pub const MAX_MEMBER_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum per-decoder LZMA/XZ dictionary or `PPMd` model allocation.
pub const MAX_DICTIONARY_BYTES: usize = 64 * 1024 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

/// Header-only observation; no member payload is retained.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZipMember {
    /// Physical central-directory position, not a deduplicated name lookup.
    pub index: usize,
    /// UTF-8 pathname, matching the previous Android default charset.
    pub name: String,
    /// Declared uncompressed size.
    pub size: u64,
    /// Regular file, excluding directories, links and special Unix types.
    pub regular: bool,
    /// Encryption bit observed without trying to decrypt the member.
    pub encrypted: bool,
}

/// Result of header inspection and optional password validation.
#[derive(Debug)]
pub enum ZipScan {
    /// Header scan completed; caller owns image/metadata filtering and ordering.
    Complete(Vec<ZipMember>),
    /// Password verification failed; no partially scanned plan is exposed.
    PasswordRequired,
}

/// Stable codec/resource failures with no archive names or passwords in messages.
#[derive(Debug, Error)]
pub enum ProductionZipError {
    /// Source access, entry decoding, integrity verification or output failed.
    #[error("ZIP source, integrity or output operation failed")]
    Io(#[from] io::Error),
    /// Footer or directory failed structural validation.
    #[error("ZIP directory is invalid")]
    Directory(#[from] rawzip::Error),
    /// ZIP codec reported a structural, encryption or compression failure.
    #[error("ZIP codec operation failed")]
    Codec(#[from] ZipError),
    /// Metadata or decoded bytes exceed application resource limits.
    #[error("ZIP resource limit exceeded")]
    Limit,
    /// The member no longer matches the previously scanned identity.
    #[error("ZIP member changed between scan and extraction")]
    ChangedMember,
    /// Actual decoded length differs from its header.
    #[error("ZIP decoded length does not match metadata")]
    SizeMismatch,
}

/// Recognize a seekable ZIP footer, including self-extracting prefixes.
/// Source position is preserved even when recognition fails.
///
/// # Errors
/// Returns source seek errors; an absent or invalid ZIP footer returns `false`.
pub fn recognizes<R: Read + Seek>(source: &mut R) -> Result<bool, ProductionZipError> {
    with_position(source, |source| {
        let mut buffer = vec![0; MAX_CENTRAL_DIRECTORY_RECORD_SIZE];
        Ok(RawZipArchive::from_seekable(source, &mut buffer).is_ok())
    })
}

/// Inspect all ZIP members without extracting files or buffering payloads.
/// Optional password validation reads one encrypted regular member fully.
///
/// # Errors
/// Rejects malformed directories, unsupported codecs/encryption and resource limits.
pub fn scan<R: Read + Seek>(
    source: &mut R,
    password: Option<&[u8]>,
    validate_password: bool,
) -> Result<ZipScan, ProductionZipError> {
    lzma_rust2::limits::with_decoder_limits(MAX_DICTIONARY_BYTES, MAX_ENTRIES, || {
        with_position(source, |source| {
            let count = preflight(source)?;
            if count == 0 {
                return Ok(ZipScan::Complete(Vec::new()));
            }
            let mut archive = open_indexed(source)?;
            if archive.len() as u64 != count {
                return Err(ProductionZipError::ChangedMember);
            }
            let mut members = Vec::with_capacity(archive.len());
            for index in 0..archive.len() {
                let member = observe(&archive.by_index_raw(index)?, index);
                members.push(member);
            }
            if validate_password
                && let Some(member) = members
                    .iter()
                    .find(|member| member.regular && member.encrypted)
            {
                let Some(password) = password else {
                    return Ok(ZipScan::PasswordRequired);
                };
                let mut file = match archive.by_index_decrypt(member.index, password) {
                    Ok(file) => file,
                    Err(ZipError::InvalidPassword) => return Ok(ZipScan::PasswordRequired),
                    Err(error) => return Err(error.into()),
                };
                match copy_exact(&mut file, &mut io::sink(), member.size) {
                    Ok(_) => {}
                    Err(ProductionZipError::Io(error))
                        if error.kind() != io::ErrorKind::OutOfMemory =>
                    {
                        return Ok(ZipScan::PasswordRequired);
                    }
                    Err(error) => return Err(error),
                }
            }
            Ok(ZipScan::Complete(members))
        })
    })
}

/// Decode a scanned regular member, checking identity, length, CRC and AES MAC.
/// Copying uses a fixed buffer; source position is always restored.
///
/// # Errors
/// Rejects changed members, wrong/missing passwords, unsupported codecs, corrupted
/// payloads, output failures and application resource limits.
pub fn extract<R: Read + Seek, W: Write + ?Sized>(
    source: &mut R,
    password: Option<&[u8]>,
    member: &ZipMember,
    output: &mut W,
) -> Result<u64, ProductionZipError> {
    lzma_rust2::limits::with_decoder_limits(MAX_DICTIONARY_BYTES, MAX_ENTRIES, || {
        with_position(source, |source| {
            preflight(source)?;
            let mut archive = open_indexed(source)?;
            let mut file = match password {
                Some(password) => archive.by_index_decrypt(member.index, password)?,
                None => archive.by_index(member.index)?,
            };
            if !member.regular || observe(&file, member.index) != *member {
                return Err(ProductionZipError::ChangedMember);
            }
            copy_exact(&mut file, output, member.size)
        })
    })
}

fn preflight<R: Read + Seek>(source: &mut R) -> Result<u64, ProductionZipError> {
    let mut buffer = vec![0; MAX_CENTRAL_DIRECTORY_RECORD_SIZE];
    let archive = RawZipArchive::from_seekable(source, &mut buffer)?;
    if archive.entries_hint() > MAX_ENTRIES {
        return Err(ProductionZipError::Limit);
    }
    let mut entries = archive.entries(&mut buffer);
    let mut count = 0;
    while let Some(entry) = entries.next_entry()? {
        count += 1;
        if count > MAX_ENTRIES
            || archive
                .eocd_offset()
                .saturating_sub(entry.central_directory_offset())
                > MAX_DIRECTORY_BYTES
        {
            return Err(ProductionZipError::Limit);
        }
    }
    if count != archive.entries_hint() {
        return Err(ProductionZipError::ChangedMember);
    }
    Ok(count)
}

fn open_indexed<R: Read + Seek>(source: R) -> Result<ZipArchive<R>, ProductionZipError> {
    Ok(ZipArchive::with_config(
        Config {
            indexed_entries: true,
            ..Config::default()
        },
        source,
    )?)
}

fn observe<R: Read>(file: &ZipFile<'_, R>, index: usize) -> ZipMember {
    let bytes = file.name_raw();
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    ZipMember {
        index,
        name: String::from_utf8_lossy(&bytes[..end]).into_owned(),
        size: file.size(),
        regular: file.is_file()
            && file.unix_mode().is_none_or(|mode| {
                let kind = mode & 0o170_000;
                kind == 0 || kind == 0o100_000
            }),
        encrypted: file.encrypted(),
    }
}

fn copy_exact<W: Write + ?Sized>(
    reader: &mut dyn Read,
    output: &mut W,
    expected: u64,
) -> Result<u64, ProductionZipError> {
    if expected > MAX_MEMBER_BYTES {
        return Err(ProductionZipError::Limit);
    }
    let mut buffer = vec![0; COPY_BUFFER_BYTES];
    let mut total = 0_u64;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .ok_or(ProductionZipError::Limit)?;
        if total > expected {
            return Err(ProductionZipError::SizeMismatch);
        }
        output.write_all(&buffer[..count])?;
    }
    if total != expected {
        return Err(ProductionZipError::SizeMismatch);
    }
    Ok(total)
}

fn with_position<R: Read + Seek, T>(
    source: &mut R,
    operation: impl FnOnce(&mut R) -> Result<T, ProductionZipError>,
) -> Result<T, ProductionZipError> {
    let position = source.stream_position()?;
    source.seek(SeekFrom::Start(0))?;
    let result = operation(source);
    let restored = source.seek(SeekFrom::Start(position));
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            restored?;
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests;
