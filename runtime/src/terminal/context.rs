use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
use ghostty_vt::{ContextAction, ContextSignal};

#[cfg(feature = "host")]
const MAX_CONTEXTS: usize = 64;
#[cfg(feature = "host")]
const MAX_ID_BYTES: usize = 64;
const MAX_NAME_CHARS: usize = 255;

/// Where the program of a terminal runs when it is not the terminal's own
/// computer, and whether it runs as root, from the contexts that programs
/// report with OSC 3008.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<ContextPlace>,
    pub root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextPlace {
    pub kind: PlaceKind,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    Remote,
    Container,
    Vm,
}

impl TerminalContext {
    pub fn is_valid(&self) -> bool {
        (self.place.is_some() || self.root)
            && self
                .place
                .as_ref()
                .is_none_or(|place| super::status::is_display_text(&place.name, MAX_NAME_CHARS))
    }
}

/// This computer as contexts name it.
#[cfg(feature = "host")]
#[derive(Debug, Clone, Default)]
pub(super) struct LocalMachine {
    pub hostname: Option<String>,
    pub machine_id: Option<String>,
}

#[cfg(feature = "host")]
impl LocalMachine {
    pub(super) fn get() -> &'static Self {
        static LOCAL: std::sync::OnceLock<LocalMachine> = std::sync::OnceLock::new();
        LOCAL.get_or_init(|| Self {
            hostname: kodosi_os::process::host_name(),
            machine_id: kodosi_os::process::machine_id(),
        })
    }
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Remote,
    Container,
    Vm,
    Elevate,
    ChangedPrivileges,
    Session,
    Other,
}

#[cfg(feature = "host")]
#[derive(Debug)]
struct Context {
    id: Vec<u8>,
    /// The foreground process group of the terminal when the context started.
    group: Option<u32>,
    kind: Kind,
    user: Option<String>,
    target_user: Option<String>,
    hostname: Option<String>,
    machine_id: Option<String>,
    target_host: Option<String>,
    container: Option<String>,
    vm: Option<String>,
}

/// The open contexts of a terminal, innermost last. A start with a new
/// identifier opens a context inside the active one, a start with an open
/// identifier replaces it and ends the contexts inside it, and an end closes
/// the context and the contexts inside it.
#[cfg(feature = "host")]
#[derive(Debug, Default)]
pub(super) struct Contexts {
    stack: Vec<Context>,
}

#[cfg(feature = "host")]
impl Contexts {
    pub(super) fn apply(&mut self, signal: ContextSignal, group: Option<u32>) {
        if !(1..=MAX_ID_BYTES).contains(&signal.id.len())
            || !signal.id.iter().all(|byte| (b' '..=b'~').contains(byte))
        {
            return;
        }
        let open = self
            .stack
            .iter()
            .position(|context| context.id == signal.id);
        match signal.action {
            ContextAction::Start => {
                if let Some(index) = open {
                    self.stack.truncate(index);
                } else if self.stack.len() >= MAX_CONTEXTS {
                    return;
                }
                self.stack
                    .push(Context::parse(signal.id, &signal.fields, group));
            }
            ContextAction::End => {
                if let Some(index) = open {
                    self.stack.truncate(index);
                }
            }
        }
    }

    /// The shell of the terminal is in front again, so the contexts that its
    /// jobs reported have ended, even those a job never ended, as when a
    /// connection drops.
    pub(super) fn shell_in_front(&mut self, shell: u32) {
        let kept = self
            .stack
            .iter()
            .rposition(|context| context.group == Some(shell))
            .map_or(0, |index| index + 1);
        self.stack.truncate(kept);
    }

    pub(super) fn summary(&self, local: &LocalMachine) -> Option<TerminalContext> {
        let place = self
            .stack
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, context)| {
                context.place(local, || {
                    self.stack[..index]
                        .iter()
                        .rev()
                        .find_map(Context::transition)
                })
            });
        let root = self
            .stack
            .iter()
            .rev()
            .find_map(Context::user)
            .is_some_and(|user| user == "root");
        let context = TerminalContext { place, root };
        context.is_valid().then_some(context)
    }
}

#[cfg(feature = "host")]
impl Context {
    fn parse(id: Vec<u8>, fields: &[u8], group: Option<u32>) -> Self {
        let mut context = Self {
            id,
            group,
            kind: Kind::Other,
            user: None,
            target_user: None,
            hostname: None,
            machine_id: None,
            target_host: None,
            container: None,
            vm: None,
        };
        for field in fields.split(|&byte| byte == b';') {
            let Some(equals) = field.iter().position(|&byte| byte == b'=') else {
                continue;
            };
            let Some(value) = unescape(&field[equals + 1..]) else {
                continue;
            };
            let slot = match &field[..equals] {
                b"type" => {
                    context.kind = Kind::of(&value);
                    continue;
                }
                b"user" => &mut context.user,
                b"targetuser" => &mut context.target_user,
                b"hostname" => &mut context.hostname,
                b"machineid" => &mut context.machine_id,
                b"targethost" => &mut context.target_host,
                b"container" => &mut context.container,
                b"vm" => &mut context.vm,
                _ => continue,
            };
            *slot = Some(value);
        }
        context
    }

    /// The place this context names: the target of a login, a container or a
    /// virtual machine, or the computer of a context that another computer
    /// reported, inside the nearest enclosing login, container or machine.
    fn place(
        &self,
        local: &LocalMachine,
        enclosing: impl FnOnce() -> Option<PlaceKind>,
    ) -> Option<ContextPlace> {
        let (kind, name) = match self.kind {
            Kind::Remote => (PlaceKind::Remote, self.target_host.as_ref()?),
            Kind::Container => (
                PlaceKind::Container,
                self.container.as_ref().or(self.hostname.as_ref())?,
            ),
            Kind::Vm => (PlaceKind::Vm, self.vm.as_ref().or(self.hostname.as_ref())?),
            _ if self.elsewhere(local) => (
                enclosing().unwrap_or(PlaceKind::Remote),
                self.hostname.as_ref()?,
            ),
            _ => return None,
        };
        Some(ContextPlace {
            kind,
            name: name.clone(),
        })
    }

    const fn transition(&self) -> Option<PlaceKind> {
        match self.kind {
            Kind::Remote => Some(PlaceKind::Remote),
            Kind::Container => Some(PlaceKind::Container),
            Kind::Vm => Some(PlaceKind::Vm),
            _ => None,
        }
    }

    fn elsewhere(&self, local: &LocalMachine) -> bool {
        let differs = |theirs: Option<&String>, ours: Option<&String>| {
            theirs
                .zip(ours)
                .map(|(theirs, ours)| !theirs.eq_ignore_ascii_case(ours))
        };
        differs(self.machine_id.as_ref(), local.machine_id.as_ref())
            .or_else(|| differs(self.hostname.as_ref(), local.hostname.as_ref()))
            .unwrap_or(false)
    }

    /// The user that the programs of this context run as.
    fn user(&self) -> Option<&str> {
        let changes_user = matches!(
            self.kind,
            Kind::Remote
                | Kind::Container
                | Kind::Vm
                | Kind::Elevate
                | Kind::ChangedPrivileges
                | Kind::Session
        );
        changes_user
            .then_some(self.target_user.as_deref())
            .flatten()
            .or(self.user.as_deref())
    }
}

#[cfg(feature = "host")]
impl Kind {
    fn of(name: &str) -> Self {
        match name {
            "remote" => Self::Remote,
            "container" => Self::Container,
            "vm" => Self::Vm,
            "elevate" => Self::Elevate,
            "chpriv" => Self::ChangedPrivileges,
            "session" => Self::Session,
            _ => Self::Other,
        }
    }
}

/// A field value as the specification escapes it: `;` as `\x3b` and `\` as
/// `\x5c`. A value that is empty, too long or not text is ignored.
#[cfg(feature = "host")]
fn unescape(raw: &[u8]) -> Option<String> {
    let mut value = Vec::with_capacity(raw.len());
    let mut rest = raw;
    while let Some((&byte, tail)) = rest.split_first() {
        let escaped = match tail {
            [b'x' | b'X', b'3', b'b' | b'B', ..] if byte == b'\\' => Some(b';'),
            [b'x' | b'X', b'5', b'c' | b'C', ..] if byte == b'\\' => Some(b'\\'),
            _ => None,
        };
        if let Some(escaped) = escaped {
            value.push(escaped);
            rest = &tail[3..];
        } else {
            value.push(byte);
            rest = tail;
        }
    }
    let value = String::from_utf8(value).ok()?;
    super::status::is_display_text(&value, MAX_NAME_CHARS).then_some(value)
}

#[cfg(all(test, feature = "host"))]
#[path = "context_tests.rs"]
mod tests;
