#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReplayCursor {
    pub checkpoint_revision: u64,
    pub presentation_revision: u64,

    pub next_sequence: Option<u64>,
    pub key_generation: u32,
    pub checkpoint_counter: Option<u64>,
    pub raw_counter: Option<u64>,
    pub presentation_counter: Option<u64>,
    pub pending_permissions_counter: Option<u64>,
    pub pending_permissions_generation: u64,
}

impl ReplayCursor {
    pub fn reset_terminal_replay(&mut self) {
        self.checkpoint_revision = 0;
        self.presentation_revision = 0;
        self.next_sequence = None;
        self.reset_terminal_frame_admission();
    }

    pub const fn has_fresh_terminal_replay(&self) -> bool {
        self.checkpoint_revision == 0
            && self.presentation_revision == 0
            && self.next_sequence.is_none()
    }

    pub fn reset_encrypted_frame_admission(&mut self) {
        self.reset_terminal_frame_admission();
        self.pending_permissions_counter = None;
    }

    fn reset_terminal_frame_admission(&mut self) {
        self.checkpoint_counter = None;
        self.raw_counter = None;
        self.presentation_counter = None;
    }

    pub fn observe_key_generation(&mut self, key_generation: u32) -> bool {
        if key_generation <= self.key_generation {
            return false;
        }
        self.key_generation = key_generation;
        self.reset_encrypted_frame_admission();
        true
    }

    pub fn accepts_checkpoint_frame(
        &self,
        revision: u64,
        next_sequence: u64,
        key_generation: u32,
        counter: u64,
    ) -> bool {
        key_generation == self.key_generation
            && revision > self.checkpoint_revision
            && self
                .next_sequence
                .is_none_or(|accepted| next_sequence >= accepted)
            && self
                .checkpoint_counter
                .is_none_or(|accepted| counter > accepted)
    }

    pub fn observe_checkpoint_frame(&mut self, revision: u64, counter: u64, next_sequence: u64) {
        self.checkpoint_revision = revision;
        self.checkpoint_counter = Some(counter);
        self.next_sequence = Some(next_sequence);
    }

    pub fn accepts_raw_batch(
        &self,
        first_sequence: u64,
        next_sequence: u64,
        key_generation: u32,
        counter: u64,
    ) -> bool {
        key_generation == self.key_generation
            && next_sequence > first_sequence
            && self.next_sequence == Some(first_sequence)
            && self.raw_counter.is_none_or(|accepted| counter > accepted)
    }

    pub fn observe_raw_batch(&mut self, counter: u64, next_sequence: u64) {
        self.raw_counter = Some(counter);
        self.next_sequence = Some(next_sequence);
    }

    pub fn accepts_presentation_frame(
        &self,
        revision: u64,
        key_generation: u32,
        counter: u64,
    ) -> bool {
        key_generation == self.key_generation
            && revision > self.presentation_revision
            && self
                .presentation_counter
                .is_none_or(|accepted| counter > accepted)
    }

    pub fn observe_presentation_frame(&mut self, revision: u64, counter: u64) {
        self.presentation_revision = revision;
        self.presentation_counter = Some(counter);
    }

    pub fn observe_pending_permissions_frame(&mut self, counter: u64, generation: u64) {
        self.pending_permissions_counter = Some(counter);
        self.pending_permissions_generation = generation;
    }
}

#[cfg(test)]
mod tests {
    use super::ReplayCursor;

    #[test]
    fn terminal_replay_reset_preserves_key_and_pending_snapshot_generation() {
        let mut cursor = ReplayCursor {
            checkpoint_revision: 7,
            presentation_revision: 9,
            next_sequence: Some(12),
            key_generation: 3,
            checkpoint_counter: Some(4),
            raw_counter: Some(5),
            presentation_counter: Some(6),
            pending_permissions_counter: Some(8),
            pending_permissions_generation: 41,
        };

        cursor.reset_terminal_replay();

        assert!(cursor.has_fresh_terminal_replay());
        assert_eq!(cursor.key_generation, 3);
        assert_eq!(cursor.pending_permissions_generation, 41);
        assert_eq!(cursor.pending_permissions_counter, Some(8));
    }

    #[test]
    fn encrypted_frame_admission_reset_preserves_replay_cursor() {
        let mut cursor = ReplayCursor {
            checkpoint_revision: 7,
            presentation_revision: 9,
            next_sequence: Some(12),
            key_generation: 3,
            checkpoint_counter: Some(4),
            raw_counter: Some(5),
            presentation_counter: Some(6),
            pending_permissions_counter: Some(8),
            pending_permissions_generation: 41,
        };

        let mut expected = cursor;
        expected.checkpoint_counter = None;
        expected.raw_counter = None;
        expected.presentation_counter = None;
        expected.pending_permissions_counter = None;

        cursor.reset_encrypted_frame_admission();

        assert_eq!(cursor, expected);
    }

    #[test]
    fn exact_raw_admission_rejects_gap_duplicate_and_overlap() {
        let mut cursor = ReplayCursor {
            key_generation: 5,
            next_sequence: Some(7),
            ..ReplayCursor::default()
        };
        assert!(cursor.accepts_raw_batch(7, 9, 5, 1));
        cursor.observe_raw_batch(1, 9);
        assert!(!cursor.accepts_raw_batch(7, 9, 5, 2));
        assert!(!cursor.accepts_raw_batch(10, 11, 5, 2));
        assert!(!cursor.accepts_raw_batch(9, 9, 5, 2));
        assert!(cursor.accepts_raw_batch(9, 10, 5, 2));
    }

    #[test]
    fn checkpoint_and_presentation_revisions_advance_independently() {
        let mut cursor = ReplayCursor {
            key_generation: 3,
            ..ReplayCursor::default()
        };

        assert!(cursor.accepts_checkpoint_frame(7, 12, 3, 1));
        cursor.observe_checkpoint_frame(7, 1, 12);
        assert!(cursor.accepts_presentation_frame(7, 3, 1));
        cursor.observe_presentation_frame(7, 1);

        assert_eq!(cursor.checkpoint_revision, 7);
        assert_eq!(cursor.presentation_revision, 7);
        assert_eq!(cursor.next_sequence, Some(12));
    }

    #[test]
    fn checkpoint_resets_exact_raw_boundary() {
        let mut cursor = ReplayCursor {
            key_generation: 2,
            next_sequence: Some(100),
            ..ReplayCursor::default()
        };
        assert!(!cursor.accepts_checkpoint_frame(1, 8, 2, 4));
        assert!(cursor.accepts_checkpoint_frame(1, 100, 2, 4));
        cursor.observe_checkpoint_frame(1, 4, 100);
        assert_eq!(cursor.next_sequence, Some(100));
        assert!(cursor.accepts_raw_batch(100, 101, 2, 4));
    }
}
