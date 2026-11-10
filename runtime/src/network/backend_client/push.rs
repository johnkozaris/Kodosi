use std::time::Duration;

use reqwest::Method;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::{BackendClient, Credentials, Error, Result, wire};

const PUSH_TOKEN: &str = "api/me/push-token";
const SIGN_OUT_WAIT: Duration = Duration::from_secs(3);

impl BackendClient {
    pub(super) async fn register_push(&self, args: &Value) -> Result<()> {
        let credentials = self.credentials()?;
        let mut body = json!({
            "platform": wire::text(args, "platform")?,
            "token": wire::text(args, "token")?,
        });
        if let Some(environment) = args.get("environment").and_then(Value::as_str) {
            body["environment"] = json!(environment);
        }
        available(
            self.inner
                .http
                .device(Method::PUT, PUSH_TOKEN, &credentials, Some(body))
                .await,
        )
    }

    pub(super) async fn unregister_push(&self) -> Result<()> {
        match self.credentials() {
            Ok(credentials) => self.remove_push_token(&credentials).await,
            Err(Error::SignedOut) => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub(super) async fn unregister_push_before_sign_out(&self) {
        let Some(mut credentials) = self
            .inner
            .credentials
            .read()
            .ok()
            .and_then(|credentials| credentials.clone())
        else {
            return;
        };
        credentials.cancel = CancellationToken::new();
        match tokio::time::timeout(SIGN_OUT_WAIT, self.remove_push_token(&credentials)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, "the push token of this device stays on the server after the sign-out");
            }
            Err(_) => {
                tracing::warn!(
                    "the push token of this device stays on the server, because it took too long"
                );
            }
        }
    }

    async fn remove_push_token(&self, credentials: &Credentials) -> Result<()> {
        if !credentials.enrolled {
            return Ok(());
        }
        available(
            self.inner
                .http
                .device(Method::DELETE, PUSH_TOKEN, credentials, None)
                .await,
        )
    }
}

fn available(result: Result<Value>) -> Result<()> {
    match result {
        Ok(_) | Err(Error::Backend { status: 404, .. }) => Ok(()),
        Err(error) => Err(error),
    }
}
