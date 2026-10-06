//! JNI and CBOR adapters for the Rust-first core.
//!
//! `eh_ffi` is the only target crate that translates JVM buffers, Android image
//! storage, and versioned wire contracts into calls to `eh_archive`, [`eh_domain`],
//! `eh_image`, and [`eh_net`]. It contains no UI, Android lifecycle, persistence,
//! credentials, or legacy parser behavior. The root packaging facade links this
//! crate while checked manifests keep the Java symbols stable.
//!
//! # Dependency direction
//!
//! ```text
//! eh_archive / eh_domain / eh_image / eh_net <- eh_ffi <- Android shell
//! ```
//!
//! # Example
//!
//! JVM builds enable `jvm`; Android builds add `android` or `android-26` for Bitmap
//! and `HardwareBuffer` access. Host builds can test the health contract, wire codec,
//! and WebP decoder without loading Android or a JVM.

#[cfg(any(feature = "jvm", test))]
mod codec;

pub mod health;

#[cfg(any(feature = "jvm", test))]
mod recovery_scan;

#[cfg(feature = "jvm")]
pub mod jvm;

#[cfg(feature = "jvm")]
mod webp;
