//! Host-testable health contract for the Rust-first core boundary.
//!
//! This module owns the stable V1 `rustCore` request and response DTOs used by
//! Android readiness, protocol-compatibility, and nonce-echo diagnostics. It has
//! no JNI, Android, network, storage, logging, or process-startup dependency.

use eh_domain::contracts::{ApiEnvelope, RustError};
use serde::{Deserialize, Serialize};

/// Version of the health response envelope produced by `eh_ffi`.
pub const RUST_CORE_API_VERSION: u8 = 1;

/// Protocol version implemented by the Rust-first core.
pub const RUST_CORE_PROTOCOL_VERSION: u8 = 1;

/// Stable module identifier carried by every Rust Core health envelope.
pub const RUST_CORE_MODULE: &str = "rustCore";

const UNSUPPORTED_REQUEST_VERSION: &str = "unsupported Rust Core health request version";

/// Request for one synchronous Rust Core health probe.
///
/// The camelCase CBOR map contains only a protocol version and a signed scalar
/// nonce. It carries no user, device, credential, path, or network data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustCoreHealthRequest {
    /// Requested health API version; V1 callers send exactly `1`.
    pub version: u8,
    /// Signed 64-bit scalar echoed byte-for-byte in a successful payload.
    pub nonce: i64,
}

/// Successful Rust Core health payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustCoreHealth {
    /// Implemented core protocol version used for compatibility checks.
    pub protocol_version: u8,
    /// Signed 64-bit request nonce echoed without normalization.
    pub nonce: i64,
}

/// Builds the stable V1 health envelope for one decoded request.
///
/// Supported requests return [`RustCoreHealth`] with the current protocol version
/// and exact nonce. Unsupported versions return a fixed, sanitized
/// [`RustError::Error`] envelope and never include request values in error text.
/// The caller retains ownership of the request and any transport buffers.
#[must_use]
pub fn build_envelope(request: RustCoreHealthRequest) -> ApiEnvelope<RustCoreHealth> {
    if request.version != RUST_CORE_API_VERSION {
        return ApiEnvelope::err(
            RUST_CORE_API_VERSION,
            RUST_CORE_MODULE,
            RustError::Error(UNSUPPORTED_REQUEST_VERSION.to_owned()),
        );
    }

    ApiEnvelope::ok(
        RUST_CORE_API_VERSION,
        RUST_CORE_MODULE,
        RustCoreHealth {
            protocol_version: RUST_CORE_PROTOCOL_VERSION,
            nonce: request.nonce,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use eh_domain::contracts::{ApiEnvelope, RustError};

    use super::{
        RUST_CORE_API_VERSION, RUST_CORE_MODULE, RUST_CORE_PROTOCOL_VERSION, RustCoreHealth,
        RustCoreHealthRequest, UNSUPPORTED_REQUEST_VERSION, build_envelope,
    };

    const REQUEST_CBOR_HEX: &str = "a26776657273696f6e01656e6f6e6365182a";
    const RESPONSE_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656872757374436f7265677061796c6f6164a26f70726f",
        "746f636f6c56657273696f6e01656e6f6e6365182a656572726f72f6",
    );

    #[test]
    fn success_preserves_metadata_and_every_signed_nonce_boundary() {
        for nonce in [42, i64::MIN, i64::MAX] {
            let envelope = build_envelope(RustCoreHealthRequest {
                version: RUST_CORE_API_VERSION,
                nonce,
            });

            assert_eq!(envelope.version, RUST_CORE_API_VERSION);
            assert_eq!(envelope.module, RUST_CORE_MODULE);
            assert_eq!(
                envelope.payload,
                Some(RustCoreHealth {
                    protocol_version: RUST_CORE_PROTOCOL_VERSION,
                    nonce,
                })
            );
            assert_eq!(envelope.error, None);
            assert!(envelope.is_ok());
            assert!(envelope.is_valid());
        }
    }

    #[test]
    fn unsupported_version_returns_one_sanitized_typed_error() {
        let envelope = build_envelope(RustCoreHealthRequest {
            version: u8::MAX,
            nonce: i64::MIN,
        });

        assert_eq!(envelope.version, RUST_CORE_API_VERSION);
        assert_eq!(envelope.module, RUST_CORE_MODULE);
        assert_eq!(envelope.payload, None);
        assert_eq!(
            envelope.error,
            Some(RustError::Error(UNSUPPORTED_REQUEST_VERSION.to_owned()))
        );
        assert!(!envelope.is_ok());
        assert!(envelope.is_valid());

        let error_text = envelope.error.as_ref().map(ToString::to_string);
        assert_eq!(
            error_text.as_deref(),
            Some("Error: unsupported Rust Core health request version")
        );
        assert!(!error_text.as_deref().unwrap_or_default().contains("255"));
        assert!(
            !error_text
                .as_deref()
                .unwrap_or_default()
                .contains("9223372036854775808")
        );
    }

    #[test]
    fn request_and_response_cbor_goldens_round_trip_exactly() {
        let request = RustCoreHealthRequest {
            version: RUST_CORE_API_VERSION,
            nonce: 42,
        };
        let request_bytes = serde_cbor::to_vec(&request).expect("request should serialize");
        assert_eq!(to_hex(&request_bytes), REQUEST_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<RustCoreHealthRequest>(&request_bytes)
                .expect("request should decode"),
            request
        );

        let response = build_envelope(request);
        let response_bytes = serde_cbor::to_vec(&response).expect("response should serialize");
        assert_eq!(to_hex(&response_bytes), RESPONSE_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<RustCoreHealth>>(&response_bytes)
                .expect("response should decode"),
            response
        );
    }

    fn to_hex(bytes: &[u8]) -> String {
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut output, "{byte:02x}").expect("writing to a String should succeed");
        }
        output
    }
}
