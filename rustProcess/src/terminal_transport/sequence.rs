#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDataSequenceDecision {
    Stale,

    Exact,

    Gap { expected: u64, actual: u64 },

    Exhausted,
}

pub fn classify_data_frame(
    sequence: u64,
    next_data_sequence: &mut Option<u64>,
) -> TerminalDataSequenceDecision {
    let Some(expected) = next_data_sequence else {
        let Some(next) = sequence.checked_add(1) else {
            return TerminalDataSequenceDecision::Exhausted;
        };
        *next_data_sequence = Some(next);
        return TerminalDataSequenceDecision::Exact;
    };
    if sequence < *expected {
        return TerminalDataSequenceDecision::Stale;
    }
    if sequence > *expected {
        return TerminalDataSequenceDecision::Gap {
            expected: *expected,
            actual: sequence,
        };
    }
    let Some(next) = expected.checked_add(1) else {
        return TerminalDataSequenceDecision::Exhausted;
    };
    *expected = next;
    TerminalDataSequenceDecision::Exact
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;

    #[test]
    fn drops_stale_frames_and_rejects_gaps() {
        let mut gate = Some(3);
        assert_eq!(
            classify_data_frame(1, &mut gate),
            TerminalDataSequenceDecision::Stale
        );
        assert_eq!(
            classify_data_frame(2, &mut gate),
            TerminalDataSequenceDecision::Stale
        );
        assert_eq!(
            classify_data_frame(3, &mut gate),
            TerminalDataSequenceDecision::Exact
        );
        assert_eq!(
            classify_data_frame(5, &mut gate),
            TerminalDataSequenceDecision::Gap {
                expected: 4,
                actual: 5,
            }
        );
        let mut open = None;
        assert_eq!(
            classify_data_frame(9, &mut open),
            TerminalDataSequenceDecision::Exact
        );
        assert_eq!(open, Some(10));
    }

    #[test]
    fn uses_checked_sequence_arithmetic() {
        let mut armed = Some(u64::MAX);
        assert_eq!(
            classify_data_frame(u64::MAX, &mut armed),
            TerminalDataSequenceDecision::Exhausted
        );
        assert_eq!(armed, Some(u64::MAX));

        let mut open = None;
        assert_eq!(
            classify_data_frame(u64::MAX, &mut open),
            TerminalDataSequenceDecision::Exhausted
        );
        assert_eq!(open, None);
    }

    #[test]
    fn data_frames_carry_independent_sequences() {
        let first = super::super::TerminalDataFrame::new(1, Bytes::from_static(b"a"));
        let second = super::super::TerminalDataFrame::new(2, Bytes::from_static(b"b"));
        assert_eq!(second.sequence, first.sequence + 1);
        assert_ne!(first, second);
    }
}
