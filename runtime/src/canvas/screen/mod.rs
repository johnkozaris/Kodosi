//! This computer's screen on a terminal's canvas, which its owner's devices watch and control
//! once the person at the computer allowed it here. Kodosi Screen, an app inside Kodosi's, does
//! the capture, so that macOS grants Screen Recording and Accessibility to it alone.

#[cfg(not(target_os = "macos"))]
mod absent;
#[cfg(target_os = "macos")]
mod mac;

use std::{
    path::Path,
    sync::{Arc, Weak},
};

use serde::{Deserialize, Serialize};
use shared_browser::Viewer;
use tokio::sync::watch;

#[cfg(not(target_os = "macos"))]
use self::absent::Screens;
#[cfg(target_os = "macos")]
use self::mac::Screens;
use super::{PipeEnds, ScreenAccess, ScreenShown};
use crate::{runtime::saver::Saver, text::Text};

/// Whether the person at the computer lets its owner's other devices watch its screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Setting {
    allowed: bool,
}

#[derive(Clone)]
pub(crate) struct ScreenHost(Arc<Shared>);

struct Shared {
    allowed: watch::Sender<bool>,
    saver: Saver,
    /// `None` where Kodosi Screen did not come with the app.
    screens: Option<Screens>,
}

impl ScreenHost {
    pub(crate) fn new(data_root: &Path) -> Self {
        let file = data_root.join("screen.json");
        let setting: Setting = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let saver = Saver::file(file, "whether the screen is shared");
        Self(Arc::new_cyclic(|host: &Weak<Shared>| {
            let host = Weak::clone(host);
            let screens = Screens::new(data_root, move || {
                if let Some(host) = host.upgrade() {
                    host.set(false);
                }
            });
            Shared {
                allowed: watch::channel(setting.allowed && screens.is_some()).0,
                saver,
                screens,
            }
        }))
    }

    pub(crate) fn available(&self) -> bool {
        self.0.screens.is_some()
    }

    pub(crate) fn allowed(&self) -> watch::Receiver<bool> {
        self.0.allowed.subscribe()
    }

    /// The person at the computer turns the sharing on there, so it is the time for macOS to
    /// ask them for what Kodosi Screen may not do yet.
    pub(crate) fn allow(&self, allowed: bool) -> Result<(), Text> {
        if allowed {
            self.offered()?;
        }
        self.0.set(allowed);
        if let Some(screens) = self.0.screens.clone().filter(|_| allowed) {
            tokio::spawn(screens.ask());
        }
        Ok(())
    }

    /// What Kodosi Screen may do, while the sharing is on.
    pub(crate) async fn access(&self) -> Option<ScreenAccess> {
        self.screens().ok()?.access().await
    }

    /// Refuses where Kodosi Screen did not come with the app, so the screen never shows.
    pub(crate) fn offered(&self) -> Result<(), Text> {
        if self.available() {
            Ok(())
        } else {
            Err(text!("This computer cannot share its screen."))
        }
    }

    /// Refuses unless the person at the computer allowed the sharing.
    pub(crate) fn check(&self) -> Result<(), Text> {
        self.offered()?;
        if *self.0.allowed.borrow() {
            Ok(())
        } else {
            Err(off())
        }
    }

    pub(crate) async fn watch(&self, viewer: Viewer) -> Result<PipeEnds, Text> {
        self.screens()?.watch(viewer).await
    }

    pub(crate) async fn show(&self, shown: ScreenShown) -> Result<(), Text> {
        self.screens()?.show(shown).await;
        Ok(())
    }

    /// Ends every view of the screen, so Kodosi Screen quits, and waits for the setting to be saved.
    pub(crate) async fn close(&self) {
        if let Some(screens) = self.0.screens.clone() {
            screens.stop().await;
        }
        self.0.saver.saved().await;
    }

    fn screens(&self) -> Result<Screens, Text> {
        self.check()?;
        self.0.screens.clone().ok_or_else(off)
    }
}

impl Shared {
    /// Turning the sharing off also ends every view of the screen.
    fn set(&self, allowed: bool) {
        let changed = self
            .allowed
            .send_if_modified(|now| std::mem::replace(now, allowed) != allowed);
        if !changed {
            return;
        }
        match serde_json::to_vec(&Setting { allowed }) {
            Ok(bytes) => self.saver.save(bytes),
            Err(error) => tracing::warn!(%error, "could not save whether the screen is shared"),
        }
        if !allowed && let Some(screens) = self.screens.clone() {
            tokio::spawn(screens.stop());
        }
    }
}

fn off() -> Text {
    text!(
        "Screen sharing is off on this computer. The person at it can turn it on in Kodosi's settings there."
    )
}
