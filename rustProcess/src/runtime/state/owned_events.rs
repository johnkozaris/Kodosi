use time::OffsetDateTime;

use crate::{
    discovery::ShelfItem,
    runtime::{AppState, FailedSessionRuntimeDisposition, SessionEventEffects},
    session_runtime::project::{CachedProjectDiscovery, ProjectDiscovery},
};
use kodosi_domain::{
    ids::SessionId,
    lifecycle::StopReason,
    session::{SessionMode, SessionState},
    terminal::TerminalSize,
};

impl AppState {
    pub(crate) fn apply_terminal_output_observed(&mut self, id: SessionId) {
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary.last_update = OffsetDateTime::now_utc();
        }
    }

    pub(crate) fn apply_capture_metadata(
        &mut self,
        id: SessionId,
        size: TerminalSize,
        working_dir: Option<String>,
        refresh_working_dir: bool,
        effects: &mut SessionEventEffects,
    ) {
        let next_working_dir = if refresh_working_dir {
            working_dir
        } else {
            self.local
                .sessions
                .record(id)
                .and_then(|record| record.summary.working_dir.clone())
        };
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary.size = size;
            record.summary.last_update = OffsetDateTime::now_utc();
            record.summary.working_dir.clone_from(&next_working_dir);
        }
        if let Some(working_dir) = next_working_dir {
            effects.project_discovery =
                self.queue_project_discovery(&working_dir, refresh_working_dir);
        }
        self.shelf.sync_owned_sessions(self.local.sessions.ids());
    }

    pub(crate) fn apply_working_dir_changed(
        &mut self,
        id: SessionId,
        working_dir: String,
        effects: &mut SessionEventEffects,
    ) {
        effects.project_discovery = self.queue_project_discovery(&working_dir, true);
        let (changed, detected_agent) =
            self.local
                .sessions
                .record(id)
                .map_or((false, None), |record| {
                    (
                        record.summary.working_dir.as_deref() != Some(working_dir.as_str()),
                        record.summary.detected_agent.clone(),
                    )
                });
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary.working_dir = Some(working_dir);
            record.summary.last_update = OffsetDateTime::now_utc();
        }
        if changed && self.agent_intel.registry.active(id) {
            effects.agent_intel_teardown = Some(id);
            if let Some(agent_name) = detected_agent {
                effects.agent_intel_spawn = Some((id, agent_name));
            }
        }
    }

    pub(crate) fn apply_runtime_metadata(
        &mut self,
        id: SessionId,
        working_dir: Option<String>,
        running_command: Option<String>,
        detected_agent: Option<&str>,
        effects: &mut SessionEventEffects,
    ) {
        let next_working_dir = working_dir.or_else(|| {
            self.local
                .sessions
                .record(id)
                .and_then(|record| record.summary.working_dir.clone())
        });
        let previous = self.local.sessions.record(id).map(|record| {
            (
                record.summary.detected_agent.clone(),
                record.summary.working_dir.clone(),
            )
        });
        let previous_agent = previous.as_ref().and_then(|(agent, _)| agent.clone());
        let previous_working_dir = previous.and_then(|(_, cwd)| cwd);
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary.working_dir.clone_from(&next_working_dir);
            record.summary.running_command = running_command;
            if record.summary.detected_agent.as_deref() != detected_agent {
                record.summary.mode = SessionMode::Normal;
            }
            record.summary.detected_agent = detected_agent.map(ToOwned::to_owned);
            record.summary.last_update = OffsetDateTime::now_utc();
        }
        let agent_changed = detected_agent != previous_agent.as_deref();
        let working_dir_changed = next_working_dir != previous_working_dir;
        if agent_changed || working_dir_changed {
            if previous_agent.is_some() && self.agent_intel.registry.active(id) {
                effects.agent_intel_teardown = Some(id);
            }
            if let Some(agent_name) = detected_agent {
                effects.agent_intel_spawn = Some((id, agent_name.to_owned()));
            }
        }
        if let Some(working_dir) = next_working_dir {
            effects.project_discovery = self.queue_project_discovery(&working_dir, true);
        }
    }

    pub(crate) fn apply_project_discovery_ready(
        &mut self,
        working_dir: String,
        discovery: ProjectDiscovery,
    ) {
        self.project_discovery_in_flight.remove(&working_dir);

        while self.project_discovery.len() >= Self::MAX_PROJECT_DISCOVERY_ENTRIES {
            let oldest_key = self
                .project_discovery
                .iter()
                .min_by_key(|(_, v)| v.refreshed_at())
                .map(|(k, _)| k.clone());
            match oldest_key {
                Some(key) => {
                    self.project_discovery.remove(&key);
                }
                None => break,
            }
        }
        self.project_discovery
            .insert(working_dir, CachedProjectDiscovery::new(discovery));
    }

    pub(crate) fn apply_project_discovery_failed(&mut self, working_dir: &str, message: &str) {
        self.project_discovery_in_flight.remove(working_dir);
        self.record_log(format!(
            "project discovery failed for {working_dir}: {message}"
        ));
    }

    pub(crate) fn apply_owned_state_changed(
        &mut self,
        id: SessionId,
        state: SessionState,
        relay_generation: u64,
    ) {
        let current_generation = self.sharing.host_relays.generation(id);
        if relay_generation == crate::sharing::host_relay::registry::NO_HOST_RELAY_GENERATION
            || relay_generation != current_generation
        {
            tracing::debug!(
                session_id = %id,
                ?state,
                relay_generation,
                current_generation,
                "dropped state change from a retired host relay",
            );
            return;
        }

        self.local.sessions.update_state(id, state);
        if state == SessionState::Running {
            self.shelf.activate(ShelfItem::Owned(id));
        }
        self.sync_host_relay_status();
    }

    pub(crate) fn apply_host_frame_reservation_exhausted(&mut self, id: SessionId) {
        if self.sharing.host_relays.retire_for_immediate_restart(id) {
            self.local
                .sessions
                .update_state(id, SessionState::Reconnecting);
            self.record_log(format!(
                "{} host relay exhausted its frame reservation; restarting with fresh ranges",
                id.short()
            ));
            self.sync_host_relay_status();
        }
    }

    pub(crate) fn apply_info(&mut self, id: SessionId, message: &str) {
        self.record_log(format!("{}: {message}", id.short()));
    }

    pub(crate) fn apply_log_error(&mut self, id: SessionId, message: &str) {
        self.record_log(format!("{} error: {message}", id.short()));
    }

    pub(crate) fn apply_owned_stopped(&mut self, id: SessionId, reason: StopReason) {
        self.consume_terminal_scope_restore(id);
        self.mark_session_stopped(id);
        self.sharing.host_relays.teardown(id);
        self.agent_intel.registry.detach(id);
        self.reset_owned_agent_summary(id);
        self.shelf.sync_owned_sessions(self.local.sessions.ids());
        self.record_log(format!("{} stopped ({reason:?})", id.short()));
        self.sync_host_relay_status();
        self.pending_work.clear_delete(id);
        self.pending_work.queue_ready_delete(id);
    }

    pub(crate) fn apply_owned_failed(&mut self, id: SessionId, message: &str) {
        self.consume_terminal_scope_restore(id);
        if self.pending_work.promote_delete_after_stop(id) {
            self.mark_session_failed(id, FailedSessionRuntimeDisposition::DropNow);
        } else {
            self.mark_session_failed(id, FailedSessionRuntimeDisposition::AwaitStoppedEvent);
        }

        self.sharing.shared_sessions.clear(id);
        self.sharing.host_relays.teardown(id);
        self.agent_intel.registry.detach(id);
        self.reset_owned_agent_summary(id);
        self.record_log(format!("{} failed: {message}", id.short()));
        self.sync_host_relay_status();
    }

    fn reset_owned_agent_summary(&mut self, id: SessionId) {
        if let Some(record) = self.local.sessions.record_mut(id) {
            record.summary.mode = SessionMode::Normal;
            record.summary.running_command = None;
            record.summary.detected_agent = None;
        }
    }
}
