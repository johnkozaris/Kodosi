//! What a click on a link does, for every app.
//!
//! The text of a link can come from anyone, so it opens by Ghostty's rules: web and mail links open
//! in the browser of the device, a file or folder of this device opens, a link to another app asks
//! first, and a malformed or disguised link does not open. A file or a local address of a terminal
//! on another computer is on that computer, so it does not open here. Only whether opening a file
//! runs a program stays with the app, because its system decides that.

use std::path::{Component, Path, PathBuf};

use percent_encoding::percent_decode_str;
use serde::Serialize;
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory as _};
use url::Url;

const MOST_CHARACTERS: usize = 8_192;

/// Schemes whose link runs code in the app that opens it, or opens a part of the device itself.
const PROGRAM_SCHEMES: [&str; 7] = [
    "about",
    "blob",
    "content",
    "data",
    "intent",
    "javascript",
    "vbscript",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkDecision {
    pub action: LinkAction,
    /// The address that opens or that the app asks about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The target as a person reads it, on one line, with hidden characters written out.
    pub shown: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<LinkRefusal>,
    /// The computer that has the file or the address.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub place: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkAction {
    Open,
    Ask,
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkRefusal {
    Malformed,
    HiddenCharacters,
    MissingFile,
    Program,
    FileElsewhere,
    AddressElsewhere,
}

/// Decides a link that a terminal shows. `other_computer` names the computer of a terminal that is
/// not on this device.
pub fn decide(value: &str, other_computer: Option<&str>) -> LinkDecision {
    let other = other_computer.filter(|name| !name.is_empty());
    if value.is_empty() || value.chars().nth(MOST_CHARACTERS).is_some() {
        return refuse(value, LinkRefusal::Malformed, None);
    }
    if has_hidden_characters(value) {
        return refuse(value, LinkRefusal::HiddenCharacters, None);
    }
    let target = match other {
        None => local_target(value),
        Some(_) => value.to_owned(),
    };
    let Ok(url) = Url::parse(&target) else {
        return other.map_or_else(
            || refuse(value, LinkRefusal::Malformed, None),
            |computer| refuse(value, LinkRefusal::FileElsewhere, Some(computer)),
        );
    };
    match url.scheme() {
        "http" | "https" => {
            if !target
                .get(url.scheme().len() + 1..)
                .is_some_and(|rest| rest.starts_with("//"))
            {
                return refuse(&target, LinkRefusal::Malformed, None);
            }
            if let Some(computer) = other
                && url.host_str().is_some_and(is_loopback)
            {
                return refuse(&target, LinkRefusal::AddressElsewhere, Some(computer));
            }
            // A name before @ can make the address look like another server.
            let disguised = !url.username().is_empty() || url.password().is_some();
            decided(
                if disguised {
                    LinkAction::Ask
                } else {
                    LinkAction::Open
                },
                &url,
                &target,
            )
        }
        "mailto" if url.path().is_empty() => refuse(&target, LinkRefusal::Malformed, None),
        "mailto" => decided(LinkAction::Open, &url, &target),
        "file" => file_decision(&url, &target, other),
        scheme if PROGRAM_SCHEMES.contains(&scheme) => refuse(&target, LinkRefusal::Program, None),
        _ => decided(LinkAction::Ask, &url, &target),
    }
}

fn file_decision(url: &Url, target: &str, other: Option<&str>) -> LinkDecision {
    let path = || {
        percent_decode_str(url.path())
            .decode_utf8_lossy()
            .into_owned()
    };
    if url.query().is_some() || url.fragment().is_some() {
        return refuse(target, LinkRefusal::Malformed, None);
    }
    if let Some(computer) = other {
        return refuse(&path(), LinkRefusal::FileElsewhere, Some(computer));
    }
    let mut local = url.clone();
    if let Some(host) = url.host_str().filter(|host| !host.is_empty()) {
        if !is_this_computer(host) {
            return refuse(&path(), LinkRefusal::FileElsewhere, Some(host));
        }
        if local.set_host(None).is_err() {
            return refuse(target, LinkRefusal::Malformed, None);
        }
    }
    let Ok(named) = local.to_file_path() else {
        return refuse(target, LinkRefusal::Malformed, None);
    };
    let Some(file) = kodosi_os::fs::canonicalize(&named)
        .ok()
        .filter(|file| file.is_dir() || file.is_file())
    else {
        return refuse(&named.to_string_lossy(), LinkRefusal::MissingFile, None);
    };
    let shown = file.to_string_lossy().into_owned();
    match Url::from_file_path(&file) {
        Ok(opened) => LinkDecision {
            action: LinkAction::Open,
            target: Some(opened.into()),
            shown: written_out(&shown),
            refusal: None,
            place: None,
        },
        Err(()) => refuse(&shown, LinkRefusal::Malformed, None),
    }
}

/// Ghostty also finds paths in the text. On this device a path opens as a file.
fn local_target(value: &str) -> String {
    let path = if value == "~" || value.starts_with("~/") {
        kodosi_os::fs::home_dir()
            .map(|home| home.join(value.trim_start_matches('~').trim_start_matches('/')))
    } else if Url::parse(value).is_err() || is_drive_path(value) {
        Ok(PathBuf::from(value))
    } else {
        return value.to_owned();
    };
    path.ok()
        .filter(|path| path.is_absolute())
        .and_then(|path| Url::from_file_path(normal(&path)).ok())
        .map_or_else(|| value.to_owned(), Into::into)
}

fn is_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    cfg!(windows)
        && bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/')
}

fn normal(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            part => normal.push(part),
        }
    }
    normal
}

/// Programs such as `ls --hyperlink` name the computer in a file link.
fn is_this_computer(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || kodosi_os::process::host_name().is_some_and(|name| name.eq_ignore_ascii_case(host))
}

/// An address on the loopback interface names the computer that serves it.
fn is_loopback(host: &str) -> bool {
    let name = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    if ["localhost", "0.0.0.0", "::", "::1"].contains(&name.as_str())
        || name.ends_with(".localhost")
    {
        return true;
    }
    let parts = name.split('.').collect::<Vec<_>>();
    parts.len() == 4 && parts[0] == "127" && parts.iter().all(|part| part.parse::<u8>().is_ok())
}

fn has_hidden_characters(value: &str) -> bool {
    value.chars().any(is_hidden)
        || percent_decode_str(value)
            .decode_utf8_lossy()
            .chars()
            .any(is_hidden)
}

/// Controls, line breaks, direction marks and invisible marks can make a link look like another
/// one.
fn is_hidden(letter: char) -> bool {
    matches!(
        letter.general_category(),
        GeneralCategory::Control
            | GeneralCategory::Format
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator
    )
}

fn written_out(value: &str) -> String {
    value
        .chars()
        .take(MOST_CHARACTERS)
        .map(|letter| {
            if is_hidden(letter) {
                format!("\\u{{{:X}}}", u32::from(letter))
            } else {
                letter.to_string()
            }
        })
        .collect()
}

fn decided(action: LinkAction, url: &Url, target: &str) -> LinkDecision {
    LinkDecision {
        action,
        target: Some(url.as_str().to_owned()),
        shown: written_out(target),
        refusal: None,
        place: None,
    }
}

fn refuse(shown: &str, refusal: LinkRefusal, place: Option<&str>) -> LinkDecision {
    LinkDecision {
        action: LinkAction::Refuse,
        target: None,
        shown: written_out(shown),
        refusal: Some(refusal),
        place: place.map(str::to_owned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(value: &str, other: Option<&str>) -> Option<(LinkRefusal, Option<String>)> {
        let decision = decide(value, other);
        decision.refusal.map(|refusal| (refusal, decision.place))
    }

    #[test]
    fn a_link_that_can_look_like_another_one_does_not_open() {
        for value in [
            "https://example.com/\u{202E}gpj.exe",
            "https://exa\u{200B}mple.com",
            "https://example.com/%E2%80%AEgpj.exe",
            "https://example.com/\u{E0041}",
            "https://example.com/a\nb",
        ] {
            assert_eq!(
                refusal(value, None),
                Some((LinkRefusal::HiddenCharacters, None)),
                "{value:?}"
            );
        }
        assert!(
            decide("https://example.com/\u{202E}x", None)
                .shown
                .contains("\\u{202E}")
        );
    }

    #[test]
    fn a_local_address_or_a_file_of_another_computer_names_that_computer() {
        let studio = Some("studio");
        for value in [
            "http://localhost:3000",
            "http://127.0.0.1:8080/x",
            "http://[::1]:5173",
            "http://app.localhost",
        ] {
            assert_eq!(
                refusal(value, studio),
                Some((LinkRefusal::AddressElsewhere, Some("studio".into()))),
                "{value}"
            );
        }
        for value in [
            "file:///home/ana/notes.md",
            "/home/ana/notes.md",
            "src/main.rs",
        ] {
            assert_eq!(
                refusal(value, studio),
                Some((LinkRefusal::FileElsewhere, Some("studio".into()))),
                "{value}"
            );
        }
        assert_eq!(
            decide("http://studio.local:3000", studio).action,
            LinkAction::Open
        );
        assert_eq!(
            decide("http://127.example.com", studio).action,
            LinkAction::Open
        );
        assert_eq!(
            decide("http://localhost:3000", None).action,
            LinkAction::Open
        );
    }

    #[test]
    fn a_file_link_that_names_another_computer_does_not_open_here() {
        assert_eq!(
            refusal("file://elsewhere-host/etc/hosts", None),
            Some((LinkRefusal::FileElsewhere, Some("elsewhere-host".into())))
        );
    }

    #[test]
    fn a_file_of_this_device_opens_by_its_resolved_path() {
        let folder = std::env::temp_dir().join(format!("kodosi-links-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("notes.md");
        std::fs::write(&file, "notes").unwrap();
        let resolved = kodosi_os::fs::canonicalize(&file).unwrap();
        let spelled = format!("{}/./notes.md", folder.display());
        let decision = decide(&spelled, None);
        assert_eq!(decision.action, LinkAction::Open);
        assert_eq!(
            decision.target,
            Some(Url::from_file_path(&resolved).unwrap().into())
        );
        assert_eq!(decision.shown, resolved.to_string_lossy());
        assert_eq!(
            refusal(&format!("{}/gone.md", folder.display()), None).map(|r| r.0),
            Some(LinkRefusal::MissingFile)
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_link_to_another_app_asks_and_one_that_runs_code_does_not_open() {
        assert_eq!(decide("vscode://file/a.rs", None).action, LinkAction::Ask);
        assert_eq!(
            decide("https://bank.example@evil.example/login", None).action,
            LinkAction::Ask
        );
        for value in [
            "javascript:alert(1)",
            "data:text/html,<script>x</script>",
            "intent://x#Intent;end",
        ] {
            assert_eq!(
                refusal(value, None).map(|r| r.0),
                Some(LinkRefusal::Program),
                "{value}"
            );
        }
        for value in [
            "https://",
            "https:relative",
            "https:\\\\example.com",
            "https://exa mple.com/",
            "src/main.rs",
        ] {
            assert_eq!(
                refusal(value, None).map(|r| r.0),
                Some(LinkRefusal::Malformed),
                "{value}"
            );
        }
        assert_eq!(
            refusal("mailto:", None).map(|r| r.0),
            Some(LinkRefusal::Malformed)
        );
        assert_eq!(
            decide("mailto:ana@example.com", None).action,
            LinkAction::Open
        );
    }
}
