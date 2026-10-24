use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use kodosi_os::{
    fs::{read_private, write_private},
    secrets::{SecretStore, system_store_available},
};
use zeroize::Zeroizing;

use crate::{
    network::{Result, invalid},
    protocol::CredentialStore,
};

/// Names the place that a data folder chose on Linux.
const LOCATION_FILE: &str = "location";
const IN_KEYCHAIN: &str = "keychain";
const IN_FILES: &str = "files";

#[derive(Clone)]
enum Place {
    System(Arc<SecretStore>),
    Files,
}

#[derive(Clone)]
pub struct Secrets {
    gate: Arc<std::sync::Mutex<()>>,
    slots: Arc<tokio::sync::Semaphore>,
    root: PathBuf,
    service: Arc<str>,
    place: Arc<std::sync::Mutex<Option<Place>>>,
}

/// Where a data folder keeps its sign-in, once it keeps one.
#[derive(Clone)]
pub struct CredentialLocation {
    place: Arc<std::sync::Mutex<Option<Place>>>,
    folder: PathBuf,
}

impl CredentialLocation {
    /// The kind of store, and the folder when the store keeps files.
    pub fn get(&self) -> Option<(CredentialStore, Option<PathBuf>)> {
        Some(match self.place.lock().ok()?.as_ref()? {
            Place::Files => (CredentialStore::PrivateFile, Some(self.folder.clone())),
            Place::System(_) if cfg!(windows) => {
                (CredentialStore::ProtectedFile, Some(self.folder.clone()))
            }
            Place::System(_) => (CredentialStore::Keychain, None),
        })
    }
}

impl Secrets {
    pub fn new(root: PathBuf, service: &str, isolated: bool) -> Self {
        let place = if isolated {
            Some(Place::Files)
        } else if cfg!(target_os = "linux") {
            None
        } else {
            Some(Place::System(Arc::new(SecretStore::new(service, &root))))
        };
        Self {
            gate: Arc::new(std::sync::Mutex::new(())),
            slots: Arc::new(tokio::sync::Semaphore::new(4)),
            root,
            service: service.into(),
            place: Arc::new(std::sync::Mutex::new(place)),
        }
    }

    pub fn location(&self) -> CredentialLocation {
        CredentialLocation {
            place: Arc::clone(&self.place),
            folder: self.root.clone(),
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

    fn place(&self) -> Result<Option<Place>> {
        let mut place = self.lock_place()?;
        if place.is_none() {
            *place = self.remembered()?;
        }
        Ok(place.clone())
    }

    fn lock_place(&self) -> Result<std::sync::MutexGuard<'_, Option<Place>>> {
        self.place
            .lock()
            .map_err(|_| invalid("Credential store lock failed"))
    }

    fn remembered(&self) -> Result<Option<Place>> {
        let location = self.root.join(LOCATION_FILE);
        match fs::read_to_string(&location) {
            Ok(value) => match value.trim() {
                IN_KEYCHAIN => Ok(Some(self.system())),
                IN_FILES => Ok(Some(Place::Files)),
                _ => Err(invalid(format!(
                    "Kodosi cannot read where this computer keeps the sign-in: {}",
                    location.display()
                ))),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn system(&self) -> Place {
        Place::System(Arc::new(SecretStore::new(&self.service, &self.root)))
    }

    pub fn load(&self, key: &str) -> Result<Option<Zeroizing<String>>> {
        let label = Self::label(key)?;
        match self.place()? {
            None => Ok(None),
            Some(Place::System(system)) => system.load(&label).map_err(|error| {
                invalid(format!("{error}; the existing identity was not replaced"))
            }),
            Some(Place::Files) => {
                let Some(bytes) = read_private(&self.root.join(&label))? else {
                    return Ok(None);
                };
                String::from_utf8(bytes.to_vec())
                    .map(|value| Some(Zeroizing::new(value)))
                    .map_err(|_| invalid("A stored credential is unreadable."))
            }
        }
    }

    pub fn store(&self, key: &str, value: &str) -> Result<()> {
        self.keep(&Self::label(key)?, value, system_store_available)
    }

    /// A Linux data folder chooses its place when it first keeps a secret: the Secret
    /// Service when the computer has one, or else files that only the user can read. It
    /// remembers the place once the first secret is there.
    fn keep(&self, label: &str, value: &str, keychain: impl FnOnce() -> bool) -> Result<()> {
        let (place, first) = match self.place()? {
            Some(place) => (place, false),
            None if keychain() => (self.system(), true),
            None => (Place::Files, true),
        };
        match &place {
            Place::System(system) => system
                .store(label, value)
                .map_err(|error| invalid(error.to_string()))?,
            Place::Files => write_private(&self.root.join(label), value.as_bytes())?,
        }
        if first {
            let name = match place {
                Place::System(_) => IN_KEYCHAIN,
                Place::Files => IN_FILES,
            };
            write_private(&self.root.join(LOCATION_FILE), name.as_bytes())?;
            *self.lock_place()? = Some(place);
        }
        Ok(())
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        let label = Self::label(key)?;
        match self.place()? {
            None => Ok(()),
            Some(Place::System(system)) => system
                .delete(&label)
                .map_err(|error| invalid(error.to_string())),
            Some(Place::Files) => remove(&self.root.join(label)),
        }
    }
}

fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
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

    #[test]
    fn a_data_folder_remembers_its_place_after_the_first_secret() {
        let data = tempfile::tempdir().unwrap();
        let root = data.path().join("secrets");
        let store = Secrets::new(root.clone(), "test", true);
        *store.place.lock().unwrap() = None;
        assert!(store.load("tokens").unwrap().is_none());
        store.delete("tokens").unwrap();
        assert!(store.location().get().is_none());
        assert!(!root.join(LOCATION_FILE).exists());

        store.keep("kodosi.tokens", "secret", || false).unwrap();
        assert_eq!(
            fs::read_to_string(root.join(LOCATION_FILE)).unwrap(),
            IN_FILES
        );
        assert_eq!(store.load("tokens").unwrap().unwrap().as_str(), "secret");
        store
            .keep("kodosi.other", "secret", || unreachable!())
            .unwrap();

        assert!(matches!(store.remembered().unwrap(), Some(Place::Files)));
        fs::write(root.join(LOCATION_FILE), IN_KEYCHAIN).unwrap();
        assert!(matches!(
            store.remembered().unwrap(),
            Some(Place::System(_))
        ));
        fs::write(root.join(LOCATION_FILE), "elsewhere").unwrap();
        assert!(store.remembered().is_err());
    }

    #[test]
    fn a_first_secret_that_cannot_be_kept_chooses_no_place() {
        let data = tempfile::tempdir().unwrap();
        fs::write(data.path().join("secrets"), "not a folder").unwrap();
        let store = Secrets::new(data.path().join("secrets"), "test", true);
        *store.place.lock().unwrap() = None;
        assert!(store.keep("kodosi.tokens", "secret", || false).is_err());
        assert!(store.location().get().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_that_others_can_read_is_refused() {
        use std::os::unix::fs::PermissionsExt as _;
        let data = tempfile::tempdir().unwrap();
        let store = Secrets::new(data.path().join("secrets"), "test", true);
        store.store("tokens", "secret").unwrap();
        fs::set_permissions(
            data.path().join("secrets/kodosi.tokens"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(store.load("tokens").is_err());
    }

    #[test]
    fn an_isolated_folder_reports_its_private_files() {
        let data = tempfile::tempdir().unwrap();
        let store = Secrets::new(data.path().join("secrets"), "test", true);
        assert_eq!(
            store.location().get(),
            Some((
                CredentialStore::PrivateFile,
                Some(data.path().join("secrets"))
            ))
        );
    }
}
