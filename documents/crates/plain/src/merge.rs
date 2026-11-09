//! A write of another program, merged three ways as Git merges: what the writer changed since the
//! saved state that it read goes into the text as it is now. Lines that a person changed and the
//! writer did not stay as the person left them. Where both changed the same lines, the writer's
//! lines win.

use std::ops::Range;

use crate::{
    Change, Model, diff,
    lines::{Edit, Text},
};

/// Lines of one text that became lines of another.
type Hunk = (Range<usize>, Range<usize>);

/// What another program wrote: the saved state that it read, its new text, and where they
/// differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    base: Text,
    write: Text,
    hunks: Vec<Hunk>,
}

impl Written {
    /// The lines that the writer removed and added.
    pub(crate) fn distance(&self) -> usize {
        self.hunks
            .iter()
            .map(|(old, new)| old.len() + new.len())
            .sum()
    }
}

pub(crate) fn compare(from: &Model, to: &Model) -> Vec<Change> {
    let base = Text::new(&from.doc);
    let write = Text::new(&to.doc);
    let hunks = diff::changed(&base.lines(), &write.lines());
    if hunks.is_empty() {
        return Vec::new();
    }
    vec![Change::Written(Written { base, write, hunks })]
}

/// The edits that merge a write into the text as it is now, in order.
pub(crate) fn edits(now: &Text, written: &Written) -> Vec<Edit> {
    let base = written.base.lines();
    let write = written.write.lines();
    let current = now.lines();
    let person = one_to_one(diff::changed(&base, &current));
    let hunks = &written.hunks;
    let mut edits = Vec::new();
    let (mut person_base, mut person_now, mut next_person) = (0, 0, 0);
    let mut next = 0;
    while let Some((first, _)) = hunks.get(next) {
        let mut stretch = first.clone();
        while let Some((old, new)) = person.get(next_person)
            && old.end <= stretch.start
        {
            person_base += old.len();
            person_now += new.len();
            next_person += 1;
        }
        let inserted_before = person[..next_person]
            .last()
            .filter(|(old, _)| old.is_empty() && old.start == stretch.start)
            .map(|(_, new)| &current[new.clone()]);
        let first_person = next_person;
        let mut last = next;
        loop {
            let mut grew = false;
            while let Some((old, _)) = person.get(next_person)
                && overlaps(old, &stretch)
            {
                stretch = stretch.start.min(old.start)..stretch.end.max(old.end);
                next_person += 1;
                grew = true;
            }
            while let Some((old, _)) = hunks.get(last + 1)
                && overlaps(old, &stretch)
            {
                stretch = stretch.start.min(old.start)..stretch.end.max(old.end);
                last += 1;
                grew = true;
            }
            if !grew {
                break;
            }
        }
        let start = stretch.start - person_base + person_now;
        for (old, new) in &person[first_person..next_person] {
            person_base += old.len();
            person_now += new.len();
        }
        let end = stretch.end - person_base + person_now;
        let mut lines: Vec<&str> = Vec::new();
        let mut at = stretch.start;
        for (old, new) in &hunks[next..=last] {
            lines.extend(&base[at..old.start]);
            lines.extend(&write[new.clone()]);
            at = old.end;
        }
        lines.extend(&base[at..stretch.end]);
        let there = current[start..end] == lines[..]
            || (start == end && inserted_before.is_some_and(|inserted| inserted == lines));
        if !there {
            edits.extend(now.lines_edit(start..end, &lines));
        }
        next = last + 1;
    }
    edits
}

/// Splits stretches that a person changed line for line, so a writer's change next to one of
/// those lines leaves the others as the person made them.
fn one_to_one(hunks: Vec<Hunk>) -> Vec<Hunk> {
    hunks
        .into_iter()
        .flat_map(|(old, new)| {
            if old.len() == new.len() {
                old.zip(new)
                    .map(|(line, now)| (line..line + 1, now..now + 1))
                    .collect()
            } else {
                vec![(old, new)]
            }
        })
        .collect()
}

/// Whether two changes touch the same lines. Lines added at the edge of a change, or two
/// insertions at one place, do not.
fn overlaps(one: &Range<usize>, other: &Range<usize>) -> bool {
    match (one.is_empty(), other.is_empty()) {
        (true, true) => false,
        (true, false) => (other.start + 1..other.end).contains(&one.start),
        (false, true) => (one.start + 1..one.end).contains(&other.start),
        (false, false) => one.start < other.end && other.start < one.end,
    }
}
