const SPINNER: std::ops::RangeInclusive<char> = '\u{2800}'..='\u{28ff}';
const MARKS: [char; 8] = [
    '\u{2733}', '\u{2722}', '\u{2736}', '\u{273b}', '\u{273d}', '\u{b7}', '\u{25cf}', '\u{25cb}',
];
const MAX_WORDS_CHARS: usize = 512;

pub(crate) fn title_spins(title: &str) -> bool {
    title
        .chars()
        .next()
        .is_some_and(|first| SPINNER.contains(&first))
}

#[cfg(feature = "host")]
pub(crate) fn titles_look_same(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            title_spins(left) == title_spins(right) && words(left) == words(right)
        }
        (left, right) => left == right,
    }
}

pub(crate) fn title_words(title: &str) -> Option<String> {
    super::status::display_text(words(title), MAX_WORDS_CHARS)
}

fn words(title: &str) -> &str {
    title.trim_start_matches(|mark: char| {
        SPINNER.contains(&mark) || MARKS.contains(&mark) || mark.is_whitespace()
    })
}

#[cfg(test)]
#[path = "title_tests.rs"]
mod tests;
