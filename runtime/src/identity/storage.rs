use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use crate::network::{Result, invalid};
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct Secrets {
    gate: std::sync::Arc<std::sync::Mutex<()>>,
    slots: std::sync::Arc<tokio::sync::Semaphore>,
    root: PathBuf,
    service: String,
    isolated: bool,
}

impl Secrets {
    pub fn new(root: PathBuf, service: String, isolated: bool) -> Self {
        Self {
            gate: std::sync::Arc::new(std::sync::Mutex::new(())),
            slots: std::sync::Arc::new(tokio::sync::Semaphore::new(4)),
            root,
            service,
            isolated,
        }
    }

    pub async fn run<T: Send + 'static>(
        &self,
        cancel: tokio_util::sync::CancellationToken,
        work: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = std::sync::Arc::clone(&self.slots)
            .try_acquire_owned()
            .map_err(|_| crate::network::Error::Busy)?;
        let store = self.clone();
        let (reply, response) = tokio::sync::oneshot::channel();
        let worker_cancel = cancel.clone();
        std::thread::Builder::new()
            .name("kodosi-credentials".into())
            .spawn(move || {
                let _permit = permit;
                let result = (|| {
                    let _held = store
                        .gate
                        .lock()
                        .map_err(|_| invalid("Credential store lock failed"))?;
                    if worker_cancel.is_cancelled() {
                        return Err(crate::network::Error::Stale);
                    }
                    work(&store)
                })();
                drop(reply.send(result));
            })
            .map_err(crate::network::Error::from)?;
        tokio::select! {
            () = cancel.cancelled() => Err(crate::network::Error::Stale),
            result = response => result.map_err(|_| crate::network::Error::Closed)?,
        }
    }

    fn label(key: &str) -> Result<String> {
        if key.is_empty()
            || key.len() > 200
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return Err(invalid("Invalid credential identity."));
        }
        Ok(format!("terminal-first.{key}"))
    }

    pub fn load(&self, key: &str) -> Result<Option<Zeroizing<String>>> {
        let label = Self::label(key)?;
        if self.isolated {
            return match fs::read_to_string(self.root.join(&label)) {
                Ok(value) => Ok(Some(Zeroizing::new(value))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            };
        }
        platform::load(&self.service, &label)
    }

    pub fn store(&self, key: &str, value: &str) -> Result<()> {
        let label = Self::label(key)?;
        if self.isolated {
            return private_write(&self.root.join(label), value.as_bytes());
        }
        platform::store(&self.service, &label, value)
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        let label = Self::label(key)?;
        if self.isolated {
            return match fs::remove_file(self.root.join(label)) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            };
        }
        platform::delete(&self.service, &label)
    }
}

pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    let parent = path
        .parent()
        .ok_or_else(|| invalid("Private storage path has no parent."))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let metadata = fs::symlink_metadata(parent)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(invalid("Private storage parent is not a directory."));
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Result, Zeroizing, invalid};
    use security_framework::passwords::{
        PasswordOptions, delete_generic_password_options, generic_password,
        set_generic_password_options,
    };

    fn options(service: &str, label: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(service, label);
        options.use_protected_keychain();
        options
    }

    pub(super) fn load(service: &str, label: &str) -> Result<Option<Zeroizing<String>>> {
        match generic_password(options(service, label)) {
            Ok(bytes) => String::from_utf8(bytes)
                .map(|value| Some(Zeroizing::new(value)))
                .map_err(|_| invalid("Stored credential is unreadable.")),
            Err(error) if error.code() == -25300 => Ok(None),
            Err(error) => Err(invalid(format!(
                "Keychain is unavailable; existing identity was not replaced: {error}"
            ))),
        }
    }
    pub(super) fn store(service: &str, label: &str, value: &str) -> Result<()> {
        set_generic_password_options(value.as_bytes(), options(service, label))
            .map_err(|error| invalid(format!("Could not store credential in Keychain: {error}")))
    }
    pub(super) fn delete(service: &str, label: &str) -> Result<()> {
        match delete_generic_password_options(options(service, label)) {
            Ok(()) => Ok(()),
            Err(error) if error.code() == -25300 => Ok(()),
            Err(error) => Err(invalid(format!(
                "Could not remove credential from Keychain: {error}"
            ))),
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use keyring::{Entry, Error};

    pub(super) fn load(service: &str, label: &str) -> Result<Option<Zeroizing<String>>> {
        let entry = Entry::new(service, label)
            .map_err(|error| invalid(format!("Secret Service is unavailable: {error}")))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(Zeroizing::new(value))),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(invalid(format!(
                "Secret Service is unavailable; existing identity was not replaced: {error}"
            ))),
        }
    }
    pub(super) fn store(service: &str, label: &str, value: &str) -> Result<()> {
        Entry::new(service, label)
            .and_then(|entry| entry.set_password(value))
            .map_err(|error| {
                invalid(format!(
                    "Could not store credential in Secret Service: {error}"
                ))
            })
    }
    pub(super) fn delete(service: &str, label: &str) -> Result<()> {
        match Entry::new(service, label).and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(invalid(format!(
                "Could not remove credential from Secret Service: {error}"
            ))),
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("Kodosi identity storage supports macOS and Linux.");

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_credential_work_releases_the_async_caller() {
        let root = tempfile::tempdir().unwrap();
        let store = Secrets::new(root.path().to_owned(), "test".into(), true);
        let cancel = tokio_util::sync::CancellationToken::new();
        let token = cancel.clone();
        let (started, waiting) = tokio::sync::oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let task = tokio::spawn(async move {
            store
                .run(token, move |_| {
                    let _sent = started.send(());
                    blocked.recv().unwrap();
                    Ok(())
                })
                .await
        });
        waiting.await.unwrap();
        cancel.cancel();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        release.send(()).unwrap();
    }

    #[test]
    fn isolated_secrets_are_private_and_explicit() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let store = Secrets::new(root.path().join("secrets"), "test".into(), true);
        assert!(store.load("device").unwrap().is_none());
        store.store("device", "secret").unwrap();
        assert_eq!(store.load("device").unwrap().unwrap().as_str(), "secret");
        assert_eq!(
            fs::metadata(root.path().join("secrets/terminal-first.device"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        store.delete("device").unwrap();
        assert!(store.load("device").unwrap().is_none());
        assert!(store.store("../escape", "secret").is_err());
    }
}
