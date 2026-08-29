pub(crate) mod account_runtimes;
pub(crate) mod auth_lifecycle;
pub(crate) mod backend_access;
pub(crate) mod backend_adapters;
mod device_flow_runtime;
pub(crate) mod device_keys;
pub(crate) mod device_link;
pub(crate) mod device_list;
mod session_key_trust;
pub(crate) mod state;
pub(crate) use account_runtimes::AccountRuntimes;
pub(crate) use device_flow_runtime::DeviceFlowRuntime;
pub(crate) use device_keys::DeviceKeyAccess;
#[cfg(feature = "cli")]
pub(crate) use device_link::{DeviceLinkApprovalOutcome, SelfDeviceLinkStart};
#[cfg(feature = "cli")]
pub(crate) use device_list::DeviceRevocationOutcome;
pub(crate) use session_key_trust::IdentitySessionKeyTrust;
pub(crate) use state::IdentityState;

#[cfg(feature = "cli")]
pub(crate) enum DeviceRpcOutcome {
    Revoked(DeviceRevocationOutcome),
    LinkApproved(DeviceLinkApprovalOutcome),
    SelfLinkStarted(SelfDeviceLinkStart),
    SelfLinkCancellationRequested,
}

pub(crate) async fn reset_trust_and_refresh_discovery(
    app: &mut super::Runtime,
    user_id: &str,
) -> crate::Result<bool> {
    let cleared = app.pin_store.reset(user_id).await?;
    super::discovery::refresh(app).await;
    Ok(cleared)
}
