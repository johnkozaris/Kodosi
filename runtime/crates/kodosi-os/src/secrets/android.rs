use std::{io, path::Path, sync::OnceLock};

use jni::{
    Env, JValue, JavaVM, jni_sig, jni_str,
    objects::{Global, JByteArray, JClass, JString},
};
use zeroize::Zeroizing;

struct Android {
    vm: JavaVM,
    class: Global<JClass<'static>>,
}

static ANDROID: OnceLock<Android> = OnceLock::new();

pub(super) fn initialize(env: &mut Env<'_>) -> Result<(), jni::errors::Error> {
    if ANDROID.get().is_none() {
        let class = env.find_class(jni_str!("com/kodosi/mobile/runtime/KeystoreSecrets"))?;
        let android = Android {
            vm: env.get_java_vm()?,
            class: env.new_global_ref(class)?,
        };
        drop(ANDROID.set(android));
    }
    Ok(())
}

pub(super) const fn available() -> bool {
    true
}

pub(super) struct Store {
    service: String,
}

impl Store {
    pub(super) fn new(service: &str, directory: &Path) -> Self {
        Self {
            service: format!("{}:{service}", directory.display()),
        }
    }

    fn with_java<T>(
        &self,
        name: &str,
        work: impl FnOnce(
            &mut Env<'_>,
            &Global<JClass<'static>>,
            JString<'_>,
        ) -> Result<T, jni::errors::Error>,
    ) -> io::Result<T> {
        let android = ANDROID
            .get()
            .ok_or_else(|| io::Error::other("Android credential storage is not initialized"))?;
        android
            .vm
            .attach_current_thread_for_scope(|env| {
                let name = env.new_string(format!("{}:{name}", self.service))?;
                work(env, &android.class, name)
            })
            .map_err(|_| io::Error::other("Android credential storage is unavailable"))
    }

    pub(super) fn load(&self, name: &str) -> io::Result<Option<Zeroizing<String>>> {
        let bytes = self.with_java(name, |env, class, name| {
            let value = env
                .call_static_method(
                    class,
                    jni_str!("read"),
                    jni_sig!((JString) -> [jbyte]),
                    &[JValue::Object(name.as_ref())],
                )?
                .l()?;
            if value.is_null() {
                return Ok(None);
            }
            let value = env.cast_local::<JByteArray<'_>>(value)?;
            env.convert_byte_array(value).map(Some)
        })?;
        bytes
            .map(|bytes| {
                String::from_utf8(bytes).map(Zeroizing::new).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Stored credential is unreadable",
                    )
                })
            })
            .transpose()
    }

    pub(super) fn store(&self, name: &str, value: &str) -> io::Result<()> {
        self.with_java(name, |env, class, name| {
            let bytes = env.byte_array_from_slice(value.as_bytes())?;
            env.call_static_method(
                class,
                jni_str!("write"),
                jni_sig!((JString, [jbyte]) -> ()),
                &[
                    JValue::Object(name.as_ref()),
                    JValue::Object(bytes.as_ref()),
                ],
            )?;
            Ok(())
        })
    }

    pub(super) fn delete(&self, name: &str) -> io::Result<()> {
        self.with_java(name, |env, class, name| {
            env.call_static_method(
                class,
                jni_str!("delete"),
                jni_sig!((JString) -> ()),
                &[JValue::Object(name.as_ref())],
            )?;
            Ok(())
        })
    }
}
