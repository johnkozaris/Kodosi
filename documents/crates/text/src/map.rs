/// The ranges of a step: for each one its start, its old size and its new size.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepMap {
    ranges: Vec<usize>,
}

impl StepMap {
    pub(crate) fn new(ranges: Vec<usize>) -> Self {
        Self { ranges }
    }

    /// With a negative `assoc`, a position at an insertion stays before it.
    pub fn map(&self, pos: usize, assoc: i8) -> usize {
        let mut diff = 0_isize;
        for [start, old_size, new_size] in self.ranges.as_chunks::<3>().0.iter().copied() {
            if start > pos {
                break;
            }
            let end = start + old_size;
            if pos <= end {
                let side = if old_size == 0 {
                    assoc
                } else if pos == start {
                    -1
                } else if pos == end {
                    1
                } else {
                    assoc
                };
                let moved = start + if side < 0 { 0 } else { new_size };
                return moved.saturating_add_signed(diff);
            }
            diff += isize::try_from(new_size).unwrap_or(isize::MAX)
                - isize::try_from(old_size).unwrap_or(isize::MAX);
        }
        pos.saturating_add_signed(diff)
    }
}
