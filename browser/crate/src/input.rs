use serde_json::{Value, json};

use crate::message::{KeyAction, MouseAction, MouseButton};

const META: u8 = 4;
const CONTROL: u8 = 2;
const SHIFT: u8 = 8;
const SEARCH: &str = "https://www.google.com/search?q=";

/// The address of what a person typed. An address with no scheme gets `https`, and one on this
/// computer or the local network, such as an agent's server at `localhost:3000`, gets `http`.
/// Words that are no address, such as a question, go to the search engine.
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
            .map(|name| format!("{name}]"))
            .unwrap_or_default();
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

/// On a computer that is not a Mac, a Mac's Command is Control.
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

/// A Mac takes these keys from its menus, which a hidden browser never shows, so the key event
/// names its command.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_address_gets_http_on_this_computer_and_https_elsewhere() {
        assert_eq!(address("example.com/a?b"), "https://example.com/a?b");
        assert_eq!(address(" localhost:3000 "), "http://localhost:3000");
        assert_eq!(
            address("127.0.0.1:47390/x.html"),
            "http://127.0.0.1:47390/x.html"
        );
        assert_eq!(address("app.localhost"), "http://app.localhost");
        assert_eq!(address("studio:8080"), "http://studio:8080");
        assert_eq!(address("[::1]:8000/"), "http://[::1]:8000/");
        assert_eq!(address("printer.local"), "http://printer.local");
        assert_eq!(address("http://example.com"), "http://example.com");
        assert_eq!(address("about:blank"), "about:blank");
    }

    #[test]
    fn words_that_are_no_address_are_a_search() {
        assert_eq!(
            address("how to cook pasta?"),
            "https://www.google.com/search?q=how+to+cook+pasta%3F"
        );
        assert_eq!(address("pasta"), "https://www.google.com/search?q=pasta");
        assert_eq!(address("1+1"), "https://www.google.com/search?q=1%2B1");
        assert_eq!(
            address("καλημέρα"),
            "https://www.google.com/search?q=%CE%BA%CE%B1%CE%BB%CE%B7%CE%BC%CE%AD%CF%81%CE%B1"
        );
        assert_eq!(
            address("me@example.com"),
            "https://www.google.com/search?q=me%40example.com"
        );
        assert_eq!(address("news.example.org"), "https://news.example.org");
        assert_eq!(address("localhost"), "http://localhost");
    }

    fn command_key(key: &str, shift: bool) -> Key<'_> {
        Key {
            action: KeyAction::Down,
            key,
            code: "KeyA",
            text: None,
            key_code: 65,
            modifiers: META | if shift { SHIFT } else { 0 },
        }
    }

    #[test]
    fn a_mac_command_is_control_on_other_computers() {
        let event = key(command_key("a", false), false);
        assert_eq!(event["modifiers"], CONTROL);
        assert!(event.get("commands").is_none());
    }

    #[test]
    fn a_mac_names_its_editing_commands() {
        assert_eq!(
            key(command_key("a", false), true)["commands"],
            json!(["selectAll"])
        );
        assert_eq!(
            key(command_key("z", true), true)["commands"],
            json!(["redo"])
        );
    }

    #[test]
    fn the_meta_key_itself_becomes_control() {
        let event = key(
            Key {
                action: KeyAction::Down,
                key: "Meta",
                code: "MetaRight",
                text: None,
                key_code: 93,
                modifiers: 0,
            },
            false,
        );
        assert_eq!(event["key"], "Control");
        assert_eq!(event["code"], "ControlRight");
    }

    #[test]
    fn enter_types_a_return() {
        let event = key(
            Key {
                action: KeyAction::Down,
                key: "Enter",
                code: "Enter",
                text: None,
                key_code: 13,
                modifiers: 0,
            },
            false,
        );
        assert_eq!(event["text"], "\r");
    }

    #[test]
    fn a_held_button_shows_while_the_mouse_moves() {
        let event = Mouse {
            action: MouseAction::Move,
            x: 1.0,
            y: 2.0,
            button: MouseButton::Left,
            clicks: 0,
            delta_x: 0.0,
            delta_y: 0.0,
            modifiers: 0,
        };
        assert_eq!(mouse(event, true)["buttons"], 1);
        let released = Mouse {
            action: MouseAction::Release,
            ..event
        };
        assert_eq!(mouse(released, true)["buttons"], 0);
    }
}
