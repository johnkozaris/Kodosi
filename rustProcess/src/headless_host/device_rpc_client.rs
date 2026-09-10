use std::path::Path;

use tokio::time;

use crate::{AppError, Result, support::io::framed_json};

use super::{
    device_rpc::DeviceRpcResponse,
    handshake::{ConnectionLane, HOST_PROTOCOL_VERSION, HostHelloRequest, HostHelloResponse},
    lifecycle::classify_rejection,
    local_endpoint::{connect_local, is_connection_missing, is_not_a_socket},
    state_file::{cleanup_host_state_file_if_matches, host_runtime_dir, load_host_state_file_in},
};

const DEVICE_RPC_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);

pub(crate) async fn revoke_device(
    expected_account_user_id: String,
    device_id: String,
) -> Result<crate::runtime::identity::DeviceRevocationOutcome> {
    match call(ConnectionLane::DeviceRevoke {
        expected_account_user_id,
        device_id,
    })
    .await?
    {
        DeviceRpcResponse::Revoked {
            history_warning,
            revoked_device_id,
            new_generation,
        } => Ok(crate::runtime::identity::DeviceRevocationOutcome {
            history_warning,
            revoked_device_id,
            new_generation,
        }),
        DeviceRpcResponse::Error { message } => Err(AppError::Unsupported { reason: message }),
        _ => Err(AppError::InvalidBackendData {
            field: "headless.deviceRpc".to_owned(),
            reason: "headless host returned the wrong device RPC result".to_owned(),
        }),
    }
}

pub(crate) async fn approve_device_link(
    expected_account_user_id: String,
    user_code: String,
) -> Result<crate::runtime::identity::DeviceLinkApprovalOutcome> {
    match call(ConnectionLane::DeviceApproveLink {
        expected_account_user_id,
        user_code,
    })
    .await?
    {
        DeviceRpcResponse::LinkApproved {
            approved_user_code,
            approved_device_id,
            approved_device_label,
            new_generation,
        } => Ok(crate::runtime::identity::DeviceLinkApprovalOutcome {
            approved_user_code,
            approved_device_id,
            approved_device_label,
            new_generation,
        }),
        DeviceRpcResponse::Error { message } => Err(AppError::Unsupported { reason: message }),
        _ => Err(AppError::InvalidBackendData {
            field: "headless.deviceRpc".to_owned(),
            reason: "headless host returned the wrong device RPC result".to_owned(),
        }),
    }
}

pub(crate) async fn start_self_device_link(
    expected_account_user_id: String,
    label: Option<String>,
) -> Result<crate::runtime::identity::SelfDeviceLinkStart> {
    match call(ConnectionLane::DeviceStartSelfLink {
        expected_account_user_id,
        label,
    })
    .await?
    {
        DeviceRpcResponse::SelfLinkStarted {
            device_id,
            user_code,
            expires_at,
        } => Ok(crate::runtime::identity::SelfDeviceLinkStart {
            device_id,
            user_code,
            expires_at,
        }),
        DeviceRpcResponse::Error { message } => Err(AppError::Unsupported { reason: message }),
        _ => Err(AppError::InvalidBackendData {
            field: "headless.deviceRpc".to_owned(),
            reason: "headless host returned the wrong device RPC result".to_owned(),
        }),
    }
}

pub(crate) async fn cancel_self_device_link(expected_account_user_id: String) -> Result<()> {
    match call(ConnectionLane::DeviceCancelSelfLink {
        expected_account_user_id,
    })
    .await?
    {
        DeviceRpcResponse::SelfLinkCancellationRequested => Ok(()),
        DeviceRpcResponse::Error { message } => Err(AppError::Unsupported { reason: message }),
        _ => Err(AppError::InvalidBackendData {
            field: "headless.deviceRpc".to_owned(),
            reason: "headless host returned the wrong device RPC result".to_owned(),
        }),
    }
}

async fn call(lane: ConnectionLane) -> Result<DeviceRpcResponse> {
    let runtime_dir = host_runtime_dir().ok_or_else(|| AppError::Unsupported {
        reason: "headless host runtime directory is unavailable".to_owned(),
    })?;
    let Some(state) = load_host_state_file_in(&runtime_dir)? else {
        return Err(AppError::Unsupported {
            reason: "headless host is not running".to_owned(),
        });
    };
    super::lifecycle::ensure_matching_config(&state)?;
    let socket_path = Path::new(&state.socket_path);
    let stream = match connect_local(socket_path).await {
        Ok(stream) => stream,
        Err(error) if is_connection_missing(&error) || is_not_a_socket(socket_path) => {
            cleanup_host_state_file_if_matches(&runtime_dir, &state);
            return Err(AppError::Unsupported {
                reason: "headless host is not running".to_owned(),
            });
        }
        Err(error) => return Err(AppError::Io(error)),
    };
    let mut framed = framed_json::framed(stream);
    framed_json::send_json(
        &mut framed,
        &HostHelloRequest {
            token: state.token.clone(),
            protocol_version: HOST_PROTOCOL_VERSION,
            lane,
        },
    )
    .await?;
    let hello = time::timeout(
        DEVICE_RPC_TIMEOUT,
        framed_json::next_json::<_, HostHelloResponse>(&mut framed),
    )
    .await
    .map_err(|_| AppError::Unsupported {
        reason: "timed out waiting for headless host device RPC acceptance".to_owned(),
    })??
    .ok_or_else(|| AppError::Unsupported {
        reason: "headless host closed the device RPC handshake".to_owned(),
    })?;
    if !hello.accepted {
        return match classify_rejection(&hello) {
            super::lifecycle::HostConnectOutcome::Incompatible { message, .. }
            | super::lifecycle::HostConnectOutcome::Rejected(message) => {
                Err(AppError::Unsupported { reason: message })
            }
            _ => Err(AppError::Unsupported {
                reason: "headless host rejected device RPC".to_owned(),
            }),
        };
    }
    time::timeout(
        DEVICE_RPC_TIMEOUT,
        framed_json::next_json::<_, DeviceRpcResponse>(&mut framed),
    )
    .await
    .map_err(|_| AppError::Unsupported {
        reason: "timed out waiting for headless host device RPC result".to_owned(),
    })??
    .ok_or_else(|| AppError::Unsupported {
        reason: "headless host closed before device RPC completed".to_owned(),
    })
}
