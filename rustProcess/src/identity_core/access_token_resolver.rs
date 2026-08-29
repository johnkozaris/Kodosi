use std::fmt;

use zeroize::Zeroizing;

use crate::{
    Result,
    identity_core::{
        device_flow::DeviceFlowClient,
        token_refresh::{TokenRefreshOutcome, ensure_fresh_tokens},
        token_store::PlatformTokenStore,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct AccessTokenResolver {
    auth_client: DeviceFlowClient,
    token_store: PlatformTokenStore,
    token_subject: String,
    refresh_skew_minutes: i64,
}

#[derive(Debug, Clone)]
pub(crate) enum AccessTokenState {
    Ready {
        access_token: Zeroizing<String>,
        refreshed: bool,
    },
    RequiresLogin {
        reason: AccessTokenIssue,
    },
    TemporarilyUnavailable {
        reason: AccessTokenIssue,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AccessTokenIssue {
    AuthNotConfigured,
    NoStoredSession,
    RefreshRejected(String),
    RefreshTemporarilyUnavailable(String),
}

impl fmt::Display for AccessTokenIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthNotConfigured => write!(f, "auth is not configured"),
            Self::NoStoredSession => write!(f, "no stored session is available"),
            Self::RefreshRejected(reason) | Self::RefreshTemporarilyUnavailable(reason) => {
                write!(f, "{reason}")
            }
        }
    }
}

impl AccessTokenResolver {
    pub(crate) fn new(
        auth_client: DeviceFlowClient,
        token_store: PlatformTokenStore,
        token_subject: String,
        refresh_skew_minutes: i64,
    ) -> Self {
        Self {
            auth_client,
            token_store,
            token_subject,
            refresh_skew_minutes,
        }
    }

    pub(crate) async fn ensure_access_token(&self) -> Result<AccessTokenState> {
        self.ensure_access_token_with_hint(None).await
    }

    pub(crate) async fn ensure_access_token_after_revocation(
        &self,
        revoked_access_token: &str,
    ) -> Result<AccessTokenState> {
        self.ensure_access_token_with_hint(Some(revoked_access_token))
            .await
    }

    async fn ensure_access_token_with_hint(
        &self,
        revoked_access_token: Option<&str>,
    ) -> Result<AccessTokenState> {
        if !self.auth_client.is_configured() {
            return Ok(AccessTokenState::RequiresLogin {
                reason: AccessTokenIssue::AuthNotConfigured,
            });
        }
        match ensure_fresh_tokens(
            &self.auth_client,
            &self.token_store,
            &self.token_subject,
            self.refresh_skew_minutes,
            revoked_access_token,
        )
        .await?
        {
            TokenRefreshOutcome::Fresh(tokens) => Ok(AccessTokenState::Ready {
                access_token: tokens.access_token,
                refreshed: false,
            }),
            TokenRefreshOutcome::Refreshed(tokens) => Ok(AccessTokenState::Ready {
                access_token: tokens.access_token,
                refreshed: true,
            }),
            TokenRefreshOutcome::NoStoredSession => Ok(AccessTokenState::RequiresLogin {
                reason: AccessTokenIssue::NoStoredSession,
            }),
            TokenRefreshOutcome::RequiresLogin(reason) => Ok(AccessTokenState::RequiresLogin {
                reason: AccessTokenIssue::RefreshRejected(reason),
            }),
            TokenRefreshOutcome::TemporarilyUnavailable { reason, .. } => {
                Ok(AccessTokenState::TemporarilyUnavailable {
                    reason: AccessTokenIssue::RefreshTemporarilyUnavailable(reason),
                })
            }
        }
    }
}
