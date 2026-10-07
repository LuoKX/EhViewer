//! Deterministic regressions against `SignInScreen.kt`, plus Rust callback correlation guards.

use eh_native::login_state::{
    FieldEmptiness, JobId, LoginAction, LoginEffect, LoginState, LoginView, PasswordIcon,
    SignInOutcome,
};

fn fields(username_empty: bool, password_empty: bool) -> FieldEmptiness {
    FieldEmptiness {
        username_empty,
        password_empty,
    }
}

fn submit(state: &mut LoginState, emptiness: FieldEmptiness) -> JobId {
    let effects = state.apply(LoginAction::Submit(emptiness));
    let [
        LoginEffect::ClearFocus,
        LoginEffect::SetProgressIndicatorVisible(true),
        LoginEffect::RequestPasswordSignIn(job),
    ] = effects.as_slice()
    else {
        panic!("expected ordered focus, spinner, password request, got {effects:?}");
    };
    *job
}

fn guest(state: &mut LoginState) -> JobId {
    let effects = state.apply(LoginAction::EnterGuest);
    let [LoginEffect::RequestGuest(job)] = effects.as_slice() else {
        panic!("expected guest request only, got {effects:?}");
    };
    *job
}

fn complete(state: &mut LoginState, job: JobId, outcome: SignInOutcome) -> Vec<LoginEffect> {
    state.apply(LoginAction::SignInCompleted { job, outcome })
}

fn seed_errors(username_error: bool, password_error: bool) -> LoginState {
    let mut state = LoginState::new();
    if password_error {
        assert!(
            state
                .apply(LoginAction::Submit(fields(false, true)))
                .is_empty()
        );
    }
    if username_error {
        assert!(
            state
                .apply(LoginAction::Submit(fields(true, false)))
                .is_empty()
        );
    }
    state
}

fn assert_ignored(state: &mut LoginState, action: LoginAction) {
    let before = state.view();
    assert!(state.apply(action).is_empty());
    assert_eq!(state.view(), before);
}

#[test]
fn sign_in_screen_fresh_defaults() {
    let expected = LoginView {
        show_username_error: false,
        show_password_error: false,
        password_hidden: true,
        progress_indicator_visible: false,
        busy: false,
        password_icon: PasswordIcon::Visibility,
    };
    assert_eq!(LoginState::new().view(), expected);
    assert_eq!(LoginState::default().view(), expected);
}

#[test]
fn sign_in_screen_validation_matrix_preserves_original_branch_order() {
    for old_username_error in [false, true] {
        for old_password_error in [false, true] {
            for username_empty in [false, true] {
                for password_empty in [false, true] {
                    let mut state = seed_errors(old_username_error, old_password_error);
                    let effects =
                        state.apply(LoginAction::Submit(fields(username_empty, password_empty)));
                    let view = state.view();
                    assert_eq!(view.show_username_error, username_empty);
                    assert_eq!(
                        view.show_password_error,
                        if username_empty {
                            old_password_error
                        } else {
                            password_empty
                        },
                        "username-empty branch must not visit password validation"
                    );
                    let valid = !username_empty && !password_empty;
                    assert_eq!(view.busy, valid);
                    assert_eq!(view.progress_indicator_visible, valid);
                    if valid {
                        assert!(matches!(
                            effects.as_slice(),
                            [
                                LoginEffect::ClearFocus,
                                LoginEffect::SetProgressIndicatorVisible(true),
                                LoginEffect::RequestPasswordSignIn(_)
                            ]
                        ));
                    } else {
                        assert!(effects.is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn sign_in_screen_empty_username_leaves_sticky_password_error() {
    let mut state = seed_errors(false, true);
    assert!(
        state
            .apply(LoginAction::Submit(fields(true, false)))
            .is_empty()
    );
    assert!(state.view().show_username_error);
    assert!(state.view().show_password_error);
    assert_eq!(state.view().password_icon, PasswordIcon::ErrorInfo);
}

#[test]
fn sign_in_screen_text_edits_do_not_clear_errors() {
    for username_error in [false, true] {
        for password_error in [false, true] {
            let mut state = seed_errors(username_error, password_error);
            assert_ignored(&mut state, LoginAction::FieldsEdited);
            assert_ignored(&mut state, LoginAction::FieldsEdited);
        }
    }
}

#[test]
fn sign_in_screen_whitespace_counts_as_nonempty_without_trimming() {
    let username = String::from(" \t\n");
    let password = String::from(" ");
    let mut state = LoginState::new();
    let _ = submit(&mut state, fields(username.is_empty(), password.is_empty()));
    assert!(state.view().busy);
    assert!(!state.view().show_username_error);
    assert!(!state.view().show_password_error);
}

#[test]
fn sign_in_screen_error_icon_hides_toggle_without_resetting_visibility() {
    let mut state = LoginState::new();
    assert!(
        state
            .apply(LoginAction::TogglePasswordVisibility)
            .is_empty()
    );
    assert!(!state.view().password_hidden);
    assert_eq!(state.view().password_icon, PasswordIcon::VisibilityOff);
    assert!(
        state
            .apply(LoginAction::Submit(fields(false, true)))
            .is_empty()
    );
    assert_eq!(state.view().password_icon, PasswordIcon::ErrorInfo);
    assert_ignored(&mut state, LoginAction::TogglePasswordVisibility);
    assert_ignored(&mut state, LoginAction::FieldsEdited);
    let _ = submit(&mut state, fields(false, false));
    assert!(!state.view().password_hidden);
    assert_eq!(state.view().password_icon, PasswordIcon::VisibilityOff);
    // Busy does not disable the original visibility control.
    assert!(
        state
            .apply(LoginAction::TogglePasswordVisibility)
            .is_empty()
    );
    assert!(state.view().password_hidden);
    assert_eq!(state.view().password_icon, PasswordIcon::Visibility);
}

#[test]
fn sign_in_screen_password_and_guest_busy_checks_precede_validation() {
    for password_job in [false, true] {
        let mut state = LoginState::new();
        let _ = if password_job {
            submit(&mut state, fields(false, false))
        } else {
            guest(&mut state)
        };
        assert_ignored(&mut state, LoginAction::Submit(fields(true, true)));
        assert_ignored(&mut state, LoginAction::Submit(fields(false, false)));
        assert_ignored(&mut state, LoginAction::EnterGuest);
        assert_ignored(&mut state, LoginAction::FieldsEdited);
        assert!(!state.view().show_username_error);
        assert!(!state.view().show_password_error);
    }
}

#[test]
fn sign_in_screen_accepted_handoff_releases_job_but_keeps_spinner() {
    let mut state = LoginState::new();
    let job = submit(&mut state, fields(false, false));
    assert!(complete(&mut state, job, SignInOutcome::Accepted).is_empty());
    assert!(!state.view().busy);
    assert!(state.view().progress_indicator_visible);
    // No invented navigation or fake session success effect; host owns screen removal.
    assert_ignored(
        &mut state,
        LoginAction::SignInCompleted {
            job,
            outcome: SignInOutcome::Failed,
        },
    );
}

#[test]
fn sign_in_screen_cancellation_releases_job_but_keeps_spinner_and_allows_retry() {
    let mut state = LoginState::new();
    let job = submit(&mut state, fields(false, false));
    assert!(complete(&mut state, job, SignInOutcome::Cancelled).is_empty());
    assert!(!state.view().busy);
    assert!(state.view().progress_indicator_visible);
    let retry = submit(&mut state, fields(false, false));
    assert_ne!(job, retry);
    assert!(state.view().busy);
}

#[test]
fn sign_in_screen_obsolete_canonical_completion_hides_spinner_without_dialog() {
    let mut state = LoginState::new();
    let job = submit(&mut state, fields(false, false));
    assert_eq!(
        complete(&mut state, job, SignInOutcome::Obsolete),
        vec![LoginEffect::SetProgressIndicatorVisible(false)]
    );
    assert!(!state.view().busy);
    assert!(!state.view().progress_indicator_visible);
    assert_ignored(
        &mut state,
        LoginAction::SignInCompleted {
            job,
            outcome: SignInOutcome::Failed,
        },
    );
}

#[test]
fn sign_in_screen_failure_orders_focus_spinner_dialog_and_retains_busy() {
    let mut state = LoginState::new();
    let job = submit(&mut state, fields(false, false));
    assert_eq!(
        complete(&mut state, job, SignInOutcome::Failed),
        vec![
            LoginEffect::ClearFocus,
            LoginEffect::SetProgressIndicatorVisible(false),
            LoginEffect::ShowSignInFailure(job),
        ]
    );
    assert!(state.view().busy);
    assert!(!state.view().progress_indicator_visible);
    assert_ignored(&mut state, LoginAction::EnterGuest);
    assert_ignored(&mut state, LoginAction::Submit(fields(true, true)));
    assert_ignored(&mut state, LoginAction::FieldsEdited);
    assert_ignored(
        &mut state,
        LoginAction::SignInCompleted {
            job,
            outcome: SignInOutcome::Failed,
        },
    );
    assert_ignored(
        &mut state,
        LoginAction::SignInCompleted {
            job,
            outcome: SignInOutcome::Accepted,
        },
    );
    assert_ignored(&mut state, LoginAction::GuestCompleted { job });
    assert!(
        state
            .apply(LoginAction::FailureDialogCompleted { job })
            .is_empty()
    );
    assert!(!state.view().busy);
    assert!(!state.view().progress_indicator_visible);
    assert_ignored(&mut state, LoginAction::FailureDialogCompleted { job });
    let _ = guest(&mut state);
}

#[test]
fn sign_in_screen_dialog_coroutine_cancellation_keeps_already_hidden_spinner() {
    let mut state = LoginState::new();
    let job = submit(&mut state, fields(false, false));
    let _ = complete(&mut state, job, SignInOutcome::Failed);
    // Dialog confirmation and cancellation both terminate its await, not another auth outcome.
    assert!(
        state
            .apply(LoginAction::FailureDialogCompleted { job })
            .is_empty()
    );
    assert!(!state.view().busy);
    assert!(!state.view().progress_indicator_visible);
}

#[test]
fn sign_in_screen_guest_success_failure_and_cancellation_have_same_ui_termination() {
    for _host_outcome in ["success", "failure", "cancellation"] {
        let mut state = seed_errors(true, true);
        let before = state.view();
        let job = guest(&mut state);
        assert_eq!(
            state.view(),
            LoginView {
                busy: true,
                ..before
            }
        );
        assert_ignored(
            &mut state,
            LoginAction::SignInCompleted {
                job,
                outcome: SignInOutcome::Failed,
            },
        );
        assert_ignored(&mut state, LoginAction::FailureDialogCompleted { job });
        assert!(state.apply(LoginAction::GuestCompleted { job }).is_empty());
        assert_eq!(state.view(), before);
        assert_ignored(&mut state, LoginAction::GuestCompleted { job });
    }
}

#[test]
fn sign_in_screen_guest_does_not_hide_spinner_left_by_cancelled_password_job() {
    let mut state = LoginState::new();
    let password = submit(&mut state, fields(false, false));
    let _ = complete(&mut state, password, SignInOutcome::Cancelled);
    let guest_job = guest(&mut state);
    assert!(state.view().progress_indicator_visible);
    assert!(
        state
            .apply(LoginAction::GuestCompleted { job: guest_job })
            .is_empty()
    );
    assert!(state.view().progress_indicator_visible);
    assert!(!state.view().busy);
}

#[test]
fn sign_in_screen_registration_and_webview_never_fake_login_or_block_while_busy() {
    for phase in 0..4 {
        let mut state = LoginState::new();
        match phase {
            1 => {
                let _ = guest(&mut state);
            }
            2 => {
                let _ = submit(&mut state, fields(false, false));
            }
            3 => {
                let job = submit(&mut state, fields(false, false));
                let _ = complete(&mut state, job, SignInOutcome::Failed);
            }
            _ => {}
        }
        let before = state.view();
        assert_eq!(
            state.apply(LoginAction::OpenRegistration),
            vec![LoginEffect::OpenRegistration]
        );
        assert_eq!(state.view(), before);
        assert_eq!(
            state.apply(LoginAction::OpenWebView),
            vec![LoginEffect::NavigateWebViewSignIn]
        );
        assert_eq!(state.view(), before);
    }
}

#[test]
fn rust_execution_stale_password_callbacks_cannot_touch_new_password_or_guest_job() {
    let mut state = LoginState::new();
    let old = submit(&mut state, fields(false, false));
    let _ = complete(&mut state, old, SignInOutcome::Cancelled);
    let current = submit(&mut state, fields(false, false));
    assert_ne!(old, current);
    for outcome in [
        SignInOutcome::Accepted,
        SignInOutcome::Obsolete,
        SignInOutcome::Failed,
        SignInOutcome::Cancelled,
    ] {
        assert_ignored(
            &mut state,
            LoginAction::SignInCompleted { job: old, outcome },
        );
    }
    assert_ignored(&mut state, LoginAction::FailureDialogCompleted { job: old });
    assert_ignored(&mut state, LoginAction::GuestCompleted { job: old });
    let _ = complete(&mut state, current, SignInOutcome::Obsolete);
    let guest_job = guest(&mut state);
    assert_ne!(guest_job, current);
    assert_ignored(
        &mut state,
        LoginAction::SignInCompleted {
            job: current,
            outcome: SignInOutcome::Failed,
        },
    );
    assert_ignored(&mut state, LoginAction::GuestCompleted { job: old });
}

#[test]
fn rust_execution_stale_guest_and_dialog_callbacks_cannot_release_new_job() {
    let mut state = LoginState::new();
    let old_guest = guest(&mut state);
    let _ = state.apply(LoginAction::GuestCompleted { job: old_guest });
    let old_password = submit(&mut state, fields(false, false));
    let _ = complete(&mut state, old_password, SignInOutcome::Failed);
    let _ = state.apply(LoginAction::FailureDialogCompleted { job: old_password });
    let current = submit(&mut state, fields(false, false));
    assert_ignored(&mut state, LoginAction::GuestCompleted { job: old_guest });
    assert_ignored(
        &mut state,
        LoginAction::FailureDialogCompleted { job: old_password },
    );
    // Correct ID, wrong phase also cannot terminate a running password request.
    assert_ignored(
        &mut state,
        LoginAction::FailureDialogCompleted { job: current },
    );
    assert_ignored(&mut state, LoginAction::GuestCompleted { job: current });
    let _ = complete(&mut state, current, SignInOutcome::Failed);
    assert_ignored(
        &mut state,
        LoginAction::FailureDialogCompleted { job: old_password },
    );
    assert!(state.view().busy);
}

#[test]
fn rust_execution_actions_effects_and_state_debug_never_include_caller_credentials() {
    let caller_username = String::from("private-account-identifier");
    let caller_password = String::from("private-password-secret");
    let fields = fields(caller_username.is_empty(), caller_password.is_empty());
    let action = LoginAction::Submit(fields);
    let mut state = LoginState::new();
    let effects = state.apply(action);
    let debug = format!("{action:?} {effects:?} {state:?} {:?}", state.view());
    assert!(!debug.contains(caller_username.as_str()));
    assert!(!debug.contains(caller_password.as_str()));
    // Public data types have only enums, booleans, and opaque local IDs, no String/&str payloads.
}
