//! Opt-in first-party conservative codec allocation budget.
use std::io::Read;
use crate::DecodingError;

pub(crate) fn check<T>(count: usize) -> Result<(), DecodingError> {
    let bytes = count.checked_mul(std::mem::size_of::<T>()).ok_or(DecodingError::MemoryLimitExceeded)?;
    eh_codec_limits::charge_allocation(bytes).map_err(|_| DecodingError::MemoryLimitExceeded)
}
pub(crate) fn capacity<T>(count: usize) -> Result<Vec<T>, DecodingError> {
    check::<T>(count)?;
    let mut data = Vec::new();
    data.try_reserve_exact(count).map_err(|_| DecodingError::MemoryLimitExceeded)?;
    Ok(data)
}
pub(crate) fn filled<T: Clone>(value: T, count: usize) -> Result<Vec<T>, DecodingError> {
    let mut data = capacity(count)?; data.resize(count, value); Ok(data)
}
pub(crate) fn read_to_end<R: Read>(reader: &mut R, data: &mut Vec<u8>) -> Result<(), DecodingError> {
    let mut buffer = [0; 8192];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 { return Ok(()); }
        let size = data.len().checked_add(count).ok_or(DecodingError::MemoryLimitExceeded)?;
        if size > data.capacity() {
            let target = size.max(data.capacity().saturating_mul(2));
            check::<u8>(target - data.capacity())?;
            data.try_reserve_exact(target - data.len()).map_err(|_| DecodingError::MemoryLimitExceeded)?;
        }
        data.extend_from_slice(&buffer[..count]);
    }
}
