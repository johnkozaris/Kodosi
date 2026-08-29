use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use agent_intel::mcp::McpHealth;
use futures_util::future::join_all;
use tokio::time::{self, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::AgentGlobalEvent;
use crate::runtime_event_bus::RuntimeEventSender;

const PROBE_INTERVAL: Duration = Duration::from_mins(1);

const INITIAL_DELAY: Duration = Duration::from_secs(2);

type StateKey = (String, String, String);

pub(crate) fn spawn(
    runtime_event_tx: RuntimeEventSender,
    cancellation: CancellationToken,
    claude_home: PathBuf,
    copilot_home: PathBuf,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run(runtime_event_tx, cancellation, claude_home, copilot_home).await;
    })
}

async fn run(
    runtime_event_tx: RuntimeEventSender,
    cancellation: CancellationToken,
    claude_home: PathBuf,
    copilot_home: PathBuf,
) {
    if wait(&cancellation, INITIAL_DELAY).await.is_err() {
        return;
    }

    let mut last_known: HashMap<StateKey, McpHealth> = HashMap::new();
    tokio::select! {
        biased;
        () = cancellation.cancelled() => return,
        () = run_cycle(
            &runtime_event_tx,
            &claude_home,
            &copilot_home,
            &mut last_known,
        ) => {}
    }

    let mut ticker = time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancellation.cancelled() => break,
            _ = ticker.tick() => {
                run_cycle(
                    &runtime_event_tx,
                    &claude_home,
                    &copilot_home,
                    &mut last_known,
                )
                .await;
            }
        }
    }
}

async fn wait(cancellation: &CancellationToken, dur: Duration) -> Result<(), ()> {
    tokio::select! {
        () = cancellation.cancelled() => Err(()),
        () = time::sleep(dur) => Ok(()),
    }
}

async fn run_cycle(
    runtime_event_tx: &RuntimeEventSender,
    claude_home: &std::path::Path,
    copilot_home: &std::path::Path,
    last_known: &mut HashMap<StateKey, McpHealth>,
) {
    let targets = agent_intel::mcp::enumerate_user_scope_targets(claude_home, copilot_home);

    let probes = targets
        .into_iter()
        .map(|t| async move {
            let health = agent_intel::mcp::probe_passive(&t.target).await;
            (t, health)
        })
        .collect::<Vec<_>>();
    let results = join_all(probes).await;

    let observed_keys: std::collections::HashSet<StateKey> = results
        .iter()
        .map(|(t, _)| (t.vendor.clone(), t.scope.clone(), t.name.clone()))
        .collect();
    let dropped: Vec<StateKey> = last_known
        .keys()
        .filter(|k| !observed_keys.contains(*k))
        .cloned()
        .collect();
    for key in dropped {
        last_known.remove(&key);
        emit(runtime_event_tx, &key.0, &key.1, &key.2, McpHealth::Unknown).await;
    }

    for (target, health) in results {
        let key: StateKey = (
            target.vendor.clone(),
            target.scope.clone(),
            target.name.clone(),
        );
        last_known.insert(key, health.clone());
        emit(
            runtime_event_tx,
            &target.vendor,
            &target.scope,
            &target.name,
            health,
        )
        .await;
    }
}

async fn emit(
    runtime_event_tx: &RuntimeEventSender,
    vendor: &str,
    scope: &str,
    server_name: &str,
    health: McpHealth,
) {
    if let Err(error) = runtime_event_tx
        .send_agent_global(AgentGlobalEvent::McpHealth {
            vendor: vendor.to_owned(),
            scope: scope.to_owned(),
            server_name: server_name.to_owned(),
            health,
        })
        .await
    {
        tracing::debug!(%vendor, %scope, %server_name, %error, "mcpHealth push dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_probe_target(claude_home: &std::path::Path) {
        fs::create_dir_all(claude_home).expect("claude home");
        fs::write(
            claude_home.join("mcp.json"),
            r#"{"mcpServers":{"health-check":{"command":"/bin/true"}}}"#,
        )
        .expect("MCP config");
    }

    async fn yield_until_event(
        receiver: &mut tokio::sync::mpsc::Receiver<AgentGlobalEvent>,
    ) -> AgentGlobalEvent {
        for _ in 0..16 {
            if let Ok(event) = receiver.try_recv() {
                return event;
            }
            tokio::task::yield_now().await;
        }
        receiver.try_recv().expect("health event")
    }

    #[tokio::test(start_paused = true)]
    async fn first_probe_runs_at_end_of_grace_then_repeats_on_interval() {
        let temp = tempfile::tempdir().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        write_probe_target(&claude);
        let (sender, mut receivers) = crate::runtime_event_bus::runtime_event_channels();
        let cancellation = CancellationToken::new();
        let task = spawn(sender, cancellation.clone(), claude, copilot);
        tokio::task::yield_now().await;

        tokio::time::advance(INITIAL_DELAY - Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert!(receivers.agent_global_rx.try_recv().is_err());

        tokio::time::advance(Duration::from_millis(1)).await;
        std::assert_matches!(
            yield_until_event(&mut receivers.agent_global_rx).await,
            AgentGlobalEvent::McpHealth { server_name, .. } if server_name == "health-check"
        );

        tokio::time::advance(PROBE_INTERVAL - Duration::from_millis(1)).await;
        tokio::task::yield_now().await;
        assert!(receivers.agent_global_rx.try_recv().is_err());

        tokio::time::advance(Duration::from_millis(1)).await;
        std::assert_matches!(
            yield_until_event(&mut receivers.agent_global_rx).await,
            AgentGlobalEvent::McpHealth { server_name, .. } if server_name == "health-check"
        );

        cancellation.cancel();
        task.await.expect("health task");
    }

    #[tokio::test(start_paused = true)]
    async fn cancellation_during_grace_never_probes() {
        let temp = tempfile::tempdir().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        write_probe_target(&claude);
        let (sender, mut receivers) = crate::runtime_event_bus::runtime_event_channels();
        let cancellation = CancellationToken::new();
        let task = spawn(sender, cancellation.clone(), claude, copilot);
        tokio::task::yield_now().await;

        cancellation.cancel();
        tokio::time::advance(INITIAL_DELAY + PROBE_INTERVAL).await;
        task.await.expect("health task");

        assert!(receivers.agent_global_rx.try_recv().is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cycle_drops_known_server_when_config_disappears() {
        let temp = tempfile::tempdir().unwrap();
        let claude = temp.path().join(".claude");
        let copilot = temp.path().join(".copilot");
        let mut last_known: HashMap<StateKey, McpHealth> = HashMap::new();
        last_known.insert(
            ("claude".to_owned(), "user".to_owned(), "ghost".to_owned()),
            McpHealth::Healthy,
        );

        let (sender, mut receivers) = crate::runtime_event_bus::runtime_event_channels();

        run_cycle(&sender, &claude, &copilot, &mut last_known).await;

        assert!(
            last_known.is_empty(),
            "known-but-vanished server gets cleared"
        );

        let event = receivers
            .agent_global_rx
            .recv()
            .await
            .expect("Unknown push should fire for the dropped server");
        let body = serde_json::to_value(&event).expect("event serialises");
        assert_eq!(
            body.get("type").and_then(|t| t.as_str()),
            Some("agent.global.mcp.health")
        );
        assert_eq!(body.get("vendor").and_then(|t| t.as_str()), Some("claude"));
        assert_eq!(
            body.get("serverName").and_then(|t| t.as_str()),
            Some("ghost")
        );
        let health = body.get("health").expect("health field present");
        assert_eq!(
            health.get("kind").and_then(|t| t.as_str()),
            Some("unknown"),
            "dropped server emits the camelCase `unknown` discriminator"
        );
    }
}
