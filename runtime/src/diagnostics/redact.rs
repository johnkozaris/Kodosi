use reqwest::Url;

const REDACTED: &str = "[REDACTED]";
const REDACTED_PEM: &str = "[REDACTED PEM]";
const NAMED_SECRETS: [&[&str]; 11] = [
    &["access", "token"],
    &["refresh", "token"],
    &["api", "key"],
    &["client", "secret"],
    &["signing", "key", "pem"],
    &["key", "material"],
    &["private", "key"],
    &["token"],
    &["secret"],
    &["password"],
    &["pem"],
];

pub(super) fn redact(text: &str, home: &[String]) -> String {
    let mut text = strip_url_secrets(text);
    text = strip_pem_blocks(&text);
    text = strip_authorization(&text);
    text = strip_named_secrets(&text);
    text = strip_jwts(&text);
    for form in home {
        text = text.replace(form.as_str(), "[HOME]");
    }
    strip_long_material(&text)
}

pub(super) fn home_forms(home: Option<&std::path::Path>) -> Vec<String> {
    let Some(home) = home.and_then(std::path::Path::to_str) else {
        return Vec::new();
    };
    let home = home.trim_end_matches(['/', '\\']);
    if home.is_empty() || home.ends_with(':') {
        return Vec::new();
    }
    let slashed = home.replace('\\', "/");
    if slashed == home {
        vec![slashed]
    } else {
        vec![home.to_owned(), slashed]
    }
}

fn strip_url_secrets(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let (mut copied, mut search) = (0, 0);
    while let Some(found) = text[search..].find("://") {
        let marker = search + found;
        let mut start = marker;
        while start > search
            && (bytes[start - 1].is_ascii_alphanumeric() || b"+.-".contains(&bytes[start - 1]))
        {
            start -= 1;
        }
        while start < marker && !bytes[start].is_ascii_alphabetic() {
            start += 1;
        }
        let end = text[marker..]
            .find(|symbol: char| symbol.is_whitespace() || "<>\"'".contains(symbol))
            .map_or(text.len(), |offset| marker + offset);
        let mut trimmed = end;
        while trimmed > marker && b".,);]".contains(&bytes[trimmed - 1]) {
            trimmed -= 1;
        }
        search = end.max(marker + 3);
        if start == marker {
            continue;
        }
        let Ok(mut url) = Url::parse(&text[start..trimmed]) else {
            continue;
        };
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        output.push_str(&text[copied..start]);
        output.push_str(url.as_str());
        copied = trimmed;
    }
    output.push_str(&text[copied..]);
    output
}

fn strip_pem_blocks(text: &str) -> String {
    const BEGIN: &str = "-----begin ";
    let lower = text.to_ascii_lowercase();
    let mut output = String::with_capacity(text.len());
    let (mut copied, mut search) = (0, 0);
    while let Some(found) = lower[search..].find(BEGIN) {
        let begin = search + found;
        let label_start = begin + BEGIN.len();
        let label = lower[label_start..]
            .find("-----")
            .map(|length| &lower[label_start..label_start + length])
            .filter(|label| {
                (1..=64).contains(&label.len())
                    && label.as_bytes()[0].is_ascii_alphanumeric()
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b' ' || byte == b'-')
            });
        let Some(label) = label else {
            search = begin + 1;
            continue;
        };
        let body = label_start + label.len() + 5;
        let close = format!("-----end {label}-----");
        output.push_str(&text[copied..begin]);
        output.push_str(REDACTED_PEM);
        let Some(end) = lower[body..].find(&close) else {
            return output;
        };
        copied = body + end + close.len();
        search = copied;
    }
    output.push_str(&text[copied..]);
    output
}

fn strip_authorization(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut output = String::with_capacity(text.len());
    let (mut copied, mut search) = (0, 0);
    while let Some(found) = lower[search..].find("authorization") {
        let start = search + found;
        search = start + 1;
        if start > 0 && is_word(lower.as_bytes()[start - 1]) {
            continue;
        }
        let mut at = skip_space(&lower, start + "authorization".len());
        if !lower[at..].starts_with([':', '=']) {
            continue;
        }
        at = skip_space(&lower, at + 1);
        let Some(scheme) = ["bearer", "basic"]
            .into_iter()
            .find(|scheme| lower[at..].starts_with(scheme))
        else {
            continue;
        };
        let value = skip_space(&lower, at + scheme.len());
        let end = lower[value..]
            .find(|symbol: char| symbol.is_whitespace() || ",;".contains(symbol))
            .map_or(lower.len(), |offset| value + offset);
        if value == at + scheme.len() || end == value {
            continue;
        }
        output.push_str(&text[copied..value]);
        output.push_str(REDACTED);
        copied = end;
        search = end;
    }
    output.push_str(&text[copied..]);
    output
}

fn strip_named_secrets(text: &str) -> String {
    let mut text = text.to_owned();
    let mut search = 0;
    while let Some(value_start) = next_named_value(&text, search) {
        if value_start >= text.len() {
            break;
        }
        let bytes = text.as_bytes();
        let quote = bytes[value_start];
        let mut value_end = value_start;
        if quote == b'"' || quote == b'\'' {
            value_end += 1;
            while value_end < bytes.len() {
                match bytes[value_end] {
                    b'\\' => value_end = (value_end + 2).min(bytes.len()),
                    byte if byte == quote => {
                        value_end += 1;
                        break;
                    }
                    _ => value_end += 1,
                }
            }
            while !text.is_char_boundary(value_end) {
                value_end += 1;
            }
        } else {
            value_end = text[value_start..]
                .find(|symbol: char| symbol.is_whitespace() || ",;}".contains(symbol))
                .map_or(text.len(), |offset| value_start + offset);
        }
        if value_end == value_start {
            search = value_start + 1;
            continue;
        }
        text.replace_range(value_start..value_end, REDACTED);
        search = value_start + REDACTED.len();
    }
    text
}

fn next_named_value(text: &str, from: usize) -> Option<usize> {
    let lower = text.as_bytes();
    (from..lower.len()).find_map(|start| {
        let key = start + usize::from(matches!(lower[start], b'"' | b'\''));
        NAMED_SECRETS.iter().find_map(|parts| {
            let mut at = key;
            for (index, part) in parts.iter().enumerate() {
                if index > 0 && matches!(lower.get(at), Some(b'_' | b'-')) {
                    at += 1;
                }
                if !lower
                    .get(at..at + part.len())
                    .is_some_and(|word| word.eq_ignore_ascii_case(part.as_bytes()))
                {
                    return None;
                }
                at += part.len();
            }
            if matches!(lower.get(at), Some(b'"' | b'\'')) {
                at += 1;
            }
            at = skip_space(text, at);
            matches!(lower.get(at), Some(b':' | b'=')).then(|| skip_space(text, at + 1))
        })
    })
}

fn strip_jwts(text: &str) -> String {
    let bytes = text.as_bytes();
    let run = |from: usize| {
        from + bytes[from..]
            .iter()
            .take_while(|byte| is_token(**byte))
            .count()
    };
    let mut output = String::with_capacity(text.len());
    let (mut copied, mut at) = (0, 0);
    while at < bytes.len() {
        if !is_token(bytes[at]) {
            at += 1;
            continue;
        }
        let header = run(at);
        let jwt = (bytes[at..].starts_with(b"eyJ")
            && header - at >= 11
            && bytes.get(header) == Some(&b'.'))
        .then(|| {
            let payload = run(header + 1);
            (bytes[header + 1..].starts_with(b"eyJ")
                && payload - header > 11
                && bytes.get(payload) == Some(&b'.'))
            .then(|| run(payload + 1))
            .filter(|signature| signature - payload > 16)
        })
        .flatten();
        if let Some(end) = jwt {
            output.push_str(&text[copied..at]);
            output.push_str(REDACTED);
            copied = end;
            at = end;
        } else {
            at = header;
        }
    }
    output.push_str(&text[copied..]);
    output
}

fn strip_long_material(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let (mut copied, mut at) = (0, 0);
    while at < bytes.len() {
        let length = bytes[at..]
            .iter()
            .take_while(|byte| byte.is_ascii_alphanumeric() || b"_+/=-".contains(byte))
            .count();
        if length >= 48 {
            output.push_str(&text[copied..at]);
            output.push_str(REDACTED);
            copied = at + length;
        }
        at += length.max(1);
    }
    output.push_str(&text[copied..]);
    output
}

const fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

const fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'
}

fn skip_space(text: &str, from: usize) -> usize {
    text.get(from..).map_or(from, |rest| {
        from + rest.len() - rest.trim_start_matches(char::is_whitespace).len()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(text: &str) -> String {
        redact(text, &[])
    }

    #[test]
    fn credentials_keys_and_home_paths_are_removed() {
        let jwt =
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJzZWNyZXQtdXNlciJ9.abcdefghijklmnopqrstuvwxyz012345";
        let pem = "-----BEGIN PRIVATE KEY-----\nshort\nwrapped\n-----END PRIVATE KEY-----";
        let text = format!(
            "Authorization: Bearer bearer-secret authorization=Basic aHVudGVyMg== \
             {{\"privateKey\":\"private-material\"}} {jwt} \"{pem}\" \
             {{\"apiKey\":\"api-material\",\"clientSecret\":\"client-material\",\
             \"signingKeyPem\":\"signing-material\",\"pem\":\"pem-material\",\
             \"keyMaterial\":\"key-material\"}} password: hunter2 privateKey=private-material \
             https://alice:hunter2@example.com/path?token=query#fragment, \
             /home/alice/private 0123456789abcdef0123456789abcdef0123456789abcdef"
        );
        let safe = redact(
            &text,
            &home_forms(Some(std::path::Path::new("/home/alice"))),
        );
        for secret in [
            "bearer-secret",
            "aHVudGVyMg",
            "hunter2",
            "private-material",
            "eyJhbGci",
            "short",
            "wrapped",
            "api-material",
            "client-material",
            "signing-material",
            "pem-material",
            "key-material",
            "alice",
            "query",
            "fragment",
            "0123456789abcdef",
        ] {
            assert!(!safe.contains(secret), "{secret} leaked from {safe}");
        }
        assert!(safe.contains("Authorization: Bearer [REDACTED]"));
        assert!(safe.contains("https://example.com/path, "));
        assert!(safe.contains("[REDACTED PEM]"));
        assert!(safe.contains("[HOME]/private"));
    }

    #[test]
    fn quoted_named_secrets_are_removed_whole() {
        for (index, key) in [
            "password",
            "token",
            "apiKey",
            "clientSecret",
            "privateKey",
            "pem",
            "keyMaterial",
        ]
        .into_iter()
        .enumerate()
        {
            let (input, expected) = if index % 2 == 0 {
                (
                    format!(
                        r#"{{"{key}":"prefix\"quote\\backslash\u005ctiny-suffix","safe":"visible"}}"#
                    ),
                    format!(r#"{{"{key}":[REDACTED],"safe":"visible"}}"#),
                )
            } else {
                (
                    format!(r"{key}='prefix\'quote\\backslash\u005ctiny-suffix' safe=visible"),
                    format!("{key}=[REDACTED] safe=visible"),
                )
            };
            assert_eq!(clean(&input), expected);
        }
        assert_eq!(
            clean(r#"token="prefix\"quote, short leaked suffix"#),
            "token=[REDACTED]"
        );
        assert_eq!(
            clean(r"apiKey='prefix\'quote; short leaked suffix"),
            "apiKey=[REDACTED]"
        );
        assert_eq!(
            clean("tokens are fine safe=value"),
            "tokens are fine safe=value"
        );
    }

    #[test]
    fn an_unterminated_key_is_removed_to_the_end() {
        assert_eq!(
            clean("before -----BEGIN RSA PRIVATE KEY-----\nMIIEow leaked"),
            "before [REDACTED PEM]"
        );
        assert_eq!(
            clean("-----BEGIN CERTIFICATE-----\nabc\n-----END CERTIFICATE----- after"),
            "[REDACTED PEM] after"
        );
    }

    #[test]
    fn ordinary_text_is_kept() {
        for text in [
            "session 0190a0b2-7c1e-7d3a-9f00-123456789abc resized to 120x40",
            "opened https://kodosi.com/rooms/abc",
            "eyJ.not.a.token and a short eyJabc",
            "naïve ünïcode: keeps its 😀 characters",
        ] {
            assert_eq!(clean(text), text);
        }
    }

    #[test]
    fn windows_home_folders_are_removed_in_both_separators() {
        let forms = home_forms(Some(std::path::Path::new(r"C:\Users\Alice")));
        assert_eq!(
            redact(r"C:\Users\Alice\repo and C:/Users/Alice/repo", &forms),
            r"[HOME]\repo and [HOME]/repo"
        );
        assert_eq!(
            home_forms(Some(std::path::Path::new("/"))),
            [] as [String; 0]
        );
        assert_eq!(
            home_forms(Some(std::path::Path::new(r"C:\"))),
            [] as [String; 0]
        );
    }
}
