use std::time::Duration;

use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::runtime_event_bus::RuntimeEventSender;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

pub(crate) async fn heartbeat_loop(tx: RuntimeEventSender, cancellation: CancellationToken) {
    let mut interval = time::interval(HEARTBEAT_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            () = cancellation.cancelled() => break,
            _ = interval.tick() => {
                if !tx.try_send_heartbeat() {
                    break;
                }
            }
        }
    }
}
