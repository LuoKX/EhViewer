//! Read Config

/// Configuration for reading ZIP archives.
#[derive(Debug, Default, Clone, Copy)]
pub struct Config {
    /// An offset into the reader to use to find the start of the archive.
    pub archive_offset: ArchiveOffset,
    /// Preserve every central-directory entry by index, including duplicate names.
    /// Name-based lookup is unavailable in this mode; use `by_index` instead.
    pub indexed_entries: bool,
}

/// The offset of the start of the archive from the beginning of the reader.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveOffset {
    /// Try to detect the archive offset automatically.
    ///
    /// This will look at the central directory specified by `FromCentralDirectory` for a header.
    /// If missing, this will behave as if `None` were specified.
    #[default]
    Detect,
    /// Use the central directory length and offset to determine the start of the archive.
    #[deprecated(since = "2.3.0", note = "use `Detect` instead")]
    FromCentralDirectory,
    /// Specify a fixed archive offset.
    Known(u64),
}
