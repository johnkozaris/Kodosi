use std::collections::BTreeMap;

use agent_intel::AgentIntelSnapshot;
use kodosi_domain::ids::SessionId;
use uuid::Uuid;

use crate::host_protocol::{AgentIntelEvent, LiveAgentIntelEntry};

#[derive(Debug)]
struct RetainedAgentIntel {
    session_incarnation_id: Uuid,
    snapshot: AgentIntelSnapshot,
}

#[derive(Debug)]
pub(crate) struct LiveAgentIntelAuthority {
    authority_incarnation_id: Uuid,
    revision: u64,
    entries: BTreeMap<SessionId, RetainedAgentIntel>,
}

impl Default for LiveAgentIntelAuthority {
    fn default() -> Self {
        Self {
            authority_incarnation_id: Uuid::now_v7(),
            revision: 0,
            entries: BTreeMap::new(),
        }
    }
}

impl LiveAgentIntelAuthority {
    pub(crate) fn upsert(
        &mut self,
        session_id: SessionId,
        session_incarnation_id: Uuid,
        snapshot: AgentIntelSnapshot,
    ) -> Option<AgentIntelEvent> {
        if self.entries.get(&session_id).is_some_and(|current| {
            current.session_incarnation_id == session_incarnation_id && current.snapshot == snapshot
        }) {
            return None;
        }
        self.entries.insert(
            session_id,
            RetainedAgentIntel {
                session_incarnation_id,
                snapshot,
            },
        );
        self.advance_revision();
        Some(self.current(None))
    }

    pub(crate) fn clear(
        &mut self,
        session_id: SessionId,
        session_incarnation_id: Uuid,
    ) -> Option<AgentIntelEvent> {
        let current = self.entries.get(&session_id)?;
        if current.session_incarnation_id != session_incarnation_id {
            return None;
        }
        self.entries.remove(&session_id);
        self.advance_revision();
        Some(self.current(None))
    }

    pub(crate) fn current(&self, request_id: Option<String>) -> AgentIntelEvent {
        AgentIntelEvent::LiveSet {
            request_id,
            authority_incarnation_id: self.authority_incarnation_id.to_string(),
            revision: self.revision,
            entries: self
                .entries
                .iter()
                .map(|(session_id, retained)| LiveAgentIntelEntry {
                    session_id: session_id.to_string(),
                    session_incarnation_id: retained.session_incarnation_id.to_string(),
                    snapshot: retained.snapshot.clone().into(),
                })
                .collect(),
        }
    }

    fn advance_revision(&mut self) {
        if self.revision == u64::MAX {
            self.authority_incarnation_id = Uuid::now_v7();
            self.revision = 0;
        }
        self.revision += 1;
    }

    #[cfg(test)]
    fn with_incarnation(authority_incarnation_id: Uuid) -> Self {
        Self {
            authority_incarnation_id,
            revision: 0,
            entries: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(title: &str) -> AgentIntelSnapshot {
        let mut snapshot = AgentIntelSnapshot::default();
        snapshot.identity.agent_type = "claude".to_owned();
        snapshot.identity.title = Some(title.to_owned());
        snapshot
    }

    #[test]
    fn full_set_is_deterministic_and_no_ops_do_not_advance() {
        let authority_id = Uuid::now_v7();
        let mut authority = LiveAgentIntelAuthority::with_incarnation(authority_id);
        let second = SessionId::new();
        let first = SessionId::new();
        let first_incarnation = Uuid::now_v7();
        let second_incarnation = Uuid::now_v7();

        assert!(
            authority
                .upsert(second, second_incarnation, snapshot("second"))
                .is_some()
        );
        assert!(
            authority
                .upsert(first, first_incarnation, snapshot("first"))
                .is_some()
        );
        assert!(
            authority
                .upsert(first, first_incarnation, snapshot("first"))
                .is_none()
        );

        let AgentIntelEvent::LiveSet {
            authority_incarnation_id,
            revision,
            entries,
            ..
        } = authority.current(Some("request".to_owned()))
        else {
            panic!("expected live set");
        };
        assert_eq!(authority_incarnation_id, authority_id.to_string());
        assert_eq!(revision, 2);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].session_id < entries[1].session_id);
    }

    #[test]
    fn stale_clear_cannot_remove_replacement_incarnation() {
        let mut authority = LiveAgentIntelAuthority::default();
        let session_id = SessionId::new();
        let retired = Uuid::now_v7();
        let current = Uuid::now_v7();
        authority.upsert(session_id, retired, snapshot("retired"));
        authority.upsert(session_id, current, snapshot("current"));

        assert!(authority.clear(session_id, retired).is_none());
        let AgentIntelEvent::LiveSet {
            revision, entries, ..
        } = authority.current(None)
        else {
            panic!("expected live set");
        };
        assert_eq!(revision, 2);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_incarnation_id, current.to_string());
    }

    #[test]
    fn v34_wire_serializes_every_nullable_field_explicitly() {
        let authority = LiveAgentIntelAuthority::default();
        let value = serde_json::to_value(authority.current(Some("request".to_owned())))
            .expect("serialize empty set");
        assert_eq!(value.get("requestId"), Some(&serde_json::json!("request")));

        let mut authority = LiveAgentIntelAuthority::default();
        let mut snapshot = AgentIntelSnapshot::default();
        snapshot.source.detail = None;
        authority.upsert(SessionId::new(), Uuid::now_v7(), snapshot);
        let value = serde_json::to_value(authority.current(None)).expect("serialize live set");
        assert!(
            value
                .get("requestId")
                .is_some_and(serde_json::Value::is_null)
        );
        let snapshot = &value["entries"][0]["snapshot"];
        for field in [
            "attention",
            "pendingInteraction",
            "currentActivity",
            "outcome",
            "exceptionalState",
        ] {
            assert!(
                snapshot.get(field).is_some_and(serde_json::Value::is_null),
                "{field} must be explicit null"
            );
        }
        for field in [
            "version",
            "model",
            "title",
            "cwd",
            "vendorSessionId",
            "processId",
        ] {
            assert!(
                snapshot["identity"]
                    .get(field)
                    .is_some_and(serde_json::Value::is_null),
                "identity.{field} must be explicit null"
            );
        }
        assert!(
            snapshot["source"]
                .get("detail")
                .is_some_and(serde_json::Value::is_null)
        );
    }
}
