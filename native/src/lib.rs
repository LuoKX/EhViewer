//! Standalone Rust UI bootstrap. Not a replacement for the production application.
//!
//! The bootstrap has no HTTP client, persistence, credentials, or JNI core routing.
//! Its core integration is an in-process call to the existing URL parser. The standalone
//! login interaction reducer freezes original behavior, but is not an authentication runtime.

#[cfg(target_os = "android")]
mod android_ime;
mod app;
pub mod ime_session;
pub mod login_state;
mod navigation;
pub mod signin_artwork;
pub mod signin_layout;

/// Starts the diagnostic UI using the caller's window/lifecycle integration.
///
/// # Errors
///
/// Returns an eframe error if window creation, graphics initialization, or the
/// event loop fails. Android also rejects any repeated invocation in one process
/// (including after a failed first invocation), because eframe caches its event
/// loop/handler. Restart the prototype process to retry; no Kotlin fallback.
pub fn run(options: eframe::NativeOptions) -> eframe::Result {
    #[cfg(target_os = "android")]
    let (options, bridge) = {
        use winit::platform::android::EventLoopBuilderExtAndroid as _;
        static STARTED: ime_session::OneShotRuntime = ime_session::OneShotRuntime::new();
        if !STARTED.claim() {
            return Err(eframe::Error::AppCreation(Box::new(std::io::Error::other(
                "Android diagnostic runtime already invoked; restart prototype process",
            ))));
        }
        let mut options = options;
        let bridge = options
            .android_app
            .clone()
            .map(android_ime::AndroidIme::new);
        if let Some(handler) = bridge.clone() {
            let previous = options.event_loop_builder.take();
            options.event_loop_builder = Some(Box::new(move |builder| {
                if let Some(previous) = previous {
                    previous(builder);
                }
                builder.with_android_input_handler(move |event| handler.event(event));
            }));
        }
        (options, bridge)
    };
    eframe::run_native(
        "EhViewer Rust — bootstrap",
        options,
        Box::new(move |context| {
            #[cfg(target_os = "android")]
            {
                if let Some(bridge) = bridge {
                    bridge.attach(context.egui_ctx.clone());
                    return Ok(Box::new(app::NativeApp::with_ime(bridge)));
                }
            }
            #[cfg(not(target_os = "android"))]
            let _ = context;
            Ok(Box::<app::NativeApp>::default())
        }),
    )
}

#[cfg(target_os = "android")]
// SAFETY: This separate cdylib is the only owner of android_main. The function
// uses the Rust ABI expected by android-activity, not a fabricated C ABI.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
extern "Rust" fn android_main(activity: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("EhViewerRust")
            .with_max_level(log::LevelFilter::Error),
    );
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run(eframe::NativeOptions {
            android_app: Some(activity),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        })
    }));
    // Runtime diagnostics must not serialize application input or credentials.
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(_)) => log::error!("Rust UI runtime failed"),
        Err(_) => log::error!("Rust UI panic contained at application entry"),
    }
}
