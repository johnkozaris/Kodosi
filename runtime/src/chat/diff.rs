use super::{ChangeLine, ChangeSign, MOST_LINES};

/// Above this number of line pairs, the lines of the old text all count as removed.
const MOST_PAIRS: usize = 250_000;
const MOST_LINE: usize = 400;

pub(super) struct Change {
    pub(super) added: u32,
    pub(super) removed: u32,
    pub(super) lines: Vec<ChangeLine>,
}

pub(super) fn between(old: &str, new: &str) -> Change {
    let before = old.lines().collect::<Vec<_>>();
    let after = new.lines().collect::<Vec<_>>();
    let mut change = Change {
        added: 0,
        removed: 0,
        lines: Vec::new(),
    };
    if before.len().saturating_mul(after.len()) > MOST_PAIRS {
        for line in before {
            change.push(ChangeSign::Removed, line);
        }
        for line in after {
            change.push(ChangeSign::Added, line);
        }
        return change;
    }
    let width = after.len() + 1;
    let mut common = vec![0_u32; (before.len() + 1) * width];
    for row in (0..before.len()).rev() {
        for column in (0..after.len()).rev() {
            common[row * width + column] = if before[row] == after[column] {
                common[(row + 1) * width + column + 1] + 1
            } else {
                common[(row + 1) * width + column].max(common[row * width + column + 1])
            };
        }
    }
    let (mut row, mut column) = (0, 0);
    while row < before.len() || column < after.len() {
        if row < before.len() && column < after.len() && before[row] == after[column] {
            change.push(ChangeSign::Same, before[row]);
            row += 1;
            column += 1;
        } else if column == after.len()
            || (row < before.len()
                && common[(row + 1) * width + column] >= common[row * width + column + 1])
        {
            change.push(ChangeSign::Removed, before[row]);
            row += 1;
        } else {
            change.push(ChangeSign::Added, after[column]);
            column += 1;
        }
    }
    change
}

pub(super) fn written(text: &str) -> Change {
    between("", text)
}

impl Change {
    fn push(&mut self, sign: ChangeSign, line: &str) {
        match sign {
            ChangeSign::Added => self.added = self.added.saturating_add(1),
            ChangeSign::Removed => self.removed = self.removed.saturating_add(1),
            ChangeSign::Same => {}
        }
        if self.lines.len() < MOST_LINES {
            self.lines.push(ChangeLine {
                sign,
                text: line.chars().take(MOST_LINE).collect(),
            });
        }
    }

    pub(super) fn then(mut self, next: Self) -> Self {
        self.added = self.added.saturating_add(next.added);
        self.removed = self.removed.saturating_add(next.removed);
        let room = MOST_LINES.saturating_sub(self.lines.len());
        self.lines.extend(next.lines.into_iter().take(room));
        self
    }
}
