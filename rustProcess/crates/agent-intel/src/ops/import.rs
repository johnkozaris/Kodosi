use crate::{AgentKind, ConversationPage};

pub async fn read_session_conversation(
    agent: AgentKind,
    cwd: &str,
    session_id: &str,
    before_byte: Option<u64>,
    max_records: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    crate::ops::conversation::read_session_conversation(
        agent,
        cwd,
        session_id,
        before_byte,
        max_records,
        max_bytes,
    )
    .await
}

pub async fn read_subagent_transcript(
    agent: AgentKind,
    path: &str,
    before_byte: Option<u64>,
    max_records: Option<usize>,
    max_bytes: Option<usize>,
) -> Result<ConversationPage, String> {
    crate::ops::conversation::read_subagent_transcript(
        agent,
        path,
        before_byte,
        max_records,
        max_bytes,
    )
    .await
}
