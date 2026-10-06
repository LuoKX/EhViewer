//! Pure domain types and stable contracts for `EhViewer`'s Rust-first core.
//!
//! This crate owns business-facing data structures shared by the future
//! network, storage, and Android boundary crates. It intentionally has no
//! dependency on Android, JNI, filesystem access, or networking; dependencies
//! point from those adapters toward this crate, never in the opposite direction.
//!
//! [`contracts`] models versioned responses, typed domain failures, and the DTOs
//! crossing the Rust-to-Android boundary. [`daily`] owns daily-check eligibility,
//! [`download`] owns the pure queue state machine, [`download_catalog`] owns
//! catalog sorting/filtering, [`gallery_filter`] owns Gallery list/comment filter
//! decisions, [`gallery_rules`] owns category and similar-title compatibility,
//! [`library`] owns Android-persistence-facing plans, [`file_rules`] owns filename
//! compatibility, [`storage_rules`] owns recovery candidate ordering and retention,
//! [`recovery_scan`] owns bounded BFS and provider identity deduplication,
//! [`text_spans`] owns batched text interval normalization, [`search`] owns query
//! normalization, [`settings`] owns portable repair rules, [`reader`] plans page
//! scheduling and deterministic image-fetch transitions, and [`session`] plans
//! sanitized lifecycle transitions and classifies response failures without
//! receiving cookie values, credentials, or bodies. Android
//! retains services, persistence, notifications, network workers, credentials,
//! image objects, and file handles. Domain failures
//! describe business outcomes only; an FFI adapter remains responsible for
//! mapping transport, buffer, serialization, and panic failures into bridge
//! errors.
//!
//! # Example
//!
//! ```
//! use eh_domain::contracts::{ApiEnvelope, GalleryPage};
//!
//! let page = GalleryPage {
//!     previous: None,
//!     next: None,
//!     items: Vec::new(),
//! };
//! let response = ApiEnvelope::ok(1, "gallery", page);
//!
//! assert!(response.is_ok());
//! ```

pub mod backup_import;
pub mod bbcode;
pub mod comic_info;
pub mod comic_info_policy;
pub mod contracts;
pub mod daily;
pub mod download;
pub mod download_catalog;
pub mod download_metadata;
pub mod file_rules;
pub mod gallery_enrichment;
pub mod gallery_filter;
pub mod gallery_rules;
pub mod library;
pub mod local_pages;
pub mod reader;
pub mod reader_policy;
pub mod recovery;
pub mod recovery_directory;
pub mod recovery_eh_lookup;
pub mod recovery_scan;
pub mod recovery_search;
pub mod search;
pub mod session;
pub mod settings;
pub mod spider_info;
pub mod storage_rules;
pub mod text_spans;
pub mod transfer_speed;
