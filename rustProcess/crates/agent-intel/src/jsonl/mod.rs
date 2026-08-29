use std::borrow::Cow;

pub mod claude_code_metadata;
pub mod copilot_metadata;
pub mod subagents;
pub mod tail;

pub use tail::{JsonlTail, TailRead};

pub(crate) fn scrub_secret_patterns(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut redact_next = false;
    for segment in value.split_inclusive(char::is_whitespace) {
        let token = segment.trim_end_matches(char::is_whitespace);
        let whitespace = &segment[token.len()..];
        if redact_next {
            output.push_str("<redacted>");
            redact_next = false;
        } else {
            push_scrubbed_token(&mut output, token);
            redact_next =
                token.eq_ignore_ascii_case("Bearer") || token.eq_ignore_ascii_case("Basic");
        }
        output.push_str(whitespace);
    }
    output
}

fn push_scrubbed_token(output: &mut String, token: &str) {
    const PREFIXES: &[&str] = &["gho_", "ghp_", "ghs_", "ghu_", "ghr_", "github_pat_", "sk-"];
    if PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() > prefix.len() + 4)
    {
        output.push_str("<redacted>");
        return;
    }

    if let Some(equals) = token.find('=') {
        let name = &token[..equals];
        let upper: Cow<'_, str> = if name.bytes().any(|byte| byte.is_ascii_lowercase()) {
            Cow::Owned(name.to_ascii_uppercase())
        } else {
            Cow::Borrowed(name)
        };
        if ["TOKEN", "KEY", "SECRET", "PASSWORD", "PASSWD"]
            .iter()
            .any(|marker| upper.contains(marker))
        {
            output.push_str(name);
            output.push_str("=<redacted>");
            return;
        }
    }
    output.push_str(token);
}

#[cfg(test)]
mod tests {
    use super::scrub_secret_patterns;

    #[test]
    fn scrubber_removes_tokens_assignments_and_bearer_values() {
        let scrubbed = scrub_secret_patterns(
            "ghp_abcdefghijklmnopqrstuvwxyz api_token=abc123 bearer secret-value safe",
        );
        assert_eq!(
            scrubbed,
            "<redacted> api_token=<redacted> bearer <redacted> safe"
        );
    }
}
