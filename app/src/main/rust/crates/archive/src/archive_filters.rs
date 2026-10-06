//! Seekable nested gzip/XZ views with bounded heap and replay work.
//! Compressed payloads are never buffered wholesale or written to temporary paths.
//! A full first pass checks every outer trailer before exposing the archive view.

use std::cell::{Cell, RefCell};
use std::io::{self, BufReader, Cursor, Read, Seek, SeekFrom};
use std::rc::Rc;

use eh_codec_limits::AllocationBudget;
use flate2::bufread::MultiGzDecoder;
use lzma_rust2::XzReader;

use crate::production_zip::{MAX_DICTIONARY_BYTES, MAX_ENTRIES};

/// Maximum filter nesting depth.
pub const MAX_FILTERS: usize = 8;
/// Maximum decoded container size, independent of member size limits.
pub const MAX_DECODED_BYTES: u64 = 64 * 1024 * 1024 * 1024;
/// Maximum repeated decompression work per view operation.
pub const MAX_REPLAY_BYTES: u64 = 256 * 1024 * 1024 * 1024;
const BUFFER: usize = 64 * 1024;

#[derive(Clone, Copy)]
enum Filter {
    Gzip,
    Xz,
}

struct Shared<R>(Rc<RefCell<R>>);
impl<R: Read> Read for Shared<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.borrow_mut().read(bytes)
    }
}

/// Owning seekable decompression view; backward seeks replay with fixed buffers.
pub struct DecodedSource<'a, R> {
    source: Rc<RefCell<R>>,
    reader: Box<dyn Read + 'a>,
    filters: Vec<Filter>,
    position: u64,
    length: u64,
    work: Rc<Cell<u64>>,
    budget: AllocationBudget,
}

impl<'a, R: Read + Seek + 'a> DecodedSource<'a, R> {
    /// Decode/validate outer streams and identify nested filters by content.
    ///
    /// # Errors
    /// Rejects corrupt trailers, excessive nesting, dictionaries, decoded size or work.
    pub fn new(mut source: R) -> io::Result<Self> {
        source.seek(SeekFrom::Start(0))?;
        let source = Rc::new(RefCell::new(source));
        let budget = AllocationBudget::new(MAX_DICTIONARY_BYTES);
        let work = Rc::new(Cell::new(0));
        let (reader, filters) = budget.run(|| {
            eh_codec_limits::with_decoder_limits(MAX_DICTIONARY_BYTES, MAX_ENTRIES, || {
                let mut reader: Box<dyn Read + 'a> = Box::new(Shared(source.clone()));
                let mut filters = Vec::new();
                loop {
                    let mut prefix = Vec::with_capacity(6);
                    reader.by_ref().take(6).read_to_end(&mut prefix)?;
                    let buffered =
                        BufReader::with_capacity(BUFFER, Cursor::new(prefix.clone()).chain(reader));
                    let filter = if prefix.starts_with(b"\x1f\x8b") {
                        Some(Filter::Gzip)
                    } else if prefix.starts_with(b"\xfd7zXZ\0") {
                        Some(Filter::Xz)
                    } else {
                        None
                    };
                    let Some(filter) = filter else {
                        return Ok((Box::new(buffered) as Box<dyn Read>, filters));
                    };
                    if filters.len() == MAX_FILTERS {
                        return Err(limit("archive filter depth exceeds limit"));
                    }
                    filters.push(filter);
                    reader = wrap_filter(filter, buffered, work.clone());
                }
            })
        })?;
        let mut view = Self {
            source,
            reader,
            filters,
            position: 0,
            length: 0,
            work,
            budget,
        };
        if view.filters.is_empty() {
            view.length = view.source.borrow_mut().seek(SeekFrom::End(0))?;
        } else {
            let mut buffer = vec![0; BUFFER];
            while view.read(&mut buffer)? != 0 {}
            view.length = view.position;
        }
        view.rewind()?;
        Ok(view)
    }

    /// Shared credit covers both outer filters and the inner ZIP decoder.
    #[must_use]
    pub fn allocation_budget(&self) -> AllocationBudget {
        self.budget.clone()
    }

    fn rewind(&mut self) -> io::Result<()> {
        // Drop dictionaries before constructing replacements, retaining quota accounting.
        self.reader = Box::new(io::empty());
        self.source.borrow_mut().seek(SeekFrom::Start(0))?;
        let mut reader: Box<dyn Read + 'a> = Box::new(Shared(self.source.clone()));
        for filter in &self.filters {
            let buffered = BufReader::with_capacity(BUFFER, reader);
            reader = wrap_filter(*filter, buffered, self.work.clone());
        }
        self.reader = reader;
        self.position = 0;
        Ok(())
    }

    fn move_to(&mut self, target: u64) -> io::Result<u64> {
        if target > self.length {
            return Err(invalid("archive seek exceeds decoded source"));
        }
        if self.filters.is_empty() {
            self.reader = Box::new(io::empty());
            self.source.borrow_mut().seek(SeekFrom::Start(target))?;
            self.reader = Box::new(Shared(self.source.clone()));
            self.position = target;
            return Ok(target);
        }
        if target < self.position {
            self.rewind()?;
        }
        let mut buffer = vec![0; BUFFER];
        while self.position < target {
            let amount = usize::try_from((target - self.position).min(BUFFER as u64))
                .expect("bounded copy count");
            if self.read(&mut buffer[..amount])? == 0 {
                return Err(invalid("archive replay ended early"));
            }
        }
        Ok(self.position)
    }
}

impl<R: Read + Seek> Read for DecodedSource<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.budget.run(|| {
            eh_codec_limits::with_decoder_limits(MAX_DICTIONARY_BYTES, MAX_ENTRIES, || {
                self.reader.read(bytes)
            })
        })?;
        self.position = self
            .position
            .checked_add(count as u64)
            .ok_or_else(|| limit("archive decoded length overflow"))?;
        if !self.filters.is_empty()
            && (self.position > MAX_DECODED_BYTES || self.work.get() > MAX_REPLAY_BYTES)
        {
            return Err(limit("archive decoded size or replay work exceeds limit"));
        }
        Ok(count)
    }
}

impl<'a, R: Read + Seek + 'a> Seek for DecodedSource<'a, R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(delta) => self.position.checked_add_signed(delta),
            SeekFrom::End(delta) => self.length.checked_add_signed(delta),
        }
        .ok_or_else(|| invalid("archive seek arithmetic exceeds range"))?;
        self.move_to(target)
    }
}

struct Layer<'a> {
    reader: Box<dyn Read + 'a>,
    decoded: u64,
    work: Rc<Cell<u64>>,
}
impl Read for Layer<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = self.reader.read(bytes)?;
        self.decoded = self
            .decoded
            .checked_add(count as u64)
            .ok_or_else(|| limit("archive layer length overflow"))?;
        self.work.set(
            self.work
                .get()
                .checked_add(count as u64)
                .ok_or_else(|| limit("archive replay count overflow"))?,
        );
        if self.decoded > MAX_DECODED_BYTES || self.work.get() > MAX_REPLAY_BYTES {
            return Err(limit("archive layer size or replay work exceeds limit"));
        }
        Ok(count)
    }
}
fn wrap_filter<'a, R: Read + 'a>(
    filter: Filter,
    input: BufReader<R>,
    work: Rc<Cell<u64>>,
) -> Box<dyn Read + 'a> {
    let reader: Box<dyn Read + 'a> = match filter {
        Filter::Gzip => Box::new(MultiGzDecoder::new(input)),
        Filter::Xz => Box::new(XzReader::new(input, true)),
    };
    Box::new(Layer {
        reader,
        decoded: 0,
        work,
    })
}

fn limit(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, message)
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests;
