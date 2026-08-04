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
//! catalog sorting/filtering, [`library`] owns Android-persistence-facing plans,
//! [`search`] owns query normalization, [`settings`] owns portable repair rules,
//! [`reader`] plans page scheduling and deterministic image-fetch transitions,
//! and [`session`] plans sanitized lifecycle transitions and classifies response
//! failures without receiving cookie values, credentials, or bodies. Android
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

pub mod contracts;
pub mod daily;
pub mod download;
pub mod download_catalog;
pub mod library;
pub mod reader;
pub mod recovery;
pub mod search;
pub mod session;
pub mod settings;
