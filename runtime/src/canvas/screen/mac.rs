//! Kodosi Screen on a Mac, through the screen library.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use bytes::Bytes;
use shared_browser::Viewer;

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
    /// Kodosi Screen is in the Helpers folder of Kodosi's app; `KODOSI_SCREEN_HELPER` names
    /// another one for development. Its socket goes in the host's private IPC folder, since Unix
    /// socket paths are short.
    pub(super) fn new(
        data_root: &Path,
        stopped: impl Fn() + Send + Sync + 'static,
    ) -> Option<Self> {
        let helper = std::env::var_os("KODOSI_SCREEN_HELPER")
            .map(PathBuf::from)
            .or_else(|| {
                let program = std::env::current_exe().ok()?;
                let program = kodosi_os::fs::canonicalize(&program).unwrap_or(program);
                Some(
                    program
                        .parent()?
                        .parent()?
                        .join("Helpers/Kodosi Screen.app"),
                )
            })
            .filter(|helper| helper.is_dir())?;
        let sockets = kodosi_os::ipc::private_dir(&crate::local_host::namespace(data_root))
            .map_or_else(|_| data_root.join("screen"), |folder| folder.join("screen"));
        Some(Self(screen::Screens::new(screen::Settings {
            helper,
            sockets,
            arguments: Vec::new(),
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
}

impl Viewed for screen::Pipe {
    async fn send(&self, message: &[u8]) -> bool {
        Self::send(self, message).await.is_ok()
    }

    async fn receive(&mut self) -> Option<Bytes> {
        Self::receive(self).await
    }
}
