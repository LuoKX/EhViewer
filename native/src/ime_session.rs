//! Focused Android text transport, not the original secure-field editing policy.
//!
//! Native spans are Java UTF-16 offsets; keep them separate from UTF-8 bytes and
//! renderer character cursors. Local lease IDs protect already-tagged callbacks;
//! they cannot identify an old native event which the SDK delivers after a focus
//! change without its own session ID. Real focus/IME/device gates remain open.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// One-shot runtime claim. eframe caches Android event-loop handlers across
/// invocations; this prototype rejects re-entry instead of binding a new editor
/// to an old handler. A failed first invocation also requires process restart.
#[derive(Debug, Default)]
pub struct OneShotRuntime(AtomicBool);

impl OneShotRuntime {
    /// New process-local claim flag, initially unclaimed.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// Claim once atomically. Never reset at app drop or failed window creation.
    #[must_use]
    pub fn claim(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// Bounded diagnostic transport limit; oversize input rejects, never truncates.
/// This is not an accepted production username/password limit.
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
const MAX_ACTIONS: usize = 32;

/// UTF-16 selection/composing offsets, never UTF-8 byte or grapheme indices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Utf16Span {
    /// Selection anchor or composing start, inclusive.
    pub start: usize,
    /// Selection focus or composing end, exclusive; reversed selection allowed.
    pub end: usize,
}

/// Full IME buffer and offsets. Debug deliberately omits all text.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct TextState {
    text: String,
    /// Native UTF-16 selection; may point inside a surrogate pair.
    selection: Utf16Span,
    /// Native composing region; `None` after commit/cancel.
    composing: Option<Utf16Span>,
}

impl fmt::Debug for TextState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TextState { text: <redacted>, offsets: <redacted> }")
    }
}

/// Invalid native transport boundary; errors contain no user input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    /// Input exceeds diagnostic transport budget.
    TextTooLarge,
    /// Selection/composing offset outside actual UTF-16 extent.
    InvalidSpan,
    /// Stale or inactive local focus lease.
    StaleLease,
    /// Local ID exhausted; never wrap or reuse a lease.
    LeaseExhausted,
    /// Action queue overflow; invalidate focus instead of silently dropping work.
    ActionOverflow,
}

impl TextState {
    /// Validate complete buffer/offsets atomically without normalizing text.
    ///
    /// # Errors
    /// Returns a text-free error for oversize buffers or invalid UTF-16 spans.
    pub fn new(
        text: String,
        selection: Utf16Span,
        composing: Option<Utf16Span>,
    ) -> Result<Self, TransportError> {
        if text.len() > MAX_TEXT_BYTES {
            return Err(TransportError::TextTooLarge);
        }
        let units = text.encode_utf16().count();
        for span in std::iter::once(selection).chain(composing) {
            if span.start > units || span.end > units {
                return Err(TransportError::InvalidSpan);
            }
        }
        Ok(Self {
            text,
            selection,
            composing,
        })
    }

    /// Borrow text for caller-owned rendering/effects only; do not log or persist.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Native selection, preserving UTF-16 units and reversed anchor/focus.
    #[must_use]
    pub const fn selection(&self) -> Utf16Span {
        self.selection
    }

    /// Native composing span; independent of selection and renderer cursors.
    #[must_use]
    pub const fn composing(&self) -> Option<Utf16Span> {
        self.composing
    }
}

/// Explicit editor action, independent of hardware Enter events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorAction {
    /// SDK requests receiver-defined action.
    Unspecified,
    /// Newline/no explicit action.
    None,
    /// Navigate current text.
    Go,
    /// Search current text.
    Search,
    /// Send current text.
    Send,
    /// Focus next editor.
    Next,
    /// Complete current editor.
    Done,
    /// Focus previous editor.
    Previous,
    /// Unrecognized SDK integer preserved; no fake default action.
    Unknown(i32),
}

impl From<i32> for EditorAction {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Unspecified,
            1 => Self::None,
            2 => Self::Go,
            3 => Self::Search,
            4 => Self::Send,
            5 => Self::Next,
            6 => Self::Done,
            7 => Self::Previous,
            other => Self::Unknown(other),
        }
    }
}

/// Opaque local callback lease; never a canonical authentication/session token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lease(u64);

/// One complete editor-action callback, with text snapshot at receipt time.
/// Later local edits cannot change this snapshot. SDK native text/action slots
/// are not atomic or chronological; this does not prove original submit text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionEvent {
    /// Explicit SDK action; caller decides effects.
    pub action: EditorAction,
    /// Full state when action arrived; Debug stays redacted.
    pub state: TextState,
}

/// One focused editor buffer plus bounded ordered action snapshots, memory-only.
/// No OS polling, keyboard calls, authentication or secure-text behavior here.
#[derive(Default)]
pub struct Session {
    next: u64,
    active: Option<Lease>,
    state: TextState,
    actions: VecDeque<ActionEvent>,
    revision: u64,
    native_owned: bool,
}

impl fmt::Debug for Session {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Session { input: <redacted>, callbacks: <redacted> }")
    }
}

impl Session {
    /// Start explicit focus lease; discard old buffer/actions without retaining
    /// them for undo or restoring another field.
    ///
    /// # Errors
    /// Exhausted monotonically increasing lease IDs fail without wrapping.
    pub fn focus(&mut self, state: TextState) -> Result<Lease, TransportError> {
        let next = self
            .next
            .checked_add(1)
            .ok_or(TransportError::LeaseExhausted)?;
        self.blur();
        self.next = next;
        let lease = Lease(next);
        self.active = Some(lease);
        self.state = state;
        Ok(lease)
    }

    /// Discard active text/actions on explicit blur/lifecycle disposal.
    /// Memory allocator zeroization is not claimed by clearing a Rust String.
    /// Native-buffer ownership deliberately survives local invalidation until
    /// caller consumes its explicit reset request.
    pub fn blur(&mut self) {
        self.active = None;
        self.state = TextState::default();
        self.actions.clear();
        self.revision = 0;
    }

    /// Mark that caller has configured this editor's native SDK buffer.
    pub fn claim_native_buffer(&mut self) {
        self.native_owned = true;
    }

    /// Consume exactly one requested SDK reset, even after local error/blur.
    /// Call native reset after releasing the transport lock. Repeated idle frames
    /// must not reset another widget's IME buffer.
    #[must_use]
    pub fn take_native_clear_request(&mut self) -> bool {
        std::mem::take(&mut self.native_owned)
    }

    /// Current lease for synchronously tagging native callback receipt.
    /// Does not prove the SDK event originated from this native focus generation.
    #[must_use]
    pub const fn active(&self) -> Option<Lease> {
        self.active
    }

    /// Full buffered native state, only while focus is active.
    #[must_use]
    pub fn state(&self) -> Option<&TextState> {
        self.active.map(|_| &self.state)
    }

    /// Apply already-validated complete text/selection/composition, not a diff or
    /// invented keypress. Superseded updates are coalesced by the native SDK.
    ///
    /// # Errors
    /// Reject inactive/stale tagged callbacks without touching new editor state.
    pub fn update(&mut self, lease: Lease, state: TextState) -> Result<(), TransportError> {
        self.require(lease)?;
        self.state = state;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Preserve ordered SDK actions with the current full text snapshot.
    ///
    /// # Errors
    /// Reject stale lease. Overflow invalidates editor; no callback is silently
    /// dropped or converted into successful submission.
    pub fn action(&mut self, lease: Lease, action: EditorAction) -> Result<(), TransportError> {
        self.require(lease)?;
        if self.actions.len() == MAX_ACTIONS {
            self.blur();
            return Err(TransportError::ActionOverflow);
        }
        self.actions.push_back(ActionEvent {
            action,
            state: self.state.clone(),
        });
        Ok(())
    }

    /// Remove exactly one pending action snapshot; caller owns validation/effect.
    pub fn take_action(&mut self) -> Option<ActionEvent> {
        self.actions.pop_front()
    }

    /// Observable local update counter; not an input or native session token.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    fn require(&self, lease: Lease) -> Result<(), TransportError> {
        if self.active == Some(lease) {
            Ok(())
        } else {
            Err(TransportError::StaleLease)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(text: &str, start: usize, end: usize, composing: Option<Utf16Span>) -> TextState {
        TextState::new(text.into(), Utf16Span { start, end }, composing).unwrap()
    }

    #[test]
    fn teardown_reset_survives_local_invalidation_and_runtime_reentry_is_rejected() {
        let runtime = OneShotRuntime::new();
        assert!(runtime.claim());
        let mut session = Session::default();
        assert!(!session.take_native_clear_request());
        let lease = session.focus(state("dummy", 5, 5, None)).unwrap();
        session.claim_native_buffer();
        session.action(lease, EditorAction::Done).unwrap();
        session.blur();
        assert_eq!(session.state(), None);
        assert_eq!(session.take_action(), None);
        assert_eq!(session.active(), None);
        assert!(session.take_native_clear_request());
        assert!(!session.take_native_clear_request());
        assert!(!runtime.claim());
        assert!(OneShotRuntime::default().claim());
        // SDK reset also survives action-overflow invalidation.
        let lease = session.focus(TextState::default()).unwrap();
        session.claim_native_buffer();
        for _ in 0..MAX_ACTIONS {
            session.action(lease, EditorAction::Done).unwrap();
        }
        assert_eq!(
            session.action(lease, EditorAction::Done),
            Err(TransportError::ActionOverflow)
        );
        assert!(session.take_native_clear_request());
    }

    #[test]
    fn full_unicode_buffer_nul_utf16_selection_and_composition_preserved() {
        let text = "a中😀\0e\u{301}";
        let composing = Some(Utf16Span { start: 1, end: 4 });
        let original = state(text, 4, 1, composing);
        assert_eq!(original.text(), text);
        assert_eq!(original.selection(), Utf16Span { start: 4, end: 1 });
        assert_eq!(original.composing(), composing);
        // UTF-16 code-unit transport may represent a mid-surrogate cursor;
        // renderer/secure editing must separately settle that boundary.
        assert!(TextState::new("😀".into(), Utf16Span { start: 1, end: 1 }, None).is_ok());
        for span in [
            Utf16Span { start: 3, end: 0 },
            Utf16Span { start: 0, end: 3 },
        ] {
            assert_eq!(
                TextState::new("😀".into(), span, None),
                Err(TransportError::InvalidSpan)
            );
            assert_eq!(
                TextState::new("😀".into(), Utf16Span::default(), Some(span)),
                Err(TransportError::InvalidSpan)
            );
        }
        assert_eq!(
            TextState::new("x".repeat(MAX_TEXT_BYTES + 1), Utf16Span::default(), None),
            Err(TransportError::TextTooLarge)
        );
    }

    #[test]
    fn locally_tagged_callbacks_reject_stale_leases() {
        let mut session = Session::default();
        assert_eq!(session.active(), None);
        assert_eq!(session.state(), None);
        let first = session.focus(state("first", 5, 5, None)).unwrap();
        let second = session.focus(state("second", 6, 6, None)).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            session.update(first, state("old", 3, 3, None)),
            Err(TransportError::StaleLease)
        );
        assert_eq!(
            session.action(first, EditorAction::Done),
            Err(TransportError::StaleLease)
        );
        assert_eq!(session.state().unwrap().text(), "second");
        session.blur();
        assert_eq!(session.state(), None);
        assert_eq!(
            session.update(second, TextState::default()),
            Err(TransportError::StaleLease)
        );
        assert_eq!(session.take_action(), None);
        session.next = u64::MAX;
        assert_eq!(
            session.focus(TextState::default()),
            Err(TransportError::LeaseExhausted)
        );
    }

    #[test]
    fn preedit_commit_cancel_and_selection_update_replace_whole_state() {
        let mut session = Session::default();
        let lease = session.focus(TextState::default()).unwrap();
        session
            .update(
                lease,
                state("ni", 2, 2, Some(Utf16Span { start: 0, end: 2 })),
            )
            .unwrap();
        assert_eq!(session.state().unwrap().text(), "ni");
        session.update(lease, state("你", 1, 1, None)).unwrap();
        assert_eq!(session.state().unwrap().composing, None);
        session.update(lease, state("", 0, 0, None)).unwrap();
        assert_eq!(session.state().unwrap().text(), "");
        assert_eq!(session.revision(), 3);
    }

    #[test]
    fn ordered_actions_snapshot_submission_text_before_later_edits() {
        let mut session = Session::default();
        let lease = session.focus(state("before", 6, 6, None)).unwrap();
        session.action(lease, EditorAction::Next).unwrap();
        session.update(lease, state("after", 5, 5, None)).unwrap();
        session.action(lease, EditorAction::Done).unwrap();
        let first = session.take_action().unwrap();
        assert_eq!(first.action, EditorAction::Next);
        assert_eq!(first.state.text(), "before");
        let second = session.take_action().unwrap();
        assert_eq!(second.action, EditorAction::Done);
        assert_eq!(second.state.text(), "after");
        assert_eq!(session.take_action(), None);
        for _ in 0..MAX_ACTIONS {
            session.action(lease, EditorAction::Done).unwrap();
        }
        assert_eq!(
            session.action(lease, EditorAction::Done),
            Err(TransportError::ActionOverflow)
        );
        assert_eq!(session.active(), None);
        assert_eq!(session.take_action(), None);
    }

    #[test]
    fn actions_preserve_unknown_numbers_and_debug_never_exposes_text() {
        assert_eq!(
            (0..8).map(EditorAction::from).collect::<Vec<_>>(),
            vec![
                EditorAction::Unspecified,
                EditorAction::None,
                EditorAction::Go,
                EditorAction::Search,
                EditorAction::Send,
                EditorAction::Next,
                EditorAction::Done,
                EditorAction::Previous
            ]
        );
        assert_eq!(EditorAction::from(-9), EditorAction::Unknown(-9));
        let mut session = Session::default();
        let lease = session
            .focus(state("dummy-private-string", 1, 1, None))
            .unwrap();
        session.action(lease, EditorAction::Done).unwrap();
        assert!(!format!("{session:?}").contains("dummy-private-string"));
        assert!(!format!("{:?}", session.state()).contains("dummy-private-string"));
        assert!(!format!("{:?}", session.take_action()).contains("dummy-private-string"));
    }
}
