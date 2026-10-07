//! Valid-UTF8 subset of `AndroidX` `DataStore` Preferences 1.2.1's binary format.
//!
//! Field definitions and duplicate semantics are frozen by the cached JVM oracle
//! in `native/parity/preferences`. Untouched documents retain their exact wire
//! bytes. Any unknown field makes the whole document read-only: mutation must not
//! erase future fields, including fields in overwritten duplicate entries.
//! Malformed UTF8 values/sets are rejected, unlike JVM replacement decoding.
//!
//! `FixtureStore` is a synchronous, cooperating-writer, synthetic-file runtime,
//! not a production `DataStore` replacement. Callers supply an existing private
//! directory; no application path discovery, migration, or settings defaults.
//! Linux host locking is crash-released. Android persistent execution is blocked
//! (installed std's Android file locking is unsupported); unsupported hosts fail
//! closed. Neither byte comparison nor advisory locks protect against a writer
//! that ignores the ownership contract or replaces the lock inode/directory.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum encoded document size, including unknown fields: one MiB.
pub const MAX_BYTES: usize = 1_048_576;
/// Maximum map entries, including overwritten duplicate entries while decoding.
pub const MAX_ENTRIES: usize = 4096;
/// Maximum repeated strings per value, including duplicates while decoding.
pub const MAX_SET_STRINGS: usize = 4096;
const MAX_FIELDS: usize = 65_536;
const MAX_GROUP_DEPTH: usize = 32;
const DATA_FILE: &str = "preferences.pb";
const LOCK_FILE: &str = ".preferences.lock";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Redacted error categories: never contain paths, keys, values, or OS messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Input ends before its declared field completes.
    Truncated,
    /// Invalid tag, wire type, group, or overflowing varint.
    InvalidWire,
    /// String bytes are not valid UTF8; JVM value replacement is not implemented.
    InvalidUtf8,
    /// Map value has no supported oneof member; no default is substituted.
    MissingValue,
    /// Document exceeds a size, count, or nesting bound.
    LimitExceeded,
    /// Mutation would discard uninterpreted wire fields.
    UnknownFields,
    /// Another cooperating runtime owns this directory.
    Owned,
    /// Kernel file locking is unavailable on this platform.
    LockUnsupported,
    /// File data differs from the runtime's last observed committed snapshot.
    ExternalChange,
    /// Filesystem operation failed before the rename commit point.
    Io,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "preferences error: {self:?}")
    }
}
impl std::error::Error for Error {}

/// Bounded string set preserving first-occurrence encounter order.
///
/// Order is observable in original archive-password attempts. Private entries
/// prohibit duplicate or unchecked mutation; immutable iteration preserves order.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct PreferenceSet {
    strings: Vec<String>,
    encoded_len: usize,
}
impl fmt::Debug for PreferenceSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreferenceSet([redacted])")
    }
}
impl PreferenceSet {
    /// Build from encounter-ordered strings, keeping first occurrence only.
    ///
    /// # Errors
    /// Returns `LimitExceeded` for more than 4096 input items (including
    /// duplicates) or more than one MiB of encoded unique string fields.
    pub fn from_strings(strings: impl IntoIterator<Item = String>) -> Result<Self, Error> {
        let mut set = Self::default();
        for (index, string) in strings.into_iter().enumerate() {
            if index >= MAX_SET_STRINGS {
                return Err(Error::LimitExceeded);
            }
            set.insert(string)?;
        }
        Ok(set)
    }

    /// Append a new member. Duplicate insertion leaves encounter order unchanged.
    /// Returns whether a member was inserted.
    ///
    /// # Errors
    /// Returns `LimitExceeded` without mutation if member/byte bounds would fail.
    pub fn insert(&mut self, string: String) -> Result<bool, Error> {
        if self.strings.contains(&string) {
            return Ok(false);
        }
        if self.strings.len() >= MAX_SET_STRINGS {
            return Err(Error::LimitExceeded);
        }
        let mut prefix = Vec::new();
        varint(
            &mut prefix,
            u64::try_from(string.len()).map_err(|_| Error::LimitExceeded)?,
        );
        let encoded_len = self
            .encoded_len
            .saturating_add(string.len())
            .saturating_add(1)
            .saturating_add(prefix.len());
        if encoded_len > MAX_BYTES {
            return Err(Error::LimitExceeded);
        }
        self.strings.push(string);
        self.encoded_len = encoded_len;
        Ok(true)
    }

    /// Ordered immutable members; no mutable string/vector access is exposed.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &str> {
        self.strings.iter().map(String::as_str)
    }

    /// Number of unique members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether no members exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

/// Typed preference value. Float/double bits preserve NaNs and signed zero.
#[derive(Clone, Eq, PartialEq)]
pub enum Value {
    /// Boolean oneof field 1.
    Boolean(bool),
    /// IEEE754 binary32 bits, oneof field 2.
    FloatBits(u32),
    /// Signed int32 (not zigzag), oneof field 3.
    Integer(i32),
    /// Signed int64 (not zigzag), oneof field 4.
    Long(i64),
    /// Valid UTF8 string, oneof field 5.
    String(String),
    /// First-occurrence-ordered strings, matching serializer semantics, field 6.
    StringSet(PreferenceSet),
    /// IEEE754 binary64 bits, oneof field 7.
    DoubleBits(u64),
    /// Opaque bytes, oneof field 8.
    Bytes(Vec<u8>),
}
impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Value([redacted])")
    }
}

/// Validated document with private wire-retention and mutation invariants.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct Preferences {
    entries: BTreeMap<String, Value>,
    original: Option<Vec<u8>>,
    unknown: bool,
}
impl fmt::Debug for Preferences {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Preferences([redacted])")
    }
}
impl Preferences {
    /// Decode bounded wire data. Duplicate keys/oneof scalars use last member.
    ///
    /// # Errors
    /// Returns redacted corruption/limit errors. Unknown fields are retained
    /// verbatim but prohibit mutation. Effective unset values (after all map
    /// entries reduce with last-key-wins) and malformed UTF8 fail.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::LimitExceeded);
        }
        let mut parser = Parser {
            fields: 0,
            unknown: false,
        };
        let mut cursor = Cursor::new(bytes);
        let mut entries = BTreeMap::new();
        let mut count = 0;
        while let Some(tag) = parser.tag(&mut cursor)? {
            if tag == 10 {
                count += 1;
                if count > MAX_ENTRIES {
                    return Err(Error::LimitExceeded);
                }
                let (key, value) = parser.entry(cursor.bytes()?)?;
                entries.insert(key, value);
            } else {
                parser.skip(&mut cursor, tag, 0)?;
            }
        }
        // Physical entries may be unset and later replaced. Validate only the
        // effective map, matching generated parsing then serializer validation.
        let entries = entries
            .into_iter()
            .map(|(key, value)| Ok((key, value.ok_or(Error::MissingValue)?)))
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        Ok(Self {
            entries,
            original: Some(bytes.to_vec()),
            unknown: parser.unknown,
        })
    }

    /// Encode exact original bytes if untouched; otherwise canonical map key
    /// order and preserved string-set encounter order. Original storage exposes
    /// key lookup only; set member iteration is observable and is not sorted.
    ///
    /// # Errors
    /// Returns `LimitExceeded` if encoded output exceeds bounded document size.
    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        if let Some(original) = &self.original {
            return Ok(original.clone());
        }
        encode_entries(&self.entries)
    }

    /// Look up a typed value. Absence remains absence, not a settings default.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.get(key)
    }

    /// Whether mutation is prohibited because unknown fields were observed.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.unknown
    }

    /// Number of effective keys (after last-key-wins decoding).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no keys exist.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Set a typed value, preserving the previous document on validation failure.
    ///
    /// # Errors
    /// Unknown fields prohibit all mutation. Oversized keys/values/maps fail.
    pub fn set(&mut self, key: String, value: Value) -> Result<(), Error> {
        self.mutate(|entries| {
            entries.insert(key, value);
        })
    }

    /// Remove a key. Even no-op mutation is refused for unknown-field documents.
    ///
    /// # Errors
    /// Returns `UnknownFields` for a read-only document.
    pub fn remove(&mut self, key: &str) -> Result<(), Error> {
        self.mutate(|entries| {
            entries.remove(key);
        })
    }

    fn mutate(&mut self, change: impl FnOnce(&mut BTreeMap<String, Value>)) -> Result<(), Error> {
        if self.unknown {
            return Err(Error::UnknownFields);
        }
        let mut entries = self.entries.clone();
        change(&mut entries);
        encode_entries(&entries)?;
        self.entries = entries;
        self.original = None;
        Ok(())
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.checked_add(count).ok_or(Error::InvalidWire)?;
        let bytes = self.bytes.get(self.offset..end).ok_or(Error::Truncated)?;
        self.offset = end;
        Ok(bytes)
    }
    fn varint(&mut self) -> Result<u64, Error> {
        let mut value = 0;
        for shift in (0..70).step_by(7) {
            let byte = self.take(1)?[0];
            if shift == 63 && byte > 1 {
                return Err(Error::InvalidWire);
            }
            value |= u64::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return Ok(value);
            }
        }
        Err(Error::InvalidWire)
    }
    fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let len = usize::try_from(self.varint()?).map_err(|_| Error::InvalidWire)?;
        self.take(len)
    }
    fn string(&mut self) -> Result<String, Error> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| Error::InvalidUtf8)
    }
    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        self.take(N)?.try_into().map_err(|_| Error::Truncated)
    }
}
struct Parser {
    fields: usize,
    unknown: bool,
}
impl Parser {
    fn tag(&mut self, cursor: &mut Cursor<'_>) -> Result<Option<u32>, Error> {
        if cursor.offset == cursor.bytes.len() {
            return Ok(None);
        }
        self.fields += 1;
        if self.fields > MAX_FIELDS {
            return Err(Error::LimitExceeded);
        }
        let tag = u32::try_from(cursor.varint()?).map_err(|_| Error::InvalidWire)?;
        if tag >> 3 == 0 || tag & 7 > 5 {
            return Err(Error::InvalidWire);
        }
        Ok(Some(tag))
    }
    fn skip(&mut self, cursor: &mut Cursor<'_>, tag: u32, depth: usize) -> Result<(), Error> {
        self.unknown = true;
        match tag & 7 {
            0 => {
                cursor.varint()?;
            }
            1 => {
                cursor.take(8)?;
            }
            2 => {
                cursor.bytes()?;
            }
            3 => {
                if depth >= MAX_GROUP_DEPTH {
                    return Err(Error::LimitExceeded);
                }
                loop {
                    let nested = self.tag(cursor)?.ok_or(Error::Truncated)?;
                    if nested & 7 == 4 {
                        if nested >> 3 != tag >> 3 {
                            return Err(Error::InvalidWire);
                        }
                        break;
                    }
                    self.skip(cursor, nested, depth + 1)?;
                }
            }
            5 => {
                cursor.take(4)?;
            }
            _ => return Err(Error::InvalidWire),
        }
        Ok(())
    }
    fn entry(&mut self, bytes: &[u8]) -> Result<(String, Option<Value>), Error> {
        let mut cursor = Cursor::new(bytes);
        let mut key = String::new();
        let mut value = None;
        while let Some(tag) = self.tag(&mut cursor)? {
            match tag {
                10 => key = cursor.string()?,
                // Generated fast map decoder replaces each duplicate value field.
                18 => value = self.value(cursor.bytes()?)?,
                _ => self.skip(&mut cursor, tag, 0)?,
            }
        }
        Ok((key, value))
    }
    fn value(&mut self, bytes: &[u8]) -> Result<Option<Value>, Error> {
        let mut cursor = Cursor::new(bytes);
        let mut value = None;
        let mut set_count = 0;
        while let Some(tag) = self.tag(&mut cursor)? {
            let next = match tag {
                8 => Value::Boolean(cursor.varint()? != 0),
                21 => Value::FloatBits(u32::from_le_bytes(cursor.fixed()?)),
                24 => {
                    let raw = cursor.varint()?.to_le_bytes();
                    Value::Integer(i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
                }
                32 => Value::Long(i64::from_le_bytes(cursor.varint()?.to_le_bytes())),
                42 => Value::String(cursor.string()?),
                50 => {
                    let mut set = if let Some(Value::StringSet(set)) = value.take() {
                        set
                    } else {
                        set_count = 0;
                        PreferenceSet::default()
                    };
                    self.set(cursor.bytes()?, &mut set, &mut set_count)?;
                    Value::StringSet(set)
                }
                57 => Value::DoubleBits(u64::from_le_bytes(cursor.fixed()?)),
                66 => Value::Bytes(cursor.bytes()?.to_vec()),
                _ => {
                    self.skip(&mut cursor, tag, 0)?;
                    continue;
                }
            };
            value = Some(next);
        }
        Ok(value)
    }
    fn set(
        &mut self,
        bytes: &[u8],
        set: &mut PreferenceSet,
        count: &mut usize,
    ) -> Result<(), Error> {
        let mut cursor = Cursor::new(bytes);
        while let Some(tag) = self.tag(&mut cursor)? {
            if tag == 10 {
                *count += 1;
                if *count > MAX_SET_STRINGS {
                    return Err(Error::LimitExceeded);
                }
                set.insert(cursor.string()?)?;
            } else {
                self.skip(&mut cursor, tag, 0)?;
            }
        }
        Ok(())
    }
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        out.push((value.to_le_bytes()[0] & 127) | 128);
        value >>= 7;
    }
    out.push(value.to_le_bytes()[0]);
}
fn delimited(out: &mut Vec<u8>, tag: u8, bytes: &[u8]) -> Result<(), Error> {
    let length = u64::try_from(bytes.len()).map_err(|_| Error::LimitExceeded)?;
    let mut prefix = vec![tag];
    varint(&mut prefix, length);
    if out
        .len()
        .saturating_add(bytes.len())
        .saturating_add(prefix.len())
        > MAX_BYTES
    {
        return Err(Error::LimitExceeded);
    }
    out.extend_from_slice(&prefix);
    out.extend_from_slice(bytes);
    Ok(())
}
fn encode_value(value: &Value) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    match value {
        Value::Boolean(v) => {
            out.push(8);
            varint(&mut out, u64::from(*v));
        }
        Value::FloatBits(v) => {
            out.push(21);
            out.extend_from_slice(&v.to_le_bytes());
        }
        Value::Integer(v) => {
            out.push(24);
            varint(&mut out, u64::from_le_bytes(i64::from(*v).to_le_bytes()));
        }
        Value::Long(v) => {
            out.push(32);
            varint(&mut out, u64::from_le_bytes(v.to_le_bytes()));
        }
        Value::String(v) => delimited(&mut out, 42, v.as_bytes())?,
        Value::StringSet(set) => {
            if set.len() > MAX_SET_STRINGS {
                return Err(Error::LimitExceeded);
            }
            let mut strings = Vec::new();
            for string in set.iter() {
                delimited(&mut strings, 10, string.as_bytes())?;
            }
            delimited(&mut out, 50, &strings)?;
        }
        Value::DoubleBits(v) => {
            out.push(57);
            out.extend_from_slice(&v.to_le_bytes());
        }
        Value::Bytes(v) => delimited(&mut out, 66, v)?,
    }
    Ok(out)
}
fn encode_entries(entries: &BTreeMap<String, Value>) -> Result<Vec<u8>, Error> {
    if entries.len() > MAX_ENTRIES {
        return Err(Error::LimitExceeded);
    }
    let mut out = Vec::new();
    let mut fields = 0;
    for (key, value) in entries {
        fields += 4 + match value {
            Value::StringSet(set) => set.len(),
            _ => 0,
        };
        if fields > MAX_FIELDS {
            return Err(Error::LimitExceeded);
        }
        let mut entry = Vec::new();
        delimited(&mut entry, 10, key.as_bytes())?;
        delimited(&mut entry, 18, &encode_value(value)?)?;
        delimited(&mut out, 10, &entry)?;
    }
    Ok(out)
}

/// Rename has committed new bytes; this outcome is never a precommit error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// File and containing directory sync both succeeded on this filesystem.
    Synced,
    /// New file committed, but directory sync was unavailable or failed.
    DirectorySyncUnavailable,
}

/// Restricted current-snapshot editor. Cannot accept a replacement snapshot.
///
/// Only runtime constructs this editor; stale copied documents cannot be
/// assigned through it. Methods apply to runtime's current owned snapshot.
pub struct FixtureEdit<'a> {
    preferences: &'a mut Preferences,
}
impl fmt::Debug for FixtureEdit<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FixtureEdit([redacted])")
    }
}
impl FixtureEdit<'_> {
    /// Look up current owned value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.preferences.get(key)
    }

    /// Set current value; no whole-snapshot replacement is exposed.
    ///
    /// # Errors
    /// Returns validation or unknown-field errors without partial mutation.
    pub fn set(&mut self, key: String, value: Value) -> Result<(), Error> {
        self.preferences.set(key, value)
    }

    /// Remove current key.
    ///
    /// # Errors
    /// Returns `UnknownFields` if document is read-only.
    pub fn remove(&mut self, key: &str) -> Result<(), Error> {
        self.preferences.remove(key)
    }
}

/// Single non-cloneable owner of caller-supplied synthetic directory.
///
/// All participating writers must hold this same stable lock-file inode. Caller
/// must not replace/unlink lock file, rename directory, or allow untrusted path
/// manipulation. Lock handle is never cloned, and kernel releases it on crash.
/// Runtime holds no live-app paths and does not coordinate with JVM `DataStore`.
/// Each update starts from owned current snapshot, not caller's stale document.
pub struct FixtureStore {
    directory: PathBuf,
    _ownership: Ownership,
    baseline: Option<Vec<u8>>,
    preferences: Preferences,
}
impl fmt::Debug for FixtureStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FixtureStore([redacted])")
    }
}
struct Ownership(File);
impl Drop for Ownership {
    fn drop(&mut self) {
        // Release now, not when an unrelated concurrently spawning process closes
        // its transient fork-inherited descriptor on exec. File close remains
        // fallback if unlock fails; abrupt process death is kernel-released.
        let _ = self.0.unlock();
    }
}
impl FixtureStore {
    /// Open an existing private fixture directory and acquire nonblocking lock.
    /// Missing data file yields empty preferences without creating data file.
    ///
    /// # Errors
    /// Android returns `LockUnsupported` without filesystem writes. Other hosts
    /// return lock, filesystem, or decode errors; corrupt file is not overwritten.
    pub fn open(directory: &Path) -> Result<Self, Error> {
        if cfg!(target_os = "android") {
            return Err(Error::LockUnsupported);
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let lock = options
            .open(directory.join(LOCK_FILE))
            .map_err(|_| Error::Io)?;
        lock.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => Error::Owned,
            TryLockError::Error(error) if error.kind() == std::io::ErrorKind::Unsupported => {
                Error::LockUnsupported
            }
            TryLockError::Error(_) => Error::Io,
        })?;
        // Guard also releases on load/decode failure before a store exists.
        let ownership = Ownership(lock);
        let baseline = read_optional(&directory.join(DATA_FILE))?;
        let preferences = Preferences::decode(baseline.as_deref().unwrap_or_default())?;
        Ok(Self {
            directory: directory.to_path_buf(),
            _ownership: ownership,
            baseline,
            preferences,
        })
    }

    /// Owned current snapshot. Cannot be replaced by an externally stale snapshot.
    #[must_use]
    pub const fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    /// Mutate current snapshot and atomically commit using same-directory temp.
    /// Callback failure never changes in-memory or persisted baseline.
    ///
    /// # Errors
    /// Returns precommit errors for observed drift, corruption, validation,
    /// callback failure, or filesystem failure. Runtime never overwrites target
    /// on these errors (an external writer may already have changed it).
    /// After rename, baseline updates immediately; directory-sync failure is an
    /// explicit committed outcome, not an error or instruction to replay update.
    pub fn update(
        &mut self,
        change: impl FnOnce(&mut FixtureEdit<'_>) -> Result<(), Error>,
    ) -> Result<CommitOutcome, Error> {
        let path = self.directory.join(DATA_FILE);
        if read_optional(&path)? != self.baseline {
            return Err(Error::ExternalChange);
        }
        if self.preferences.is_read_only() {
            return Err(Error::UnknownFields);
        }
        let mut next = self.preferences.clone();
        change(&mut FixtureEdit {
            preferences: &mut next,
        })?;
        let bytes = next.encode()?;
        let mut temp = Temporary::create(&self.directory)?;
        #[cfg(test)]
        tests::fault(tests::Fault::Write)?;
        temp.file.write_all(&bytes).map_err(|_| Error::Io)?;
        #[cfg(test)]
        tests::fault(tests::Fault::FileSync)?;
        temp.file.sync_all().map_err(|_| Error::Io)?;
        // Recheck immediately before commit; only cooperating writers are atomic.
        if read_optional(&path)? != self.baseline {
            return Err(Error::ExternalChange);
        }
        #[cfg(test)]
        tests::fault(tests::Fault::Rename)?;
        fs::rename(&temp.path, &path).map_err(|_| Error::Io)?;
        self.baseline = Some(bytes);
        self.preferences = next;
        Ok(sync_directory(&self.directory))
    }
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::Io),
    };
    if !file.metadata().map_err(|_| Error::Io)?.is_file() {
        return Err(Error::Io);
    }
    let mut bytes = Vec::new();
    file.take(u64::try_from(MAX_BYTES + 1).map_err(|_| Error::LimitExceeded)?)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Io)?;
    if bytes.len() > MAX_BYTES {
        return Err(Error::LimitExceeded);
    }
    Ok(Some(bytes))
}
fn sync_directory(path: &Path) -> CommitOutcome {
    #[cfg(test)]
    if tests::fault(tests::Fault::DirectorySync).is_err() {
        return CommitOutcome::DirectorySyncUnavailable;
    }
    if File::open(path).and_then(|file| file.sync_all()).is_ok() {
        CommitOutcome::Synced
    } else {
        CommitOutcome::DirectorySyncUnavailable
    }
}
struct Temporary {
    path: PathBuf,
    file: File,
}
impl Temporary {
    fn create(directory: &Path) -> Result<Self, Error> {
        Self::create_from(directory, TEMP_SEQUENCE.fetch_add(32, Ordering::Relaxed))
    }
    fn create_from(directory: &Path, start: u64) -> Result<Self, Error> {
        // Bounded retries; never adopt/delete another process's stale temp file.
        for offset in 0..32 {
            let sequence = start.wrapping_add(offset);
            let path = directory.join(format!(
                ".preferences-{}-{sequence}.tmp",
                std::process::id()
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(_) => return Err(Error::Io),
            }
        }
        Err(Error::Io)
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Clone, Copy, Eq, PartialEq)]
    pub(super) enum Fault {
        Write,
        FileSync,
        Rename,
        DirectorySync,
    }
    thread_local! { static FAILURE: Cell<Option<Fault>> = const { Cell::new(None) }; }
    pub(super) fn fault(point: Fault) -> Result<(), Error> {
        FAILURE.with(|failure| {
            if failure.get() == Some(point) {
                failure.set(None);
                Err(Error::Io)
            } else {
                Ok(())
            }
        })
    }
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            let root = option_env!("CARGO_MANIFEST_DIR").map_or_else(
                || PathBuf::from("native/parity/preferences/build"),
                |manifest| Path::new(manifest).join("parity/preferences/build"),
            );
            let path = root.join(format!(
                "unit-{}-{}",
                std::process::id(),
                TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn injected_write_sync_rename_errors_preserve_real_reopened_file() {
        for point in [Fault::Write, Fault::FileSync, Fault::Rename] {
            let directory = Directory::new();
            let mut store = FixtureStore::open(&directory.0).unwrap();
            store
                .update(|doc| doc.set("x".to_owned(), Value::Integer(1)))
                .unwrap();
            let original = fs::read(directory.0.join(DATA_FILE)).unwrap();
            FAILURE.with(|failure| failure.set(Some(point)));
            assert_eq!(
                store.update(|doc| doc.set("x".to_owned(), Value::Integer(2))),
                Err(Error::Io)
            );
            assert_eq!(store.preferences().get("x"), Some(&Value::Integer(1)));
            assert_eq!(fs::read(directory.0.join(DATA_FILE)).unwrap(), original);
            assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
            drop(store);
            assert_eq!(
                FixtureStore::open(&directory.0)
                    .unwrap()
                    .preferences()
                    .get("x"),
                Some(&Value::Integer(1))
            );
        }
    }

    #[test]
    fn committed_directory_sync_failure_updates_baseline_before_retry() {
        let directory = Directory::new();
        let mut store = FixtureStore::open(&directory.0).unwrap();
        FAILURE.with(|failure| failure.set(Some(Fault::DirectorySync)));
        assert_eq!(
            store.update(|doc| doc.set("x".to_owned(), Value::Integer(1))),
            Ok(CommitOutcome::DirectorySyncUnavailable)
        );
        assert_eq!(store.preferences().get("x"), Some(&Value::Integer(1)));
        // Next operation observes committed state, not a replay of old state.
        assert_eq!(
            store.update(|doc| {
                assert_eq!(doc.get("x"), Some(&Value::Integer(1)));
                doc.set("x".to_owned(), Value::Integer(2))
            }),
            Ok(CommitOutcome::Synced)
        );
        drop(store);
        assert_eq!(
            FixtureStore::open(&directory.0)
                .unwrap()
                .preferences()
                .get("x"),
            Some(&Value::Integer(2))
        );
        assert_eq!(
            sync_directory(&directory.0.join("absent")),
            CommitOutcome::DirectorySyncUnavailable
        );
    }

    #[test]
    fn encoder_entry_field_bounds_and_private_validation_fail_closed() {
        let mut entries = BTreeMap::new();
        for index in 0..=MAX_ENTRIES {
            entries.insert(index.to_string(), Value::Boolean(false));
        }
        assert_eq!(encode_entries(&entries), Err(Error::LimitExceeded));
        let strings =
            PreferenceSet::from_strings((0..MAX_SET_STRINGS).map(|i| i.to_string())).unwrap();
        entries.clear();
        for index in 0..17 {
            entries.insert(index.to_string(), Value::StringSet(strings.clone()));
        }
        assert_eq!(encode_entries(&entries), Err(Error::LimitExceeded));
        let mut out = vec![0; MAX_BYTES];
        assert_eq!(delimited(&mut out, 42, b""), Err(Error::LimitExceeded));
        assert_eq!(Cursor::new(&[]).take(usize::MAX), Err(Error::Truncated));
        let mut cursor = Cursor::new(&[1]);
        cursor.take(1).unwrap();
        assert_eq!(cursor.take(usize::MAX), Err(Error::InvalidWire));
    }

    #[test]
    fn temporary_collision_retries_but_never_removes_existing_file() {
        let directory = Directory::new();
        let sequence = 42;
        let stale = directory.0.join(format!(
            ".preferences-{}-{sequence}.tmp",
            std::process::id()
        ));
        fs::write(&stale, b"synthetic stale temp").unwrap();
        let temporary = Temporary::create_from(&directory.0, sequence).unwrap();
        assert_ne!(temporary.path, stale);
        drop(temporary);
        assert_eq!(fs::read(&stale).unwrap(), b"synthetic stale temp");
        // Private-directory deterministic candidates; safe with parallel tests.
        let start = sequence + 2;
        for sequence in start..start + 32 {
            fs::write(
                directory.0.join(format!(
                    ".preferences-{}-{sequence}.tmp",
                    std::process::id()
                )),
                [],
            )
            .unwrap();
        }
        assert!(matches!(
            Temporary::create_from(&directory.0, start),
            Err(Error::Io)
        ));
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 33);
    }

    #[test]
    fn filesystem_missing_directory_data_permissions_and_read_error() {
        let directory = Directory::new();
        assert!(matches!(
            FixtureStore::open(&directory.0.join("absent")),
            Err(Error::Io)
        ));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let path = directory.0.join(DATA_FILE);
            fs::write(&path, []).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            assert_eq!(read_optional(&path), Err(Error::Io));
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            let mut store = FixtureStore::open(&directory.0).unwrap();
            store
                .update(|doc| doc.set("x".to_owned(), Value::Boolean(false)))
                .unwrap();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(directory.0.join(LOCK_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
