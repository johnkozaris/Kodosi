//! The chat of a terminal: what its agent does, as items that a person reads and answers.
//!
//! The agent reports each step through its own interface, so the chat reads nothing from the
//! terminal's screen.

use serde::{Deserialize, Serialize};

#[cfg(all(feature = "host", unix))]
mod acp;
#[cfg(feature = "host")]
mod agent;
#[cfg(feature = "host")]
mod antigravity;
#[cfg(feature = "host")]
mod claude;
#[cfg(feature = "host")]
mod codex;
#[cfg(all(feature = "host", unix))]
mod codex_link;
#[cfg(feature = "host")]
mod copilot;
#[cfg(feature = "host")]
mod diff;
#[cfg(feature = "host")]
mod http;
#[cfg(feature = "host")]
mod link;
#[cfg(feature = "host")]
mod room;
#[cfg(feature = "host")]
mod state;
#[cfg(all(test, feature = "host"))]
mod tests;

#[cfg(all(feature = "host", unix))]
pub(crate) use acp::{Choices as AcpChoices, run as run_acp};
#[cfg(feature = "host")]
pub(crate) use claude::MODULE as CLAUDE_MODULE;
#[cfg(feature = "host")]
pub(crate) use copilot::EXTENSION as COPILOT_EXTENSION;
#[cfg(feature = "host")]
pub(crate) use link::{Links, TerminalChat};
#[cfg(feature = "host")]
pub(crate) use room::ChatHandle;

/// The variable that gives the programs of a terminal the address where an agent reports to the chat.
#[cfg(feature = "host")]
pub(crate) const LINK_VARIABLE: &str = "KODOSI_AGENT_LINK";
/// The variable that gives a terminal the address of its own app-server of Codex.
#[cfg(feature = "host")]
pub(crate) const CODEX_VARIABLE: &str = "KODOSI_CODEX_SERVER";

/// The key of the pipe that carries a terminal's chat.
pub const CHAT_PIPE: &str = "chat";
pub const MOST_ITEMS: usize = 400;
pub const MOST_QUEUED: usize = 16;
pub const MOST_MESSAGE: usize = 64 * 1024;
#[cfg(feature = "host")]
const MOST_TEXT: usize = 48 * 1024;
#[cfg(feature = "host")]
const MOST_OUTPUT: usize = 4 * 1024;
#[cfg(feature = "host")]
const MOST_TARGET: usize = 400;
#[cfg(feature = "host")]
const MOST_LINES: usize = 160;
#[cfg(feature = "host")]
const MOST_ASKS: usize = 16;
#[cfg(feature = "host")]
const MOST_TASKS: usize = 64;

/// What the terminal's computer sends on the chat pipe: the chat as it is, then each change in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatFrame {
    Snapshot(Box<ChatSnapshot>),
    Update(ChatUpdate),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatSnapshot {
    pub revision: u64,
    pub state: ChatState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<ChatNow>,
    pub facts: ChatFacts,
    pub items: Vec<ChatItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<ChatAsk>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queued: Vec<ChatQueued>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<ChatTask>,
}

/// The changes that make the chat of one revision from the chat of the revision before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatUpdate {
    pub revision: u64,
    pub changes: Vec<ChatChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "change", rename_all = "camelCase")]
pub enum ChatChange {
    /// A new item at the end, or an item with this identifier in its place.
    Item {
        item: ChatItem,
    },
    /// More text at the end of the answer that the agent writes.
    Text {
        id: String,
        text: String,
    },
    Remove {
        id: String,
    },
    Asks {
        asks: Vec<ChatAsk>,
    },
    Queued {
        queued: Vec<ChatQueued>,
    },
    /// The agent's list of its work, whole.
    Tasks {
        tasks: Vec<ChatTask>,
    },
    Facts {
        facts: Box<ChatFacts>,
    },
    State {
        state: ChatState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        now: Option<ChatNow>,
    },
    /// The agent started a new conversation: the items and the questions are gone.
    Reset,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ChatState {
    /// No agent of this terminal reports to the chat.
    #[default]
    Away,
    Idle,
    Working,
    /// The agent waits for an answer of a person.
    Asking,
}

/// What the agent does at this moment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatNow {
    Thinking,
    Writing,
    Tool { item: String },
    Compacting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatItem {
    pub id: String,
    /// In milliseconds since 1970 on the terminal's computer.
    pub at: u64,
    pub body: ChatBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatBody {
    Person {
        text: String,
        /// The person who sent it from the chat. A message without one was typed in the terminal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<ChatPerson>,
    },
    Text {
        text: String,
        /// The agent still writes it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        live: bool,
    },
    Tool {
        tool: Box<ChatTool>,
    },
    /// The agent's turn ended here.
    Turn {
        end: TurnEnd,
        ms: u64,
    },
    Note {
        note: ChatNote,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TurnEnd {
    Done,
    Stopped,
    Failed,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatNote {
    /// The agent made its conversation shorter to free its context.
    Compacted,
    /// The agent is not signed in, and signs in with its own steps in its terminal.
    SignedOut,
    /// The agent could not continue. `error` is the agent's word for the cause.
    Failed {
        error: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatTool {
    /// The agent's name of the tool.
    pub name: String,
    pub kind: ToolKind,
    pub state: ToolState,
    /// The short subject: a command, a file, a pattern, an address. A file in the terminal's
    /// folder shows by its name, and a file elsewhere by its place.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ToolDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    Command,
    Read,
    Edit,
    Write,
    Search,
    Web,
    Agent,
    Skill,
    Question,
    Plan,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ToolState {
    /// The agent writes the call.
    Preparing,
    /// The call waits for a person's permission or answer.
    Asking,
    Running,
    Done,
    Failed,
    Denied,
    Stopped,
}

impl ToolState {
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Preparing | Self::Asking | Self::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolDetail {
    Command {
        command: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        output: String,
        /// The command runs on after its call ended.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        background: bool,
    },
    Change {
        path: String,
        added: u32,
        removed: u32,
        lines: Vec<ChangeLine>,
    },
    Output {
        text: String,
    },
    Question {
        questions: Vec<ChatQuestion>,
        /// For each question in order, what the person chose or wrote.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        answers: Vec<String>,
    },
    Plan {
        text: String,
    },
    Agent {
        /// The agent's name of the helper's kind.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        steps: u32,
        /// The helper's last step.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doing: Option<ChatStep>,
        /// The start of what the helper wrote last.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        said: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatStep {
    pub kind: ToolKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChangeLine {
    pub sign: ChangeSign,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ChangeSign {
    Same,
    Added,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatQuestion {
    pub question: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub header: String,
    pub options: Vec<ChatOption>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatOption {
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// A question of the agent that is open. The agent shows the same question in the terminal, and
/// the first answer counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatAsk {
    pub id: String,
    /// The item of the tool call that waits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub body: AskBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AskBody {
    Permission {
        tool: Box<ChatTool>,
        /// What the agent offers to allow from now on.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        always: Option<AskAlways>,
        /// The task of the helper that asks.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        helper: String,
    },
    Question {
        questions: Vec<ChatQuestion>,
    },
    Plan {
        text: String,
        /// The ways on that the agent offers, the one that it recommends first.
        choices: Vec<PlanChoice>,
    },
}

/// A way on after a plan of the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PlanChoice {
    /// Carry out the plan, and the agent asks as it is set.
    Go,
    /// Carry out the plan, and the agent asks before each change.
    Ask,
    /// Carry out the plan, and the agent changes files without asking.
    Edits,
    /// Carry out the plan, and the agent's safety check decides what it may do.
    Auto,
    /// The agent carries out the plan alone and asks nothing.
    Alone,
    /// Helpers of the agent carry out parts of the plan side by side, alone.
    Helpers,
    /// The agent leaves plan mode and does not carry out the plan.
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AskAlways {
    /// Calls that match this rule, such as a command.
    Rule { tool: String, text: String },
    /// Each call that this permission mode allows, until the session ends.
    Mode { mode: ChatMode },
    /// A folder that the agent may also work in.
    Folder { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ChatAnswer {
    Allow,
    /// Allow, with what the question offers to allow from now on.
    Always,
    /// With words, the agent reads them and continues. Without, the agent stops.
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// For each question in order, the labels that the person chose or one text of their own.
    Answers {
        answers: Vec<Vec<String>>,
    },
    /// The way on after a plan that the person chose.
    Plan {
        choice: PlanChoice,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatQueued {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<ChatPerson>,
    /// The agent did not take the message, in the agent's words. The message waits until a person
    /// sends another, removes it, or writes to the agent in the terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
}

/// A step of the list that the agent keeps of its work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatTask {
    pub text: String,
    pub state: TaskState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Pending,
    Working,
    Done,
}

/// A person of the terminal: the account, or for a person with no account the view that sent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatPerson {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatFacts {
    /// The agent's name, as the terminal's program has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ChatMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ChatContext>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limits: Vec<crate::agents::UsageLimit>,
    /// What the agent estimates that the conversation cost, in US dollars.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// Helpers that work now.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub helpers: u32,
    /// Commands that run on after their calls ended.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub background: u32,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde gives the field by reference"
)]
const fn is_zero(count: &u32) -> bool {
    *count == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatContext {
    pub tokens: u64,
    pub window: u64,
}

/// When the agent asks before it acts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatMode {
    /// Before each change and each command.
    Ask,
    /// Before each command, and not before a change of a file.
    Edits,
    /// The agent plans and changes nothing.
    Plan,
    /// The agent decides which calls are safe.
    Auto,
    /// The agent does what a rule allows and refuses the rest. It asks nothing.
    Rules,
    /// The agent asks nothing.
    Unrestricted,
}

/// What an app sends on the chat pipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "do", rename_all = "camelCase", deny_unknown_fields)]
pub enum ChatRequest {
    /// A message for the agent. It waits in the queue while the agent works. Of a message that is
    /// longer than the chat takes, such as a pasted log, the agent gets the start and the end,
    /// where a person's question usually is.
    Send {
        text: String,
    },
    Unqueue {
        id: String,
    },
    Answer {
        ask: String,
        answer: ChatAnswer,
    },
    /// Stop the agent's turn and the commands of the turn.
    Stop,
}
