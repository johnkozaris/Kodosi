use crate::{
    AppError,
    identity_core::access_token_resolver::{
        AccessTokenIssue, AccessTokenResolver, AccessTokenState,
    },
};
use kodosi_backend_client::auth::{
    BackendAccessTokenFuture, BackendAccessTokenIssue, BackendAccessTokenProvider,
    BackendAccessTokenState, BackendAuthError, BackendAuthProvider,
};

#[derive(Debug, Clone)]
pub(crate) struct IdentityBackendAccess {
    session: AccessTokenResolver,
}

impl IdentityBackendAccess {
    pub(crate) fn provider(session: AccessTokenResolver) -> BackendAuthProvider {
        BackendAuthProvider::new(Self { session })
    }
}

impl BackendAccessTokenProvider for IdentityBackendAccess {
    fn access_token(&self, revoked_access_token: Option<String>) -> BackendAccessTokenFuture<'_> {
        Box::pin(async move {
            let state = match revoked_access_token {
                Some(revoked) => {
                    self.session
                        .ensure_access_token_after_revocation(&revoked)
                        .await
                }
                None => self.session.ensure_access_token().await,
            }
            .map_err(backend_auth_error)?;

            Ok(match state {
                AccessTokenState::Ready {
                    access_token,
                    refreshed,
                } => BackendAccessTokenState::Ready {
                    access_token,
                    refreshed,
                },
                AccessTokenState::RequiresLogin { reason } => {
                    BackendAccessTokenState::RequiresLogin {
                        reason: backend_access_issue(reason),
                    }
                }
                AccessTokenState::TemporarilyUnavailable { reason } => {
                    BackendAccessTokenState::TemporarilyUnavailable {
                        reason: backend_access_issue(reason),
                    }
                }
            })
        })
    }
}

fn backend_auth_error(error: AppError) -> BackendAuthError {
    BackendAuthError::from_error(error)
}

fn backend_access_issue(issue: AccessTokenIssue) -> BackendAccessTokenIssue {
    match issue {
        AccessTokenIssue::AuthNotConfigured => BackendAccessTokenIssue::AuthNotConfigured,
        AccessTokenIssue::NoStoredSession => BackendAccessTokenIssue::NoStoredSession,
        AccessTokenIssue::RefreshRejected(reason) => {
            BackendAccessTokenIssue::RefreshRejected(reason)
        }
        AccessTokenIssue::RefreshTemporarilyUnavailable(reason) => {
            BackendAccessTokenIssue::RefreshTemporarilyUnavailable(reason)
        }
    }
}
