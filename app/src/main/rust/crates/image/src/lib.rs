//! Host-testable image analysis for `EhViewer`'s Rust-first core.
//!
//! This crate owns pixel-format normalization, CPU-only image algorithms, and
//! validated byte-row region copies. It has no Android or JNI dependency: the
//! compatibility static library locks Android storage, constructs a borrowed
//! [`image::ImageBuffer`] or bounded byte slices, and delegates computation here.
//! Raw pointer ownership remains confined to the Android compatibility boundary
//! and never enters this crate.
//!
//! # Example
//!
//! ```
//! use eh_image::border::DetectBorder;
//! use eh_image::pixel::{ImageConsumer, Rgba8888};
//! use image::ImageBuffer;
//!
//! let pixels = [0_u8; 4];
//! let image = ImageBuffer::<Rgba8888, _>::from_raw(1, 1, pixels.as_slice())
//!     .expect("the example buffer has one RGBA pixel");
//! let bounds = DetectBorder.apply(&image).expect("the image is non-empty");
//! assert_eq!(bounds.len(), 4);
//! ```

pub mod border;
pub mod crop_policy;
pub mod gif;
pub mod pixel;
pub mod qr;
pub mod region;
pub mod webp;
