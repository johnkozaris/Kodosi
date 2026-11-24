use super::title_words;

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
    assert_eq!(
        title_words("[ ! ] Action Required | Fix the cart").as_deref(),
        Some("Action Required | Fix the cart")
    );
    assert_eq!(
        title_words("[main] vim notes.md").as_deref(),
        Some("[main] vim notes.md")
    );
    assert_eq!(title_words("⠂ "), None);
    assert_eq!(
        title_words(&"a".repeat(600)).map(|words| words.len()),
        Some(512)
    );
}
