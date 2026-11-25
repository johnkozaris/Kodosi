use serde_json::{Value, json};

use stream::viewer::{KeyAction, MouseAction, MouseButton};

const META: u8 = 4;
const CONTROL: u8 = 2;
const SHIFT: u8 = 8;
const SEARCH: &str = "https://www.google.com/search?q=";

/// Resolves omnibox input to a URL: local hosts default to `http`, domains to `https`, and
/// non-URL queries to a search URL.
pub(crate) fn address(typed: &str) -> String {
    let typed = typed.trim();
    if typed.contains("://") || typed.starts_with("about:") || typed.starts_with("data:") {
        return typed.to_owned();
    }
    let host = typed.split(['/', '?', '#']).next().unwrap_or_default();
    if typed.contains(char::is_whitespace) || host.is_empty() || host.contains('@') {
        return search(typed);
    }
    let (name, port) = if host.starts_with('[') {
        let name = host
            .split(']')
            .next()
            .map_or_default(|name| format!("{name}]"));
        (name, false)
    } else if let Some((name, port)) = host.rsplit_once(':') {
        if !port.bytes().all(|byte| byte.is_ascii_digit()) {
            return search(typed);
        }
        (name.to_owned(), !port.is_empty())
    } else {
        (host.to_owned(), false)
    };
    let name = name.to_ascii_lowercase();
    let last = name.rsplit('.').next().unwrap_or_default();
    let local = matches!(last, "localhost" | "local")
        || name.starts_with('[')
        || name.parse::<std::net::Ipv4Addr>().is_ok()
        || (!name.contains('.') && port);
    if local {
        return format!("http://{typed}");
    }
    let domain = name.contains('.') && last.len() >= 2 && last.chars().all(char::is_alphabetic);
    if domain {
        format!("https://{typed}")
    } else {
        search(typed)
    }
}

fn search(words: &str) -> String {
    use std::fmt::Write as _;

    let mut address = SEARCH.to_owned();
    for byte in words.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                address.push(char::from(byte));
            }
            b' ' => address.push('+'),
            _ => drop(write!(address, "%{byte:02X}")),
        }
    }
    address
}

/// Maps macOS Command (`META`) to `CONTROL` when the browser host is not macOS.
fn host_modifiers(modifiers: u8, mac: bool) -> u8 {
    if mac || modifiers & META == 0 {
        modifiers
    } else {
        (modifiers & !META) | CONTROL
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Mouse {
    pub action: MouseAction,
    pub x: f64,
    pub y: f64,
    pub button: MouseButton,
    pub clicks: u8,
    pub delta_x: f64,
    pub delta_y: f64,
    pub modifiers: u8,
}

pub(crate) fn mouse(event: Mouse, mac: bool) -> Value {
    let kind = match event.action {
        MouseAction::Press => "mousePressed",
        MouseAction::Release => "mouseReleased",
        MouseAction::Move => "mouseMoved",
        MouseAction::Wheel => "mouseWheel",
    };
    let (button, held) = match event.button {
        MouseButton::None => ("none", 0),
        MouseButton::Left => ("left", 1),
        MouseButton::Right => ("right", 2),
        MouseButton::Middle => ("middle", 4),
    };
    let buttons = if event.action == MouseAction::Release {
        0
    } else {
        held
    };
    json!({
        "type": kind,
        "x": event.x,
        "y": event.y,
        "button": button,
        "buttons": buttons,
        "clickCount": event.clicks,
        "deltaX": event.delta_x,
        "deltaY": event.delta_y,
        "modifiers": host_modifiers(event.modifiers, mac),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_field_names,
    reason = "a key event has a key and a key code"
)]
pub(crate) struct Key<'a> {
    pub action: KeyAction,
    pub key: &'a str,
    pub code: &'a str,
    pub text: Option<&'a str>,
    pub key_code: u32,
    pub modifiers: u8,
}

pub(crate) fn key(event: Key<'_>, mac: bool) -> Value {
    let (key, code) = if !mac && event.key == "Meta" {
        let side = if event.code == "MetaRight" {
            "ControlRight"
        } else {
            "ControlLeft"
        };
        ("Control", side)
    } else {
        (event.key, event.code)
    };
    let down = event.action == KeyAction::Down;
    let text = event
        .text
        .or_else(|| (key == "Enter" && down).then_some("\r"));
    let mut value = json!({
        "type": if down { "keyDown" } else { "keyUp" },
        "key": key,
        "code": code,
        "windowsVirtualKeyCode": event.key_code,
        "modifiers": host_modifiers(event.modifiers, mac),
    });
    if let Some(text) = text {
        value["text"] = json!(text);
    }
    if mac
        && down
        && let Some(command) = editing_command(key, event.modifiers)
    {
        value["commands"] = json!([command]);
    }
    value
}

/// macOS normally dispatches Cmd editing shortcuts through the app menu bar, which a windowless
/// browser lacks, so we pass explicit CDP editing commands instead.
fn editing_command(key: &str, modifiers: u8) -> Option<&'static str> {
    if modifiers & META == 0 {
        return None;
    }
    let shift = modifiers & SHIFT != 0;
    match (key.to_ascii_lowercase().as_str(), shift) {
        ("a", false) => Some("selectAll"),
        ("c", false) => Some("copy"),
        ("x", false) => Some("cut"),
        ("z", false) => Some("undo"),
        ("z", true) => Some("redo"),
        _ => None,
    }
}
