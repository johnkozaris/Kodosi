use std::collections::HashMap;

use uuid::Uuid;

use kodosi_domain::{
    ids::SessionId,
    permissions::{AccessLevel, ShareScope},
    session::{LocalSessionRecoveryState, SessionState, SessionSummary},
};

#[derive(Debug, Default)]
#[must_use = "each reported failure must be dispatched as RuntimeSessionEvent::Failed"]
pub(crate) struct TaskHealthReport {
    pub(crate) messages: Vec<String>,
    pub(crate) force_failed: Vec<(SessionId, Uuid, String)>,
    pub(crate) restart_agent_intel: Vec<(SessionId, String)>,
}

#[derive(Debug)]
pub(crate) struct SessionRecord {
    pub(crate) summary: SessionSummary,
    pub(crate) terminal_title: Option<String>,
    pub(crate) create_request_id: Option<String>,
    pub(crate) resume_source:
        Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
    pub(crate) launch_committed: bool,
    pub(crate) local_incarnation_id: Uuid,
    pub(crate) recovery: LocalSessionRecoveryState,
    pub(crate) stopping_since: Option<std::time::Instant>,
}

#[derive(Debug, Default)]
pub(crate) struct SessionRegistry {
    ordered_ids: Vec<SessionId>,
    sessions: HashMap<SessionId, SessionRecord>,
}

impl SessionRegistry {
    pub(crate) fn insert(&mut self, summary: SessionSummary) {
        self.insert_discovered(
            summary,
            None,
            Uuid::now_v7(),
            LocalSessionRecoveryState::Live,
        );
    }

    pub(crate) fn insert_discovered(
        &mut self,
        summary: SessionSummary,
        create_request_id: Option<String>,
        local_incarnation_id: Uuid,
        recovery: LocalSessionRecoveryState,
    ) {
        let id = summary.id;
        if !self.sessions.contains_key(&id) {
            self.ordered_ids.push(id);
        }
        self.sessions.insert(
            id,
            SessionRecord {
                summary,
                terminal_title: None,
                create_request_id,
                resume_source: None,
                launch_committed: true,
                local_incarnation_id,
                recovery,
                stopping_since: None,
            },
        );
    }

    pub(crate) fn ids(&self) -> &[SessionId] {
        &self.ordered_ids
    }

    pub(crate) fn record(&self, id: SessionId) -> Option<&SessionRecord> {
        self.sessions.get(&id)
    }

    pub(crate) fn record_mut(&mut self, id: SessionId) -> Option<&mut SessionRecord> {
        self.sessions.get_mut(&id)
    }

    pub(crate) fn set_resume_source(
        &mut self,
        id: SessionId,
        source: Option<kodosi_domain::provider_conversation::ProviderConversationIdentity>,
    ) {
        if let Some(record) = self.sessions.get_mut(&id) {
            record.resume_source = source;
        }
    }

    pub(crate) fn update_state(&mut self, id: SessionId, state: SessionState) {
        self.update_state_with_recovery(id, state, LocalSessionRecoveryState::Live);
    }

    pub(crate) fn update_state_with_recovery(
        &mut self,
        id: SessionId,
        state: SessionState,
        recovery: LocalSessionRecoveryState,
    ) {
        if let Some(record) = self.sessions.get_mut(&id) {
            if record.summary.state == SessionState::Stopping
                && !matches!(state, SessionState::Stopped | SessionState::Failed)
            {
                return;
            }

            record.stopping_since = if state == SessionState::Stopping {
                Some(std::time::Instant::now())
            } else {
                None
            };
            record.summary.state = state;
            record.recovery = recovery;
            record.summary.last_update = time::OffsetDateTime::now_utc();
        }
    }

    pub(crate) fn promote_stopping_timeouts(&mut self) -> TaskHealthReport {
        const STOPPING_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

        let mut messages = Vec::new();
        let mut force_failed = Vec::new();
        for (id, record) in &mut self.sessions {
            if record.summary.state == SessionState::Stopped {
                continue;
            }

            if record.summary.state == SessionState::Stopping
                && let Some(since) = record.stopping_since
                && since.elapsed() >= STOPPING_DEADLINE
            {
                record.summary.state = SessionState::Failed;
                record.summary.last_update = time::OffsetDateTime::now_utc();
                record.stopping_since = None;
                let elapsed = since.elapsed();
                messages.push(format!(
                    "{} watchdog promoted Stopping → Failed after {:?}",
                    id.short(),
                    elapsed,
                ));
                force_failed.push((
                    *id,
                    record.local_incarnation_id,
                    format!("stopping watchdog timeout after {elapsed:?}"),
                ));
            }
        }
        TaskHealthReport {
            messages,
            force_failed,
            restart_agent_intel: Vec::new(),
        }
    }

    pub(crate) fn mark_stopped(&mut self, id: SessionId) {
        if let Some(record) = self.sessions.get_mut(&id) {
            record.summary.state = SessionState::Stopped;
            record.summary.scope = ShareScope::JustMe;
            record.summary.access = AccessLevel::Inject;
            record.summary.active_count = 0;
            record.summary.entitled_count = 1;
            record.summary.last_update = time::OffsetDateTime::now_utc();
            record.recovery = LocalSessionRecoveryState::Recoverable;
        }
    }

    pub(crate) fn mark_failed(&mut self, id: SessionId) {
        if let Some(record) = self.sessions.get_mut(&id) {
            record.summary.state = SessionState::Failed;
            record.recovery = LocalSessionRecoveryState::Recoverable;
            record.summary.last_update = time::OffsetDateTime::now_utc();
            record.stopping_since = None;
        }
    }

    pub(crate) fn delete(&mut self, id: SessionId) -> bool {
        let Some(_record) = self.sessions.remove(&id) else {
            return false;
        };
        self.ordered_ids.retain(|candidate| *candidate != id);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::SessionRegistry;
    use kodosi_domain::{
        ids::SessionId,
        permissions::ShareScope,
        session::{SessionState, SessionSummary},
        terminal::TerminalSize,
    };

    fn test_size() -> TerminalSize {
        TerminalSize::new(8, 20).unwrap_or_else(|error| panic!("fixed size: {error}"))
    }

    #[test]
    fn replacing_same_id_does_not_duplicate_catalog_order() {
        let id = SessionId::new();
        let first = SessionSummary::new_owned(
            id,
            "first".to_owned(),
            "runtime".to_owned(),
            test_size(),
            None,
        );
        let mut second = first.clone();
        second.title = "second".to_owned();
        let mut registry = SessionRegistry::default();

        registry.insert(first);
        registry.insert(second);

        assert_eq!(registry.ids(), &[id]);
        assert_eq!(registry.record(id).unwrap().summary.title, "second");
    }

    #[test]
    fn cached_and_stopped_sessions_are_metadata_only() {
        let id = SessionId::new();
        let mut cached_summary = SessionSummary::new_owned(
            id,
            "Owned Session".to_owned(),
            "kodosi-test".to_owned(),
            test_size(),
            None,
        );
        cached_summary.state = kodosi_domain::session::SessionState::Stopped;

        let mut registry = SessionRegistry::default();
        registry.insert(cached_summary);
        assert!(registry.record(id).is_some());

        let running_id = SessionId::new();
        let running_summary = SessionSummary::new_owned(
            running_id,
            "Running Session".to_owned(),
            "kodosi-test-running".to_owned(),
            test_size(),
            None,
        );
        registry.insert(running_summary);

        registry.mark_stopped(running_id);
        let stopped = registry
            .record(running_id)
            .unwrap_or_else(|| panic!("stopped session should remain in registry"));
        assert_eq!(stopped.summary.scope, ShareScope::JustMe);
    }

    #[tokio::test]
    async fn stopping_watchdog_promotes_to_failed_and_stays_failed() {
        let id = SessionId::new();
        let summary = SessionSummary::new_owned(
            id,
            "stuck".to_owned(),
            "kodosi-test".to_owned(),
            test_size(),
            None,
        );
        let mut registry = SessionRegistry::default();
        registry.insert(summary);

        registry.update_state(id, SessionState::Stopping);
        if let Some(record) = registry.record_mut(id) {
            record.stopping_since =
                Some(std::time::Instant::now() - std::time::Duration::from_mins(1));
        }

        let report = registry.promote_stopping_timeouts();
        assert!(
            report
                .messages
                .iter()
                .any(|m| m.contains("watchdog promoted"))
        );
        assert!(
            report
                .force_failed
                .iter()
                .any(|(force_id, _, _)| *force_id == id)
        );
        let state_after_first_tick = registry
            .record(id)
            .map(|r| r.summary.state)
            .expect("record present");
        assert_eq!(state_after_first_tick, SessionState::Failed);

        tokio::task::yield_now().await;

        let _ = registry.promote_stopping_timeouts();
        let state_after_second_tick = registry
            .record(id)
            .map(|r| r.summary.state)
            .expect("record present");
        assert_eq!(
            state_after_second_tick,
            SessionState::Failed,
            "watchdog-promoted Failed must survive subsequent ticks"
        );
    }

    #[tokio::test]
    async fn update_state_stopping_seeds_stopping_since_timestamp() {
        let id = SessionId::new();
        let summary = SessionSummary::new_owned(
            id,
            "stuck".to_owned(),
            "kodosi-test".to_owned(),
            test_size(),
            None,
        );
        let mut registry = SessionRegistry::default();
        registry.insert(summary);
        assert!(registry.record(id).and_then(|r| r.stopping_since).is_none());

        let before = std::time::Instant::now();
        registry.update_state(id, SessionState::Stopping);
        let after = std::time::Instant::now();

        let seeded = registry
            .record(id)
            .and_then(|r| r.stopping_since)
            .expect("stopping_since must seed on entering Stopping");
        assert!(seeded >= before && seeded <= after);

        registry.update_state(id, SessionState::Running);
        assert_eq!(
            registry.record(id).and_then(|r| r.stopping_since),
            Some(seeded),
            "rejected transition must not reseed the deadline"
        );

        registry.update_state(id, SessionState::Stopped);
        assert!(registry.record(id).and_then(|r| r.stopping_since).is_none());

        let id2 = SessionId::new();
        let summary2 = SessionSummary::new_owned(
            id2,
            "stuck".to_owned(),
            "kodosi-test".to_owned(),
            test_size(),
            None,
        );
        registry.insert(summary2);
        registry.update_state(id2, SessionState::Stopping);
        assert!(
            registry
                .record(id2)
                .and_then(|r| r.stopping_since)
                .is_some()
        );
        registry.mark_failed(id2);
        assert!(
            registry
                .record(id2)
                .and_then(|r| r.stopping_since)
                .is_none()
        );
    }
}
