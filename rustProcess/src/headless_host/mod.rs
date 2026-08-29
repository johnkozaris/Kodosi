mod client;
mod device_rpc;
mod device_rpc_client;
mod dispatch;
mod handshake;
mod lifecycle;
pub(super) mod local_endpoint;
mod server;
mod snapshot;
mod state_file;
pub(crate) mod terminal_lane;

#[cfg(test)]
mod tests;

pub(crate) use client::{HeadlessHostClient, ROOM_ACTION_WAIT_BOUND_SECS};
pub(crate) use device_rpc_client::{
    approve_device_link, cancel_self_device_link, revoke_device, start_self_device_link,
};
pub(crate) use lifecycle::{
    connect_existing_host, connect_existing_or_replace_incompatible_host,
    connect_terminal_capture_lane, ensure_host_running, host_info, stop_host,
};
pub(crate) use server::run_server;
pub(crate) use snapshot::HeadlessHostAuthState;

pub(crate) fn acquire_runtime_lock() -> crate::Result<std::fs::File> {
    let runtime_dir =
        state_file::host_runtime_dir().ok_or_else(|| crate::AppError::Unsupported {
            reason: "headless host runtime directory is unavailable".to_owned(),
        })?;
    local_endpoint::acquire_host_lock(&runtime_dir)
}
