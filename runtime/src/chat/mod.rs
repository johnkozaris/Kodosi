//! Structured chat view of a terminal's coding agent.
//!
//! Agents report events through their own link interfaces; the chat never scrapes terminal output.

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
#[cfg(feature = "host")]
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

/// Env var with the Unix socket path where terminal agents report chat events.
#[cfg(feature = "host")]
pub(crate) const LINK_VARIABLE: &str = "KODOSI_AGENT_LINK";
/// Env var with the Windows named pipe path used by Node-based agent links.
#[cfg(feature = "host")]
pub(crate) const PIPE_VARIABLE: &str = "KODOSI_AGENT_PIPE";
/// Env var with the per-terminal Codex app-server address.
#[cfg(feature = "host")]
pub(crate) const CODEX_VARIABLE: &str = "KODOSI_CODEX_SERVER";

pub const CHAT_PIPE: &str = "chat";
#[cfg(feature = "host")]
const MOST_ITEMS: usize = 400;
#[cfg(feature = "host")]
const MOST_QUEUED: usize = 16;
#[cfg(feature = "host")]
const MOST_MESSAGE: usize = 64 * 1024;
#[cfg(feature = "host")]
const MOST_CALLS: usize = 2048;
#[cfg(feature = "host")]
const MOST_QUESTIONS: usize = 8;
#[cfg(feature = "host")]
const MOST_OPTIONS: usize = 16;
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
#[cfg(feature = "host")]
const MOST_COMMANDS: usize = 256;

/// Host sends a `Snapshot` on attach, then sequential `Update` frames.
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<ChatCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatUpdate {
    pub revision: u64,
    pub changes: Vec<ChatChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "change", rename_all = "camelCase")]
pub enum ChatChange {
    /// Appends a new item or replaces an existing item with the same `id` in place.
    Item {
        item: ChatItem,
    },
    /// Appends a streaming text delta to an existing text item.
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
    Tasks {
        tasks: Vec<ChatTask>,
    },
    /// The agent's slash commands, whole.
    Commands {
        commands: Vec<ChatCommand>,
    },
    Facts {
        facts: Box<ChatFacts>,
    },
    State {
        state: ChatState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        now: Option<ChatNow>,
    },
    /// Clears `items`, `asks`, and `tasks` for a new conversation while keeping `queued`,
    /// `commands`, `facts`, and `state`.
    Reset,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum ChatState {
    /// No agent is currently linked to this terminal.
    #[default]
    Away,
    Idle,
    Working,
    Asking,
}

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
    /// Unix timestamp in milliseconds from the host's clock.
    pub at: u64,
    pub body: ChatBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatBody {
    Person {
        text: String,
        /// Sender when submitted from chat; `None` if typed directly in the terminal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<ChatPerson>,
    },
    Text {
        text: String,
        /// True while the agent is still streaming this message.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        live: bool,
    },
    Tool {
        tool: Box<ChatTool>,
    },
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
    /// The agent compacted its conversation to free up context.
    Compacted,
    /// The agent is signed out and signs in through its own flow in its terminal.
    SignedOut,
    /// What the agent says happened while the person was away.
    Recap { text: String },
    /// The agent could not continue. `error` is the agent's word for the cause; `text` is
    /// optional detail.
    Failed {
        error: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatTool {
    pub name: String,
    pub kind: ToolKind,
    pub state: ToolState,
    /// Short subject (command, file, search pattern, or URL). Files under the terminal's
    /// working directory use their basename; files outside use a `~`-relative or full path.
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
    /// The agent is still streaming the tool call arguments.
    Preparing,
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
        /// True if the command continues running after the tool call returned.
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
        /// User answers, aligned by index with `questions`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        answers: Vec<String>,
    },
    Plan {
        text: String,
    },
    Agent {
        /// Subagent type or display name reported by the agent.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        steps: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        doing: Option<ChatStep>,
        /// Truncated prefix of the subagent's latest message.
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

/// An open prompt from the agent. Shown in both the terminal and chat; the first answer wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatAsk {
    pub id: String,
    /// `ChatItem::id` of the waiting tool call, if it has a top-level item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub body: AskBody,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AskBody {
    Permission {
        tool: Box<ChatTool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        always: Option<AskAlways>,
        /// Task description of the requesting subagent; empty for the main agent.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        helper: String,
    },
    Question {
        questions: Vec<ChatQuestion>,
    },
    Plan {
        text: String,
        /// Available next actions, with the agent's recommended option first.
        choices: Vec<PlanChoice>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PlanChoice {
    /// Execute the plan using the session's current permission settings.
    Go,
    /// Execute the plan, asking before each change.
    Ask,
    /// Execute the plan, auto-approving file edits.
    Edits,
    /// Execute the plan, letting the agent's safety classifier approve calls.
    Auto,
    /// Execute the plan autonomously without asking for approval.
    Alone,
    /// Execute the plan autonomously in parallel using subagents.
    Helpers,
    /// Exit plan mode without executing the plan.
    Leave,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AskAlways {
    /// Allows calls matching `tool` and `text` from now on.
    Rule { tool: String, text: String },
    /// Switches to `mode` for the rest of the session.
    Mode { mode: ChatMode },
    /// Grants the agent access to an additional directory.
    Folder { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ChatAnswer {
    Allow,
    Always,
    /// With `text`, sends feedback so the agent can adjust and continue; without, stops the turn.
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// One list per question (in order): selected option labels or a single custom text entry.
    Answers {
        answers: Vec<Vec<String>>,
    },
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
    /// Error from the agent if delivery was refused. The message waits until the user sends
    /// another message, unqueues it, or types in the terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    /// The agent shows this command in its terminal, where a person finishes it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminal: bool,
    /// The agent has this message for the turn that runs and reads it before its next step, so
    /// it can no longer be removed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub given: bool,
}

/// A slash command of the agent, as its own menu lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatCommand {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// The control of the chat that does what the command does, in place of the agent's own view
    /// in its terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<ChatControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatControl {
    /// The agent's model, effort and fast mode.
    Model,
    /// The limits of the agent's plan, its context and the cost of the conversation.
    Usage,
}

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

/// A chat participant, identified by account ID or (when unauthenticated) by viewer ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatPerson {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatFacts {
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
    /// Estimated conversation cost in USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    /// Running background command count.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatMode {
    /// Prompt before file edits and shell commands.
    Ask,
    /// Auto-approve file edits; prompt before shell commands.
    Edits,
    /// Read-only planning mode.
    Plan,
    /// Let the agent's own safety check decide which calls to approve.
    Auto,
    /// Allow only rule-matched calls and deny everything else without prompting.
    Rules,
    /// Run all calls without prompting.
    Unrestricted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "do", rename_all = "camelCase", deny_unknown_fields)]
pub enum ChatRequest {
    /// Queues a message while the agent is busy. Oversized messages are truncated in the middle
    /// so pasted logs keep both the start and the trailing prompt.
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
    Stop,
}
