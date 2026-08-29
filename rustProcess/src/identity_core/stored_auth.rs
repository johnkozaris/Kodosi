use std::fmt;

use zeroize::Zeroizing;

use crate::{
    Result,
    identity_core::{
        device_flow::DeviceFlowClient,
        token_refresh::{TokenRefreshOutcome, ensure_fresh_tokens, token_still_usable},
        token_store::{StoredTokens, TokenStore},
    },
};

#[derive(Debug, Clone)]
pub(crate) enum BackendAccessState {
    SignedOut,
    Ready {
        tokens: StoredTokens,
    },
    RequiresLogin(StoredAuthIssue),
    TemporarilyUnavailable {
        tokens: StoredTokens,
        reason: StoredAuthIssue,
    },
    StorageUnavailable(StoredAuthIssue),
}

#[derive(Debug, Clone)]
pub(crate) enum RefreshStoredAuthResult {
    Refreshed(StoredTokens),
    RequiresLogin(StoredAuthIssue),
    TemporarilyUnavailable(StoredAuthIssue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredAuthIssue {
    StorageUnavailable(String),
    RefreshRejected(String),
    RefreshTemporarilyUnavailable(String),
    ExpiredAndRefreshTemporarilyUnavailable(String),
    BoundToOtherBackend { bound: String, configured: String },
    NoStoredSession,
}

impl fmt::Display for StoredAuthIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StorageUnavailable(reason)
            | Self::RefreshRejected(reason)
            | Self::RefreshTemporarilyUnavailable(reason) => write!(f, "{reason}"),
            Self::ExpiredAndRefreshTemporarilyUnavailable(reason) => write!(
                f,
                "stored sign-in expired and automatic refresh is temporarily unavailable: {reason}"
            ),
            Self::BoundToOtherBackend { bound, configured } => write!(
                f,
                "stored sign-in belongs to {bound}, not configured backend {configured}"
            ),
            Self::NoStoredSession => write!(f, "no stored session is available"),
        }
    }
}

pub(crate) trait AccessTokenSink: Send {
    fn set_access_token(&mut self, token: Option<Zeroizing<String>>);
}

pub(crate) struct StoredAuthAccessor<'a, T: TokenStore> {
    access_tokens: &'a mut dyn AccessTokenSink,
    auth_client: &'a DeviceFlowClient,
    token_store: &'a T,
    subject: &'a str,
    refresh_skew_minutes: i64,
}

#[expect(
    clippy::future_not_send,
    reason = "single-owner orchestrator; never spawned across workers"
)]
impl<'a, T: TokenStore> StoredAuthAccessor<'a, T> {
    pub(crate) fn new(
        access_tokens: &'a mut dyn AccessTokenSink,
        auth_client: &'a DeviceFlowClient,
        token_store: &'a T,
        subject: &'a str,
        refresh_skew_minutes: i64,
    ) -> Self {
        Self {
            access_tokens,
            auth_client,
            token_store,
            subject,
            refresh_skew_minutes,
        }
    }

    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn ensure_backend_access(&mut self) -> Result<BackendAccessState> {
        let outcome = match ensure_fresh_tokens(
            self.auth_client,
            self.token_store,
            self.subject,
            self.refresh_skew_minutes,
            None,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.access_tokens.set_access_token(None);
                return Ok(BackendAccessState::StorageUnavailable(
                    StoredAuthIssue::StorageUnavailable(error.to_string()),
                ));
            }
        };

        match outcome {
            TokenRefreshOutcome::Fresh(tokens) | TokenRefreshOutcome::Refreshed(tokens) => {
                self.access_tokens
                    .set_access_token(Some(tokens.access_token.clone()));
                Ok(BackendAccessState::Ready { tokens })
            }
            TokenRefreshOutcome::NoStoredSession => {
                self.access_tokens.set_access_token(None);
                Ok(BackendAccessState::SignedOut)
            }
            TokenRefreshOutcome::RequiresLogin(reason) => {
                self.access_tokens.set_access_token(None);
                Ok(BackendAccessState::RequiresLogin(
                    StoredAuthIssue::RefreshRejected(reason),
                ))
            }
            TokenRefreshOutcome::TemporarilyUnavailable { stored, reason } => {
                if token_still_usable(stored.expires_at) {
                    self.access_tokens
                        .set_access_token(Some(stored.access_token.clone()));
                    Ok(BackendAccessState::TemporarilyUnavailable {
                        tokens: stored,
                        reason: StoredAuthIssue::RefreshTemporarilyUnavailable(reason),
                    })
                } else {
                    self.access_tokens.set_access_token(None);
                    Ok(BackendAccessState::RequiresLogin(
                        StoredAuthIssue::ExpiredAndRefreshTemporarilyUnavailable(reason),
                    ))
                }
            }
        }
    }

    #[tracing::instrument(skip_all, err)]
    pub(crate) async fn refresh_access_token(
        &mut self,
        revoked_access_token: Option<&str>,
    ) -> Result<RefreshStoredAuthResult> {
        let outcome = match ensure_fresh_tokens(
            self.auth_client,
            self.token_store,
            self.subject,
            self.refresh_skew_minutes,
            revoked_access_token,
        )
        .await
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.access_tokens.set_access_token(None);
                tracing::warn!(%error, "token store unavailable during token refresh");
                return Ok(RefreshStoredAuthResult::RequiresLogin(
                    StoredAuthIssue::StorageUnavailable(
                        "token store unavailable — sign in again".to_owned(),
                    ),
                ));
            }
        };

        match outcome {
            TokenRefreshOutcome::Fresh(tokens) | TokenRefreshOutcome::Refreshed(tokens) => {
                self.access_tokens
                    .set_access_token(Some(tokens.access_token.clone()));
                Ok(RefreshStoredAuthResult::Refreshed(tokens))
            }
            TokenRefreshOutcome::NoStoredSession => {
                self.access_tokens.set_access_token(None);
                Ok(RefreshStoredAuthResult::RequiresLogin(
                    StoredAuthIssue::NoStoredSession,
                ))
            }
            TokenRefreshOutcome::RequiresLogin(reason) => {
                self.access_tokens.set_access_token(None);
                Ok(RefreshStoredAuthResult::RequiresLogin(
                    StoredAuthIssue::RefreshRejected(reason),
                ))
            }
            TokenRefreshOutcome::TemporarilyUnavailable { reason, .. } => {
                self.access_tokens.set_access_token(None);
                Ok(RefreshStoredAuthResult::TemporarilyUnavailable(
                    StoredAuthIssue::RefreshTemporarilyUnavailable(reason),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use time::OffsetDateTime;

    use super::{AccessTokenSink, BackendAccessState, RefreshStoredAuthResult, StoredAuthAccessor};
    use crate::{
        AppError, Result,
        config::AuthConfig,
        identity_core::{
            device_flow::DeviceFlowClient,
            token_store::{StoredTokens, TokenStore},
        },
    };

    #[derive(Debug, Clone, Default)]
    struct MemoryTokenStore {
        stored: Option<StoredTokens>,
        fail_load: bool,
    }

    impl TokenStore for MemoryTokenStore {
        fn load(&self, _subject: &str) -> Result<Option<StoredTokens>> {
            if self.fail_load {
                return Err(AppError::Keychain {
                    reason: "keyring unavailable".to_owned(),
                });
            }
            Ok(self.stored.clone())
        }

        fn save(&self, _subject: &str, _tokens: &StoredTokens) -> Result<()> {
            Ok(())
        }

        fn clear(&self, _subject: &str) -> Result<()> {
            Ok(())
        }

        async fn acquire_refresh_lock(
            &self,
            _subject: &str,
        ) -> Result<Option<crate::identity_core::token_store::RefreshLock>> {
            Ok(None)
        }
    }

    #[derive(Debug, Default)]
    struct MemoryAccessTokenSink {
        access_token: Option<zeroize::Zeroizing<String>>,
    }

    impl MemoryAccessTokenSink {
        fn access_token(&self) -> Option<&str> {
            self.access_token.as_ref().map(|token| token.as_str())
        }
    }

    impl AccessTokenSink for MemoryAccessTokenSink {
        fn set_access_token(&mut self, token: Option<zeroize::Zeroizing<String>>) {
            self.access_token = token;
        }
    }

    fn test_auth_client() -> DeviceFlowClient {
        let mut config = AuthConfig::default();
        config.client_id = "kodosi".to_owned();
        DeviceFlowClient::new(&config)
            .unwrap_or_else(|error| panic!("auth client should construct: {error}"))
    }

    #[tokio::test]
    async fn ensure_backend_access_reports_signed_out_when_tokens_are_missing() {
        let mut access_tokens = MemoryAccessTokenSink::default();
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore::default();
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .ensure_backend_access()
            .await
            .unwrap_or_else(|error| panic!("ensure backend access should succeed: {error}"));

        std::assert_matches!(result, BackendAccessState::SignedOut);
    }

    #[tokio::test]
    async fn ensure_backend_access_reuses_fresh_tokens_without_refreshing() {
        let mut access_tokens = MemoryAccessTokenSink::default();
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore {
            stored: Some(StoredTokens::new(
                "access-token".to_owned(),
                Some("refresh-token".to_owned()),
                OffsetDateTime::now_utc() + time::Duration::hours(1),
            )),
            fail_load: false,
        };
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .ensure_backend_access()
            .await
            .unwrap_or_else(|error| panic!("ensure backend access should succeed: {error}"));

        match result {
            BackendAccessState::Ready { tokens } => {
                assert_eq!(tokens.access_token.as_str(), "access-token");
            }
            other => panic!("unexpected backend access state: {other:?}"),
        }
        assert_eq!(access_tokens.access_token(), Some("access-token"));
    }

    #[tokio::test]
    async fn ensure_backend_access_flags_storage_failures_without_crashing() {
        let mut access_tokens = MemoryAccessTokenSink::default();
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore {
            stored: None,
            fail_load: true,
        };
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .ensure_backend_access()
            .await
            .unwrap_or_else(|error| panic!("ensure backend access should succeed: {error}"));

        std::assert_matches!(
            result,
            BackendAccessState::StorageUnavailable(reason)
                if reason.to_string().contains("keyring unavailable")
        );
    }

    #[tokio::test]
    async fn refresh_requires_login_when_refresh_token_is_missing() {
        let mut access_tokens = MemoryAccessTokenSink::default();
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore {
            stored: Some(StoredTokens::new(
                "access-token".to_owned(),
                None,
                OffsetDateTime::now_utc() - time::Duration::minutes(1),
            )),
            fail_load: false,
        };
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .refresh_access_token(None)
            .await
            .unwrap_or_else(|error| {
                panic!("refresh should complete with a structured result: {error}")
            });

        std::assert_matches!(
            result,
            RefreshStoredAuthResult::RequiresLogin(reason)
                if reason.to_string().contains("cannot be refreshed")
        );
    }

    #[tokio::test]
    async fn ensure_backend_access_keeps_usable_token_when_refresh_is_temporarily_unavailable() {
        let mut access_tokens = MemoryAccessTokenSink::default();
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore {
            stored: Some(StoredTokens::new(
                "access-token".to_owned(),
                Some("refresh-token".to_owned()),
                OffsetDateTime::now_utc() + time::Duration::minutes(1),
            )),
            fail_load: false,
        };
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .ensure_backend_access()
            .await
            .unwrap_or_else(|error| panic!("ensure backend access should succeed: {error}"));

        std::assert_matches!(
            result,
            BackendAccessState::TemporarilyUnavailable { tokens, reason }
                if tokens.access_token.as_str() == "access-token"
                    && reason.to_string().contains("temporarily unavailable")
        );
        assert_eq!(access_tokens.access_token(), Some("access-token"));
    }

    #[tokio::test]
    async fn ensure_backend_access_clears_expired_token_when_refresh_is_temporarily_unavailable() {
        let mut access_tokens = MemoryAccessTokenSink {
            access_token: Some(zeroize::Zeroizing::new("stale-token".to_owned())),
        };
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore {
            stored: Some(StoredTokens::new(
                "expired-token".to_owned(),
                Some("refresh-token".to_owned()),
                OffsetDateTime::now_utc() - time::Duration::minutes(1),
            )),
            fail_load: false,
        };
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .ensure_backend_access()
            .await
            .unwrap_or_else(|error| panic!("ensure backend access should succeed: {error}"));

        std::assert_matches!(
            result,
            BackendAccessState::RequiresLogin(reason)
                if reason.to_string().contains("expired")
                    && reason.to_string().contains("temporarily unavailable")
        );
        assert_eq!(access_tokens.access_token(), None);
    }

    #[tokio::test]
    async fn refresh_access_token_clears_backend_token_when_refresh_is_temporarily_unavailable() {
        let mut access_tokens = MemoryAccessTokenSink {
            access_token: Some(zeroize::Zeroizing::new("stale-token".to_owned())),
        };
        let auth_client = test_auth_client();
        let token_store = MemoryTokenStore {
            stored: Some(StoredTokens::new(
                "access-token".to_owned(),
                Some("refresh-token".to_owned()),
                OffsetDateTime::now_utc() + time::Duration::minutes(1),
            )),
            fail_load: false,
        };
        let mut service =
            StoredAuthAccessor::new(&mut access_tokens, &auth_client, &token_store, "default", 5);

        let result = service
            .refresh_access_token(None)
            .await
            .unwrap_or_else(|error| {
                panic!("refresh should complete with a structured result: {error}")
            });

        std::assert_matches!(
            result,
            RefreshStoredAuthResult::TemporarilyUnavailable(reason)
                if reason.to_string().contains("temporarily unavailable")
        );
        assert_eq!(access_tokens.access_token(), None);
    }
}
