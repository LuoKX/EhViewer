//! Authentication response and account-profile protocols.
//!
//! Android retains credential input, HTTP execution, and `CookieManager` storage
//! during this migration slice. This module owns only host-testable remote
//! response interpretation and versioned domain output; credentials and cookies
//! never enter these APIs.

pub mod profile;
pub mod sign_in;

/// Current authentication bridge protocol version.
pub const API_VERSION: u8 = 1;

/// Stable authentication module discriminator used by Android bridge adapters.
pub const MODULE: &str = "auth";
