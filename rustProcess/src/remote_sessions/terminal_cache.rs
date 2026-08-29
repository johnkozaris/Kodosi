use std::collections::HashMap;

use kodosi_domain::{
    ids::SessionId,
    terminal::{TerminalCheckpointV2, TerminalPresentationV2},
};

#[derive(Debug, Clone)]
pub(crate) struct RemoteTerminalState {
    pub(crate) checkpoint: Option<TerminalCheckpointV2>,
    pub(crate) next_sequence: u64,
    pub(crate) presentation: Option<TerminalPresentationV2>,
}

#[derive(Debug, Default)]
pub(crate) struct RemoteTerminalCache {
    states: HashMap<SessionId, RemoteTerminalState>,
}

impl RemoteTerminalCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn install_checkpoint(
        &mut self,
        id: SessionId,
        checkpoint: TerminalCheckpointV2,
        next_sequence: u64,
    ) -> RemoteTerminalState {
        let presentation = self.states.remove(&id).and_then(|state| state.presentation);
        let state = RemoteTerminalState {
            checkpoint: Some(checkpoint),
            next_sequence,
            presentation,
        };
        self.states.insert(id, state.clone());
        state
    }

    pub(crate) fn apply_presentation(
        &mut self,
        id: SessionId,
        presentation: TerminalPresentationV2,
    ) {
        let state = self
            .states
            .entry(id)
            .or_insert_with(|| RemoteTerminalState {
                checkpoint: None,
                next_sequence: 0,
                presentation: None,
            });
        state.presentation = Some(presentation);
    }

    pub(crate) fn cached(&self, id: SessionId) -> Option<RemoteTerminalState> {
        self.states.get(&id).cloned()
    }

    pub(crate) fn forget(&mut self, id: SessionId) {
        self.states.remove(&id);
    }

    pub(crate) fn ids(&self) -> impl Iterator<Item = SessionId> + '_ {
        self.states.keys().copied()
    }

    pub(crate) fn drain_all(&mut self) -> Vec<SessionId> {
        self.states.drain().map(|(id, _)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kodosi_domain::terminal::{TerminalScreen, TerminalSize};

    fn fake_checkpoint(rows: u16, cols: u16) -> TerminalCheckpointV2 {
        TerminalCheckpointV2::new(
            TerminalSize::new(rows, cols).expect("valid size"),
            TerminalScreen::Primary,
            br#"{"schemaVersion":1}"#.to_vec(),
            0,
            0,
            false,
        )
        .expect("checkpoint")
    }

    #[test]
    fn checkpoint_is_required_authority_and_replacement_updates_cut() {
        let mut cache = RemoteTerminalCache::new();
        let id = SessionId::new();
        assert!(cache.cached(id).is_none());

        let state = cache.install_checkpoint(id, fake_checkpoint(40, 100), 7);
        assert_eq!(
            state
                .checkpoint
                .as_ref()
                .map(|checkpoint| (checkpoint.rows(), checkpoint.cols())),
            Some((40, 100))
        );
        assert_eq!(state.next_sequence, 7);

        let state = cache.install_checkpoint(id, fake_checkpoint(24, 80), 11);
        assert_eq!(
            state
                .checkpoint
                .as_ref()
                .map(|checkpoint| (checkpoint.rows(), checkpoint.cols())),
            Some((24, 80))
        );
        assert_eq!(state.next_sequence, 11);
    }

    #[test]
    fn forget_and_drain_remove_decrypted_authority() {
        let mut cache = RemoteTerminalCache::new();
        let first = SessionId::new();
        let second = SessionId::new();
        cache.install_checkpoint(first, fake_checkpoint(24, 80), 1);
        cache.install_checkpoint(second, fake_checkpoint(24, 80), 2);

        cache.forget(first);
        assert!(cache.cached(first).is_none());
        assert_eq!(cache.drain_all(), [second]);
        assert!(cache.drain_all().is_empty());
    }
}
