use super::{title_spins, titles_look_same};

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
