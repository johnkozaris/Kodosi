use std::ffi::c_void;

use jni::{EnvUnowned, Outcome, objects::JObject};

use super::{
    KODOSI_FFI_NULL_HANDLE, KODOSI_FFI_OK, KODOSI_FFI_PANIC, KODOSI_FFI_PLATFORM_FAILED, caught,
};

/// Initializes Android JVM bindings (secrets store and platform TLS verifier).
/// Must be called once with a `JNIEnv *` and application `Context` before
/// starting the runtime. Clears any pending Java exception on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_android_init(env: *mut c_void, context: *mut c_void) -> i32 {
    caught(|| {
        if env.is_null() || context.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        let mut env = unsafe { EnvUnowned::from_raw(env.cast()) };
        let outcome = env
            .with_env(|env| {
                let initialized = kodosi_os::secrets::initialize_android(env).and_then(|()| {
                    let context = unsafe { JObject::from_raw(env, context.cast()) };
                    rustls_platform_verifier::android::init_with_env(env, context)
                });
                if initialized.is_err() {
                    env.exception_clear();
                }
                initialized
            })
            .into_outcome();
        match outcome {
            Outcome::Ok(()) => KODOSI_FFI_OK,
            Outcome::Err(_) => KODOSI_FFI_PLATFORM_FAILED,
            Outcome::Panic(_) => KODOSI_FFI_PANIC,
        }
    })
}
