use std::time::Duration;

use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use zeroize::{Zeroize as _, Zeroizing};

use crate::network::{Result, invalid};

#[derive(Clone)]
pub struct Oidc {
    client: Client,
    issuer: Url,
    client_id: String,
    scope: String,
    audience: Option<String>,
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    token_endpoint: String,
    device_authorization_endpoint: String,
}

#[derive(Deserialize)]
pub struct DeviceLogin {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    pub interval: Option<u64>,
    #[serde(skip)]
    token_endpoint: String,
}

impl Drop for DeviceLogin {
    fn drop(&mut self) {
        self.device_code.zeroize();
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub token_endpoint: String,
}
impl Drop for Tokens {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(refresh) = self.refresh_token.as_mut() {
            refresh.zeroize();
        }
    }
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    token_type: String,
}

impl Drop for TokenReply {
    fn drop(&mut self) {
        self.access_token.zeroize();
        if let Some(refresh) = self.refresh_token.as_mut() {
            refresh.zeroize();
        }
    }
}

impl Oidc {
    pub fn new(
        issuer: &str,
        client_id: String,
        scopes: &[String],
        audience: Option<String>,
    ) -> Result<Self> {
        let issuer =
            Url::parse(issuer).map_err(|_| invalid("The sign-in issuer is not configured."))?;
        validate_url(&issuer)?;
        if issuer.query().is_some() {
            return Err(invalid("OIDC issuer must not contain a query."));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            issuer,
            client_id,
            scope: scopes.join(" "),
            audience,
        })
    }

    async fn discover(&self) -> Result<Discovery> {
        let endpoint = format!(
            "{}/.well-known/openid-configuration",
            self.issuer.as_str().trim_end_matches('/')
        );
        let response = self.client.get(endpoint).send().await?.error_for_status()?;
        let bytes = read_bounded(response, 256 * 1024).await?;
        let discovery: Discovery = serde_json::from_slice(&bytes)?;
        if discovery.issuer.trim_end_matches('/') != self.issuer.as_str().trim_end_matches('/') {
            return Err(invalid("The sign-in server returned a different issuer."));
        }
        for endpoint in [
            &discovery.token_endpoint,
            &discovery.device_authorization_endpoint,
        ] {
            let endpoint =
                Url::parse(endpoint).map_err(|_| invalid("Invalid sign-in endpoint."))?;
            validate_url(&endpoint)?;
            if endpoint.origin() != self.issuer.origin() {
                return Err(invalid(
                    "Sign-in endpoints must belong to the configured issuer.",
                ));
            }
        }
        Ok(discovery)
    }

    pub async fn start(&self) -> Result<DeviceLogin> {
        let discovery = self.discover().await?;
        let mut form = vec![
            ("client_id", self.client_id.as_str()),
            ("scope", self.scope.as_str()),
        ];
        if let Some(audience) = &self.audience {
            form.push(("audience", audience));
        }
        let response = self
            .client
            .post(&discovery.device_authorization_endpoint)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        let mut login: DeviceLogin =
            serde_json::from_slice(&read_bounded(response, 256 * 1024).await?)?;
        for uri in [
            Some(login.verification_uri.as_str()),
            login.verification_uri_complete.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_url(&Url::parse(uri).map_err(|_| invalid("Invalid verification URL."))?)?;
        }
        if login.expires_in == 0 || login.expires_in > 3600 || login.device_code.len() > 4096 {
            return Err(invalid("Sign-in challenge is invalid."));
        }
        login.token_endpoint = discovery.token_endpoint;
        Ok(login)
    }

    pub async fn poll(&self, login: DeviceLogin, cancel: &CancellationToken) -> Result<Tokens> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(login.expires_in);
        let mut interval = login.interval.unwrap_or(5).clamp(1, 60);
        loop {
            tokio::select! {
                () = cancel.cancelled() => return Err(invalid("Sign-in cancelled.")),
                () = tokio::time::sleep_until(deadline) => return Err(invalid("Sign-in code expired.")),
                () = tokio::time::sleep(Duration::from_secs(interval)) => {}
            }
            let response = tokio::select! {
                () = cancel.cancelled() => return Err(invalid("Sign-in cancelled.")),
                result = self.client.post(&login.token_endpoint).form(&[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", login.device_code.as_str()), ("client_id", self.client_id.as_str()),
                ]).send() => result?,
            };
            let status = response.status();
            let bytes = Zeroizing::new(read_bounded(response, 256 * 1024).await?);
            if status.is_success() {
                return tokens(&bytes, &login.token_endpoint, None);
            }
            let error = serde_json::from_slice::<serde_json::Value>(&bytes)?;
            match error.get("error").and_then(serde_json::Value::as_str) {
                Some("authorization_pending") => {}
                Some("slow_down") => interval = (interval + 5).min(60),
                Some("access_denied") => return Err(invalid("Sign-in was declined.")),
                Some("expired_token") => return Err(invalid("Sign-in code expired.")),
                _ => {
                    return Err(invalid(
                        "The sign-in server could not complete this request.",
                    ));
                }
            }
        }
    }

    pub async fn refresh(&self, previous: &Tokens) -> Result<Tokens> {
        let refresh = previous
            .refresh_token
            .as_deref()
            .ok_or_else(|| invalid("Sign in again to continue."))?;
        let discovery = self.discover().await?;
        let response = self
            .client
            .post(&discovery.token_endpoint)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
                ("client_id", self.client_id.as_str()),
            ])
            .send()
            .await?
            .error_for_status()?;
        tokens(
            &Zeroizing::new(read_bounded(response, 256 * 1024).await?),
            &discovery.token_endpoint,
            Some(refresh),
        )
    }
}

fn tokens(bytes: &[u8], endpoint: &str, previous_refresh: Option<&str>) -> Result<Tokens> {
    let mut reply: TokenReply = serde_json::from_slice(bytes)?;
    if !reply.token_type.eq_ignore_ascii_case("bearer")
        || reply.access_token.is_empty()
        || reply.expires_in == 0
    {
        return Err(invalid(
            "The sign-in response did not contain a valid access token.",
        ));
    }
    Ok(Tokens {
        access_token: std::mem::take(&mut reply.access_token),
        refresh_token: reply
            .refresh_token
            .take()
            .or_else(|| previous_refresh.map(str::to_owned)),
        expires_at: super::now_ms().saturating_add(reply.expires_in.saturating_mul(1000)),
        token_endpoint: endpoint.to_owned(),
    })
}

pub(crate) fn validate_url(url: &Url) -> Result<()> {
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(invalid(
            "Connection URL must not contain credentials or fragments.",
        ));
    }
    if url.scheme() == "https" {
        return Ok(());
    }
    let loopback = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if url.scheme() == "http" && loopback {
        return Ok(());
    }
    Err(invalid(
        "Connections require HTTPS except for a local development server.",
    ))
}

pub(crate) async fn read_bounded(response: reqwest::Response, max: usize) -> Result<Vec<u8>> {
    use futures_util::StreamExt as _;
    if response
        .content_length()
        .is_some_and(|len| len > max as u64)
    {
        return Err(invalid("Server response exceeds its size limit."));
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > max {
            return Err(invalid("Server response exceeds its size limit."));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
