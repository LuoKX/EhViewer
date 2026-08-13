#![cfg(feature = "jvm")]

use android_logger::Config;
use jni::JavaVM;
use jni::sys::{JNI_VERSION_1_6, jint};
use log::LevelFilter;
use std::ffi::c_void;

#[unsafe(no_mangle)]
// The JVM discovery ABI requires this exact exported identifier.
#[allow(non_snake_case)]
pub extern "system" fn JNI_OnLoad(_: JavaVM, _: *mut c_void) -> jint {
    android_logger::init_once(Config::default().with_max_level(LevelFilter::Debug));
    JNI_VERSION_1_6
}
