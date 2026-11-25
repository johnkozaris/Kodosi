//! Kodosi Screen captures only Macs so far, so elsewhere no screens exist.

use std::{convert::Infallible, future::Ready, path::Path};

use shared_browser::Viewer;

use crate::{
    canvas::{PipeEnds, ScreenShown},
    text::Text,
};

#[derive(Clone)]
pub(super) struct Screens(Infallible);

impl Screens {
    pub(super) fn new(_data_root: &Path, _stopped: impl Fn()) -> Option<Self> {
        None
    }

    pub(super) fn watch(self, _viewer: Viewer) -> Ready<Result<PipeEnds, Text>> {
        match self.0 {}
    }

    pub(super) fn show(self, _shown: ScreenShown) -> Ready<()> {
        match self.0 {}
    }

    pub(super) fn stop(self) -> Ready<()> {
        match self.0 {}
    }
}
