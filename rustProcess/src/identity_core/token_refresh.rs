use std::sync::LazyLock;

use subtle::ConstantTimeEq;
use time::{Duration, OffsetDateTime};
use tokio::sync::Mutex;

use crate::{
    AppError, Result,
    identity_core::{
        device_flow::{DeviceFlowClient, token_expires_at},
        token_store::{StoredTokens, TokenStore},
    },
};

static TOKEN_REFRESH_GATE: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone)]
pub(crate) enum TokenRefreshOutcome {
    Fresh(StoredTokens),
    Refreshed(StoredTokens),
    NoStoredSession,
    RequiresLogin(String),
    TemporarilyUnavailable {
        stored: StoredTokens,
        reason: String,
    },
}

pub(crate) fn token_needs_refresh(expires_at: OffsetDateTime, skew_minutes: i64) -> bool {
    let skew = Duration::minutes(skew_minutes);
    expires_at <= (OffsetDateTime::now_utc() + skew)
}

pub(crate) fn token_still_usable(expires_at: OffsetDateTime) -> bool {
    expires_at > (OffsetDateTime::now_utc() + Duration::seconds(5))
}

#[expect(
    clippy::future_not_send,
    reason = "single-owner orchestrator; never spawned across workers"
)]
#[tracing::instrument(skip_all, err)]
pub(crate) async fn ensure_fresh_tokens<T: TokenStore>(
    auth_client: &DeviceFlowClient,
    token_store: &T,
    subject: &str,
    refresh_skew_minutes: i64,
    revoked_access_token: Option<&str>,
) -> Result<TokenRefreshOutcome> {
    let _gate = TOKEN_REFRESH_GATE.lock().await;

    let _file_lock = token_store.acquire_refresh_lock(subject).await?;

    let Some(stored) = token_store.load(subject)? else {
        return Ok(TokenRefreshOutcome::NoStoredSession);
    };

    let must_refresh = match revoked_access_token {
        Some(revoked) => stored
            .access_token
            .as_str()
            .as_bytes()
            .ct_eq(revoked.as_bytes())
            .into(),
        None => token_needs_refresh(stored.expires_at, refresh_skew_minutes),
    };

    if !must_refresh {
        return Ok(TokenRefreshOutcome::Fresh(stored));
    }

    let Some(refresh_token) = stored.refresh_token.as_deref() else {
        return Ok(TokenRefreshOutcome::RequiresLogin(
            "stored session cannot be refreshed — sign in again".to_owned(),
        ));
    };

    match auth_client.refresh(refresh_token).await {
        Ok(tokens) => {
            let carried_refresh = tokens
                .refresh_token
                .or_else(|| stored.refresh_token.as_ref().map(|t| (**t).clone()));
            let mut refreshed = StoredTokens::new(
                tokens.access_token,
                carried_refresh,
                token_expires_at(tokens.expires_in),
            );
            refreshed.backend_origin.clone_from(&stored.backend_origin);
            refreshed
                .backend_user_id
                .clone_from(&stored.backend_user_id);
            token_store.save(subject, &refreshed)?;
            Ok(TokenRefreshOutcome::Refreshed(refreshed))
        }
        Err(AppError::AuthRejected { reason }) => Ok(TokenRefreshOutcome::RequiresLogin(format!(
            "stored sign-in expired ({reason}) — sign in again"
        ))),
        Err(error) => Ok(TokenRefreshOutcome::TemporarilyUnavailable {
            stored,
            reason: format!("sign-in refresh is temporarily unavailable: {error}"),
        }),
    }
}
