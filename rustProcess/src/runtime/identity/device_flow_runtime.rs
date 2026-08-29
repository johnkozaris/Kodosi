use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::{
    Result,
    identity_core::device_flow::{DeviceAuthorizationResponse, DeviceFlowClient, DeviceFlowResult},
};

const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

#[derive(Debug)]
pub(crate) struct DeviceFlowRuntime {
    client: DeviceFlowClient,
    result_tx: mpsc::Sender<DeviceFlowResult>,
    result_rx: mpsc::Receiver<DeviceFlowResult>,
    cancellation: Option<CancellationToken>,
    poll_task: Option<tokio::task::JoinHandle<()>>,
}

impl DeviceFlowRuntime {
    pub(crate) fn new(client: DeviceFlowClient) -> Self {
        let (result_tx, result_rx) = mpsc::channel(1);
        Self {
            client,
            result_tx,
            result_rx,
            cancellation: None,
            poll_task: None,
        }
    }

    pub(crate) fn is_configured(&self) -> bool {
        self.client.is_configured()
    }

    pub(crate) fn client(&self) -> &DeviceFlowClient {
        &self.client
    }

    pub(crate) async fn start_authorization(&self) -> Result<DeviceAuthorizationResponse> {
        self.client.start().await
    }

    pub(crate) fn start_poll(
        &mut self,
        response: &DeviceAuthorizationResponse,
        shutdown: &CancellationToken,
    ) {
        self.cancel();
        let cancellation = shutdown.child_token();
        let poll_task = self.client.spawn_poll(
            response.device_code.clone(),
            response.token_endpoint.clone(),
            response.interval.unwrap_or(DEFAULT_POLL_INTERVAL_SECS),
            response.expires_in,
            cancellation.clone(),
            self.result_tx.clone(),
        );
        self.cancellation = Some(cancellation);
        self.poll_task = Some(poll_task);
    }

    pub(crate) fn drain_results(&mut self) -> Vec<DeviceFlowResult> {
        let mut results = Vec::new();
        while let Ok(result) = self.result_rx.try_recv() {
            results.push(result);
        }

        if results.is_empty() {
            if self
                .poll_task
                .as_ref()
                .is_some_and(tokio::task::JoinHandle::is_finished)
            {
                self.cancellation = None;
                self.poll_task = None;
            }
            return results;
        }

        self.finish_poll();
        results
    }

    pub(crate) fn cancel(&mut self) -> bool {
        let had_cancellation = self.cancellation.take().is_some_and(|cancellation| {
            cancellation.cancel();
            true
        });
        let had_task = self.poll_task.take().is_some_and(|poll_task| {
            poll_task.abort();
            true
        });
        self.replace_result_channel();
        had_cancellation || had_task
    }

    fn finish_poll(&mut self) {
        self.cancellation = None;
        if let Some(poll_task) = self.poll_task.take()
            && !poll_task.is_finished()
        {
            poll_task.abort();
        }
    }

    fn replace_result_channel(&mut self) {
        let (result_tx, result_rx) = mpsc::channel(1);
        self.result_tx = result_tx;
        self.result_rx = result_rx;
    }

    #[cfg(test)]
    pub(crate) fn attach_for_test(
        &mut self,
        cancellation: CancellationToken,
        poll_task: tokio::task::JoinHandle<()>,
    ) {
        self.cancel();
        self.cancellation = Some(cancellation);
        self.poll_task = Some(poll_task);
    }

    #[cfg(test)]
    pub(crate) fn is_active_for_test(&self) -> bool {
        self.cancellation.is_some() || self.poll_task.is_some()
    }

    #[cfg(test)]
    pub(crate) fn enqueue_result_for_test(&self, result: DeviceFlowResult) {
        self.result_tx
            .try_send(result)
            .expect("test device-flow result should enqueue");
    }
}

impl Drop for DeviceFlowRuntime {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::DeviceFlowRuntime;
    use crate::{
        config::AuthConfig,
        identity_core::device_flow::{DeviceFlowClient, DeviceFlowResult},
    };

    fn test_client() -> DeviceFlowClient {
        DeviceFlowClient::new(&AuthConfig::default())
            .unwrap_or_else(|error| panic!("test device-flow client should construct: {error}"))
    }

    fn test_runtime() -> DeviceFlowRuntime {
        DeviceFlowRuntime::new(test_client())
    }

    fn pending_poll_task() -> (CancellationToken, tokio::task::JoinHandle<()>) {
        (
            CancellationToken::new(),
            tokio::spawn(std::future::pending::<()>()),
        )
    }

    async fn wait_for_finished(abort_handle: &tokio::task::AbortHandle) {
        for _ in 0..10 {
            if abort_handle.is_finished() {
                return;
            }
            tokio::task::yield_now().await;
        }
        assert!(abort_handle.is_finished());
    }

    #[tokio::test]
    async fn cancel_cancels_and_aborts_poll_task() {
        let mut runtime = test_runtime();
        let (cancellation, poll_task) = pending_poll_task();
        let abort_handle = poll_task.abort_handle();
        runtime.attach_for_test(cancellation.clone(), poll_task);

        assert!(runtime.cancel());

        assert!(cancellation.is_cancelled());
        assert!(!runtime.is_active_for_test());
        wait_for_finished(&abort_handle).await;
    }

    #[tokio::test]
    async fn drain_results_returns_terminal_result_and_clears_active_poll() {
        let mut runtime = test_runtime();
        let (cancellation, poll_task) = pending_poll_task();
        let abort_handle = poll_task.abort_handle();
        runtime.attach_for_test(cancellation, poll_task);
        runtime
            .result_tx
            .try_send(DeviceFlowResult::Denied)
            .expect("test result should enqueue");

        let results = runtime.drain_results();

        std::assert_matches!(results.as_slice(), [DeviceFlowResult::Denied]);
        assert!(!runtime.is_active_for_test());
        wait_for_finished(&abort_handle).await;
    }

    #[tokio::test]
    async fn cancel_discards_queued_stale_results() {
        let mut runtime = test_runtime();
        runtime
            .result_tx
            .try_send(DeviceFlowResult::Expired)
            .expect("test result should enqueue");

        assert!(!runtime.cancel());

        assert!(runtime.drain_results().is_empty());
    }

    #[tokio::test]
    async fn cancel_invalidates_late_sender_clones() {
        let mut runtime = test_runtime();
        let stale_sender = runtime.result_tx.clone();
        let (cancellation, poll_task) = pending_poll_task();
        runtime.attach_for_test(cancellation, poll_task);

        assert!(runtime.cancel());

        assert!(stale_sender.try_send(DeviceFlowResult::Denied).is_err());
        assert!(runtime.drain_results().is_empty());
    }
}
