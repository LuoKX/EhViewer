//! Scalar V1 transfer-window handles with leased, mutex-protected native ownership.
use super::jni_throwing;
use anyhow::{Context as _, Result, ensure};
use eh_domain::transfer_speed::SpeedWindow;
use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{jint, jlong};
use jni_fn::jni_fn;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

type WindowLease = Arc<Mutex<SpeedWindow>>;
static WINDOWS: OnceLock<Mutex<HashMap<i64, WindowLease>>> = OnceLock::new();
static NEXT_HANDLE: AtomicI64 = AtomicI64::new(1);
fn registry() -> &'static Mutex<HashMap<i64, WindowLease>> {
    WINDOWS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn open(version: i32, window: i64) -> Result<i64> {
    ensure!(version == 1, "Unsupported transfer speed API version");
    let window = SpeedWindow::new(window)?;
    let handle = NEXT_HANDLE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| anyhow::anyhow!("Transfer speed handles exhausted"))?;
    registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("Transfer speed registry unavailable"))?
        .insert(handle, Arc::new(Mutex::new(window)));
    Ok(handle)
}
fn lease(handle: i64) -> Result<WindowLease> {
    registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("Transfer speed registry unavailable"))?
        .get(&handle)
        .cloned()
        .context("Transfer speed handle is closed")
}
fn command(handle: i64, operation: i32, now: i64, bytes: i32) -> Result<i64> {
    let lease = lease(handle)?;
    let mut window = lease
        .lock()
        .map_err(|_| anyhow::anyhow!("Transfer speed window unavailable"))?;
    Ok(match operation {
        0 => {
            window.reset();
            0
        }
        1 => {
            window.track(now, bytes)?;
            0
        }
        2 => window.sample(now)?.unwrap_or(-1),
        _ => anyhow::bail!("Invalid transfer speed operation"),
    })
}
fn close(handle: i64) -> Result<()> {
    registry()
        .lock()
        .map_err(|_| anyhow::anyhow!("Transfer speed registry unavailable"))?
        .remove(&handle);
    Ok(())
}

/// Creates a V1 native window; publish only after complete validation.
#[jni_fn("com.hippo.ehviewer.bridge.download.TransferSpeedBridgeKt")]
pub fn nativeOpenTransferSpeedV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    version: jint,
    window: jlong,
) -> jlong {
    jni_throwing(&mut env, |_| open(version, window))
}

/// Resets, tracks or samples while an Arc lease protects a retired handle.
#[jni_fn("com.hippo.ehviewer.bridge.download.TransferSpeedBridgeKt")]
pub fn nativeUpdateTransferSpeedV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    handle: jlong,
    operation: jint,
    now: jlong,
    bytes: jint,
) -> jlong {
    jni_throwing(&mut env, |_| command(handle, operation, now, bytes))
}

/// Idempotently retires one handle; no Java references are retained.
#[jni_fn("com.hippo.ehviewer.bridge.download.TransferSpeedBridgeKt")]
pub fn nativeCloseTransferSpeedV1(mut env: JNIEnv<'_>, _: JClass<'_>, handle: jlong) {
    jni_throwing(&mut env, |_| close(handle));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handle_lifetime_commands_and_failures() {
        assert!(open(2, 1_000_000_000).is_err());
        assert!(open(1, 1).is_err());
        let handle = open(1, 1_000_000_000).unwrap();
        assert_eq!(command(handle, 2, 0, 0).unwrap(), -1);
        command(handle, 1, 0, 50).unwrap();
        assert_eq!(command(handle, 2, 10_000_000, 0).unwrap(), 5_000);
        assert!(command(handle, 1, 0, -1).is_err());
        assert!(command(handle, 3, 0, 0).is_err());
        command(handle, 0, 0, 0).unwrap();
        assert_eq!(command(handle, 2, 0, 0).unwrap(), -1);
        let active = lease(handle).unwrap();
        close(handle).unwrap();
        close(handle).unwrap();
        assert!(lease(handle).is_err());
        assert!(command(handle, 0, 0, 0).is_err());
        active.lock().unwrap().track(0, 7).unwrap();
        assert_eq!(
            active.lock().unwrap().sample(10_000_000).unwrap(),
            Some(700)
        );
    }
}
