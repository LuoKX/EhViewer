//! Bounded TAR indexing and extraction: V7/USTAR, GNU, PAX and sparse 0.x/1.0.
//! Header helpers come from tar-rs; no paths are unpacked and no links are followed.

use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom, Write};
use tar::{GnuExtSparseHeader, GnuSparseHeader, Header};
use thiserror::Error;

const BLOCK: u64 = 512;
/// Maximum visible TAR members and sparse map records.
pub const MAX_ENTRIES: usize = 100_000;
/// Maximum one extended metadata record or in-band sparse map.
pub const MAX_EXTENSION_BYTES: u64 = 512 * 1024;
/// Maximum accumulated extended metadata, names and sparse-map storage.
pub const MAX_METADATA_BYTES: u64 = 32 * 1024 * 1024;
const MAX_HEADERS: usize = 1_000_000;
const BUFFER: usize = 64 * 1024;

/// Physical TAR member identity and optional sparse data layout.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TarMember {
    /// Stable visible-member position; extension records do not become pages.
    pub index: usize,
    /// UTF-8 pathname with GNU/PAX overrides and C-style NUL termination.
    pub name: String,
    /// Logical decoded size, including sparse holes.
    pub size: u64,
    /// Regular or contiguous file, not a directory/link/device.
    pub regular: bool,
    data_offset: u64,
    stored_size: u64,
    sparse: Option<Vec<(u64, u64)>>,
    inplace_sparse: bool,
}

/// TAR structure, I/O or resource failure; no member names enter diagnostics.
#[derive(Debug, Error)]
pub enum TarError {
    /// Reading, seeking, truncation or writing failed.
    #[error("TAR source or output operation failed")]
    Io(#[from] io::Error),
    /// Header, metadata, sparse layout or length is invalid.
    #[error("TAR structure or sparse layout is invalid")]
    Invalid,
    /// Resource bounds would be exceeded before allocation/copy.
    #[error("TAR resource limit exceeded")]
    Limit,
    /// Unrecognized sparse format cannot be treated as an image payload.
    #[error("TAR sparse format is unsupported")]
    UnsupportedSparse,
    /// Header identity no longer matches the scanned view.
    #[error("TAR member changed between scan and extraction")]
    ChangedMember,
}

/// Recognize a complete checksummed TAR header, including signed legacy checksums.
#[must_use]
pub fn recognizes_header(bytes: &[u8]) -> bool {
    if bytes.len() != 512 {
        return false;
    }
    let header = Header::from_byte_slice(bytes);
    let Ok(checksum) = header.cksum() else {
        return false;
    };
    let data = bytes[..148].iter().chain(&bytes[156..]);
    let unsigned = data
        .clone()
        .fold(256_u32, |sum, byte| sum + u32::from(*byte));
    let signed = data.fold(256_i64, |sum, byte| {
        sum + i64::from(i8::from_ne_bytes([*byte]))
    });
    checksum == unsigned || i64::from(checksum) == signed
}

/// Inspect extended and sparse metadata without retaining member payloads.
///
/// # Errors
/// Rejects invalid checksums, truncated bodies, metadata limits and sparse maps.
pub fn scan<R: Read + Seek>(source: &mut R) -> Result<Vec<TarMember>, TarError> {
    with_position(source, |source| scan_inner(source))
}

/// Copy one member, reconstructing holes with a fixed zero buffer.
///
/// # Errors
/// Rejects changed identity, oversized output, corrupt/truncated data and output failure.
pub fn extract<R: Read + Seek, W: Write + ?Sized>(
    source: &mut R,
    member: &TarMember,
    output: &mut W,
) -> Result<u64, TarError> {
    with_position(source, |source| {
        if !member.regular || member.size > crate::production_zip::MAX_MEMBER_BYTES {
            return Err(TarError::Limit);
        }
        let members = scan_inner(source)?;
        if members.get(member.index) != Some(member) {
            return Err(TarError::ChangedMember);
        }
        source.seek(SeekFrom::Start(member.data_offset))?;
        if let Some(map) = &member.sparse {
            let zeros = vec![0; BUFFER];
            let mut position = 0;
            for &(offset, size) in map {
                fill_zeros(output, offset - position, &zeros)?;
                if member.inplace_sparse {
                    source.seek(SeekFrom::Start(member.data_offset + offset))?;
                }
                copy_bytes(source, output, size)?;
                position = offset + size;
            }
            fill_zeros(output, member.size - position, &zeros)?;
        } else {
            copy_bytes(source, output, member.size)?;
        }
        Ok(member.size)
    })
}

#[derive(Default)]
struct Extensions {
    global: BTreeMap<String, Vec<u8>>,
    local: Vec<(String, Vec<u8>)>,
    long_name: Option<Vec<u8>>,
    pending: bool,
}
impl Extensions {
    fn value(&self, key: &str) -> Option<&[u8]> {
        self.local
            .iter()
            .rev()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_slice())
            .or_else(|| self.global.get(key).map(Vec::as_slice))
            .filter(|value| !value.is_empty())
    }
    fn number(&self, key: &str) -> Result<Option<u64>, TarError> {
        self.value(key).map(decimal).transpose()
    }
    fn apply(&mut self, tag: u8, data: Vec<u8>, metadata: &mut u64) -> Result<(), TarError> {
        match tag {
            b'L' => {
                self.long_name = Some(data);
                self.pending = true;
            }
            b'K' => self.pending = true,
            b'x' | b'X' => {
                self.local.extend(pax_records(&data, metadata)?);
                self.pending = true;
            }
            b'g' => {
                for (key, value) in pax_records(&data, metadata)? {
                    if value.is_empty() {
                        self.global.remove(&key);
                    } else {
                        self.global.insert(key, value);
                    }
                }
            }
            _ => unreachable!(),
        }
        Ok(())
    }
    fn stored_size(&self) -> Result<Option<u64>, TarError> {
        if self.number("GNU.sparse.major")? == Some(1)
            && let Some(size) = self.number("GNU.sparse.size")?
        {
            return Ok(Some(size));
        }
        self.number("size")
    }
    fn reset_local(&mut self) {
        self.local.clear();
        self.long_name = None;
        self.pending = false;
    }
}

fn scan_inner<R: Read + Seek>(source: &mut R) -> Result<Vec<TarMember>, TarError> {
    let length = source.seek(SeekFrom::End(0))?;
    source.seek(SeekFrom::Start(0))?;
    let mut members = Vec::new();
    let mut metadata_bytes = 0;
    let mut extensions = Extensions::default();
    for _ in 0..MAX_HEADERS {
        let position = source.stream_position()?;
        if position == length {
            if extensions.pending {
                return Err(TarError::Invalid);
            }
            return Ok(members);
        }
        let mut bytes = [0; 512];
        source.read_exact(&mut bytes)?;
        if bytes.iter().all(|byte| *byte == 0) {
            if extensions.pending {
                return Err(TarError::Invalid);
            }
            return Ok(members);
        }
        if !recognizes_header(&bytes) {
            return Err(TarError::Invalid);
        }
        let header = Header::from_byte_slice(&bytes);
        let tag = bytes[156];
        let extension = matches!(tag, b'x' | b'g' | b'L' | b'K' | b'X');
        let stored = if extension {
            header.entry_size()?
        } else {
            match extensions.stored_size()? {
                Some(size) => size,
                None => header.entry_size()?,
            }
        };
        let mut data_offset = source.stream_position()?;
        if extension {
            account(&mut metadata_bytes, stored)?;
            if stored > MAX_EXTENSION_BYTES {
                return Err(TarError::Limit);
            }
            let end = bounded_end(data_offset, stored, length)?;
            let mut data = vec![0; usize::try_from(stored).map_err(|_| TarError::Limit)?];
            source.read_exact(&mut data)?;
            extensions.apply(tag, data, &mut metadata_bytes)?;
            source.seek(SeekFrom::Start(end))?;
            continue;
        }
        if members.len() == MAX_ENTRIES {
            return Err(TarError::Limit);
        }
        let (logical_size, data_start, sparse, inplace_sparse) = sparse_layout(
            source,
            header,
            tag,
            &extensions,
            stored,
            &mut metadata_bytes,
        )?;
        data_offset = data_start;
        let header_data_start = position + BLOCK;
        // Old-GNU extension blocks precede the declared stored body; PAX-1 map is inside it.
        let body_start = if tag == b'S' {
            data_offset
        } else {
            header_data_start
        };
        let end = bounded_end(body_start, stored, length)?;
        let map_bytes = data_offset - body_start;
        let stored_size = stored.checked_sub(map_bytes).ok_or(TarError::Invalid)?;
        if let Some(map) = &sparse {
            let expected = if inplace_sparse {
                if stored_size != logical_size {
                    return Err(TarError::Invalid);
                }
                map.iter().map(|(_, size)| *size).sum()
            } else {
                stored_size
            };
            validate_map(map, logical_size, expected)?;
            account(&mut metadata_bytes, (map.len() as u64) * 16)?;
        }
        let name = member_name(header, &extensions);
        account(&mut metadata_bytes, name.len() as u64)?;
        members.push(TarMember {
            index: members.len(),
            name,
            size: logical_size,
            regular: matches!(tag, 0 | b'0' | b'7' | b'S'),
            data_offset,
            stored_size,
            sparse,
            inplace_sparse,
        });
        extensions.reset_local();
        source.seek(SeekFrom::Start(end))?;
    }
    Err(TarError::Limit)
}

fn member_name(header: &Header, extensions: &Extensions) -> String {
    if let Some(path) = extensions
        .value("GNU.sparse.name")
        .or(extensions.value("path"))
        .or(extensions.long_name.as_deref())
    {
        return String::from_utf8_lossy(path.split(|byte| *byte == 0).next().unwrap_or_default())
            .into_owned();
    }
    let bytes = header.as_bytes();
    let filename = bytes[..100]
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default();
    let mut path = Vec::new();
    if header.as_gnu().is_none() && bytes[257..262] == *b"ustar" {
        let prefix = bytes[345..500]
            .split(|byte| *byte == 0)
            .next()
            .unwrap_or_default();
        if !prefix.is_empty() {
            path.extend(prefix);
            path.push(b'/');
        }
    }
    path.extend(filename);
    String::from_utf8_lossy(&path).into_owned()
}

type SparseLayout = (u64, u64, Option<Vec<(u64, u64)>>, bool);
fn sparse_layout<R: Read + Seek>(
    source: &mut R,
    header: &Header,
    tag: u8,
    extensions: &Extensions,
    stored: u64,
    metadata_bytes: &mut u64,
) -> Result<SparseLayout, TarError> {
    let mut sparse = None;
    let mut inplace_sparse = false;
    let mut data_offset = source.stream_position()?;
    let mut logical_size = stored;
    if tag == b'S' {
        let gnu = header.as_gnu().ok_or(TarError::Invalid)?;
        logical_size = gnu.real_size()?;
        let mut map = Vec::new();
        append_gnu_map(&mut map, &gnu.sparse)?;
        let mut extended = gnu.is_extended();
        while extended {
            account(metadata_bytes, BLOCK)?;
            let mut extra = GnuExtSparseHeader::new();
            source.read_exact(extra.as_mut_bytes())?;
            append_gnu_map(&mut map, extra.sparse())?;
            extended = extra.is_extended();
        }
        data_offset = source.stream_position()?;
        sparse = Some(map);
    } else if extensions.value("GNU.sparse.size").is_some()
        || extensions.value("GNU.sparse.realsize").is_some()
        || extensions.value("GNU.sparse.major").is_some()
    {
        logical_size = extensions
            .number("GNU.sparse.realsize")?
            .or(extensions.number("GNU.sparse.size")?)
            .ok_or(TarError::Invalid)?;
        let major = extensions.number("GNU.sparse.major")?.unwrap_or(0);
        let minor = extensions.number("GNU.sparse.minor")?.unwrap_or(0);
        let map = match (major, minor) {
            (1, 0) => {
                let start = source.stream_position()?;
                let count = read_decimal_line(source, start)?;
                if count > MAX_ENTRIES as u64 {
                    return Err(TarError::Limit);
                }
                let mut map = Vec::new();
                for _ in 0..count {
                    let offset = read_decimal_line(source, start)?;
                    let size = read_decimal_line(source, start)?;
                    map.push((offset, size));
                }
                let consumed = source.stream_position()? - start;
                let padded = padded(consumed)?;
                if padded > stored {
                    return Err(TarError::Invalid);
                }
                account(metadata_bytes, padded)?;
                let mut padding =
                    vec![0; usize::try_from(padded - consumed).map_err(|_| TarError::Limit)?];
                source.read_exact(&mut padding)?;
                if padding.iter().any(|byte| *byte != 0) {
                    return Err(TarError::Invalid);
                }
                data_offset = source.stream_position()?;
                map
            }
            (0, 0 | 1) => pax_sparse_map(extensions)?,
            _ => return Err(TarError::UnsupportedSparse),
        };
        sparse = Some(map);
    }
    if let Some(holes) = extensions.value("SUN.holesdata") {
        if sparse.is_some() {
            return Err(TarError::Invalid);
        }
        logical_size = extensions.number("SCHILY.realsize")?.unwrap_or(stored);
        sparse = Some(sun_sparse_map(holes, logical_size)?);
        inplace_sparse = true;
    } else if sparse.is_none()
        && let Some(real) = extensions.number("SCHILY.realsize")?
    {
        logical_size = real;
        sparse = Some(vec![(0, stored)]);
    }
    Ok((logical_size, data_offset, sparse, inplace_sparse))
}

fn sun_sparse_map(value: &[u8], logical: u64) -> Result<Vec<(u64, u64)>, TarError> {
    let rest = value.strip_prefix(b" ").ok_or(TarError::Invalid)?;
    let mut map = Vec::new();
    let mut previous = 0;
    for (index, number) in rest.split(|byte| *byte == b' ').enumerate() {
        if index >= MAX_ENTRIES * 2 {
            return Err(TarError::Limit);
        }
        let end = decimal(number)?;
        if end < previous || end > logical {
            return Err(TarError::Invalid);
        }
        if index % 2 == 1 && end > previous {
            map.push((previous, end - previous));
        }
        previous = end;
    }
    if previous != logical {
        return Err(TarError::Invalid);
    }
    Ok(map)
}

fn pax_records(data: &[u8], metadata: &mut u64) -> Result<Vec<(String, Vec<u8>)>, TarError> {
    let records = parse_pax(data)?;
    account(metadata, (records.len() as u64) * 96)?;
    Ok(records)
}
fn parse_pax(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, TarError> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    while cursor < data.len() {
        let tail = &data[cursor..];
        let split = tail
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or(TarError::Invalid)?;
        let size = usize::try_from(decimal(&tail[..split])?).map_err(|_| TarError::Limit)?;
        if size <= split + 2 || size > tail.len() || tail[size - 1] != b'\n' {
            return Err(TarError::Invalid);
        }
        let pair = &tail[split + 1..size - 1];
        let equal = pair
            .iter()
            .position(|byte| *byte == b'=')
            .ok_or(TarError::Invalid)?;
        if equal == 0 || equal > 1024 {
            return Err(TarError::Invalid);
        }
        let key = std::str::from_utf8(&pair[..equal])
            .map_err(|_| TarError::Invalid)?
            .to_owned();
        if entries.len() == MAX_ENTRIES {
            return Err(TarError::Limit);
        }
        entries.push((key, pair[equal + 1..].to_vec()));
        cursor += size;
    }
    Ok(entries)
}

fn pax_sparse_map(extensions: &Extensions) -> Result<Vec<(u64, u64)>, TarError> {
    let mut map = Vec::new();
    if let Some(values) = extensions.value("GNU.sparse.map") {
        let mut numbers = values.split(|byte| *byte == b',');
        while let Some(offset) = numbers.next() {
            let size = numbers.next().ok_or(TarError::Invalid)?;
            if map.len() == MAX_ENTRIES {
                return Err(TarError::Limit);
            }
            map.push((decimal(offset)?, decimal(size)?));
        }
    } else {
        let mut offset = None;
        for (key, value) in &extensions.local {
            match key.as_str() {
                "GNU.sparse.offset" => {
                    if offset.is_some() {
                        return Err(TarError::Invalid);
                    }
                    offset = Some(decimal(value)?);
                }
                "GNU.sparse.numbytes" => {
                    if map.len() == MAX_ENTRIES {
                        return Err(TarError::Limit);
                    }
                    map.push((offset.take().ok_or(TarError::Invalid)?, decimal(value)?));
                }
                _ => {}
            }
        }
        if offset.is_some() {
            return Err(TarError::Invalid);
        }
    }
    if extensions
        .number("GNU.sparse.numblocks")?
        .is_some_and(|count| count != map.len() as u64)
    {
        return Err(TarError::Invalid);
    }
    Ok(map)
}

fn append_gnu_map(map: &mut Vec<(u64, u64)>, records: &[GnuSparseHeader]) -> Result<(), TarError> {
    for record in records {
        if record.is_empty() {
            continue;
        }
        if map.len() == MAX_ENTRIES {
            return Err(TarError::Limit);
        }
        map.push((record.offset()?, record.length()?));
    }
    Ok(())
}

fn validate_map(map: &[(u64, u64)], logical: u64, stored: u64) -> Result<(), TarError> {
    let mut previous = 0;
    let mut total = 0_u64;
    for &(offset, size) in map {
        let end = offset.checked_add(size).ok_or(TarError::Invalid)?;
        if offset < previous || end > logical {
            return Err(TarError::Invalid);
        }
        total = total.checked_add(size).ok_or(TarError::Invalid)?;
        previous = end;
    }
    if total != stored {
        return Err(TarError::Invalid);
    }
    Ok(())
}

fn read_decimal_line<R: Read + Seek>(source: &mut R, start: u64) -> Result<u64, TarError> {
    let mut bytes = Vec::new();
    loop {
        if bytes.len() == 32
            || source.stream_position()?.saturating_sub(start) >= MAX_EXTENSION_BYTES
        {
            return Err(TarError::Limit);
        }
        let mut byte = [0];
        source.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return decimal(&bytes);
        }
        bytes.push(byte[0]);
    }
}

fn decimal(bytes: &[u8]) -> Result<u64, TarError> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(TarError::Invalid);
    }
    std::str::from_utf8(bytes)
        .map_err(|_| TarError::Invalid)?
        .parse()
        .map_err(|_| TarError::Invalid)
}
fn padded(size: u64) -> Result<u64, TarError> {
    size.checked_add(BLOCK - 1)
        .map(|size| size / BLOCK * BLOCK)
        .ok_or(TarError::Invalid)
}
fn bounded_end(offset: u64, size: u64, length: u64) -> Result<u64, TarError> {
    let end = offset
        .checked_add(padded(size)?)
        .filter(|end| *end <= length)
        .ok_or(TarError::Invalid)?;
    Ok(end)
}
fn account(total: &mut u64, amount: u64) -> Result<(), TarError> {
    *total = total
        .checked_add(amount)
        .filter(|total| *total <= MAX_METADATA_BYTES)
        .ok_or(TarError::Limit)?;
    Ok(())
}
fn copy_bytes<R: Read, W: Write + ?Sized>(
    source: &mut R,
    output: &mut W,
    count: u64,
) -> Result<(), TarError> {
    let copied = io::copy(&mut source.take(count), output)?;
    if copied != count {
        return Err(TarError::Invalid);
    }
    Ok(())
}
fn fill_zeros<W: Write + ?Sized>(
    output: &mut W,
    mut count: u64,
    zeros: &[u8],
) -> Result<(), TarError> {
    while count > 0 {
        let length = usize::try_from(count.min(zeros.len() as u64)).expect("bounded zero count");
        output.write_all(&zeros[..length])?;
        count -= length as u64;
    }
    Ok(())
}
fn with_position<R: Read + Seek, T>(
    source: &mut R,
    operation: impl FnOnce(&mut R) -> Result<T, TarError>,
) -> Result<T, TarError> {
    let position = source.stream_position()?;
    source.seek(SeekFrom::Start(0))?;
    let result = operation(source);
    let restore = source.seek(SeekFrom::Start(position));
    match result {
        Err(error) => Err(error),
        Ok(value) => {
            restore?;
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests;
