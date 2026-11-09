#[cfg(feature = "host")]
mod adapters;
mod chat;
#[cfg(feature = "host")]
mod context;
mod conversation;
#[cfg(feature = "host")]
mod engine;
#[cfg(feature = "host")]
mod folder;
#[cfg(feature = "host")]
mod process;
#[cfg(feature = "host")]
mod receipt;
mod shared;
mod store;
#[cfg(all(test, feature = "host"))]
mod tests;
#[cfg(any(test, not(feature = "host")))]
mod viewer;

pub(crate) use chat::{Failure, Message, Output};
#[cfg(feature = "host")]
pub(crate) use engine::{Engine, Installed};
#[cfg(any(test, not(feature = "host")))]
pub(crate) use viewer::Viewer;

use serde::{Deserialize, Serialize};

use crate::agents::{Agent, AgentChoice};

pub const MAX_MESSAGE_BYTES: usize = 24 * 1024;
pub const MAX_INSTRUCTIONS_BYTES: usize = 32 * 1024;
pub const MAX_QUESTION_BYTES: usize = 1024;
pub const MAX_OPTION_BYTES: usize = 200;
pub const MAX_OPTIONS: usize = 8;
pub const MAX_ANSWER_BYTES: usize = 1024;
pub const TOKEN_VARIABLE: &str = "KODOSI_DOSI_TOKEN";

pub const AGENTS: [Agent; 2] = [Agent::Claude, Agent::Codex];

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiContext {
    #[serde(default)]
    pub terminal: Option<String>,
    #[serde(default)]
    pub room: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DosiState {
    Idle,
    Working,
    Stopping,
    Connecting,
    Unavailable,
    /// Dosi is off on this computer.
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiSnapshot {
    pub state: DosiState,
    pub setup: AgentChoice,
    pub agents: Vec<DosiAgent>,
    pub instructions_edited: bool,
    pub folder: String,
    pub shared: bool,
    pub remote: Option<DosiRemote>,
    pub conversation: Option<DosiConversation>,
    pub items: Vec<DosiItem>,
    pub earlier: bool,
    pub queued: Vec<DosiQueued>,
    /// The messages that wait for the computer that the person asked to answer them.
    pub asks: Vec<DosiAsk>,
    /// The computer that answered last, which answers a message from a device that runs no
    /// Dosi unless the person picks another.
    pub last: Option<DosiComputer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiComputer {
    pub device: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiAsk {
    pub item: String,
    pub computer: DosiComputer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiRemote {
    pub computer: String,
    pub since: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiAgent {
    pub agent: Agent,
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiConversation {
    pub id: String,
    pub started_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiQueued {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiItem {
    pub id: String,
    pub turn: String,
    pub at: i64,
    pub body: DosiBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DosiBody {
    Person {
        text: String,
    },
    Text {
        text: String,
        live: bool,
    },
    Reasoning {
        text: String,
        live: bool,
        seconds: Option<u32>,
    },
    Tool {
        action: DosiAction,
        target: Option<DosiTarget>,
        text: Option<String>,
        status: DosiToolStatus,
        output: Option<String>,
    },
    Question {
        question: String,
        options: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        keys: Vec<String>,
        multiple: bool,
        terminal: Option<String>,
        answer: Option<Vec<String>>,
    },
    Note {
        text: String,
    },
    Notice {
        tone: DosiTone,
        text: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DosiAction {
    Look,
    Read,
    Type,
    Press,
    Start,
    Interrupt,
    Close,
    Show,
    Wait,
    Post,
    Run,
    Write,
    Edit,
    Search,
    Fetch,
    Plan,
    Delegate,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiTarget {
    pub kind: DosiTargetKind,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DosiTargetKind {
    Terminal,
    Terminals,
    Room,
    Rooms,
    Agent,
    File,
    Url,
    Query,
    Command,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DosiToolStatus {
    Running,
    Done,
    Failed,
}

impl DosiBody {
    pub fn deed(&self) -> Option<String> {
        let Self::Tool {
            action,
            target,
            text,
            ..
        } = self
        else {
            return None;
        };
        let target = target.as_ref().map(|target| match target.kind {
            DosiTargetKind::Terminal => format!("the terminal {}", target.value),
            DosiTargetKind::Room => format!("the room {}", target.value),
            DosiTargetKind::Terminals => "the terminals".to_owned(),
            DosiTargetKind::Rooms => "the rooms".to_owned(),
            _ => target.value.clone(),
        });
        let line = format!("{} {}", action.done(), target.unwrap_or_default());
        let line = line.trim_end();
        Some(
            text.as_ref()
                .map_or_else(|| line.to_owned(), |text| format!("{line}: {text}")),
        )
    }
}

impl DosiAction {
    const fn done(self) -> &'static str {
        match self {
            Self::Look => "looked at",
            Self::Read => "read",
            Self::Type => "typed in",
            Self::Press => "pressed keys in",
            Self::Start => "started",
            Self::Interrupt => "interrupted",
            Self::Close => "closed",
            Self::Show => "showed",
            Self::Wait => "waited for",
            Self::Post => "posted in",
            Self::Run => "ran",
            Self::Write => "wrote",
            Self::Edit => "changed",
            Self::Search => "searched for",
            Self::Fetch => "fetched",
            Self::Plan => "made a plan",
            Self::Delegate => "asked a helper agent:",
            Self::Other => "used",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DosiTone {
    Stopped,
    Failed,
    SignIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiInstructions {
    pub text: String,
    pub edited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiPage {
    pub items: Vec<DosiItem>,
    pub earlier: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "record",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DosiRecord {
    Begin {
        conversation: String,
    },
    Turn {
        conversation: String,
        turn: String,
        open: bool,
        computer: String,
        at: i64,
    },
    Stop {
        turn: String,
    },
    Item {
        conversation: String,
        item: DosiItem,
    },
    /// A message of the person for `computer`, which answers it in a turn with the turn of
    /// `item`. Until that turn opens, a `Stop` of it takes the message back.
    Ask {
        conversation: String,
        computer: DosiComputer,
        /// What the person had open on the device that asked, for Dosi.
        context: Option<String>,
        item: DosiItem,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DosiEntry {
    pub sequence: u64,
    pub mine: bool,
    /// The device that wrote the record, as its signature proves.
    pub device: String,
    pub record: DosiRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedInstructions {
    pub text: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DosiLog {
    pub entries: Vec<DosiEntry>,
    pub sequence: u64,
    pub instructions: Option<SharedInstructions>,
    pub reset: bool,
    /// The person's devices now, as the keys of the Dosi room prove.
    pub devices: std::collections::BTreeSet<String>,
}
