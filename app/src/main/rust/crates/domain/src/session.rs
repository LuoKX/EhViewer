//! Pure session lifecycle and failed-response decisions.
//!
//! Android retains HTTP clients, cookies, credentials, response bodies, localized
//! messages, parse-error files, and exception types. [`lifecycle`] plans sanitized
//! startup/authentication/logout state, while this module preserves failed-response
//! precedence and returns one decision for Android to apply.

pub mod lifecycle;

use serde::{Deserialize, Serialize};

use crate::contracts::RustError;

/// Protocol version of the session-failure request and response.
pub const SESSION_POLICY_API_VERSION: u8 = 1;

/// Stable module identifier used by the session-policy response envelope.
pub const SESSION_POLICY_MODULE: &str = "sessionPolicy";

/// Gallery host category needed for the legacy empty-response message policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GallerySite {
    /// Any public/API/forums host which uses the localized IP-ban message.
    Public,
    /// The restricted `ExHentai` gallery host which uses the legacy Sad Panda message.
    ExHentai,
}

/// Original exception category after Android has preserved cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionFailureSource {
    /// An HTML/CBOR parser or serializer rejected the response body.
    Parse {
        /// Whether a text body contained no `<` marker and should be shown as an IP-ban message.
        #[serde(rename = "nonHtmlText")]
        non_html_text: bool,
    },
    /// Any other throwable which Android must preserve if no higher-priority fact applies.
    Other,
}

/// Sanitized response facts needed to classify one failed request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFailureSnapshot {
    /// Host category derived by Android without exposing the URL or query.
    pub site: GallerySite,
    /// Whether the response carries the exact `cf-mitigated: challenge` marker.
    pub cloudflare_challenge: bool,
    /// Whether the already-buffered response body contains no bytes or characters.
    pub empty_body: bool,
    /// Versioned or decoded legacy business error, without a response body.
    pub domain_error: Option<RustError>,
    /// Whether the HTTP status is in the successful range.
    pub http_status_success: bool,
    /// Category of the original Android parser/transport exception.
    pub source: SessionFailureSource,
}

/// Versioned request sent from Android to the Rust session policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFailureRequest {
    /// Protocol version; callers must use [`SESSION_POLICY_API_VERSION`].
    pub version: u8,
    /// Sanitized facts for one already-failed response.
    pub snapshot: SessionFailureSnapshot,
}

/// One platform-neutral failure action selected by the session policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionFailureDecision {
    /// Ask Android to start its existing Cloudflare/WebView bypass flow.
    CloudflareChallenge,
    /// Throw the legacy empty-response IP-ban error for the supplied host category.
    EmptyBodyIpBanned {
        /// Host category controlling Android's localized/Sad-Panda message.
        site: GallerySite,
    },
    /// Map one stable business error into the existing Android exception hierarchy.
    Domain {
        /// Typed domain failure selected after precedence checks.
        error: RustError,
    },
    /// Preserve the existing HTTP status check and exception text on Android.
    HttpStatus,
    /// Use the non-HTML text body as the existing IP-ban diagnostic without crossing JNI.
    NonHtmlTextIpBanned,
    /// Preserve Android parse-error persistence and localized exception behavior.
    ParseFailure,
    /// Rethrow the original Android exception unchanged.
    Rethrow,
}

/// Deterministic decision for one failed response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFailurePlan {
    /// Single action Android applies after validating the response envelope.
    pub decision: SessionFailureDecision,
}

/// Classifies one failed response without receiving secrets or response bytes.
///
/// The precedence intentionally matches the legacy Android mapper: Cloudflare,
/// empty response, gallery-unavailable business error, HTTP status, remaining
/// business errors, parser-specific handling, then the original exception.
#[must_use]
pub fn classify_session_failure(snapshot: &SessionFailureSnapshot) -> SessionFailurePlan {
    let decision = if snapshot.cloudflare_challenge {
        SessionFailureDecision::CloudflareChallenge
    } else if snapshot.empty_body {
        SessionFailureDecision::EmptyBodyIpBanned {
            site: snapshot.site,
        }
    } else if let Some(error @ RustError::GalleryUnavailable(_)) = snapshot.domain_error.as_ref() {
        SessionFailureDecision::Domain {
            error: error.clone(),
        }
    } else if !snapshot.http_status_success {
        SessionFailureDecision::HttpStatus
    } else if let Some(error) = snapshot.domain_error.clone() {
        SessionFailureDecision::Domain { error }
    } else {
        match snapshot.source {
            SessionFailureSource::Parse {
                non_html_text: true,
            } => SessionFailureDecision::NonHtmlTextIpBanned,
            SessionFailureSource::Parse {
                non_html_text: false,
            } => SessionFailureDecision::ParseFailure,
            SessionFailureSource::Other => SessionFailureDecision::Rethrow,
        }
    };

    SessionFailurePlan { decision }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use serde_json::json;

    use crate::contracts::{ApiEnvelope, RustError};

    use super::{
        GallerySite, SESSION_POLICY_API_VERSION, SESSION_POLICY_MODULE, SessionFailureDecision,
        SessionFailurePlan, SessionFailureRequest, SessionFailureSnapshot, SessionFailureSource,
        classify_session_failure,
    };

    #[test]
    fn challenge_and_empty_body_win_before_every_other_failure() {
        let mut snapshot = failure_snapshot();
        snapshot.cloudflare_challenge = true;
        snapshot.empty_body = true;
        snapshot.http_status_success = false;
        snapshot.domain_error = Some(RustError::GalleryUnavailable("removed".to_owned()));
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::CloudflareChallenge
        );

        snapshot.cloudflare_challenge = false;
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::EmptyBodyIpBanned {
                site: GallerySite::Public,
            }
        );
        snapshot.site = GallerySite::ExHentai;
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::EmptyBodyIpBanned {
                site: GallerySite::ExHentai,
            }
        );
    }

    #[test]
    fn gallery_unavailable_wins_before_http_status_but_other_errors_do_not() {
        let mut snapshot = failure_snapshot();
        snapshot.http_status_success = false;
        snapshot.domain_error = Some(RustError::GalleryUnavailable("removed".to_owned()));
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::Domain {
                error: RustError::GalleryUnavailable("removed".to_owned()),
            }
        );

        snapshot.domain_error = Some(RustError::NeedLogin);
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::HttpStatus
        );
    }

    #[test]
    fn successful_status_preserves_every_typed_domain_error() {
        let errors = [
            RustError::NoHits,
            RustError::NoWatched,
            RustError::NeedLogin,
            RustError::NoHathClient,
            RustError::InsufficientFunds,
            RustError::GalleryUnavailable("unavailable".to_owned()),
            RustError::IpBanned("banned".to_owned()),
            RustError::Error("failed".to_owned()),
        ];

        for error in errors {
            let mut snapshot = failure_snapshot();
            snapshot.domain_error = Some(error.clone());
            assert_eq!(
                classify_session_failure(&snapshot).decision,
                SessionFailureDecision::Domain { error }
            );
        }
    }

    #[test]
    fn parse_and_unknown_failures_keep_legacy_fallback_decisions() {
        let mut snapshot = failure_snapshot();
        snapshot.source = SessionFailureSource::Parse {
            non_html_text: true,
        };
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::NonHtmlTextIpBanned
        );

        snapshot.source = SessionFailureSource::Parse {
            non_html_text: false,
        };
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::ParseFailure
        );

        snapshot.source = SessionFailureSource::Other;
        assert_eq!(
            classify_session_failure(&snapshot).decision,
            SessionFailureDecision::Rethrow
        );
    }

    #[test]
    fn request_and_envelope_have_stable_json_and_cbor_contracts() {
        let request = SessionFailureRequest {
            version: SESSION_POLICY_API_VERSION,
            snapshot: SessionFailureSnapshot {
                site: GallerySite::ExHentai,
                cloudflare_challenge: false,
                empty_body: false,
                domain_error: Some(RustError::NeedLogin),
                http_status_success: true,
                source: SessionFailureSource::Parse {
                    non_html_text: false,
                },
            },
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request should encode as JSON"),
            json!({
                "version": 1,
                "snapshot": {
                    "site": "exHentai",
                    "cloudflareChallenge": false,
                    "emptyBody": false,
                    "domainError": {"kind": "needLogin"},
                    "httpStatusSuccess": true,
                    "source": {"kind": "parse", "nonHtmlText": false},
                },
            })
        );
        let request_cbor = serde_cbor::to_vec(&request).expect("request should encode as CBOR");
        assert_eq!(hex(&request_cbor), REQUEST_GOLDEN_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<SessionFailureRequest>(&request_cbor)
                .expect("request should decode from CBOR"),
            request
        );

        let plan = SessionFailurePlan {
            decision: SessionFailureDecision::Domain {
                error: RustError::NeedLogin,
            },
        };
        let envelope = ApiEnvelope::ok(
            SESSION_POLICY_API_VERSION,
            SESSION_POLICY_MODULE,
            plan.clone(),
        );
        let response_cbor = serde_cbor::to_vec(&envelope).expect("response should encode as CBOR");
        assert_eq!(hex(&response_cbor), RESPONSE_GOLDEN_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<SessionFailurePlan>>(&response_cbor)
                .expect("response should decode from CBOR")
                .payload,
            Some(plan)
        );
    }

    fn failure_snapshot() -> SessionFailureSnapshot {
        SessionFailureSnapshot {
            site: GallerySite::Public,
            cloudflare_challenge: false,
            empty_body: false,
            domain_error: None,
            http_status_success: true,
            source: SessionFailureSource::Other,
        }
    }

    fn hex(bytes: &[u8]) -> String {
        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
        }
        encoded
    }

    const REQUEST_GOLDEN_CBOR_HEX: &str = concat!(
        "a26776657273696f6e0168736e617073686f74a6647369746568657848656e74616973636c6f",
        "7564666c6172654368616c6c656e6765f469656d707479426f6479f46b646f6d61696e457272",
        "6f72a1646b696e64696e6565644c6f67696e716874747053746174757353756363657373f566",
        "736f75726365a2646b696e646570617273656b6e6f6e48746d6c54657874f4",
    );
    const RESPONSE_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c656d73657373696f6e506f6c696379677061796c6f",
        "6164a1686465636973696f6ea2646b696e6466646f6d61696e656572726f72a1646b696e6469",
        "6e6565644c6f67696e656572726f72f6",
    );
}
