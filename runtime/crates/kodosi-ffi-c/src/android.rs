use std::ffi::c_void;

use jni::{EnvUnowned, Outcome, objects::JObject};

use super::{
    KODOSI_FFI_NULL_HANDLE, KODOSI_FFI_OK, KODOSI_FFI_PANIC, KODOSI_FFI_PLATFORM_FAILED, caught,
};

/// Gives the runtime the Java VM and the application context, which the system's certificate
/// verifier needs for HTTPS. The app calls it once, with its `JNIEnv *` and its application
/// `Context`, before it starts the runtime. A failure leaves no Java exception pending.
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
