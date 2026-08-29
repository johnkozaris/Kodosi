use std::future::Future;

use tokio::sync::broadcast;

use crate::ports::error::Result;

#[derive(Clone, Debug)]
pub struct WatchEvent {
    pub path: std::path::PathBuf,
}

pub trait ArtifactWatcher: Send + Sync {
    fn subscribe(&self) -> broadcast::Receiver<WatchEvent>;

    fn shutdown(&self) -> impl Future<Output = Result<()>> + Send;
}
