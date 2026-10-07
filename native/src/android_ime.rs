//! Android single-owner diagnostic text bridge. Not a production secure editor.

use crate::ime_session::{EditorAction, Session, TextState, TransportError, Utf16Span};
use eframe::egui;
use std::sync::{Arc, Mutex};
use winit::platform::android::activity::{
    AndroidApp, InputStatus,
    input::{ImeOptions, InputEvent, InputType, TextInputAction, TextInputState, TextSpan},
};

#[derive(Default)]
struct Shared {
    session: Session,
    context: Option<egui::Context>,
    error: Option<TransportError>,
}

#[derive(Clone)]
pub(crate) struct AndroidIme {
    app: AndroidApp,
    shared: Arc<Mutex<Shared>>,
}

/// Configure SDK fields explicitly; actionId does not set imeOptions action bits.
/// Password branch is platform configuration only, not original field acceptance.
fn editor_info(
    password: bool,
    multiline: bool,
    action: TextInputAction,
) -> (InputType, ImeOptions) {
    let mut input = InputType::TYPE_CLASS_TEXT;
    if password {
        input |= InputType::TYPE_TEXT_VARIATION_PASSWORD;
    }
    if multiline {
        input |= InputType::TYPE_TEXT_FLAG_MULTI_LINE;
    }
    let mut options = ImeOptions::IME_FLAG_NO_FULLSCREEN;
    options.set_action(action);
    (input, options)
}

impl AndroidIme {
    pub(crate) fn new(app: AndroidApp) -> Self {
        Self {
            app,
            shared: Arc::default(),
        }
    }

    pub(crate) fn attach(&self, context: egui::Context) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.context = Some(context);
        }
    }

    /// Called only by Winit's existing input iterator, never a second poller.
    pub(crate) fn event(&self, event: &InputEvent<'_>) -> Option<InputStatus> {
        if !matches!(event, InputEvent::TextEvent(_) | InputEvent::TextAction(_)) {
            return None;
        }
        let Ok(mut shared) = self.shared.lock() else {
            // Poisoned diagnostic session fails closed. No event payload logging.
            return Some(InputStatus::Unhandled);
        };
        let Some(lease) = shared.session.active() else {
            return Some(InputStatus::Unhandled);
        };
        let mut reset_native = false;
        let result = match event {
            InputEvent::TextEvent(state) => TextState::new(
                state.text.clone(),
                Utf16Span {
                    start: state.selection.start,
                    end: state.selection.end,
                },
                state.compose_region.map(|span| Utf16Span {
                    start: span.start,
                    end: span.end,
                }),
            )
            .and_then(|state| shared.session.update(lease, state)),
            InputEvent::TextAction(action) => {
                let value: i32 = (*action).into();
                shared.session.action(lease, EditorAction::from(value))
            }
            _ => unreachable!("non-text input delegated before locking"),
        };
        if let Err(error) = result {
            shared.session.blur();
            shared.error = Some(error);
            reset_native = shared.session.take_native_clear_request();
        }
        let context = shared.context.clone();
        drop(shared);
        if reset_native {
            self.reset_sdk();
        }
        if let Some(context) = context {
            context.request_repaint();
        }
        // GameActivity ignores InputStatus; Some suppresses Winit dispatch only.
        Some(InputStatus::Handled)
    }

    pub(crate) fn begin_diagnostic(&self) {
        let Ok(mut shared) = self.shared.lock() else {
            return;
        };
        shared.error = None;
        if let Err(error) = shared.session.focus(TextState::default()) {
            shared.error = Some(error);
            return;
        }
        shared.session.claim_native_buffer();
        drop(shared);
        // Diagnostic multiline text only. Do not infer original password defaults.
        let (input, options) = editor_info(false, true, TextInputAction::None);
        self.app
            .set_ime_editor_info(input, TextInputAction::None, options);
        self.app.set_text_input_state(TextInputState {
            text: String::new(),
            selection: TextSpan { start: 0, end: 0 },
            compose_region: None,
        });
        self.app.show_soft_input(false);
    }

    pub(crate) fn blur(&self) {
        // Clear/dispose remains available after a rendering panic poisons mutex.
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.session.blur();
        let reset_native = shared.session.take_native_clear_request();
        drop(shared);
        if reset_native {
            self.reset_sdk();
        }
    }

    fn reset_sdk(&self) {
        self.app.set_text_input_state(TextInputState::default());
        self.app.hide_soft_input(false);
    }

    /// App teardown releases the handler's retained GUI context and local state.
    /// SDK reset is best-effort; no allocator/native zeroization guarantee.
    pub(crate) fn dispose(&self) {
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.session.blur();
        shared.context = None;
        shared.error = None;
        let reset_native = shared.session.take_native_clear_request();
        drop(shared);
        if reset_native {
            self.reset_sdk();
        }
    }

    pub(crate) fn ui(&self, ui: &mut egui::Ui) {
        ui.label(
            "Do not enter credentials. Diagnostic transport and lifecycle are not production-safe.",
        );
        ui.label("Full native text, selection and composing offsets. UTF-16 units, not UTF-8 cursor positions.");
        ui.horizontal(|ui| {
            if ui.button("Start dummy text session").clicked() {
                self.begin_diagnostic();
            }
            if ui.button("Blur and clear").clicked() {
                self.blur();
            }
        });
        let Ok(mut shared) = self.shared.lock() else {
            ui.label("Diagnostic session unavailable.");
            return;
        };
        if let Some(error) = shared.error {
            ui.label(format!("Diagnostic transport rejected: {error:?}"));
        }
        if let Some(state) = shared.session.state() {
            ui.label(state.text());
            ui.label(format!(
                "Selection: {:?}; composition: {:?}",
                state.selection(),
                state.composing()
            ));
            let rect = ui.min_rect();
            ui.ctx().output_mut(|output| {
                // Keyboard visibility only; this is not an accepted caret geometry.
                output.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect,
                    cursor_rect: rect,
                    should_interrupt_composition: false,
                });
            });
        }
        while let Some(event) = shared.session.take_action() {
            // Inspect action enum only. No authentication/effect or text logging.
            ui.label(format!("Received SDK editor action: {:?}", event.action));
        }
        ui.label("No original secure editing, autofill, font/inset or device parity claim. SDK text/action coalescing and stale native focus attribution remain acceptance blockers.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sdk_profiles_set_type_action_id_and_masked_action_bits_separately() {
        let (input, options) = editor_info(true, false, TextInputAction::Next);
        assert_eq!(input.bits(), 0x81);
        assert_eq!(options.action(), TextInputAction::Next);
        let (input, options) = editor_info(false, true, TextInputAction::None);
        assert_eq!(input.bits(), 0x20001);
        assert_eq!(options.action(), TextInputAction::None);
        assert!(options.contains(ImeOptions::IME_FLAG_NO_FULLSCREEN));
    }
}
