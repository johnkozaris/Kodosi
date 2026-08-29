use std::sync::Arc;

use tokio::sync::broadcast;

use crate::domain::AgentProviderState;

pub trait LiveAgentProvider: Send + Sync {
    fn current_state(&self) -> Arc<AgentProviderState>;

    fn ticks(&self) -> broadcast::Receiver<()>;

    fn feed_terminal_output(&self, bytes: &[u8]);

    fn tick_idle(&self);

    fn notify_artifact_changed(&self, path: &std::path::Path);

    fn settle_deferred_io(&self) -> BoxFuture<'_, ()> {
        Box::pin(std::future::ready(()))
    }
}

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn Future<Output = T> + Send + 'a>>;
