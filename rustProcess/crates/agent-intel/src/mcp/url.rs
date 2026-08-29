#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHttpUrl {
    pub host: String,
    pub port: Option<u16>,
    pub is_https: bool,
}

#[must_use]
pub fn parse_http_url(value: &str) -> Option<ParsedHttpUrl> {
    let (scheme, rest) = value.split_once("://")?;
    let is_https = if scheme.eq_ignore_ascii_case("https") {
        true
    } else if scheme.eq_ignore_ascii_case("http") {
        false
    } else {
        return None;
    };
    let raw_authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|authority| !authority.is_empty())?;
    if raw_authority.contains('\\')
        || raw_authority.matches('@').count() > 1
        || raw_authority
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return None;
    }
    let authority = raw_authority
        .rsplit_once('@')
        .map_or(raw_authority, |(_, host)| host);
    if authority.is_empty() {
        return None;
    }

    let (host, port) = if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        if host.len() <= 2 {
            return None;
        }
        let suffix = &authority[close + 1..];
        let port = if suffix.is_empty() {
            None
        } else {
            Some(suffix.strip_prefix(':')?.parse::<u16>().ok()?)
        };
        (host.to_owned(), port)
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty()
            || host.contains([':', '[', ']'])
            || !port.chars().all(|character| character.is_ascii_digit())
        {
            return None;
        }
        (host.to_owned(), Some(port.parse::<u16>().ok()?))
    } else {
        if authority.contains([':', '[', ']']) {
            return None;
        }
        (authority.to_owned(), None)
    };

    Some(ParsedHttpUrl {
        host,
        port,
        is_https,
    })
}

#[must_use]
pub fn redact_url_for_display(value: &str) -> Option<String> {
    let parsed = parse_http_url(value)?;
    let scheme = if parsed.is_https { "https" } else { "http" };
    Some(match parsed.port {
        Some(port) => format!("{scheme}://{}:{port}", parsed.host),
        None => format!("{scheme}://{}", parsed.host),
    })
}

#[must_use]
pub fn redact_url_scalar(value: &str) -> Option<String> {
    if !value.contains("://") {
        return None;
    }
    Some(redact_url_for_display(value).unwrap_or_else(|| "<redacted-url>".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_uppercase_scheme_and_removes_userinfo() {
        assert_eq!(
            parse_http_url("HTTPS://user:secret@example.test:8443/path?token=x#tail"),
            Some(ParsedHttpUrl {
                host: "example.test".to_owned(),
                port: Some(8443),
                is_https: true,
            })
        );
        assert_eq!(
            redact_url_for_display("HTTPS://user:secret@example.test:8443/path?token=x#tail"),
            Some("https://example.test:8443".to_owned())
        );
    }

    #[test]
    fn invalid_url_like_scalar_is_conservatively_redacted() {
        assert_eq!(
            redact_url_scalar("HTTPS://user only/secret"),
            Some("<redacted-url>".to_owned())
        );
        assert_eq!(redact_url_scalar("ordinary text"), None);
        assert_eq!(
            redact_url_scalar(r"https://example.test\@evil.test/secret"),
            Some("<redacted-url>".to_owned())
        );
        assert_eq!(
            parse_http_url(r"https://example.test\@evil.test/secret"),
            None
        );
        assert_eq!(
            parse_http_url(r"https://user\secret@example.test/path"),
            None,
            "a backslash anywhere in the raw authority is invalid"
        );
        assert_eq!(
            parse_http_url("https://user@name@example.test/path"),
            None,
            "multiple raw userinfo separators are invalid"
        );
    }
}
