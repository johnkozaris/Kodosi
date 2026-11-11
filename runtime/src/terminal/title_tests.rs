#[cfg(feature = "host")]
use super::titles_look_same;
use super::{title_spins, title_words};

#[cfg(feature = "host")]
#[test]
fn a_spinner_frame_looks_the_same_as_the_frame_before() {
    assert!(titles_look_same(
        Some("⠂ Reading files"),
        Some("⠄ Reading files")
    ));
    assert!(titles_look_same(
        Some("⠋Reading files"),
        Some("⠙ Reading files")
    ));
}

#[cfg(feature = "host")]
#[test]
fn a_status_mark_or_space_before_the_words_is_not_seen() {
    assert!(titles_look_same(
        Some("✳ Reading files"),
        Some("· Reading files")
    ));
    assert!(titles_look_same(
        Some("Reading files"),
        Some("  Reading files")
    ));
}

#[cfg(feature = "host")]
#[test]
fn new_words_or_a_stopped_spinner_look_different() {
    assert!(!titles_look_same(
        Some("⠂ Reading files"),
        Some("⠂ Writing tests")
    ));
    assert!(!titles_look_same(
        Some("⠂ Reading files"),
        Some("✳ Reading files")
    ));
    assert!(!titles_look_same(
        Some("Reading files"),
        Some("reading files")
    ));
    assert!(!titles_look_same(Some("Reading files"), None));
    assert!(titles_look_same(None, None));
}

#[test]
fn only_a_braille_pattern_first_spins() {
    assert!(title_spins("⠂ Reading files"));
    assert!(!title_spins("✳ Reading files"));
    assert!(!title_spins(" ⠂ Reading files"));
    assert!(!title_spins(""));
}

#[test]
fn the_words_of_a_title_have_no_mark_and_nothing_hidden() {
    assert_eq!(
        title_words("⠂ Reading files").as_deref(),
        Some("Reading files")
    );
    assert_eq!(title_words("✳  Done").as_deref(), Some("Done"));
    assert_eq!(
        title_words("safe\u{202e}txt.exe\u{7}").as_deref(),
        Some("safetxt.exe")
    );
    assert_eq!(title_words("⠂ "), None);
    assert_eq!(
        title_words(&"a".repeat(600)).map(|words| words.len()),
        Some(512)
    );
}
