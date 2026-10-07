//! Source-bound secure-text regressions; synthetic UTF-16 only, no UI acceptance.
use eh_native::secure_text::{
    Mode, OBFUSCATION_MASK, PlatformShowPassword, PolicyError, Presentation, REVEAL_DURATION_MS,
    SecureText,
};

fn change(start: usize, end: usize) -> [std::ops::Range<usize>; 1] {
    [std::ops::Range { start, end }]
}

fn owner() -> SecureText {
    SecureText::new(Mode::RevealLastTyped, PlatformShowPassword::Setting(1))
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn presentations(owner: &mut SecureText, text: &[u16], now: u64) -> String {
    owner
        .render_spans(text, now)
        .unwrap()
        .iter()
        .map(|span| match span.presentation {
            Presentation::Original => 'O',
            Presentation::Mask => 'M',
        })
        .collect()
}

#[test]
fn pinned_bytecode_case_fixtures() {
    // Explicit independent fixtures transcribed from frozen bytecode predicates.
    // Columns: id, UTF-16 hex, changed range, selection, observation ms, spans.
    for line in include_str!("../parity/secure-text/cases.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        let text: Vec<_> = fields[1]
            .split(',')
            .map(|unit| u16::from_str_radix(unit, 16).unwrap())
            .collect();
        let ranges: Vec<_> = fields[2]
            .split(',')
            .filter(|range| *range != "-")
            .map(|range| {
                let (start, end) = range.split_once(':').unwrap();
                start.parse().unwrap()..end.parse().unwrap()
            })
            .collect();
        let (start, end) = fields[3].split_once(':').unwrap();
        let mut state = owner();
        state
            .apply_input(
                text.len(),
                &ranges,
                start.parse().unwrap()..end.parse().unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(
            presentations(&mut state, &text, fields[4].parse().unwrap()),
            fields[5],
            "fixture {}",
            fields[0]
        );
    }
}

#[test]
fn exact_delay_boundaries() {
    assert_eq!(REVEAL_DURATION_MS, 1500);
    assert_eq!(OBFUSCATION_MASK, '\u{2022}');
    for elapsed in [1499, 1500, 1501] {
        let mut state = owner();
        state.apply_input(1, &change(0, 1), 1..1, 40).unwrap();
        assert_eq!(state.hide_deadline_ms(), Some(1540));
        let expected = if elapsed < 1500 { "O" } else { "M" };
        assert_eq!(
            presentations(&mut state, &units("x"), 40 + elapsed),
            expected
        );
    }
}

#[test]
fn same_index_replacement_does_not_extend_timer() {
    let mut state = owner();
    state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
    state.apply_input(1, &change(0, 1), 1..1, 1000).unwrap();
    assert_eq!(state.hide_deadline_ms(), Some(1500));
    assert_eq!(presentations(&mut state, &units("z"), 1500), "M");
    state.apply_input(1, &change(0, 1), 1..1, 1500).unwrap();
    assert_eq!(state.hide_deadline_ms(), Some(3000));
}

#[test]
fn changed_index_cancels_old_deadline_and_stale_wakeup() {
    let mut state = owner();
    state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
    state.apply_input(2, &change(1, 2), 2..2, 1000).unwrap();
    state.advance(1500).unwrap();
    assert_eq!(state.hide_deadline_ms(), Some(2500));
    assert_eq!(presentations(&mut state, &units("xy"), 1501), "MO");
    assert_eq!(presentations(&mut state, &units("xy"), 2500), "MM");
}

#[test]
fn selection_deletion_and_multi_change_hide() {
    for (changes, selection) in [
        (vec![], 0..0),
        (change(0, 0).to_vec(), 0..0),
        (change(0, 1).to_vec(), 0..1),
        (change(0, 1).to_vec(), std::ops::Range { start: 1, end: 0 }),
        (vec![0..1, 1..2], 2..2),
        (change(0, 2).to_vec(), 2..2),
    ] {
        let mut state = owner();
        state.apply_input(2, &change(1, 2), 2..2, 0).unwrap();
        state.apply_input(2, &changes, selection, 1).unwrap();
        assert_eq!(state.hide_deadline_ms(), None);
        assert_eq!(presentations(&mut state, &units("xy"), 1), "MM");
        // Hiding resets index, allowing same range to schedule again.
        state.apply_input(2, &change(1, 2), 2..2, 2).unwrap();
        assert_eq!(state.hide_deadline_ms(), Some(1502));
    }
}

#[test]
fn focus_notifications_not_focus_authority() {
    let mut state = owner();
    state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
    state.on_focus_changed(true);
    assert_eq!(presentations(&mut state, &units("x"), 0), "O");
    state.on_focus_changed(false);
    assert_eq!(state.hide_deadline_ms(), None);
    assert_eq!(presentations(&mut state, &units("x"), 0), "M");
    // Source predicate has no focus check. Caller decides whether transform ran.
    state.apply_input(1, &change(0, 1), 1..1, 1).unwrap();
    assert_eq!(presentations(&mut state, &units("x"), 1), "O");
}

#[test]
fn mode_change_clears_reveal_but_visible_stays_visible_on_blur() {
    let mut state = owner();
    state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
    state.set_mode(Mode::RevealLastTyped);
    assert_eq!(state.hide_deadline_ms(), Some(1500));
    state.set_mode(Mode::Visible);
    state.on_focus_changed(false);
    state.apply_input(2, &change(1, 2), 2..2, 1).unwrap();
    assert_eq!(presentations(&mut state, &units("xy"), 1), "OO");
    assert_eq!(state.hide_deadline_ms(), None);
    state.set_mode(Mode::RevealLastTyped);
    assert_eq!(presentations(&mut state, &units("xy"), 1), "MM");
    state.apply_input(2, &change(1, 2), 2..2, 2).unwrap();
    state.set_mode(Mode::Hidden);
    assert_eq!(presentations(&mut state, &units("xy"), 2), "MM");
    state.set_mode(Mode::RevealLastTyped);
    assert_eq!(presentations(&mut state, &units("xy"), 2), "MM");
}

#[test]
fn platform_setting_values_exception_and_observer_changes() {
    for setting in [-1, 0, 1, i32::MAX] {
        let platform = PlatformShowPassword::Setting(setting);
        assert_eq!(platform.allows_reveal(), setting > 0);
        let mut state = SecureText::new(Mode::RevealLastTyped, platform);
        state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
        assert_eq!(
            presentations(&mut state, &units("x"), 0),
            if setting > 0 { "O" } else { "M" }
        );
    }
    let mut state = owner();
    state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
    state.set_platform(PlatformShowPassword::Setting(2));
    assert_eq!(state.hide_deadline_ms(), Some(1500));
    state.set_platform(PlatformShowPassword::Setting(0));
    assert_eq!(state.hide_deadline_ms(), None);
    state.set_platform(PlatformShowPassword::ReadException);
    assert!(PlatformShowPassword::ReadException.allows_reveal());
    assert_eq!(presentations(&mut state, &units("x"), 0), "M");
    state.apply_input(1, &change(0, 1), 1..1, 1).unwrap();
    assert_eq!(presentations(&mut state, &units("x"), 1), "O");
    state.set_mode(Mode::Visible);
    state.set_platform(PlatformShowPassword::Setting(0));
    assert_eq!(presentations(&mut state, &units("x"), 1), "O");
}

#[test]
fn unicode_source_visual_ranges_and_utf16_ordinal_mismatch() {
    let text = units("\u{1f600}xyz");
    let mut state = owner();
    // UTF-16 minimum=2, compared to codepoint ordinal=2: reveals y, not x.
    state.apply_input(5, &change(2, 3), 3..3, 0).unwrap();
    assert_eq!(presentations(&mut state, &text, 0), "MMOM");
    let spans = state.render_spans(&text, 0).unwrap();
    assert_eq!(spans[0].source_utf16, 0..2);
    assert_eq!(spans[0].visual_utf16, 0..1);
    assert_eq!(spans[2].source_utf16, 3..4);
    assert_eq!(spans[2].visual_utf16, 2..3);
    state.set_mode(Mode::Visible);
    let spans = state.render_spans(&text, 0).unwrap();
    assert_eq!(spans[0].visual_utf16, 0..2);
    assert_eq!(spans[3].visual_utf16, 4..5);
    state.set_mode(Mode::Hidden);
    assert_eq!(presentations(&mut state, &[], 0), "");
    assert_eq!(presentations(&mut state, &[0xd800, 0x78, 0xdc00], 0), "MMM");
    assert_eq!(presentations(&mut state, &[0xd800, 0xd800], 0), "MM");
}

#[test]
fn invalid_bounds_fail_closed_without_substituting_visible_mode() {
    for (len, range, selection) in [
        (1, 0..2, 0..0),
        (1, std::ops::Range { start: 2, end: 0 }, 0..0),
        (1, 0..1, std::ops::Range { start: 2, end: 0 }),
        (1, 0..1, 0..2),
        (i32::MAX as usize + 1, 0..1, 0..0),
        (0, 0..1, 0..0),
    ] {
        let mut state = owner();
        state.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
        assert_eq!(
            state.apply_input(len, &[range], selection, 1),
            Err(PolicyError::InvalidBounds)
        );
        assert_eq!(state.hide_deadline_ms(), None);
        assert_eq!(presentations(&mut state, &units("x"), 1), "M");
    }
    let mut visible = SecureText::new(Mode::Visible, PlatformShowPassword::Setting(0));
    assert_eq!(
        visible.apply_input(0, &change(0, 1), 0..0, 0),
        Err(PolicyError::InvalidBounds)
    );
    assert_eq!(presentations(&mut visible, &units("x"), 0), "O");
}

#[test]
fn clock_regression_overflow_and_latest_clock_retained() {
    let mut state = owner();
    state.apply_input(1, &change(0, 1), 1..1, 50).unwrap();
    assert_eq!(state.advance(49), Err(PolicyError::ClockRegressed));
    assert_eq!(state.hide_deadline_ms(), None);
    assert_eq!(
        state.apply_input(1, &change(0, 1), 1..1, 49),
        Err(PolicyError::ClockRegressed)
    );
    assert_eq!(
        state.render_spans(&units("x"), 49),
        Err(PolicyError::ClockRegressed)
    );
    assert_eq!(presentations(&mut state, &units("x"), 50), "M");
    assert_eq!(
        state.apply_input(1, &change(0, 1), 1..1, u64::MAX - 1499),
        Err(PolicyError::DeadlineOverflow)
    );
    assert_eq!(state.hide_deadline_ms(), None);
    let mut edge = owner();
    edge.apply_input(1, &change(0, 1), 1..1, u64::MAX - 1500)
        .unwrap();
    assert_eq!(edge.hide_deadline_ms(), Some(u64::MAX));
    assert_eq!(presentations(&mut edge, &units("x"), u64::MAX), "M");
}

#[test]
fn independent_owners_and_redacted_debug() {
    let mut first = owner();
    let mut second = owner();
    first.apply_input(1, &change(0, 1), 1..1, 0).unwrap();
    second.apply_input(1, &change(0, 1), 1..1, 500).unwrap();
    assert_eq!(presentations(&mut first, &units("x"), 1500), "M");
    assert_eq!(presentations(&mut second, &units("x"), 1500), "O");
    assert_eq!(format!("{second:?}"), "SecureText([REDACTED])");
    assert_eq!(
        format!("{:?}", second.render_spans(&units("x"), 1500).unwrap()[0]),
        "RenderSpan([REDACTED])"
    );
    assert_eq!(
        format!("{:?}", PlatformShowPassword::Setting(123_456)),
        "PlatformShowPassword([REDACTED])"
    );
}
