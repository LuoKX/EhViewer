//! Android native packaging facade.
//!
//! Portable production behavior and JNI adapters live in the workspace crates.
//! This root static library retains only the process loader entry point and links
//! `eh_ffi` exports into the final Android shared library.

mod ffi;

#[cfg(feature = "jvm")]
// A staticlib must retain the upstream crate that owns the JNI exports even though
// this facade does not call those adapters directly.
#[allow(unused_imports)]
use eh_ffi::jvm as _;
