//! Rust-owned presentation. Unimplemented product features are never shown as ready.

use crate::navigation::{self, Grammar, Inspection};
use eframe::egui;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Screen {
    #[default]
    Overview,
    InspectLink,
    OriginalArtwork,
    ImeProbe,
}

#[derive(Default)]
pub(crate) struct NativeApp {
    screen: Screen,
    grammar: Grammar,
    input: String,
    result: Option<Inspection>,
    #[cfg(target_os = "android")]
    ime: Option<crate::android_ime::AndroidIme>,
}

impl NativeApp {
    #[cfg(target_os = "android")]
    pub(crate) fn with_ime(ime: crate::android_ime::AndroidIme) -> Self {
        Self {
            screen: Screen::default(),
            grammar: Grammar::default(),
            input: String::new(),
            result: None,
            ime: Some(ime),
        }
    }

    fn ime_probe(&self, ui: &mut egui::Ui) {
        debug_assert!(self.screen == Screen::ImeProbe);
        ui.heading("Android IME transport probe");
        #[cfg(target_os = "android")]
        if let Some(ime) = &self.ime {
            ime.ui(ui);
            return;
        }
        ui.label("Android runtime required. This is not an original sign-in field.");
    }

    fn render(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("EhViewer Rust");
            ui.label("Bootstrap only — not the migrated product.");
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.screen, Screen::Overview, "Migration status");
                ui.selectable_value(
                    &mut self.screen,
                    Screen::InspectLink,
                    "Inspect gallery link",
                );
                ui.selectable_value(
                    &mut self.screen,
                    Screen::OriginalArtwork,
                    "Original sign-in asset",
                );
                ui.selectable_value(&mut self.screen, Screen::ImeProbe, "Android IME probe");
            });
            #[cfg(target_os = "android")]
            if (self.screen != Screen::ImeProbe || !ui.input(|input| input.focused))
                && let Some(ime) = &self.ime
            {
                ime.blur();
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| match self.screen {
                Screen::Overview => Self::overview(ui),
                Screen::InspectLink => self.inspector(ui),
                Screen::OriginalArtwork => Self::original_artwork(ui),
                Screen::ImeProbe => self.ime_probe(ui),
            });
        });
    }

    fn overview(ui: &mut egui::Ui) {
        ui.heading("Full Rust target");
        ui.label("This screen and URL inspection execute in Rust, without Compose or JNI business calls.");
        ui.label("No network access, database writes, credentials, or legacy app data access.");
        ui.separator();
        ui.strong("Not migrated into this application yet:");
        for item in [
            "Authentication, cookies, HTTP, proxy and DNS",
            "Gallery lists, details, search, comments and favorites UI",
            "Reader gestures, image rendering and remaining codecs",
            "Downloads, background execution, notifications and recovery",
            "Database, settings, history and compatible backup import",
            "Update-package selection and installation",
            "Android intents, SAF, permissions and system integrations",
            "Localization, CJK/IME, accessibility and device acceptance",
        ] {
            ui.label(format!("• {item}"));
        }
        ui.separator();
        ui.label("Existing Rust parsers and policies are reused, not reimplemented.");
        ui.label("The production Android app remains unchanged until feature and data-compatibility gates pass.");
    }

    fn original_artwork(ui: &mut egui::Ui) {
        ui.heading("Original sign-in vector");
        ui.label("Source-derived geometry and colors. Not a completed sign-in screen or pixel-parity acceptance.");
        let (bounds, _) = ui.allocate_exact_size(
            crate::signin_artwork::intrinsic_size(),
            egui::Sense::hover(),
        );
        if crate::signin_artwork::paint(ui.painter(), bounds).is_err() {
            ui.label("Invalid measured asset bounds; no substitute icon was drawn.");
        }
        ui.label("Original 108dp viewport, group transform, seven paths and GPL notice retained.");
        ui.label("Formal input controls remain blocked on Android IME/editor actions, secure editing/autofill, fonts and viewport contracts.");
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        ui.heading("Offline core integration");
        ui.label("Inspects the existing navigation grammar only. Does not open a gallery or make HTTP requests.");
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            changed |= ui
                .selectable_value(&mut self.grammar, Grammar::Detail, "Gallery / MPV")
                .changed();
            changed |= ui
                .selectable_value(&mut self.grammar, Grammar::Page, "Reader page")
                .changed();
        });
        changed |= ui
            .add(
                egui::TextEdit::multiline(&mut self.input)
                    .hint_text("Paste a gallery or reader-page URL")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY)
                    .char_limit(16 * 1024),
            )
            .changed();
        if changed {
            self.result = None;
        }
        ui.horizontal(|ui| {
            if ui.button("Inspect").clicked() {
                self.result = Some(navigation::inspect(&self.input, self.grammar));
            }
            if ui.button("Clear").clicked() {
                self.input.clear();
                self.result = None;
            }
        });
        ui.separator();
        match self.result {
            None => {
                ui.label("No inspection result.");
            }
            Some(Inspection::Detail { gid }) => {
                ui.label(format!("Gallery ID: {gid}"));
            }
            Some(Inspection::Page { gid, page_index }) => {
                ui.label(format!("Gallery ID: {gid}; zero-based page: {page_index}"));
            }
            Some(Inspection::NoMatch) => {
                ui.label("No match for the selected grammar.");
            }
            Some(Inspection::Rejected) => {
                ui.label("Core rejected the input. Nothing was opened.");
            }
        }
        ui.label("Tokens are not copied into results or diagnostics. Input is not saved.");
    }
}

#[cfg(target_os = "android")]
impl Drop for NativeApp {
    fn drop(&mut self) {
        if let Some(ime) = &self.ime {
            ime.dispose();
        }
    }
}

impl eframe::App for NativeApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_explicit_migration_status_and_no_user_data() {
        let app = NativeApp::default();
        assert!(app.screen == Screen::Overview);
        assert!(app.input.is_empty());
        assert_eq!(app.result, None);
    }

    #[test]
    fn all_screens_produce_ui_without_external_effects() {
        let context = egui::Context::default();
        let mut app = NativeApp::default();
        for screen in [
            Screen::Overview,
            Screen::InspectLink,
            Screen::OriginalArtwork,
            Screen::ImeProbe,
        ] {
            app.screen = screen;
            let mut output = context.run_ui(egui::RawInput::default(), |ui| app.render(ui));
            // Headless tests have no renderer; explicitly consume the texture upload batch.
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
            assert!(output.platform_output.commands.is_empty());
            assert!(app.input.is_empty());
            assert_eq!(app.result, None);
        }
    }
}
