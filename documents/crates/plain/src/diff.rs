//! Which lines two texts share: the shortest way from one to the other, as Myers found it.

use std::{collections::HashMap, hash::Hash, ops::Range};

/// The most edits that a comparison looks for. Past it, two texts share only their first and
/// last lines, and the rest counts as one change.
const MOST_EDITS: usize = 1000;

/// The pairs of equal lines that a shortest way from `old` to `new` keeps, in order.
pub(crate) fn kept<T: Eq + Hash>(old: &[T], new: &[T]) -> Vec<(usize, usize)> {
    let head = old
        .iter()
        .zip(new)
        .take_while(|(one, other)| one == other)
        .count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(one, other)| one == other)
        .count();
    let (old_end, new_end) = (old.len() - tail, new.len() - tail);
    let mut ids: HashMap<&T, u32> = HashMap::new();
    let mut id = |line| {
        let next = u32::try_from(ids.len()).unwrap_or(u32::MAX);
        *ids.entry(line).or_insert(next)
    };
    let one: Vec<u32> = old[head..old_end].iter().map(&mut id).collect();
    let other: Vec<u32> = new[head..new_end].iter().map(&mut id).collect();
    let mut pairs: Vec<(usize, usize)> = (0..head).map(|line| (line, line)).collect();
    if let Some(middle) = shortest(&one, &other) {
        pairs.extend(middle.into_iter().map(|(at, to)| (head + at, head + to)));
    }
    pairs.extend((0..tail).map(|line| (old_end + line, new_end + line)));
    pairs
}

/// Each stretch where `new` differs from `old`: the lines of `old`, and the lines of `new` that
/// stand in their place.
pub(crate) fn changed<T: Eq + Hash>(old: &[T], new: &[T]) -> Vec<(Range<usize>, Range<usize>)> {
    let mut stretches = Vec::new();
    let (mut from, mut to) = (0, 0);
    for (at, other) in kept(old, new).into_iter().chain([(old.len(), new.len())]) {
        if at > from || other > to {
            stretches.push((from..at, to..other));
        }
        (from, to) = (at + 1, other + 1);
    }
    stretches
}

/// The equal pairs of a shortest edit, or nothing when it needs more than [`MOST_EDITS`]. A
/// diagonal `k` is `x - y`, kept at `k + offset` so that each index stays positive.
fn shortest(old: &[u32], new: &[u32]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (old.len(), new.len());
    let limit = MOST_EDITS.min(n + m);
    let offset = limit + 1;
    let mut furthest = vec![0; 2 * limit + 3];
    let mut trace: Vec<Vec<usize>> = Vec::new();
    let mut last = None;
    'search: for d in 0..=limit {
        trace.push(furthest[offset - d - 1..=offset + d + 1].to_vec());
        for diagonal in (offset - d..=offset + d).step_by(2) {
            let down = diagonal == offset - d
                || (diagonal != offset + d && furthest[diagonal - 1] < furthest[diagonal + 1]);
            let mut x = if down {
                furthest[diagonal + 1]
            } else {
                furthest[diagonal - 1] + 1
            };
            let mut y = x + offset - diagonal;
            while let (Some(one), Some(other)) = (old.get(x), new.get(y))
                && one == other
            {
                x += 1;
                y += 1;
            }
            furthest[diagonal] = x;
            if x >= n && y >= m {
                last = Some(d);
                break 'search;
            }
        }
    }
    let last = last?;
    let mut pairs = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..=last).rev() {
        let before = &trace[d];
        let at = |diagonal: usize| before[diagonal + d + 1 - offset];
        let diagonal = x + offset - y;
        let down = diagonal == offset - d
            || (diagonal != offset + d && at(diagonal - 1) < at(diagonal + 1));
        let previous = if down { diagonal + 1 } else { diagonal - 1 };
        let previous_x = at(previous);
        let previous_y = (previous_x + offset).saturating_sub(previous);
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            pairs.push((x, y));
        }
        if d > 0 {
            (x, y) = (previous_x, previous_y);
        }
    }
    pairs.reverse();
    Some(pairs)
}
