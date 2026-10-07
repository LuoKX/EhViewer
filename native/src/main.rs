//! Desktop entry for testing the same Rust UI without an Android device.

#[cfg(not(target_os = "android"))]
fn main() -> eframe::Result {
    eh_native::run(eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([480.0, 800.0])
            .with_min_inner_size([320.0, 480.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    })
}

#[cfg(target_os = "android")]
fn main() {}
