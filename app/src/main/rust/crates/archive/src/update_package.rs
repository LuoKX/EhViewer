//! Update artifacts: one regular, unencrypted APK, extracted to a caller-owned sink.
//! Member names never become filesystem paths. Installation remains Android-owned.
use crate::production_zip::{self, ProductionZipError, ZipMember, ZipScan};
use std::io::{self, Read, Seek, SeekFrom, Write};
use thiserror::Error;

/// Maximum compressed download and extracted APK size (512 MiB each).
pub const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
/// Maximum headers in an update artifact, after the shared ZIP metadata preflight.
pub const MAX_ENTRIES: usize = 256;

/// Fixed update-container errors, never containing names or paths.
#[derive(Debug, Error)]
pub enum UpdatePackageError {
    /// Seek/read/output failed.
    #[error("Update package I/O failed")]
    Io(#[from] io::Error),
    /// The existing bounded codec rejected metadata, integrity or compression.
    #[error("Update ZIP is invalid")]
    Archive(#[from] ProductionZipError),
    /// Compressed bytes, extracted bytes or entry count exceeded the update budget.
    #[error("Update package exceeds limit")]
    Limit,
    /// No APK was present.
    #[error("Update ZIP contains no APK")]
    Missing,
    /// More than one APK was present; guessing a variant is forbidden here.
    #[error("Update ZIP contains multiple APKs")]
    Ambiguous,
    /// An APK entry is empty, encrypted, non-regular or has an unsafe relative name.
    #[error("Update APK entry is invalid")]
    InvalidEntry,
}

/// Select exactly one APK without using comic-image filters or archive ordering.
///
/// # Errors
/// Rejects excessive entries, missing/ambiguous APKs and invalid APK metadata.
pub fn select_apk(members: &[ZipMember]) -> Result<&ZipMember, UpdatePackageError> {
    if members.len() > MAX_ENTRIES {
        return Err(UpdatePackageError::Limit);
    }
    let mut selected = None;
    for member in members {
        if !member
            .name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("apk"))
        {
            continue;
        }
        if !member.regular
            || member.encrypted
            || member.size == 0
            || member.name.contains(['\\', ':', '\0'])
            || member
                .name
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(UpdatePackageError::InvalidEntry);
        }
        if member.size > MAX_PACKAGE_BYTES {
            return Err(UpdatePackageError::Limit);
        }
        if selected.replace(member).is_some() {
            return Err(UpdatePackageError::Ambiguous);
        }
    }
    selected.ok_or(UpdatePackageError::Missing)
}

/// Validate and extract a uniquely selected APK through the existing ZIP codec.
/// Source position is preserved. The caller owns transactional publication of output.
///
/// # Errors
/// Rejects container budgets, selection failures, changed sources and CRC/I/O failures.
pub fn extract_apk<R: Read + Seek, W: Write>(
    source: &mut R,
    output: &mut W,
) -> Result<u64, UpdatePackageError> {
    let original = source.stream_position()?;
    let size = source.seek(SeekFrom::End(0));
    let restored = source.seek(SeekFrom::Start(original));
    let size = size?;
    restored?;
    if size == 0 || size > MAX_PACKAGE_BYTES {
        return Err(UpdatePackageError::Limit);
    }
    let ZipScan::Complete(members) = production_zip::scan(source, None, false)? else {
        return Err(UpdatePackageError::InvalidEntry);
    };
    let member = select_apk(&members)?;
    Ok(production_zip::extract(source, None, member, output)?)
}

#[cfg(test)]
mod tests;
