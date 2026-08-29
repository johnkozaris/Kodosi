use crate::{
    AppError, AuthRequiredReason, Result,
    headless_host::{self, HeadlessHostAuthState},
    runtime::one_shot::{OneShotApp, OneShotBackendAccess},
};
use kodosi_domain::ids::UserId;

use super::BACKEND_API_CONFIG_KEY;

pub(in crate::cli) async fn ensure_remote_command_access(
    app: &mut OneShotApp,
    command_name: &str,
) -> Result<()> {
    remote_command_access_authority(app, command_name)
        .await
        .map(drop)
}

pub(in crate::cli) async fn ensure_remote_device_command_access(
    app: &mut OneShotApp,
    command_name: &str,
) -> Result<Option<UserId>> {
    remote_command_access_authority(app, command_name).await
}

async fn remote_command_access_authority(
    app: &mut OneShotApp,
    command_name: &str,
) -> Result<Option<UserId>> {
    let host = existing_host_auth(command_name).await?;
    if !host.ready && !app.owns_runtime_authority_for_cli() {
        return Err(AppError::Unsupported {
            reason: format!(
                "another Kodosi runtime owns this data root but exposes no headless command lane for `{command_name}`"
            ),
        });
    }
    if host.ready {
        let host_user_id = host.user_id.ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "the running host has not resolved its account identity for `{command_name}`"
            ),
        })?;
        let (bound_origin, bound_user_id) = app.stored_backend_account()?.ok_or_else(|| {
            AppError::Unsupported {
                reason: format!(
                    "shared credentials are not bound to a backend account for `{command_name}`; wait for the running host to finish sign-in reconciliation"
                ),
            }
        })?;
        if app.configured_backend_origin() != Some(&bound_origin) {
            return Err(AppError::Unsupported {
                reason: format!(
                    "backend origin mismatch while running `{command_name}`: shared credentials belong to {bound_origin}"
                ),
            });
        }
        ensure_account_match(Some(host_user_id), Some(bound_user_id), command_name)?;
        if !host.remote_operations_ready {
            return Err(AppError::Unsupported {
                reason: "remote operations are waiting for collaboration cleanup reconciliation"
                    .to_owned(),
            });
        }
        app.prepare_host_backed_projection(&bound_origin, host_user_id)
            .await?;
        return Ok(Some(host_user_id));
    }
    let access = app.backend_access().await?;

    match access {
        OneShotBackendAccess::Ready => match app.remote_command_access().await? {
            OneShotBackendAccess::Ready => Ok(None),
            other => classify_remote_access_failure(other, host, command_name).map(|()| None),
        },
        other => classify_remote_access_failure(other, host, command_name).map(|()| None),
    }
}

fn classify_remote_access_failure(
    access: OneShotBackendAccess,
    host: ExistingHostAuth,
    command_name: &str,
) -> Result<()> {
    match access {
        OneShotBackendAccess::Ready => Ok(()),
        OneShotBackendAccess::SignedOut if host.ready => Err(AppError::Unsupported {
            reason: format!(
                "the running host is signed in, but this CLI process cannot load shared credentials needed for `{command_name}`"
            ),
        }),
        OneShotBackendAccess::SignedOut => Err(AppError::Unsupported {
            reason: format!("not signed in — run `kodosi auth login` before `{command_name}`"),
        }),
        OneShotBackendAccess::RequiresLogin(reason) if host.ready => Err(AppError::Unsupported {
            reason: format!(
                "the running host is signed in, but this CLI process cannot refresh shared credentials for `{command_name}`: {reason}"
            ),
        }),
        OneShotBackendAccess::RequiresLogin(reason) => Err(AppError::Unsupported {
            reason: format!("{reason} — run `kodosi auth login` before `{command_name}`"),
        }),
        OneShotBackendAccess::StorageUnavailable(reason) if host.ready => {
            Err(AppError::Unsupported {
                reason: format!(
                    "the running host is signed in, but this CLI process cannot access shared token storage for `{command_name}`: {reason}"
                ),
            })
        }
        OneShotBackendAccess::StorageUnavailable(reason)
        | OneShotBackendAccess::CollaborationQuarantined(reason) => {
            Err(AppError::Unsupported { reason })
        }
        OneShotBackendAccess::BackendUnconfigured => Err(AppError::Unsupported {
            reason: format!(
                "{BACKEND_API_CONFIG_KEY} is not configured — `{command_name}` needs {BACKEND_API_CONFIG_KEY}"
            ),
        }),
    }
}

pub(in crate::cli) async fn resolve_local_command_identity(
    app: &mut OneShotApp,
    command_name: &str,
    cached_user_id: Option<UserId>,
) -> Result<UserId> {
    let host = existing_host_auth(command_name).await?;
    let restored_authenticated = app.restore_local_command_auth().await?;
    select_local_identity(
        host,
        restored_authenticated,
        app.current_user_id_via_auth(),
        cached_user_id,
        command_name,
    )
}

#[derive(Debug, Clone, Copy, Default)]
struct ExistingHostAuth {
    ready: bool,
    user_id: Option<UserId>,
    remote_operations_ready: bool,
}

async fn existing_host_auth(command_name: &str) -> Result<ExistingHostAuth> {
    let Some(mut client) = headless_host::connect_existing_host().await? else {
        return Ok(ExistingHostAuth::default());
    };
    let user_id = client.account_user_id();
    let snapshot = client.snapshot().await?;
    let remote_operations_ready = client.remote_operations_ready();
    ensure_host_auth_allows_remote_command(&snapshot.auth, command_name)?;
    Ok(ExistingHostAuth {
        ready: true,
        user_id: user_id.or(snapshot.auth_user_id),
        remote_operations_ready,
    })
}

fn select_local_identity(
    host: ExistingHostAuth,
    restored_authenticated: bool,
    restored_user_id: Option<UserId>,
    cached_user_id: Option<UserId>,
    command_name: &str,
) -> Result<UserId> {
    ensure_account_match(host.user_id, restored_user_id, command_name)?;
    ensure_account_match(host.user_id, cached_user_id, command_name)?;
    ensure_account_match(restored_user_id, cached_user_id, command_name)?;

    if !host.ready && !restored_authenticated {
        return Err(AppError::Unsupported {
            reason: format!("not signed in — run `kodosi auth login` before `{command_name}`"),
        });
    }

    host.user_id
        .or(restored_user_id)
        .or(cached_user_id)
        .ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "the signed-in account identity is unavailable for `{command_name}` — reconnect to the backend and retry"
            ),
        })
}

fn ensure_account_match(
    host_or_cached_user_id: Option<UserId>,
    restored_user_id: Option<UserId>,
    command_name: &str,
) -> Result<()> {
    if let (Some(expected), Some(restored)) = (host_or_cached_user_id, restored_user_id)
        && expected != restored
    {
        return Err(AppError::Unsupported {
            reason: format!(
                "account mismatch while running `{command_name}`: the active session belongs to {expected}, but shared credentials belong to {restored}"
            ),
        });
    }
    Ok(())
}

fn ensure_host_auth_allows_remote_command(
    auth: &HeadlessHostAuthState,
    command_name: &str,
) -> Result<()> {
    match auth {
        HeadlessHostAuthState::Ready => Ok(()),
        HeadlessHostAuthState::WaitingForApproval { .. } => Err(AppError::Unsupported {
            reason: format!(
                "login is waiting for device approval in the running host — finish it before `{command_name}`"
            ),
        }),
        HeadlessHostAuthState::RequiresLogin {
            reason: AuthRequiredReason::SignedOut,
        } => Err(AppError::Unsupported {
            reason: format!("not signed in — run `kodosi auth login` before `{command_name}`"),
        }),
        HeadlessHostAuthState::RequiresLogin {
            reason: AuthRequiredReason::Expired,
        } => Err(AppError::Unsupported {
            reason: format!(
                "the running host's backend session expired — run `kodosi auth login` before `{command_name}`"
            ),
        }),
        HeadlessHostAuthState::Unknown => Err(AppError::Unsupported {
            reason: format!(
                "the running host has not published auth state yet — retry `{command_name}` shortly"
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{ExistingHostAuth, select_local_identity};
    use kodosi_domain::ids::UserId;

    fn user(value: &str) -> UserId {
        UserId::try_from(value).expect("valid user id")
    }

    #[test]
    fn local_identity_uses_cached_account_while_backend_is_offline() {
        let cached = user("11111111-1111-1111-1111-111111111111");

        for command_name in ["kodosi agent describe", "kodosi agents"] {
            let resolved = select_local_identity(
                ExistingHostAuth::default(),
                true,
                None,
                Some(cached),
                command_name,
            )
            .expect("usable cached credentials should preserve local agent commands");

            assert_eq!(resolved, cached);
        }
    }

    #[test]
    fn local_identity_rejects_host_and_restored_account_mismatch() {
        let host = user("11111111-1111-1111-1111-111111111111");
        let restored = user("22222222-2222-2222-2222-222222222222");

        let error = select_local_identity(
            ExistingHostAuth {
                ready: true,
                user_id: Some(host),
                remote_operations_ready: true,
            },
            true,
            Some(restored),
            Some(host),
            "kodosi agent describe",
        )
        .expect_err("account mismatch must fail closed");

        assert!(error.to_string().contains("account mismatch"));
    }

    #[test]
    fn local_identity_rejects_cached_and_restored_account_mismatch() {
        let cached = user("11111111-1111-1111-1111-111111111111");
        let restored = user("22222222-2222-2222-2222-222222222222");

        let error = select_local_identity(
            ExistingHostAuth::default(),
            true,
            Some(restored),
            Some(cached),
            "kodosi agent describe",
        )
        .expect_err("a profile must not be rebound to another account");

        assert!(error.to_string().contains("account mismatch"));
    }
}
