use std::{fs, path::PathBuf, sync::Arc};

use kodosi_os::{fs::write_private, secrets::SecretStore};
use zeroize::Zeroizing;

use crate::network::{Result, invalid};

#[derive(Clone)]
pub struct Secrets {
    gate: Arc<std::sync::Mutex<()>>,
    slots: Arc<tokio::sync::Semaphore>,
    root: PathBuf,
    system: Option<Arc<SecretStore>>,
}

impl Secrets {
    pub fn new(root: PathBuf, service: &str, isolated: bool) -> Self {
        let system = (!isolated).then(|| Arc::new(SecretStore::new(service, &root)));
        Self {
            gate: Arc::new(std::sync::Mutex::new(())),
            slots: Arc::new(tokio::sync::Semaphore::new(4)),
            root,
            system,
        }
    }

    pub async fn run<T: Send + 'static>(
        &self,
        cancel: tokio_util::sync::CancellationToken,
        work: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let permit = Arc::clone(&self.slots)
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
        Ok(format!("kodosi.{key}"))
    }

    pub fn load(&self, key: &str) -> Result<Option<Zeroizing<String>>> {
        let label = Self::label(key)?;
        let Some(system) = &self.system else {
            return match fs::read_to_string(self.root.join(&label)) {
                Ok(value) => Ok(Some(Zeroizing::new(value))),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error.into()),
            };
        };
        system
            .load(&label)
            .map_err(|error| invalid(format!("{error}; the existing identity was not replaced")))
    }

    pub fn store(&self, key: &str, value: &str) -> Result<()> {
        let label = Self::label(key)?;
        match &self.system {
            Some(system) => system
                .store(&label, value)
                .map_err(|error| invalid(error.to_string())),
            None => Ok(write_private(&self.root.join(label), value.as_bytes())?),
        }
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        let label = Self::label(key)?;
        match &self.system {
            Some(system) => system
                .delete(&label)
                .map_err(|error| invalid(error.to_string())),
            None => match fs::remove_file(self.root.join(label)) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.into()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancelled_credential_work_releases_the_async_caller() {
        let root = tempfile::tempdir().unwrap();
        let store = Secrets::new(root.path().to_owned(), "test", true);
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

    #[cfg(unix)]
    #[test]
    fn isolated_secrets_are_private_and_explicit() {
        use std::os::unix::fs::PermissionsExt as _;
        let root = tempfile::tempdir().unwrap();
        let store = Secrets::new(root.path().join("secrets"), "test", true);
        assert!(store.load("device").unwrap().is_none());
        store.store("device", "secret").unwrap();
        assert_eq!(store.load("device").unwrap().unwrap().as_str(), "secret");
        assert_eq!(
            fs::metadata(root.path().join("secrets/kodosi.device"))
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
