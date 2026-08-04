//! Session restore, authentication handoff, guest, and logout planning.
//!
//! Android owns credentials, HTTP, `WebView`, `CookieManager`, persisted settings,
//! and profile presentation. This module receives only sanitized state facts and
//! returns one complete plan which Android validates before applying any effect.

use serde::{Deserialize, Serialize};

use crate::contracts::RustError;

use super::GallerySite;

/// Protocol version of the session-lifecycle request and response.
pub const SESSION_LIFECYCLE_API_VERSION: u8 = 1;

/// Stable module identifier used by session-lifecycle response envelopes.
pub const SESSION_LIFECYCLE_MODULE: &str = "sessionLifecycle";

/// Canonical navigation and authentication mode of the Android session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionMode {
    /// The application must present an authentication choice before remote browsing.
    SignInRequired,
    /// The user explicitly selected anonymous public-gallery access.
    Guest,
    /// Android has a completed login plus both required identity cookies.
    Authenticated,
}

/// Canonical session state produced by the lifecycle planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLifecycleState {
    /// Current navigation and authentication mode.
    pub mode: SessionMode,
    /// Account display name when known; non-authenticated states always use `None`.
    pub username: Option<String>,
}

/// Sanitized Android facts used to plan one lifecycle command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLifecycleSnapshot {
    /// Whether `CookieManager` currently contains both identity cookies.
    pub has_auth_cookies: bool,
    /// Persisted `Settings.hasSignedIn` value.
    pub stored_signed_in: bool,
    /// Persisted `Settings.needSignIn` value, which has safety precedence on restore.
    pub sign_in_required: bool,
    /// Last known display name; no cookie value or credential is accepted here.
    pub username: Option<String>,
    /// Last selected gallery-site category.
    pub site: GallerySite,
}

/// One lifecycle event already observed by Android.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionLifecycleCommand {
    /// Reconcile persisted flags with current identity-cookie presence at startup.
    Restore,
    /// Complete a successful Android-owned password or `WebView` authentication.
    SignInSucceeded {
        /// Canonical server/profile username when available.
        username: Option<String>,
        /// Site selected after Android's restricted-site probe.
        site: GallerySite,
    },
    /// Enter anonymous public browsing and discard any authenticated identity.
    EnterGuest,
    /// End the active account session and require a new login choice.
    SignOut,
}

/// Versioned request sent from Android to the Rust lifecycle planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLifecycleRequest {
    /// Protocol version; callers must use [`SESSION_LIFECYCLE_API_VERSION`].
    pub version: u8,
    /// Sanitized current Android session facts.
    pub snapshot: SessionLifecycleSnapshot,
    /// Single lifecycle event to reduce.
    pub command: SessionLifecycleCommand,
}

/// Profile mutation Android applies after a lifecycle transition is accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SessionProfileUpdate {
    /// Preserve the current profile because no fresher value is available.
    Keep,
    /// Replace the display name with a value confirmed by the remote service.
    Set {
        /// Display name to persist. It must never be copied into diagnostics.
        username: String,
    },
    /// Remove display name and avatar data for a non-authenticated state.
    Clear,
}

/// Complete platform-neutral lifecycle plan returned to Android.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLifecyclePlan {
    /// Canonical state to publish after required effects succeed.
    pub state: SessionLifecycleState,
    /// Canonical gallery-site category to persist.
    pub site: GallerySite,
    /// Whether Android must await complete `CookieManager` removal before publishing state.
    pub clear_cookies: bool,
    /// Profile mutation to apply with the state transition.
    pub profile_update: SessionProfileUpdate,
}

/// Plans one sanitized lifecycle command without accessing Android state or secrets.
///
/// # Errors
///
/// Returns [`RustError::NeedLogin`] when Android reports a successful authentication
/// handoff without both required identity cookies. Unsupported request versions are
/// transport errors owned by the FFI adapter and are not handled here.
pub fn plan_session_lifecycle(
    snapshot: &SessionLifecycleSnapshot,
    command: SessionLifecycleCommand,
) -> Result<SessionLifecyclePlan, RustError> {
    match command {
        SessionLifecycleCommand::Restore => Ok(plan_restore(snapshot)),
        SessionLifecycleCommand::SignInSucceeded { username, site } => {
            if !snapshot.has_auth_cookies {
                return Err(RustError::NeedLogin);
            }

            let profile_update = username
                .as_ref()
                .map_or(SessionProfileUpdate::Keep, |username| {
                    SessionProfileUpdate::Set {
                        username: username.clone(),
                    }
                });
            Ok(authenticated(
                username.or_else(|| snapshot.username.clone()),
                site,
                profile_update,
            ))
        }
        SessionLifecycleCommand::EnterGuest => Ok(non_authenticated(SessionMode::Guest, true)),
        SessionLifecycleCommand::SignOut => {
            Ok(non_authenticated(SessionMode::SignInRequired, true))
        }
    }
}

fn plan_restore(snapshot: &SessionLifecycleSnapshot) -> SessionLifecyclePlan {
    if snapshot.sign_in_required {
        non_authenticated(SessionMode::SignInRequired, snapshot.has_auth_cookies)
    } else if snapshot.stored_signed_in && snapshot.has_auth_cookies {
        authenticated(
            snapshot.username.clone(),
            snapshot.site,
            SessionProfileUpdate::Keep,
        )
    } else if snapshot.stored_signed_in {
        non_authenticated(SessionMode::SignInRequired, false)
    } else {
        non_authenticated(SessionMode::Guest, snapshot.has_auth_cookies)
    }
}

fn authenticated(
    username: Option<String>,
    site: GallerySite,
    profile_update: SessionProfileUpdate,
) -> SessionLifecyclePlan {
    SessionLifecyclePlan {
        state: SessionLifecycleState {
            mode: SessionMode::Authenticated,
            username,
        },
        site,
        clear_cookies: false,
        profile_update,
    }
}

fn non_authenticated(mode: SessionMode, clear_cookies: bool) -> SessionLifecyclePlan {
    SessionLifecyclePlan {
        state: SessionLifecycleState {
            mode,
            username: None,
        },
        site: GallerySite::Public,
        clear_cookies,
        profile_update: SessionProfileUpdate::Clear,
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use serde_json::json;

    use crate::contracts::{ApiEnvelope, RustError};

    use super::{
        SESSION_LIFECYCLE_API_VERSION, SESSION_LIFECYCLE_MODULE, SessionLifecycleCommand,
        SessionLifecyclePlan, SessionLifecycleRequest, SessionLifecycleSnapshot,
        SessionLifecycleState, SessionMode, SessionProfileUpdate, plan_session_lifecycle,
    };
    use crate::session::GallerySite;

    #[test]
    fn restore_normalizes_every_persisted_mode_and_cookie_combination() {
        let required = plan(
            &snapshot(true, true, true),
            SessionLifecycleCommand::Restore,
        )
        .expect("restore should plan");
        assert_eq!(
            required,
            non_authenticated(SessionMode::SignInRequired, true)
        );

        let authenticated = plan(
            &snapshot(true, true, false),
            SessionLifecycleCommand::Restore,
        )
        .expect("restore should plan");
        assert_eq!(authenticated.state.mode, SessionMode::Authenticated);
        assert_eq!(authenticated.state.username.as_deref(), Some("reader"));
        assert_eq!(authenticated.site, GallerySite::ExHentai);
        assert_eq!(authenticated.profile_update, SessionProfileUpdate::Keep);
        assert!(!authenticated.clear_cookies);

        let expired = plan(
            &snapshot(false, true, false),
            SessionLifecycleCommand::Restore,
        )
        .expect("restore should plan");
        assert_eq!(
            expired,
            non_authenticated(SessionMode::SignInRequired, false)
        );

        let stale_guest = plan(
            &snapshot(true, false, false),
            SessionLifecycleCommand::Restore,
        )
        .expect("restore should plan");
        assert_eq!(stale_guest, non_authenticated(SessionMode::Guest, true));

        let guest = plan(
            &snapshot(false, false, false),
            SessionLifecycleCommand::Restore,
        )
        .expect("restore should plan");
        assert_eq!(guest, non_authenticated(SessionMode::Guest, false));
    }

    #[test]
    fn completed_sign_in_requires_cookies_and_updates_only_known_profile_data() {
        let known = plan(
            &snapshot(true, false, true),
            SessionLifecycleCommand::SignInSucceeded {
                username: Some("canonical".to_owned()),
                site: GallerySite::ExHentai,
            },
        )
        .expect("identity cookies should allow authentication");
        assert_eq!(known.state.mode, SessionMode::Authenticated);
        assert_eq!(known.state.username.as_deref(), Some("canonical"));
        assert_eq!(known.site, GallerySite::ExHentai);
        assert_eq!(
            known.profile_update,
            SessionProfileUpdate::Set {
                username: "canonical".to_owned(),
            }
        );

        let unknown = plan(
            &snapshot(true, false, true),
            SessionLifecycleCommand::SignInSucceeded {
                username: None,
                site: GallerySite::Public,
            },
        )
        .expect("WebView authentication may not know a username yet");
        assert_eq!(unknown.state.username.as_deref(), Some("reader"));
        assert_eq!(unknown.profile_update, SessionProfileUpdate::Keep);

        let error = plan(
            &snapshot(false, false, true),
            SessionLifecycleCommand::SignInSucceeded {
                username: Some("reader".to_owned()),
                site: GallerySite::Public,
            },
        )
        .expect_err("authentication without identity cookies must fail");
        assert_eq!(error, RustError::NeedLogin);
    }

    #[test]
    fn guest_and_sign_out_clear_identity_with_distinct_navigation_modes() {
        let snapshot = snapshot(true, true, false);
        assert_eq!(
            plan(&snapshot, SessionLifecycleCommand::EnterGuest).expect("guest should plan"),
            non_authenticated(SessionMode::Guest, true)
        );
        assert_eq!(
            plan(&snapshot, SessionLifecycleCommand::SignOut).expect("sign-out should plan"),
            non_authenticated(SessionMode::SignInRequired, true)
        );
    }

    #[test]
    fn request_and_response_keep_stable_json_and_cbor_shapes() {
        let request = SessionLifecycleRequest {
            version: SESSION_LIFECYCLE_API_VERSION,
            snapshot: snapshot(true, false, true),
            command: SessionLifecycleCommand::SignInSucceeded {
                username: Some("canonical".to_owned()),
                site: GallerySite::ExHentai,
            },
        };
        assert_eq!(
            serde_json::to_value(&request).expect("request should encode as JSON"),
            json!({
                "version": 1,
                "snapshot": {
                    "hasAuthCookies": true,
                    "storedSignedIn": false,
                    "signInRequired": true,
                    "username": "reader",
                    "site": "exHentai",
                },
                "command": {
                    "kind": "signInSucceeded",
                    "username": "canonical",
                    "site": "exHentai",
                },
            })
        );
        let request_cbor = serde_cbor::to_vec(&request).expect("request should encode as CBOR");
        assert_eq!(hex(&request_cbor), REQUEST_GOLDEN_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<SessionLifecycleRequest>(&request_cbor)
                .expect("request should decode from CBOR"),
            request
        );

        let plan = plan_session_lifecycle(&request.snapshot, request.command.clone())
            .expect("request should produce a plan");
        let envelope = ApiEnvelope::ok(
            SESSION_LIFECYCLE_API_VERSION,
            SESSION_LIFECYCLE_MODULE,
            plan.clone(),
        );
        let response_cbor = serde_cbor::to_vec(&envelope).expect("response should encode as CBOR");
        assert_eq!(hex(&response_cbor), RESPONSE_GOLDEN_CBOR_HEX);
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<SessionLifecyclePlan>>(&response_cbor)
                .expect("response should decode from CBOR")
                .payload,
            Some(plan)
        );
    }

    fn plan(
        snapshot: &SessionLifecycleSnapshot,
        command: SessionLifecycleCommand,
    ) -> Result<SessionLifecyclePlan, RustError> {
        plan_session_lifecycle(snapshot, command)
    }

    fn snapshot(
        has_auth_cookies: bool,
        stored_signed_in: bool,
        sign_in_required: bool,
    ) -> SessionLifecycleSnapshot {
        SessionLifecycleSnapshot {
            has_auth_cookies,
            stored_signed_in,
            sign_in_required,
            username: Some("reader".to_owned()),
            site: GallerySite::ExHentai,
        }
    }

    fn non_authenticated(mode: SessionMode, clear_cookies: bool) -> SessionLifecyclePlan {
        SessionLifecyclePlan {
            state: SessionLifecycleState {
                mode,
                username: None,
            },
            site: GallerySite::Public,
            clear_cookies,
            profile_update: SessionProfileUpdate::Clear,
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
        "a36776657273696f6e0168736e617073686f74a56e68617341757468436f6f6b696573f56e",
        "73746f7265645369676e6564496ef46e7369676e496e5265717569726564f568757365726e61",
        "6d6566726561646572647369746568657848656e74616967636f6d6d616e64a3646b696e646f",
        "7369676e496e53756363656564656468757365726e616d656963616e6f6e6963616c64736974",
        "6568657848656e746169",
    );
    const RESPONSE_GOLDEN_CBOR_HEX: &str = concat!(
        "a46776657273696f6e01666d6f64756c657073657373696f6e4c6966656379636c65677061",
        "796c6f6164a4657374617465a2646d6f64656d61757468656e7469636174656468757365726e",
        "616d656963616e6f6e6963616c647369746568657848656e7461696c636c656172436f6f6b69",
        "6573f46d70726f66696c65557064617465a2646b696e646373657468757365726e616d656963",
        "616e6f6e6963616c656572726f72f6",
    );
}
