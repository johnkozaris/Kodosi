use kodosi_domain::ids::SessionId;
use tokio_util::sync::CancellationToken;

use super::{TerminalCapability, TerminalSurface, hub::SubscriberHandle};

pub const RESUBSCRIBE_DELAYS: [std::time::Duration; 3] = [
    std::time::Duration::from_millis(50),
    std::time::Duration::from_millis(200),
    std::time::Duration::from_secs(1),
];

pub async fn resubscribe_terminal_hub(
    sink: &crate::RuntimeCommandSink,
    session_id: SessionId,
    surface: TerminalSurface,
    capability: TerminalCapability,
    cancellation: &CancellationToken,
) -> Option<SubscriberHandle> {
    for delay in RESUBSCRIBE_DELAYS {
        tokio::select! {
            () = cancellation.cancelled() => return None,
            () = tokio::time::sleep(delay) => {}
        }
        if !sink.is_runtime_loop_open() {
            return None;
        }
        match sink
            .subscribe_terminal_hub(session_id, surface, capability)
            .await
        {
            Ok(handle) => return Some(handle),
            Err(error) => {
                tracing::debug!(%error, %session_id, %surface, "terminal hub re-subscribe failed");
            }
        }
    }
    None
}
