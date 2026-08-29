use crate::{AgentGlobalEvent, runtime_event_bus::RuntimeEventSender};
use std::{
    future::Future,
    sync::{
        LazyLock,
        atomic::{AtomicU64, Ordering},
    },
};

type VersionProbeFuture = std::pin::Pin<Box<dyn Future<Output = Option<String>> + Send>>;
type VersionProbe = fn(kodosi_session::AgentKind) -> VersionProbeFuture;

fn query_install_version(agent: kodosi_session::AgentKind) -> VersionProbeFuture {
    Box::pin(::agent_intel::ops::cli_version::query_install_version(
        agent,
    ))
}

pub(crate) fn spawn_probed_global_status_refresh(
    runtime_event_tx: RuntimeEventSender,
    cwd: Option<String>,
    request_generation: Option<u64>,
) -> tokio::task::JoinHandle<()> {
    spawn_global_status_refresh(
        runtime_event_tx,
        cwd,
        request_generation,
        query_install_version,
    )
}

fn spawn_global_status_refresh(
    runtime_event_tx: RuntimeEventSender,
    cwd: Option<String>,
    request_generation: Option<u64>,
    version_probe: VersionProbe,
) -> tokio::task::JoinHandle<()> {
    let generation = GLOBAL_REFRESH_GENERATION
        .fetch_add(1, Ordering::SeqCst)
        .saturating_add(1);
    tokio::spawn(async move {
        let (claude_version, copilot_version) = tokio::join!(
            version_probe(kodosi_session::AgentKind::Claude),
            version_probe(kodosi_session::AgentKind::Copilot),
        );
        if GLOBAL_REFRESH_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let statuses = ::agent_intel::runtime::io_pool::spawn_io(move || {
            (
                ::agent_intel::claude::global::ClaudeGlobalStatus::read(cwd.as_deref())
                    .with_version(claude_version),
                ::agent_intel::copilot::global::CopilotGlobalStatus::read(cwd.as_deref())
                    .with_version(copilot_version),
            )
        })
        .await;
        if GLOBAL_REFRESH_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Ok((claude, copilot)) = statuses else {
            tracing::warn!("agent global status refresh task failed");
            return;
        };
        let _publish = GLOBAL_REFRESH_PUBLISH.lock().await;
        if GLOBAL_REFRESH_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let permits = runtime_event_tx
            .agent_global_event_sender()
            .reserve_many(2)
            .await;
        if GLOBAL_REFRESH_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let Ok(mut permits) = permits else {
            tracing::debug!("agent global status refresh channel closed");
            return;
        };
        let (Some(claude_slot), Some(copilot_slot)) = (permits.next(), permits.next()) else {
            return;
        };
        claude_slot.send(AgentGlobalEvent::ClaudeStatus {
            generation: request_generation,
            status: claude,
        });
        copilot_slot.send(AgentGlobalEvent::CopilotStatus {
            generation: request_generation,
            status: copilot,
        });
    })
}

static GLOBAL_REFRESH_GENERATION: AtomicU64 = AtomicU64::new(0);
static GLOBAL_REFRESH_PUBLISH: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[cfg(test)]
mod tests {
    use super::*;

    fn refresh_test_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    fn fixed_version(agent: kodosi_session::AgentKind) -> VersionProbeFuture {
        Box::pin(async move {
            match agent {
                kodosi_session::AgentKind::Claude => Some("2.1.200".to_owned()),
                kodosi_session::AgentKind::Copilot => Some("1.0.99".to_owned()),
            }
        })
    }

    #[tokio::test]
    async fn async_refresh_eventually_publishes_probed_versions() {
        let _guard = refresh_test_lock().lock().await;
        let (sender, mut receivers) = crate::runtime_event_bus::runtime_event_channels();

        let refresh = spawn_global_status_refresh(sender, None, Some(7), fixed_version);
        refresh.await.expect("refresh task");

        let first = receivers
            .agent_global_rx
            .recv()
            .await
            .expect("Claude status");
        let second = receivers
            .agent_global_rx
            .recv()
            .await
            .expect("Copilot status");
        std::assert_matches!(
            first,
            AgentGlobalEvent::ClaudeStatus { generation: Some(7), status }
                if status.claude_code_version.as_deref() == Some("2.1.200")
        );
        std::assert_matches!(
            second,
            AgentGlobalEvent::CopilotStatus { generation: Some(7), status }
                if status.copilot_cli_version.as_deref() == Some("1.0.99")
        );
    }

    #[tokio::test]
    async fn newer_refresh_generation_cancels_stale_cwd_pair() {
        let _guard = refresh_test_lock().lock().await;
        let (sender, mut receivers) = crate::runtime_event_bus::runtime_event_channels();

        let stale = spawn_global_status_refresh(
            sender.clone(),
            Some("/repo/old".to_owned()),
            Some(1),
            fixed_version,
        );
        let current = spawn_global_status_refresh(
            sender,
            Some("/repo/current".to_owned()),
            Some(2),
            fixed_version,
        );
        stale.await.expect("stale task");
        current.await.expect("current task");

        let first = receivers
            .agent_global_rx
            .recv()
            .await
            .expect("first status");
        let second = receivers
            .agent_global_rx
            .recv()
            .await
            .expect("second status");
        for event in [first, second] {
            match event {
                AgentGlobalEvent::ClaudeStatus { generation, status } => {
                    assert_eq!(generation, Some(2));
                    assert_eq!(status.cwd.as_deref(), Some("/repo/current"));
                }
                AgentGlobalEvent::CopilotStatus { generation, status } => {
                    assert_eq!(generation, Some(2));
                    assert_eq!(status.cwd.as_deref(), Some("/repo/current"));
                }
                AgentGlobalEvent::McpHealth { .. } => panic!("unexpected MCP event"),
            }
        }
        assert!(
            receivers.agent_global_rx.try_recv().is_err(),
            "stale generation must publish neither vendor"
        );
    }

    #[tokio::test]
    async fn blocked_send_rechecks_generation_and_never_splits_pair() {
        let _guard = refresh_test_lock().lock().await;
        let (sender, mut receivers) = crate::runtime_event_bus::runtime_event_channels();
        for index in 0..crate::runtime_event_bus::RUNTIME_AGENT_GLOBAL_CAPACITY {
            sender
                .send_agent_global(AgentGlobalEvent::McpHealth {
                    vendor: "filler".to_owned(),
                    scope: "user".to_owned(),
                    server_name: format!("server-{index}"),
                    health: ::agent_intel::mcp::McpHealth::Unknown,
                })
                .await
                .expect("fill");
        }
        let stale = spawn_global_status_refresh(
            sender.clone(),
            Some("/repo/old".to_owned()),
            Some(1),
            fixed_version,
        );
        tokio::task::yield_now().await;
        let current = spawn_global_status_refresh(
            sender,
            Some("/repo/current".to_owned()),
            Some(2),
            fixed_version,
        );
        for _ in 0..crate::runtime_event_bus::RUNTIME_AGENT_GLOBAL_CAPACITY {
            drop(receivers.agent_global_rx.recv().await);
        }
        stale.await.expect("stale task");
        current.await.expect("current task");

        for _ in 0..2 {
            match receivers
                .agent_global_rx
                .recv()
                .await
                .expect("current pair")
            {
                AgentGlobalEvent::ClaudeStatus { generation, status } => {
                    assert_eq!(generation, Some(2));
                    assert_eq!(status.cwd.as_deref(), Some("/repo/current"));
                }
                AgentGlobalEvent::CopilotStatus { generation, status } => {
                    assert_eq!(generation, Some(2));
                    assert_eq!(status.cwd.as_deref(), Some("/repo/current"));
                }
                AgentGlobalEvent::McpHealth { .. } => panic!("unexpected filler"),
            }
        }
        assert!(receivers.agent_global_rx.try_recv().is_err());
    }
}
