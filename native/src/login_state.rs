//! Pure interaction state from Android's `SignInScreen.kt`.
//!
//! No credentials, HTTP, cookies, persistence, or session lifecycle live here. Hosts own field
//! buffers and use their exact `is_empty()` results at submission (never trim). Execute returned
//! effects in order, using the existing session controller for both requests. [`JobId`] correlates
//! UI callbacks only; it is not a session attempt or authority to commit authentication.
//!
//! Original quirks are intentional: accepted sign-in and cancellation leave the spinner visible;
//! obsolete sign-in hides it; a failed sign-in remains busy through its confirmation dialog. The
//! host owns navigation and screen destruction after successful authentication. This module does
//! not invent that navigation or reset surviving UI state on cancellation.
//!
//! Rust execution additions: explicit job IDs reject late/duplicate callbacks, and explicit dialog
//! completion represents the suspended Kotlin job. IDs belong to one state instance; never route
//! callbacks from a destroyed screen to a new instance. No field contents or failure text occur in
//! actions/effects/Debug; host-owned errors supply the original localized failure dialog content.

/// Submission-time emptiness, derived from caller-owned field buffers without trimming.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldEmptiness {
    /// Whether username text has zero length, not whether it contains only whitespace.
    pub username_empty: bool,
    /// Whether password text has zero length, not whether it contains only whitespace.
    pub password_empty: bool,
}

/// Opaque local UI job identity; unrelated to canonical session generation tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobId(u64);

/// Outcome of the host's password request plus canonical `postLogin` handoff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInOutcome {
    /// Canonical session accepted the handoff; navigation remains host-owned.
    Accepted,
    /// Canonical `postLogin` returned false because the session attempt was superseded.
    Obsolete,
    /// Ordinary exception; host retains error text until the failure dialog completes.
    Failed,
    /// Cancellation exception; original code rethrows without changing spinner visibility.
    Cancelled,
}

/// Credential-free input to one screen instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginAction {
    /// Sign-in button or password keyboard action; both use the same branch order.
    Submit(FieldEmptiness),
    /// Guest button; invokes existing session controller, never claims authenticated success.
    EnterGuest,
    /// Click on the password visibility button; absent and ignored while password error is shown.
    TogglePasswordVisibility,
    /// Caller changed either field. Existing errors intentionally remain sticky.
    FieldsEdited,
    /// Registration button, still available during a job.
    OpenRegistration,
    /// `WebView` sign-in button, still available during a job.
    OpenWebView,
    /// Password work finished. Only a matching running password job may change UI state.
    SignInCompleted {
        /// Local UI request identity from the emitted request effect.
        job: JobId,
        /// Host-classified outcome, with no error message or account information.
        outcome: SignInOutcome,
    },
    /// Failure dialog confirmed or its awaiting coroutine cancelled; release the busy job.
    FailureDialogCompleted {
        /// Local identity carried by the failure dialog effect.
        job: JobId,
    },
    /// Guest job terminated, whether normally, exceptionally, or by cancellation.
    ///
    /// Original guest job has no sign-in failure handler. Host owns exception handling and must
    /// report termination on every path; this action changes neither spinner nor field errors.
    GuestCompleted {
        /// Local UI request identity from the emitted guest request effect.
        job: JobId,
    },
}

/// Ordered instructions for the host, containing no credentials or failure text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginEffect {
    /// Clear focus before initiating authentication or presenting its failure dialog.
    ClearFocus,
    /// Apply spinner visibility at this position in the original interaction sequence.
    ///
    /// State snapshot already contains this value. This effect exposes ordering relative to focus,
    /// requests, and dialogs; hosts must not treat it as a separate source of application state.
    SetProgressIndicatorVisible(bool),
    /// Run canonical begin-sign-in, caller-owned password authentication, then `postLogin`.
    ///
    /// Read caller-owned credentials when executing authentication, as Kotlin's coroutine does;
    /// do not snapshot, store, or log them in the action/plan. Return `SignInCompleted` for every
    /// terminal outcome. Never substitute this UI ID for the canonical session attempt.
    RequestPasswordSignIn(JobId),
    /// Run canonical enter-guest and report `GuestCompleted` even after failure/cancellation.
    RequestGuest(JobId),
    /// Show original localized sign-in-failed dialog with host-owned error text and `WebView` tip.
    ///
    /// Use original confirmation label and no cancel button; report `FailureDialogCompleted`
    /// when the await ends (including cancellation). Busy suppression lasts until then.
    ShowSignInFailure(JobId),
    /// Open the original registration URL in the host browser, without changing session state.
    OpenRegistration,
    /// Navigate to the original `WebView` sign-in route, without claiming sign-in success.
    NavigateWebViewSignIn,
}

/// Password trailing icon; error icon replaces the clickable visibility control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PasswordIcon {
    /// Non-clickable information/error icon.
    ErrorInfo,
    /// Visibility icon; password uses the original reveal-last-typed obfuscation mode.
    Visibility,
    /// Visibility-off icon; password text is visible.
    VisibilityOff,
}

/// Read-only presentation values; does not contain field buffers or persisted user/session data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Original presentation flags are independent; spinner is deliberately not busy."
)]
pub struct LoginView {
    /// Original username error/supporting text and information icon visibility.
    pub show_username_error: bool,
    /// Original password error/supporting text visibility.
    pub show_password_error: bool,
    /// True means reveal-last-typed, not a fully hidden/no-reveal password mode.
    pub password_hidden: bool,
    /// Original circular progress indicator visibility, independent of busy state.
    pub progress_indicator_visible: bool,
    /// Whether sign-in/guest must suppress another submission, including failure dialog await.
    /// Other controls are not globally disabled by this value in the original screen.
    pub busy: bool,
    /// Derived trailing icon; only visibility icons can be clicked.
    pub password_icon: PasswordIcon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveJob {
    Password(JobId),
    FailureDialog(JobId),
    Guest(JobId),
}

/// One screen's transient interaction state; no session lifecycle or credential ownership.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Preserve original independent rememberSaveable flags without normalizing combinations."
)]
pub struct LoginState {
    show_username_error: bool,
    show_password_error: bool,
    password_hidden: bool,
    progress_indicator_visible: bool,
    active_job: Option<ActiveJob>,
    last_job_id: u64,
}

impl Default for LoginState {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginState {
    /// Original fresh-screen defaults: no errors/spinner, hidden password, no active job.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            show_username_error: false,
            show_password_error: false,
            password_hidden: true,
            progress_indicator_visible: false,
            active_job: None,
            last_job_id: 0,
        }
    }

    /// Current presentation, independent of caller-owned field text and canonical session state.
    #[must_use]
    pub const fn view(&self) -> LoginView {
        LoginView {
            show_username_error: self.show_username_error,
            show_password_error: self.show_password_error,
            password_hidden: self.password_hidden,
            progress_indicator_visible: self.progress_indicator_visible,
            busy: self.active_job.is_some(),
            password_icon: if self.show_password_error {
                PasswordIcon::ErrorInfo
            } else if self.password_hidden {
                PasswordIcon::Visibility
            } else {
                PasswordIcon::VisibilityOff
            },
        }
    }

    /// Update transient state and return host instructions in original execution order.
    ///
    /// Busy check precedes validation. Username is validated first; an empty username returns
    /// without touching password error. Text edits alone clear neither error. Completion events
    /// must match both the current job ID and phase; obsolete local callbacks do nothing.
    ///
    /// # Panics
    ///
    /// Panics if a single screen starts more than `u64::MAX` jobs. IDs never wrap and accidentally
    /// authorize an old callback.
    #[must_use]
    pub fn apply(&mut self, action: LoginAction) -> Vec<LoginEffect> {
        match action {
            LoginAction::Submit(fields) => self.submit(fields),
            LoginAction::EnterGuest => {
                if self.active_job.is_some() {
                    return Vec::new();
                }
                let job = self.next_job();
                self.active_job = Some(ActiveJob::Guest(job));
                vec![LoginEffect::RequestGuest(job)]
            }
            LoginAction::TogglePasswordVisibility => {
                if !self.show_password_error {
                    self.password_hidden = !self.password_hidden;
                }
                Vec::new()
            }
            LoginAction::FieldsEdited => Vec::new(),
            LoginAction::OpenRegistration => vec![LoginEffect::OpenRegistration],
            LoginAction::OpenWebView => vec![LoginEffect::NavigateWebViewSignIn],
            LoginAction::SignInCompleted { job, outcome } => self.finish_sign_in(job, outcome),
            LoginAction::FailureDialogCompleted { job } => {
                if self.active_job == Some(ActiveJob::FailureDialog(job)) {
                    self.active_job = None;
                }
                Vec::new()
            }
            LoginAction::GuestCompleted { job } => {
                if self.active_job == Some(ActiveJob::Guest(job)) {
                    self.active_job = None;
                }
                Vec::new()
            }
        }
    }

    fn next_job(&mut self) -> JobId {
        self.last_job_id = self
            .last_job_id
            .checked_add(1)
            .expect("UI job identity exhausted");
        JobId(self.last_job_id)
    }

    fn submit(&mut self, fields: FieldEmptiness) -> Vec<LoginEffect> {
        if self.active_job.is_some() {
            return Vec::new();
        }
        if fields.username_empty {
            self.show_username_error = true;
            return Vec::new();
        }
        self.show_username_error = false;
        if fields.password_empty {
            self.show_password_error = true;
            return Vec::new();
        }
        self.show_password_error = false;
        let job = self.next_job();
        self.active_job = Some(ActiveJob::Password(job));
        self.progress_indicator_visible = true;
        vec![
            LoginEffect::ClearFocus,
            LoginEffect::SetProgressIndicatorVisible(true),
            LoginEffect::RequestPasswordSignIn(job),
        ]
    }

    fn finish_sign_in(&mut self, job: JobId, outcome: SignInOutcome) -> Vec<LoginEffect> {
        if self.active_job != Some(ActiveJob::Password(job)) {
            return Vec::new();
        }
        match outcome {
            SignInOutcome::Accepted | SignInOutcome::Cancelled => {
                self.active_job = None;
                Vec::new()
            }
            SignInOutcome::Obsolete => {
                self.active_job = None;
                self.progress_indicator_visible = false;
                vec![LoginEffect::SetProgressIndicatorVisible(false)]
            }
            SignInOutcome::Failed => {
                self.active_job = Some(ActiveJob::FailureDialog(job));
                self.progress_indicator_visible = false;
                vec![
                    LoginEffect::ClearFocus,
                    LoginEffect::SetProgressIndicatorVisible(false),
                    LoginEffect::ShowSignInFailure(job),
                ]
            }
        }
    }
}
