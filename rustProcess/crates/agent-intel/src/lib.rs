#![expect(
    clippy::option_if_let_else,
    reason = "agent provider request handlers are clearer as explicit branchy response construction"
)]
#![expect(
    clippy::too_many_lines,
    reason = "agent JSONL/provider handlers keep protocol-specific state transitions together"
)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::unwrap_used,
        reason = "tests assert preconditions via panic-on-None/Err for failure clarity"
    )
)]

pub mod agent_fs;
pub mod claude;
pub mod copilot;
pub mod domain;
pub mod factory;
pub mod jsonl;
pub mod mcp;
pub mod memory;
pub mod ops;
pub mod ports;
pub mod runtime;
pub mod terminal;
pub mod transcript;

pub use domain::AgentIntelSnapshot;
#[doc(hidden)]
pub use domain::AgentProviderState;
pub use factory::create_live_provider;
pub use kodosi_session::AgentKind;
pub use ports::live_provider::LiveAgentProvider;
pub use ports::watcher::{ArtifactWatcher, WatchEvent};
pub use runtime::watch_targets::{
    global as global_watch_targets, normalize_workspace_root, workspace as workspace_watch_targets,
};
pub use runtime::watcher_impl::{FsArtifactWatcher, WatchTarget};
pub use transcript::{ConversationEntry, ConversationPage, DecodeError, TranscriptDecoder};
