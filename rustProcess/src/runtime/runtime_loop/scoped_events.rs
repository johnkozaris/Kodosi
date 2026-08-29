use crate::{
    AgentIntelEvent, Result, SessionEvent, runtime, runtime_event_bus::RuntimeEventSender,
};

pub(super) async fn send_session(
    app: &runtime::Runtime,
    tx: &RuntimeEventSender,
    event: SessionEvent,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    tx.send_session(account_user_id, account_epoch, event).await
}

pub(super) async fn send_agent_intel(
    app: &runtime::Runtime,
    tx: &RuntimeEventSender,
    event: AgentIntelEvent,
) -> Result<()> {
    let (account_user_id, account_epoch) = app.state.identity.event_context();
    tx.send_agent_intel(account_user_id, account_epoch, event)
        .await
}
