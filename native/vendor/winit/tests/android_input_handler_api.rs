//! Compile-only Android API probe. Checked by an isolated offline Cargo harness.
#![cfg(target_os = "android")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use winit::event_loop::EventLoopBuilder;
use winit::platform::android::activity::input::InputEvent;
use winit::platform::android::activity::{AndroidApp, InputStatus};
use winit::platform::android::EventLoopBuilderExtAndroid;

/// Captured app/context need not be obtained through a borrowed raw event.
pub fn configure(
    builder: &mut EventLoopBuilder<()>,
    app: AndroidApp,
    calls: Arc<AtomicUsize>,
) -> &mut EventLoopBuilder<()> {
    builder.with_android_input_handler(move |event| {
        let _context = &app;
        calls.fetch_add(1, Ordering::Relaxed);
        match event {
            InputEvent::TextEvent(_) => Some(InputStatus::Handled),
            InputEvent::TextAction(_) => Some(InputStatus::Unhandled),
            _ => None,
        }
    })
}

/// Must fail: handler cannot require a retained `'static` event borrow.
#[cfg(feature = "reject-static-borrow")]
pub fn reject_static_borrow(builder: &mut EventLoopBuilder<()>) {
    builder.with_android_input_handler(|_: &'static InputEvent<'static>| None);
}

/// Must fail: captured context must satisfy both Send and Sync.
#[cfg(feature = "reject-non-send")]
pub fn reject_non_send(builder: &mut EventLoopBuilder<()>) {
    let context = std::rc::Rc::new(());
    builder.with_android_input_handler(move |_| {
        let _ = &context;
        None
    });
}
