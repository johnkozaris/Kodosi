//! One real browser on the host's computer, shown as live video to each viewer. A host starts it
//! through [`Browsers`], and it carries each viewer's [`Pipe`] in any way it likes. A viewer
//! needs only the messages, so a build without the `host` feature starts nothing.

mod message;

pub use message::{
    Chooser, Dialog, DialogKind, Download, Frame, Input, KeyAction, Malformed, Menu, MenuItem,
    MouseAction, MouseButton, Pointer, Size, State, Tab, ToViewer, ToWindow, TouchAction,
    TouchPoint, Viewer,
};

#[cfg(feature = "host")]
mod devtools;
#[cfg(feature = "host")]
mod engine;
#[cfg(feature = "host")]
mod extension;
#[cfg(feature = "host")]
mod flow;
#[cfg(feature = "host")]
mod input;
#[cfg(feature = "host")]
mod page;
#[cfg(feature = "host")]
mod sign_ins;
#[cfg(feature = "host")]
mod window;

#[cfg(feature = "host")]
pub use host::{Browsers, Error, Place, Settings};
#[cfg(feature = "host")]
pub use page::HostPage;
#[cfg(feature = "host")]
pub use sign_ins::{CopyError, SignInSource, SignIns, copied_sites, sign_in_sources};
#[cfg(feature = "host")]
pub use window::{Pipe, Window};

#[cfg(feature = "host")]
mod host {
    use std::{collections::HashMap, io, path::PathBuf, sync::Arc};

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

    /// What a host gives once.
    #[derive(Debug, Clone)]
    pub struct Settings {
        /// The browser program: an app bundle on a Mac, otherwise its executable.
        pub engine: PathBuf,
        pub profiles: PathBuf,
    }

    /// One window: its profile, its name among the host's windows, the page that it opens first
    /// and the folder of its downloads. One browser process runs each profile.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Place {
        pub profile: String,
        pub window: String,
        pub address: String,
        pub folder: PathBuf,
    }

    /// The host's browsers. A window opens when a viewer or an agent needs it, and the browser
    /// exits with its last window.
    pub struct Browsers {
        settings: Settings,
        engines: Mutex<HashMap<String, Arc<Engine>>>,
        windows: Mutex<HashMap<(String, String), Window>>,
    }

    impl Browsers {
        pub fn new(settings: Settings) -> Self {
            Self {
                settings,
                engines: Mutex::default(),
                windows: Mutex::default(),
            }
        }

        /// The window of this place, opened if it is not open.
        pub async fn open(&self, place: Place) -> Result<Window, Error> {
            let key = (place.profile.clone(), place.window.clone());
            let mut windows = self.windows.lock().await;
            if let Some(window) = windows.get(&key).filter(|window| window.is_open()) {
                return Ok(window.clone());
            }
            let engine = self.engine(&place.profile).await?;
            let window = crate::window::open(engine, &place.address, place.folder).await?;
            windows.insert(key, window.clone());
            drop(windows);
            Ok(window)
        }

        async fn engine(&self, profile: &str) -> Result<Arc<Engine>, Error> {
            let mut engines = self.engines.lock().await;
            if let Some(engine) = engines.get(profile).filter(|engine| !engine.is_stopped()) {
                return Ok(Arc::clone(engine));
            }
            let engine = Engine::start(&self.settings, profile).await?;
            engines.insert(profile.to_owned(), Arc::clone(&engine));
            drop(engines);
            Ok(engine)
        }

        /// A page of the host's own in the browser of a profile, which shows the files of a
        /// folder. `script` runs in the page before the page's own scripts.
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

        /// Gives a profile the sign-ins that a person read from another browser, and returns
        /// for how many sites. A host does this only for a person's own action.
        pub async fn add_sign_ins(
            &self,
            profile: &str,
            sign_ins: SignIns,
        ) -> Result<usize, CopyError> {
            let engine = self.engine(profile).await?;
            let folder = self.settings.profiles.join(profile);
            let added = sign_ins::add(engine.devtools(), &folder, sign_ins).await;
            engine.stop_if_idle().await;
            added
        }

        /// Removes what copies gave a profile, and returns for how many sites.
        pub async fn remove_copied_sign_ins(&self, profile: &str) -> Result<usize, CopyError> {
            if sign_ins::copied_sites(&self.settings.profiles, profile) == 0 {
                return Ok(0);
            }
            let engine = self.engine(profile).await?;
            let folder = self.settings.profiles.join(profile);
            let removed = sign_ins::remove(engine.devtools(), &folder).await;
            engine.stop_if_idle().await;
            removed
        }

        /// Closes the browser of a profile and deletes the profile with its sign-ins.
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
            match tokio::fs::remove_dir_all(self.settings.profiles.join(profile)).await {
                Err(problem) if problem.kind() != io::ErrorKind::NotFound => Err(problem.into()),
                _ => Ok(()),
            }
        }

        /// Closes all windows, and the browsers with them.
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
