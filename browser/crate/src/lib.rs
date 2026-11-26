//! Without the `host` feature, only wire protocol types (`link` and `message`) are compiled
//! for use by the engine binary.

pub mod link;
mod message;

pub use message::{
    Chooser, Dialog, DialogKind, Download, Menu, MenuContext, MenuItem, Pointer, Size, State, Tab,
    Viewer,
};

#[cfg(feature = "host")]
mod devtools;
#[cfg(feature = "host")]
mod driver;
#[cfg(feature = "host")]
mod engine;
#[cfg(feature = "host")]
mod flow;
#[cfg(feature = "host")]
mod input;
#[cfg(feature = "host")]
mod page;
#[cfg(feature = "host")]
mod sign_ins;
#[cfg(feature = "host")]
mod socket;
#[cfg(feature = "host")]
mod window;

#[cfg(feature = "host")]
pub use driver::Driver;
#[cfg(feature = "host")]
pub use host::{Browsers, Error, Place, Privacy, Settings};
#[cfg(feature = "host")]
pub use page::HostPage;
#[cfg(feature = "host")]
pub use sign_ins::{CopyError, SignInSource, SignIns, copied_sites, sign_in_sources};
#[cfg(feature = "host")]
pub use window::{Pipe, Window};

#[cfg(feature = "host")]
mod host {
    use std::{
        collections::HashMap,
        io,
        path::PathBuf,
        sync::{Arc, PoisonError, RwLock},
    };

    use serde::{Deserialize, Serialize};
    use tokio::sync::Mutex;

    use crate::{
        engine::Engine,
        page::HostPage,
        sign_ins::{self, CopyError, SignIns},
        window::Window,
    };

    #[derive(Debug, thiserror::Error)]
    pub enum Error {
        #[error("The browser did not start: {0}")]
        Engine(String),
        #[error("The browser stopped.")]
        Stopped,
        #[error("The message of the browser is not valid.")]
        Malformed,
        #[error(transparent)]
        Io(#[from] io::Error),
    }

    #[derive(Debug, Clone)]
    pub struct Settings {
        /// Path to the browser engine (`.app` bundle on macOS, executable otherwise).
        pub engine: PathBuf,
        pub profiles: PathBuf,
        /// Store cookie/password encryption keys in the OS keychain. Set to `false` for ephemeral
        /// or test profiles to use a mock key and avoid touching the user's keychain.
        pub system_keychain: bool,
        pub privacy: Privacy,
    }

    /// What Kodosi Browser shares beyond the pages that a person opens, each off until a person
    /// turns it on.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Privacy {
        /// Chromium sends a signature of each form to Google, which answers what its fields hold.
        pub form_predictions: bool,
        /// A site inside another site's page gets its own cookies, so it knows the person there.
        pub cross_site_cookies: bool,
    }

    /// Target browser window to open; each `profile` runs in its own engine process.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Place {
        pub profile: String,
        pub window: String,
        pub address: String,
        pub folder: PathBuf,
    }

    pub struct Browsers {
        program: RwLock<PathBuf>,
        profiles: PathBuf,
        system_keychain: bool,
        privacy: RwLock<Privacy>,
        engines: Mutex<HashMap<String, Arc<Engine>>>,
        windows: Mutex<HashMap<(String, String), Window>>,
    }

    impl Browsers {
        pub fn new(settings: Settings) -> Self {
            Self {
                program: RwLock::new(settings.engine),
                profiles: settings.profiles,
                system_keychain: settings.system_keychain,
                privacy: RwLock::new(settings.privacy),
                engines: Mutex::default(),
                windows: Mutex::default(),
            }
        }

        pub async fn open(&self, place: Place) -> Result<Window, Error> {
            let key = (place.profile.clone(), place.window.clone());
            let mut windows = self.windows.lock().await;
            if let Some(window) = windows.get(&key).filter(|window| window.is_open()) {
                return Ok(window.clone());
            }
            let engine = self.engine(&place.profile).await?;
            let window = crate::window::open(engine, &place.address, place.folder)?;
            windows.insert(key, window.clone());
            drop(windows);
            Ok(window)
        }

        async fn engine(&self, profile: &str) -> Result<Arc<Engine>, Error> {
            let mut engines = self.engines.lock().await;
            if let Some(engine) = engines.get(profile).filter(|engine| !engine.is_stopped()) {
                return Ok(Arc::clone(engine));
            }
            let program = self
                .program
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .clone();
            let privacy = *self.privacy.read().unwrap_or_else(PoisonError::into_inner);
            let engine = Engine::start(
                &program,
                &self.profiles,
                profile,
                self.system_keychain,
                privacy,
            )
            .await?;
            engines.insert(profile.to_owned(), Arc::clone(&engine));
            drop(engines);
            Ok(engine)
        }

        /// Sets the engine binary path for future launches; running engine processes are unchanged.
        pub fn use_engine(&self, program: PathBuf) {
            *self.program.write().unwrap_or_else(PoisonError::into_inner) = program;
        }

        /// Chromium reads its features only when it starts, so a change stops every engine and
        /// closes its windows, which then say that they closed for a restart. Opening a window
        /// waits until the old engine has let go of its profile.
        pub async fn use_privacy(&self, privacy: Privacy) {
            let earlier = std::mem::replace(
                &mut *self.privacy.write().unwrap_or_else(PoisonError::into_inner),
                privacy,
            );
            if earlier == privacy {
                return;
            }
            let mut windows = self.windows.lock().await;
            let mut engines = self.engines.lock().await;
            for window in windows.values() {
                window.restart().await;
            }
            windows.clear();
            drop(windows);
            for engine in engines.values() {
                engine.stop().await;
            }
            engines.clear();
        }

        /// Opens an offscreen host page, injecting `script` to run before page scripts.
        pub async fn host_page(
            &self,
            profile: &str,
            folder: PathBuf,
            (width, height): (u32, u32),
            script: &str,
        ) -> Result<HostPage, Error> {
            let engine = self.engine(profile).await?;
            let page = HostPage::open(Arc::clone(&engine), folder, width, height, script).await;
            if page.is_err() {
                engine.stop_if_idle().await;
            }
            page
        }

        /// Imports browser sign-in cookies into a profile; call only in response to an explicit
        /// user action.
        pub async fn add_sign_ins(
            &self,
            profile: &str,
            sign_ins: SignIns,
        ) -> Result<usize, CopyError> {
            let engine = self.engine(profile).await?;
            let folder = self.profiles.join(profile);
            let added = sign_ins::add(engine.devtools(), &folder, sign_ins).await;
            engine.stop_if_idle().await;
            added
        }

        pub async fn remove_copied_sign_ins(&self, profile: &str) -> Result<usize, CopyError> {
            if sign_ins::copied_sites(&self.profiles, profile) == 0 {
                return Ok(0);
            }
            let engine = self.engine(profile).await?;
            let folder = self.profiles.join(profile);
            let removed = sign_ins::remove(engine.devtools(), &folder).await;
            engine.stop_if_idle().await;
            removed
        }

        pub async fn remove_profile(&self, profile: &str) -> Result<(), Error> {
            let windows: Vec<Window> = self
                .windows
                .lock()
                .await
                .extract_if(|(owner, _), _| owner == profile)
                .map(|(_, window)| window)
                .collect();
            for window in &windows {
                window.close().await;
            }
            let engine = self.engines.lock().await.remove(profile);
            if let Some(engine) = engine {
                engine.stop().await;
            }
            match tokio::fs::remove_dir_all(self.profiles.join(profile)).await {
                Err(problem) if problem.kind() != io::ErrorKind::NotFound => Err(problem.into()),
                _ => Ok(()),
            }
        }

        pub async fn close(&self) {
            let windows = std::mem::take(&mut *self.windows.lock().await);
            for window in windows.values() {
                window.close().await;
            }
            let engines = std::mem::take(&mut *self.engines.lock().await);
            for engine in engines.values() {
                engine.stop().await;
            }
        }
    }
}
