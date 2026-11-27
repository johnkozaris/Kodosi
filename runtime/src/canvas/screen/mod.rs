//! This computer's screen on a terminal's canvas, which its owner's devices watch and control
//! once the person at the computer allowed it here. Kodosi Screen, a helper program of
//! Kodosi's, does the capture, so that macOS grants Screen Recording and Accessibility to it
//! alone and the desktop's portal on Linux asks in Kodosi's name. On Linux, only an X11 desktop
//! or a desktop whose portal shares the screen and takes input offers the sharing.

mod helper;

use std::{
    path::Path,
    sync::{Arc, Weak},
};

use serde::{Deserialize, Serialize};
use shared_browser::Viewer;
use tokio::sync::watch;

use self::helper::Screens;
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
    /// `owner` names the account that owns the host, which Kodosi Screen serves on a Mac only
    /// once the person at it allowed that account.
    pub(crate) fn new(data_root: &Path, owner: watch::Receiver<Option<String>>) -> Self {
        let file = data_root.join("screen.json");
        let setting: Setting = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let saver = Saver::file(file, "whether the screen is shared");
        Self(Arc::new_cyclic(|host: &Weak<Shared>| {
            let host = Weak::clone(host);
            let screens = Screens::new(data_root, owner, move || {
                if let Some(host) = host.upgrade() {
                    host.stop();
                }
            });
            Shared {
                allowed: watch::channel(setting.allowed && screens.is_some()).0,
                saver,
                screens,
            }
        }))
    }

    /// Whether Kodosi Screen came with the app and the desktop lets it share the screen.
    pub(crate) async fn available(&self) -> bool {
        self.0.screens.is_some() && desktop_shares().await
    }

    pub(crate) fn allowed(&self) -> watch::Receiver<bool> {
        self.0.allowed.subscribe()
    }

    /// The person at the computer turns the sharing on there. Kodosi Screen asks them first
    /// whether the devices of this account may see and control the computer, unless they allowed
    /// it before, and on a Mac only with the `vouch` of the app for their click. The command waits
    /// for their answer. Then it is the time for the system to ask them for what Kodosi Screen may
    /// not do yet.
    pub(crate) async fn turn_on(&self, question: screen::Question, vouch: Option<String>) {
        let Some(screens) = self.0.screens.clone() else {
            return;
        };
        if screens.allow(question, vouch).await {
            self.0.set(true);
            screens.ask().await;
        }
    }

    /// The person at the computer turns the sharing off there, which also takes back every
    /// account that they allowed in Kodosi Screen.
    pub(crate) fn turn_off(&self) {
        self.0.set(false);
        if let Some(screens) = self.0.screens.clone() {
            tokio::spawn(screens.forget());
        }
    }

    /// What Kodosi Screen may do, while the sharing is on.
    pub(crate) async fn access(&self) -> Option<ScreenAccess> {
        self.screens().await.ok()?.access().await
    }

    /// Refuses where Kodosi Screen did not come with the app or the desktop does not let it share
    /// the screen, so the screen never shows.
    pub(crate) async fn offered(&self) -> Result<(), Text> {
        if self.available().await {
            Ok(())
        } else {
            Err(text!("This computer cannot share its screen."))
        }
    }

    /// Refuses unless the person at the computer allowed the sharing.
    async fn check(&self) -> Result<(), Text> {
        self.offered().await?;
        if *self.0.allowed.borrow() {
            Ok(())
        } else {
            Err(off())
        }
    }

    pub(crate) async fn watch(&self, viewer: Viewer) -> Result<PipeEnds, Text> {
        self.screens().await?.watch(viewer).await
    }

    pub(crate) async fn show(&self, shown: ScreenShown) -> Result<(), Text> {
        self.screens().await?.show(shown).await;
        Ok(())
    }

    /// Ends every view of the screen, so Kodosi Screen quits, and waits for the setting to be saved.
    pub(crate) async fn close(&self) {
        if let Some(screens) = self.0.screens.clone() {
            screens.stop().await;
        }
        self.0.saver.saved().await;
    }

    async fn screens(&self) -> Result<Screens, Text> {
        self.check().await?;
        self.0.screens.clone().ok_or_else(off)
    }
}

impl Shared {
    /// Returns whether the setting changed.
    fn set(&self, allowed: bool) -> bool {
        let changed = self
            .allowed
            .send_if_modified(|now| std::mem::replace(now, allowed) != allowed);
        if changed {
            match serde_json::to_vec(&Setting { allowed }) {
                Ok(bytes) => self.saver.save(bytes),
                Err(error) => tracing::warn!(%error, "could not save whether the screen is shared"),
            }
        }
        changed
    }

    /// The sharing ended at the computer: the person stopped it in Kodosi Screen, or Kodosi
    /// Screen does not serve this account. Every view of the screen ends.
    fn stop(&self) {
        if self.set(false)
            && let Some(screens) = self.screens.clone()
        {
            tokio::spawn(screens.stop());
        }
    }
}

/// How long Kodosi's app vouches for a click, which may wait for Kodosi Screen to start.
#[cfg(target_os = "macos")]
const VOUCH_LIFE: std::time::Duration = std::time::Duration::from_mins(1);

/// Kodosi's app vouches that the person at this Mac just clicked its switch to share the screen.
///
/// Kodosi Screen asks them only with the vouch. Only the code that runs on that click calls this,
/// through the C contract, and sends what it returns with `screen.allow`; no host makes an offer.
#[cfg(target_os = "macos")]
pub fn vouch() -> std::io::Result<String> {
    ::screen::handover::vouch(VOUCH_LIFE)
}

/// What Kodosi Screen asks the person at the Mac before the devices of `account` see it. An
/// account of another server than Kodosi's says which, as anyone can run one with any names.
pub(crate) fn question(account: &str, server: Option<&str>) -> screen::Question {
    let language = crate::text::language();
    let detail = server.map_or_else(
        || text!("Kodosi asks once for each account."),
        |server| {
            text!(
                "This account is from {server}. Kodosi asks once for each account.",
                server = server
            )
        },
    );
    screen::Question {
        message: text!(
            "Let the devices of {account} see and control this Mac?",
            account = account
        )
        .in_language(language),
        detail: detail.in_language(language),
        allow: text!("Allow").in_language(language),
        refuse: text!("Don't Allow").in_language(language),
    }
}

/// Whether the desktop lets Kodosi Screen share the screen: the X server shares an X11 desktop, and
/// the desktop's portal one on Wayland, where the portal may lack what Kodosi Screen needs. A portal
/// that shared it once keeps doing so.
#[cfg(target_os = "linux")]
async fn desktop_shares() -> bool {
    use std::sync::atomic::{AtomicBool, Ordering};

    static PORTAL_SHARES: AtomicBool = AtomicBool::new(false);
    if ::screen::on_x11() || PORTAL_SHARES.load(Ordering::Relaxed) {
        return true;
    }
    let shares = tokio::task::spawn_blocking(kodosi_os::screen::portal_shares)
        .await
        .unwrap_or(false);
    PORTAL_SHARES.store(shares, Ordering::Relaxed);
    shares
}

/// A Mac and Windows let Kodosi Screen share the screen, with what they grant it.
#[cfg(not(target_os = "linux"))]
fn desktop_shares() -> std::future::Ready<bool> {
    std::future::ready(true)
}

fn off() -> Text {
    text!(
        "Screen sharing is off on this computer. The person at it can turn it on in Kodosi's settings there."
    )
}
