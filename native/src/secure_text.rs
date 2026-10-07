//! Pinned Compose secure-text obfuscation policy, not a password widget.
//!
//! Evidence: `native/parity/secure-text`. Caller owns text, focus notifications,
//! applied change ranges and timer-signal consumption. No IME, clipboard, undo,
//! keyboard-action or autofill authority lives here. Only edits for which Compose
//! runs its password input transformation may call [`SecureText::apply_input`].

use std::{fmt, ops::Range};

/// Pinned default obfuscation character (U+2022), one per JVM codepoint.
pub const OBFUSCATION_MASK: char = '\u{2022}';
/// Pinned delay, measured from consumption of a changed-index hide signal.
pub const REVEAL_DURATION_MS: u64 = 1500;

/// Compose presentation mode; original sign-in toggles the first two variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// Mask except eligible last typed index until its hide deadline.
    RevealLastTyped,
    /// Render original codepoints, including after blur.
    Visible,
    /// Mask every codepoint.
    Hidden,
}

/// Result supplied by the Android `show_password` setting owner.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum PlatformShowPassword {
    /// Actual integer returned by `Settings.System.getInt`.
    Setting(i32),
    /// Pinned resolver catches `Exception` and uses true (not a default guess).
    ReadException,
}

impl PlatformShowPassword {
    /// Matches the pinned Android resolver without doing platform I/O.
    #[must_use]
    pub const fn allows_reveal(self) -> bool {
        match self {
            Self::Setting(value) => value > 0,
            Self::ReadException => true,
        }
    }
}

impl fmt::Debug for PlatformShowPassword {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlatformShowPassword([REDACTED])")
    }
}

/// Payload-free reason for rejecting invalid caller metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyError {
    /// UTF-16 length or range endpoints exceed JVM bounds.
    InvalidBounds,
    /// Caller monotonic clock regressed; active reveal was cleared.
    ClockRegressed,
    /// Adding the pinned delay overflows caller clock; reveal was cleared.
    DeadlineOverflow,
}

/// What renderer may draw for a source span. Contains no credential characters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presentation {
    /// Draw caller-owned original UTF-16 span.
    Original,
    /// Draw [`OBFUSCATION_MASK`] instead of caller-owned source span.
    Mask,
}

/// One JVM codepoint, with UTF-16 source and visual ranges; owns no text.
#[derive(Clone, Eq, PartialEq)]
pub struct RenderSpan {
    /// Range in caller-owned UTF-16 source, including isolated surrogates.
    pub source_utf16: Range<usize>,
    /// Range in visual UTF-16 output (mask consumes one unit).
    pub visual_utf16: Range<usize>,
    /// Original span or mask instruction; not actual rendered text.
    pub presentation: Presentation,
}

impl fmt::Debug for RenderSpan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RenderSpan([REDACTED])")
    }
}

/// Single-field metadata owner. Never stores text, credentials or input origin.
///
/// Preserve source quirk: changed range minima are UTF-16 indices, but rendering
/// compares them with codepoint ordinals. Do not normalize this discrepancy.
pub struct SecureText {
    mode: Mode,
    platform: PlatformShowPassword,
    reveal_index: Option<usize>,
    deadline_ms: Option<u64>,
    last_time_ms: Option<u64>,
}

impl fmt::Debug for SecureText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecureText([REDACTED])")
    }
}

impl SecureText {
    /// Creates independent field state. Caller must supply actual platform result.
    #[must_use]
    pub const fn new(mode: Mode, platform: PlatformShowPassword) -> Self {
        Self {
            mode,
            platform,
            reveal_index: None,
            deadline_ms: None,
            last_time_ms: None,
        }
    }

    /// Changes presentation; disabling reveal clears pending reveal state.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if !self.reveal_enabled() {
            self.hide();
        }
    }

    /// Receives setting observer result; disabling reveal clears pending reveal.
    pub fn set_platform(&mut self, platform: PlatformShowPassword) {
        self.platform = platform;
        if !self.reveal_enabled() {
            self.hide();
        }
    }

    /// Receives authoritative focus notification, not a focus request.
    ///
    /// Source attaches blur hiding only when reveal is enabled. Visible mode
    /// remains visible after blur; this helper must not substitute fully-hidden UI.
    pub fn on_focus_changed(&mut self, is_focused: bool) {
        if self.reveal_enabled() && !is_focused {
            self.hide();
        }
    }

    /// Applies post-transformation metadata, with caller-injected monotonic time.
    ///
    /// `changes` contains new-buffer UTF-16 ranges, not original ranges. Reversed
    /// ranges use their absolute length/minimum, matching `TextRange`. Single-unit
    /// paste, replacement and composition are not special-cased by pinned source.
    /// Selection-only edits need an empty change list. Programmatic changes that
    /// bypass Compose input transformation must not call this method.
    ///
    /// When index changes, `now_ms` models timer-signal consumption, not raw IME
    /// arrival. Runtime scheduling/coalescing equivalence remains an external gate.
    /// Same-index edits do not reset timer. This seam never mutates caller text.
    ///
    /// # Errors
    /// Invalid bounds, clock regression or deadline overflow clear active reveal.
    pub fn apply_input(
        &mut self,
        text_utf16_len: usize,
        changes: &[Range<usize>],
        selection_utf16: Range<usize>,
        now_ms: u64,
    ) -> Result<(), PolicyError> {
        self.advance(now_ms)?;
        if text_utf16_len > i32::MAX as usize
            || selection_utf16.start > text_utf16_len
            || selection_utf16.end > text_utf16_len
            || changes
                .iter()
                .any(|range| range.start > text_utf16_len || range.end > text_utf16_len)
        {
            self.hide();
            return Err(PolicyError::InvalidBounds);
        }
        if !self.reveal_enabled() {
            return Ok(());
        }
        if changes.len() != 1
            || changes[0].start.abs_diff(changes[0].end) != 1
            || selection_utf16.start != selection_utf16.end
        {
            self.hide();
            return Ok(());
        }
        let index = changes[0].start.min(changes[0].end);
        if self.reveal_index != Some(index) {
            let Some(deadline) = now_ms.checked_add(REVEAL_DURATION_MS) else {
                self.hide();
                return Err(PolicyError::DeadlineOverflow);
            };
            self.reveal_index = Some(index);
            self.deadline_ms = Some(deadline);
        }
        Ok(())
    }

    /// Advances current owner time; stale wakeups cannot hide a newer reveal.
    ///
    /// Caller re-reads [`Self::hide_deadline_ms`] after every event rather than
    /// retaining authority in callbacks. No timer generations or IME ownership.
    ///
    /// # Errors
    /// Regressing time clears reveal and returns [`PolicyError::ClockRegressed`].
    pub fn advance(&mut self, now_ms: u64) -> Result<(), PolicyError> {
        if self.last_time_ms.is_some_and(|previous| now_ms < previous) {
            self.hide();
            return Err(PolicyError::ClockRegressed);
        }
        self.last_time_ms = Some(now_ms);
        if self.deadline_ms.is_some_and(|deadline| now_ms >= deadline) {
            self.hide();
        }
        Ok(())
    }

    /// Current owner deadline. Invalidations may clear it before it fires.
    #[must_use]
    pub const fn hide_deadline_ms(&self) -> Option<u64> {
        self.deadline_ms
    }

    /// Returns payload-free rendering instructions for borrowed JVM UTF-16 text.
    ///
    /// Valid surrogate pairs form one codepoint; unpaired surrogates consume one
    /// unit, matching JVM `codePointAt`. Visible output retains source units.
    ///
    /// # Errors
    /// Regressing time or text beyond JVM bounds clears active reveal.
    pub fn render_spans(
        &mut self,
        text_utf16: &[u16],
        now_ms: u64,
    ) -> Result<Vec<RenderSpan>, PolicyError> {
        self.advance(now_ms)?;
        if text_utf16.len() > i32::MAX as usize {
            self.hide();
            return Err(PolicyError::InvalidBounds);
        }
        let mut spans = Vec::new();
        let mut source = 0;
        let mut visual = 0;
        while source < text_utf16.len() {
            let pair = (0xd800..=0xdbff).contains(&text_utf16[source])
                && text_utf16
                    .get(source + 1)
                    .is_some_and(|unit| (0xdc00..=0xdfff).contains(unit));
            let width = if pair { 2 } else { 1 };
            let original = self.mode == Mode::Visible
                || (self.reveal_enabled() && self.reveal_index == Some(spans.len()));
            let visual_width = if original { width } else { 1 };
            spans.push(RenderSpan {
                source_utf16: source..source + width,
                visual_utf16: visual..visual + visual_width,
                presentation: if original {
                    Presentation::Original
                } else {
                    Presentation::Mask
                },
            });
            source += width;
            visual += visual_width;
        }
        Ok(spans)
    }

    fn reveal_enabled(&self) -> bool {
        self.mode == Mode::RevealLastTyped && self.platform.allows_reveal()
    }

    fn hide(&mut self) {
        self.reveal_index = None;
        self.deadline_ms = None;
    }
}
