//! Compatibility path for Rust-first domain contracts.
//!
//! New Rust code should depend on `eh_domain` directly. These re-exports keep
//! the legacy `ehviewer_rust::contracts` path available while migration is in
//! progress.

pub use eh_domain::contracts::{
    ApiEnvelope, DownloadTask, GalleryComment, GalleryCommentsPage, GalleryDetail,
    GalleryDetailPage, GalleryItemSummary, GalleryPage, GalleryPreview, GalleryPreviewKind,
    GalleryPreviewPage, GalleryTag, GalleryTagGroup, GalleryTagPower, GalleryTagVote,
    GalleryVersionSummary, ReaderPage, RustError, SessionState,
};
pub use eh_domain::download::{
    DOWNLOAD_QUEUE_API_VERSION, DOWNLOAD_QUEUE_MODULE, DownloadCommand, DownloadEntry,
    DownloadQueuePlan, DownloadQueueRecoveryRequest, DownloadQueueRequest, DownloadQueueSnapshot,
    DownloadState, DownloadStateChange,
};
