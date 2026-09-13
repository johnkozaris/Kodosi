use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::{Client, Method, Url};
use serde::de::DeserializeOwned;
use serde_json::Value;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{Error, Result, crypto, invalid};
use crate::identity::{
    keys::DeviceKeys,
    oidc::{read_bounded, validate_url},
};

#[derive(Clone)]
pub(crate) struct Http {
    pub(crate) client: Client,
    base: Url,
    compatibility: Arc<tokio::sync::Mutex<Option<tokio::time::Instant>>>,
}

#[derive(Clone)]
pub(crate) struct Credentials {
    pub(crate) user_id: String,
    pub(crate) token: Zeroizing<String>,
    pub(crate) keys: Arc<DeviceKeys>,
    pub(crate) enrolled: bool,
    pub(crate) generation: u64,
    pub(crate) cancel: tokio_util::sync::CancellationToken,
}

impl Http {
    pub(crate) fn new(mut base: Url) -> Result<Self> {
        validate_url(&base)?;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            base,
            compatibility: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    pub(crate) fn url(&self, path: &str) -> Result<Url> {
        if path.starts_with('/') || path.contains("..") {
            return Err(invalid("Backend path escaped its configured root."));
        }
        let url = self
            .base
            .join(path)
            .map_err(|_| invalid("Invalid backend path."))?;
        if url.origin() != self.base.origin() || !url.path().starts_with(self.base.path()) {
            return Err(invalid("Backend path escaped its configured root."));
        }
        Ok(url)
    }

    pub(crate) fn websocket_url(&self, path: &str) -> Result<Url> {
        let mut url = self.url(path)?;
        let scheme = if url.scheme() == "https" { "wss" } else { "ws" };
        url.set_scheme(scheme)
            .map_err(|()| invalid("Invalid relay URL."))?;
        Ok(url)
    }

    pub(crate) async fn compatible(&self) -> Result<()> {
        let mut checked = self.compatibility.lock().await;
        if checked.is_some_and(|at| at.elapsed() < Duration::from_secs(30)) {
            return Ok(());
        }
        let response = self
            .client
            .get(self.url("health/ready")?)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        let value: Value = decode(response).await?;
        if value.get("apiContractVersion").and_then(Value::as_u64)
            != Some(u64::from(crate::protocol::BACKEND_API_VERSION))
            || value.get("authContractVersion").and_then(Value::as_u64)
                != Some(u64::from(crate::protocol::AUTH_VERSION))
        {
            *checked = None;
            return Err(invalid("This server requires a different Kodosi version."));
        }
        *checked = Some(tokio::time::Instant::now());
        drop(checked);
        Ok(())
    }

    pub(crate) async fn bearer<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> Result<T> {
        self.compatible().await?;
        let mut request = self
            .client
            .request(method, self.url(path)?)
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        decode(request.send().await?).await
    }

    pub(crate) async fn device<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        credentials: &Credentials,
        body: Option<Value>,
    ) -> Result<T> {
        if credentials.cancel.is_cancelled() {
            return Err(Error::Stale);
        }
        tokio::select! {
            biased;
            () = credentials.cancel.cancelled() => Err(Error::Stale),
            result = self.device_request(method, path, credentials, body) => result,
        }
    }

    async fn device_request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        credentials: &Credentials,
        body: Option<Value>,
    ) -> Result<T> {
        if !credentials.enrolled {
            return Err(Error::EnrollmentRequired);
        }
        let challenge: Value = self
            .bearer(
                Method::POST,
                "api/me/device-proofs/challenge",
                &credentials.token,
                None,
            )
            .await?;
        let challenge_id = challenge
            .get("challengeId")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| invalid("Backend device challenge is invalid."))?;
        let challenge_bytes = challenge
            .get("challengeBytes")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("Backend device challenge is absent."))?;
        let challenge_bytes = BASE64
            .decode(challenge_bytes)
            .map_err(|_| invalid("Backend challenge is not base64."))?;
        let bytes = body
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default();
        let hash = crypto::sha256_hex(&bytes);
        let url = self.url(path)?;
        let target = url.query().map_or_else(
            || url.path().to_owned(),
            |query| format!("{}?{query}", url.path()),
        );
        let preimage = crypto::device_http_request_proof_preimage(
            &credentials.user_id,
            &credentials.keys.device_id,
            &challenge_id,
            method.as_str(),
            &target,
            &hash,
            &challenge_bytes,
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &preimage)?;
        let mut request = self
            .client
            .request(method, self.url(path)?)
            .bearer_auth(credentials.token.as_str())
            .header("X-Kodosi-Device-Id", &credentials.keys.device_id)
            .header("X-Kodosi-Device-Challenge-Id", challenge_id.to_string())
            .header("X-Kodosi-Device-Signature", BASE64.encode(signature))
            .header("X-Kodosi-Body-Sha256", hash);
        if body.is_some() {
            request = request
                .header("Content-Type", "application/json")
                .body(bytes);
        }
        decode(request.send().await?).await
    }
}

async fn decode<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
    let status = response.status();
    let bytes = read_bounded(
        response,
        if status.is_success() {
            16 * 1024 * 1024
        } else {
            16 * 1024
        },
    )
    .await?;
    if !status.is_success() {
        let message = serde_json::from_slice::<Value>(&bytes)
            .ok()
            .and_then(|value| {
                value
                    .get("detail")
                    .or_else(|| value.get("message"))
                    .or_else(|| value.get("title"))
                    .or_else(|| value.get("error"))
                    .and_then(Value::as_str)
                    .map(|s| s.chars().take(512).collect())
            })
            .unwrap_or_else(|| "The request could not be completed.".into());
        return Err(Error::Backend {
            status: status.as_u16(),
            message,
        });
    }
    if bytes.is_empty() {
        return serde_json::from_value(Value::Null).map_err(Error::from);
    }
    serde_json::from_slice(&bytes).map_err(Error::from)
}
