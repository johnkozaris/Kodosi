use tokio::sync::broadcast;

const CAPACITY: usize = 16;

#[derive(Clone, Debug)]
pub struct SnapshotBus {
    sender: broadcast::Sender<()>,
}

impl SnapshotBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.sender.subscribe()
    }

    pub fn publish(&self) {
        let _ = self.sender.send(());
    }
}

impl Default for SnapshotBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests panic for failure clarity"
)]
mod tests {
    use super::SnapshotBus;
    use tokio::sync::broadcast::error::TryRecvError;

    #[tokio::test]
    async fn subscribe_then_publish_delivers_tick() {
        let bus = SnapshotBus::new();
        let mut rx = bus.subscribe();
        bus.publish();
        rx.recv().await.expect("tick delivered");
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_is_silent() {
        let bus = SnapshotBus::new();
        bus.publish();
        let mut rx = bus.subscribe();
        let result = rx.try_recv();
        std::assert_matches!(result, Err(TryRecvError::Empty));
    }
}
