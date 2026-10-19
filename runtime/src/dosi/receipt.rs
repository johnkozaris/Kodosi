use serde_json::Value;

use super::{DosiAction, DosiTarget, DosiTargetKind};

const MAX_TEXT: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Receipt {
    pub action: DosiAction,
    pub target: Option<DosiTarget>,
    pub text: Option<String>,
}

pub(super) enum Shown {
    Receipt(Receipt),
    Hidden,
}

pub(super) fn of(name: &str, input: &Value) -> Shown {
    let field = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_owned);
    let receipt = |action, kind, value: Option<String>, text: Option<String>| {
        Shown::Receipt(Receipt {
            action,
            target: value.map(|value| target(kind, &value)),
            text: text.map(|text| short(&text)),
        })
    };
    match name {
        "Bash" | "shell" => field("command").map_or_else(
            || receipt(DosiAction::Run, DosiTargetKind::Command, None, None),
            |command| command_receipt(&command, field("description")),
        ),
        "Read" => receipt(
            DosiAction::Read,
            DosiTargetKind::File,
            field("file_path"),
            None,
        ),
        "Write" => receipt(
            DosiAction::Write,
            DosiTargetKind::File,
            field("file_path"),
            None,
        ),
        "Edit" | "MultiEdit" => receipt(
            DosiAction::Edit,
            DosiTargetKind::File,
            field("file_path"),
            None,
        ),
        "NotebookEdit" => receipt(
            DosiAction::Edit,
            DosiTargetKind::File,
            field("notebook_path"),
            None,
        ),
        "edit" => receipt(
            DosiAction::Edit,
            DosiTargetKind::File,
            input
                .get("changes")
                .and_then(Value::as_array)
                .and_then(|changes| changes.first())
                .and_then(|change| change.get("path"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            None,
        ),
        "Glob" | "Grep" => receipt(
            DosiAction::Look,
            DosiTargetKind::Query,
            field("pattern"),
            None,
        ),
        "WebSearch" | "web_search" => receipt(
            DosiAction::Search,
            DosiTargetKind::Query,
            field("query"),
            None,
        ),
        "WebFetch" => receipt(DosiAction::Fetch, DosiTargetKind::Url, field("url"), None),
        "Task" | "Agent" => receipt(
            DosiAction::Delegate,
            DosiTargetKind::Agent,
            field("subagent_type"),
            field("description"),
        ),
        "TodoWrite" | "plan" => receipt(DosiAction::Plan, DosiTargetKind::Tool, None, None),
        other => receipt(
            DosiAction::Other,
            DosiTargetKind::Tool,
            Some(tool_name(other)),
            None,
        ),
    }
}

fn tool_name(name: &str) -> String {
    match name
        .strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
    {
        Some((server, tool)) => format!("{server}: {tool}"),
        None => name.to_owned(),
    }
}

fn target(kind: DosiTargetKind, value: &str) -> DosiTarget {
    DosiTarget {
        kind,
        value: short(value),
    }
}

fn short(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.len() <= MAX_TEXT && !text.trim().contains('\n') {
        return line.to_owned();
    }
    let mut end = MAX_TEXT.min(line.len());
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &line[..end])
}

pub(super) fn command_receipt(command: &str, description: Option<String>) -> Shown {
    let words = words(command);
    let start = words.iter().position(|word| {
        word.rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| name == "kodosi" || name == "kodosi.exe")
    });
    if let Some(start) = start
        && words[..start].iter().all(|word| is_assignment(word))
        && let Some(shown) = kodosi(&words[start + 1..])
    {
        return shown;
    }
    Shown::Receipt(Receipt {
        action: DosiAction::Run,
        target: Some(target(DosiTargetKind::Command, command.trim())),
        text: description.map(|text| short(&text)),
    })
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

fn kodosi(words: &[String]) -> Option<Shown> {
    let words = words
        .iter()
        .filter(|word| word.as_str() != "--json")
        .map(String::as_str)
        .collect::<Vec<_>>();
    let (group, rest) = words.split_first()?;
    match *group {
        "terminal" => terminal(rest),
        "room" => room(rest),
        "agent" => Some(simple(DosiAction::Look, DosiTargetKind::Agent, None, None)),
        "dosi" if rest.first() == Some(&"ask") => Some(Shown::Hidden),
        _ => None,
    }
}

fn simple(
    action: DosiAction,
    kind: DosiTargetKind,
    value: Option<&str>,
    text: Option<String>,
) -> Shown {
    Shown::Receipt(Receipt {
        action,
        target: Some(DosiTarget {
            kind,
            value: value.map(short).unwrap_or_default(),
        }),
        text: text.map(|text| short(&text)),
    })
}

struct Parsed<'a> {
    positional: Vec<&'a str>,
    options: Vec<(&'a str, Option<&'a str>)>,
}

impl<'a> Parsed<'a> {
    fn new(words: &[&'a str], valued: &[&str]) -> Self {
        let mut positional = Vec::new();
        let mut options = Vec::new();
        let mut index = 0;
        while index < words.len() {
            let word = words[index];
            if let Some(flag) = word.strip_prefix("--") {
                if let Some((name, value)) = flag.split_once('=') {
                    options.push((name, Some(value)));
                } else if valued.contains(&flag) {
                    options.push((flag, words.get(index + 1).copied()));
                    index += 1;
                } else {
                    options.push((flag, None));
                }
            } else {
                positional.push(word);
            }
            index += 1;
        }
        Self {
            positional,
            options,
        }
    }

    fn option(&self, name: &str) -> Option<&'a str> {
        self.options
            .iter()
            .find(|(option, _)| *option == name)
            .and_then(|(_, value)| *value)
    }

    fn at(&self, index: usize) -> Option<&'a str> {
        self.positional.get(index).copied()
    }
}

fn terminal(words: &[&str]) -> Option<Shown> {
    let (verb, rest) = words.split_first()?;
    let valued = [
        "name",
        "directory",
        "room",
        "branch",
        "agent",
        "account",
        "model",
        "effort",
        "prompt",
        "until",
        "timeout",
    ];
    let parsed = Parsed::new(rest, &valued);
    let named = parsed.at(0);
    let one = |action| Some(simple(action, DosiTargetKind::Terminal, named, None));
    match *verb {
        "list" => Some(simple(
            DosiAction::Look,
            DosiTargetKind::Terminals,
            None,
            None,
        )),
        "info" | "read" => one(DosiAction::Read),
        "show" | "attach" => one(DosiAction::Show),
        "wait" => one(DosiAction::Wait),
        "interrupt" => one(DosiAction::Interrupt),
        "close" => one(DosiAction::Close),
        "keys" => Some(simple(
            DosiAction::Press,
            DosiTargetKind::Terminal,
            named,
            Some(parsed.positional.get(1..)?.join(" ")),
        )),
        "input" => Some(simple(
            DosiAction::Type,
            DosiTargetKind::Terminal,
            named,
            parsed.at(1).map(str::to_owned),
        )),
        "rename" => Some(simple(
            DosiAction::Edit,
            DosiTargetKind::Terminal,
            named,
            parsed.at(1).map(str::to_owned),
        )),
        "run" => Some(simple(
            DosiAction::Start,
            DosiTargetKind::Terminal,
            named,
            parsed.option("agent").map(str::to_owned),
        )),
        "start" | "resume" => Some(parsed.option("agent").map_or_else(
            || {
                simple(
                    DosiAction::Start,
                    DosiTargetKind::Terminal,
                    parsed.option("name"),
                    None,
                )
            },
            |agent| {
                simple(
                    DosiAction::Start,
                    DosiTargetKind::Agent,
                    Some(agent),
                    parsed.option("prompt").map(str::to_owned),
                )
            },
        )),
        _ => one(DosiAction::Other),
    }
}

fn room(words: &[&str]) -> Option<Shown> {
    let parsed = Parsed::new(
        words,
        &[
            "room",
            "agent",
            "since",
            "before",
            "after",
            "timeout",
            "description",
            "repo",
            "note",
            "directory",
            "provider",
        ],
    );
    let named = parsed.option("room");
    let (verb, rest) = parsed.positional.split_first()?;
    let one = |action| Some(simple(action, DosiTargetKind::Room, named, None));
    match *verb {
        "list" => Some(simple(DosiAction::Look, DosiTargetKind::Rooms, None, None)),
        "context" | "read" => one(DosiAction::Read),
        "wait" => one(DosiAction::Wait),
        "show" => one(DosiAction::Show),
        "post" => Some(simple(
            DosiAction::Post,
            DosiTargetKind::Room,
            named,
            rest.first().map(|text| (*text).to_owned()),
        )),
        "task" => Some(simple(
            if rest.first() == Some(&"list") {
                DosiAction::Read
            } else {
                DosiAction::Edit
            },
            DosiTargetKind::Room,
            named,
            rest.get(1).map(|text| (*text).to_owned()),
        )),
        _ => one(DosiAction::Other),
    }
}

pub(super) fn words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut characters = command.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\'' => {
                started = true;
                for quoted in characters.by_ref() {
                    if quoted == '\'' {
                        break;
                    }
                    word.push(quoted);
                }
            }
            '"' => {
                started = true;
                while let Some(quoted) = characters.next() {
                    match quoted {
                        '"' => break,
                        '\\' if matches!(characters.peek(), Some('"' | '\\' | '$' | '`')) => {
                            word.extend(characters.next());
                        }
                        other => word.push(other),
                    }
                }
            }
            '\\' => {
                started = true;
                word.extend(characters.next());
            }
            ';' | '&' | '|' | '\n' => break,
            space if space.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            other => {
                started = true;
                word.push(other);
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

pub(super) fn unwrap_shell(command: &str) -> String {
    let words = words(command);
    match words.as_slice() {
        [shell, flag, inner]
            if matches!(flag.as_str(), "-c" | "-lc")
                && shell
                    .rsplit('/')
                    .next()
                    .is_some_and(|name| matches!(name, "sh" | "bash" | "zsh" | "fish")) =>
        {
            inner.clone()
        }
        _ => command.to_owned(),
    }
}
