//! Kodosi Screen, the helper that captures this computer's screen, through the screen library.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use bytes::Bytes;
use shared_browser::Viewer;
use tokio::sync::watch;

use crate::{
    canvas::{
        PipeEnds, ScreenAccess, ScreenShown,
        pipe::{self, Viewed},
    },
    text::Text,
};

#[derive(Clone)]
pub(super) struct Screens(screen::Screens);

impl Screens {
    /// Kodosi Screen ships next to the host's program: in the Helpers folder of Kodosi's app on a
    /// Mac, beside the `kodosi` program elsewhere. `KODOSI_SCREEN_HELPER` names another one for
    /// development. On Linux it keeps what the desktop granted it in the data root.
    pub(super) fn new(
        data_root: &Path,
        owner: watch::Receiver<Option<String>>,
        stopped: impl Fn() + Send + Sync + 'static,
    ) -> Option<Self> {
        let helper = std::env::var_os("KODOSI_SCREEN_HELPER")
            .map(PathBuf::from)
            .or_else(beside_the_host)
            .filter(|helper| {
                if cfg!(target_os = "macos") {
                    helper.is_dir()
                } else {
                    helper.is_file()
                }
            })?;
        let arguments = if cfg!(target_os = "linux") {
            vec!["--grant".into(), data_root.join("screen-grant.json").into()]
        } else {
            Vec::new()
        };
        Some(Self(screen::Screens::new(screen::Settings {
            helper,
            arguments,
            notice: Arc::new(|devices| {
                let language = crate::text::language();
                let watching = match devices {
                    [device] => text!("{device} is watching", device = device),
                    _ => text!("{devices} are watching", devices = devices.join(", ")),
                };
                screen::Notice {
                    watching: watching.in_language(language),
                    stop: text!("Stop sharing").in_language(language),
                }
            }),
            stopped: Arc::new(stopped),
            owner,
        })))
    }

    pub(super) async fn watch(self, viewer: Viewer) -> Result<PipeEnds, Text> {
        self.0
            .watch(viewer)
            .await
            .map(pipe::carry)
            .map_err(|problem| Text::plain(problem.to_string()))
    }

    pub(super) async fn show(self, shown: ScreenShown) {
        self.0
            .show(match shown {
                ScreenShown::Screen => screen::Wanted::Screen,
                ScreenShown::Window { words } => screen::Wanted::Window(words),
            })
            .await;
    }

    pub(super) async fn stop(self) {
        self.0.stop().await;
    }

    pub(super) async fn access(self) -> Option<ScreenAccess> {
        let access = self.0.access().await.ok()?;
        Some(ScreenAccess {
            screen: access.screen,
            input: access.input,
        })
    }

    pub(super) async fn ask(self) {
        if let Err(problem) = self.0.ask().await {
            tracing::warn!(%problem, "Kodosi Screen could not ask for its permissions");
        }
    }

    /// Whether the person at the computer allowed the account, which is no when Kodosi Screen
    /// could not ask them.
    pub(super) async fn allow(&self, question: screen::Question, vouch: Option<String>) -> bool {
        self.0
            .allow(question, vouch)
            .await
            .unwrap_or_else(|problem| {
                tracing::warn!(%problem, "Kodosi Screen did not allow this account");
                false
            })
    }

    pub(super) async fn forget(self) {
        if let Err(problem) = self.0.forget().await {
            tracing::warn!(%problem, "Kodosi Screen could not forget the accounts that were allowed");
        }
    }
}

fn beside_the_host() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    let program = kodosi_os::fs::canonicalize(&program).unwrap_or(program);
    let folder = program.parent()?;
    Some(if cfg!(target_os = "macos") {
        folder.parent()?.join("Helpers/Kodosi.app")
    } else {
        folder.join(format!("kodosi-screen{}", std::env::consts::EXE_SUFFIX))
    })
}

impl Viewed for screen::Pipe {
    async fn send(&self, message: &[u8]) -> bool {
        Self::send(self, message).await.is_ok()
    }

    async fn receive(&mut self) -> Option<Bytes> {
        Self::receive(self).await
    }
}
