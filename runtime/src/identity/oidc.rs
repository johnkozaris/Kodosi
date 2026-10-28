use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio_util::sync::CancellationToken;
use zeroize::{Zeroize as _, Zeroizing};

use crate::network::{Error, Result, invalid};

#[derive(Clone)]
pub struct Oidc {
    client: Client,
    issuer: Url,
    client_id: String,
    scope: String,
}

pub const APP_REDIRECT: &str = "com.kodosi:/auth";

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    device_authorization_endpoint: String,
}

#[derive(Deserialize)]
pub struct DeviceLogin {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: Option<u64>,
    #[serde(skip)]
    token_endpoint: String,
    #[serde(skip)]
    verifier: Zeroizing<String>,
}

impl DeviceLogin {
    pub fn code_page(&self) -> String {
        format!("{}#{}", self.verification_uri, self.user_code)
    }
}

impl Drop for DeviceLogin {
    fn drop(&mut self) {
        self.device_code.zeroize();
    }
}

pub struct BrowserLogin {
    pub page: String,
    redirect_uri: String,
    state: Zeroizing<String>,
    verifier: Zeroizing<String>,
    token_endpoint: String,
    issuer: String,
}

impl BrowserLogin {
    pub fn owns(&self, answer: &Url) -> bool {
        let mut address = answer.clone();
        address.set_query(None);
        address.set_fragment(None);
        address.as_str() == self.redirect_uri
            && answer.query_pairs().any(|(key, value)| {
                key == "state"
                    && aws_lc_rs::constant_time::verify_slices_are_equal(
                        value.as_bytes(),
                        self.state.as_bytes(),
                    )
                    .is_ok()
            })
    }

    fn code(&self, answer: &Url) -> Result<Zeroizing<String>> {
        if !self.owns(answer) {
            return Err(invalid("This answer does not belong to the sign-in."));
        }
        let value = |name: &str| {
            answer
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| Zeroizing::new(value.into_owned()))
        };
        if value("iss")
            .is_some_and(|issuer| issuer.trim_end_matches('/') != self.issuer.trim_end_matches('/'))
        {
            return Err(invalid("The sign-in answer came from a different issuer."));
        }
        match value("error").as_deref().map(String::as_str) {
            None => {}
            Some("access_denied") => return Err(invalid("Sign-in was declined.")),
            Some(_) => {
                return Err(invalid(
                    "The sign-in server could not complete this request.",
                ));
            }
        }
        value("code")
            .filter(|code| !code.is_empty() && code.len() <= 4096)
            .ok_or_else(|| invalid("The sign-in answer has no code."))
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
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
    pub fn new(issuer: &str, client_id: String, scopes: &[String]) -> Result<Self> {
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
            &discovery.authorization_endpoint,
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
        let verifier = random_text()?;
        let challenge = challenge(&verifier);
        let form = [
            ("client_id", self.client_id.as_str()),
            ("scope", self.scope.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ];
        let response = self
            .client
            .post(&discovery.device_authorization_endpoint)
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        let mut login: DeviceLogin =
            serde_json::from_slice(&read_bounded(response, 256 * 1024).await?)?;
        let page = Url::parse(&login.verification_uri)
            .map_err(|_| invalid("Invalid verification URL."))?;
        validate_url(&page)?;
        if page.fragment().is_some()
            || login.user_code.is_empty()
            || login.user_code.len() > 32
            || !login
                .user_code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid("Sign-in challenge is invalid."));
        }
        if login.expires_in == 0 || login.expires_in > 3600 || login.device_code.len() > 4096 {
            return Err(invalid("Sign-in challenge is invalid."));
        }
        login.token_endpoint = discovery.token_endpoint;
        login.verifier = verifier;
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
                    ("code_verifier", login.verifier.as_str()),
                ]).send() => result?,
            };
            let status = response.status();
            let bytes = Zeroizing::new(read_bounded(response, 256 * 1024).await?);
            if status.is_success() {
                return tokens(&bytes, None);
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

    pub async fn browser(&self, redirect_uri: &str) -> Result<BrowserLogin> {
        let discovery = self.discover().await?;
        let verifier = random_text()?;
        let state = random_text()?;
        let challenge = challenge(&verifier);
        let mut page = Url::parse(&discovery.authorization_endpoint)
            .map_err(|_| invalid("Invalid sign-in endpoint."))?;
        page.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("scope", &self.scope)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(BrowserLogin {
            page: page.into(),
            redirect_uri: redirect_uri.to_owned(),
            state,
            verifier,
            token_endpoint: discovery.token_endpoint,
            issuer: discovery.issuer,
        })
    }

    pub async fn redeem(&self, login: &BrowserLogin, answer: &Url) -> Result<Tokens> {
        let code = login.code(answer)?;
        let response = self
            .client
            .post(&login.token_endpoint)
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code.as_str()),
                ("redirect_uri", login.redirect_uri.as_str()),
                ("client_id", self.client_id.as_str()),
                ("code_verifier", login.verifier.as_str()),
            ])
            .send()
            .await?;
        let status = response.status();
        let bytes = Zeroizing::new(read_bounded(response, 256 * 1024).await?);
        if status.is_success() {
            return tokens(&bytes, None);
        }
        if grant_ended(&bytes) {
            return Err(invalid("The sign-in has expired. Sign in again."));
        }
        Err(invalid(
            "The sign-in server could not complete this request.",
        ))
    }

    pub async fn refresh(&self, previous: &Tokens) -> Result<Tokens> {
        let refresh = previous.refresh_token.as_deref().ok_or(Error::SignedOut)?;
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
            .await?;
        let failure = response.error_for_status_ref().err();
        let bytes = Zeroizing::new(read_bounded(response, 256 * 1024).await?);
        let Some(failure) = failure else {
            return tokens(&bytes, Some(refresh));
        };
        if grant_ended(&bytes) {
            return Err(Error::SignedOut);
        }
        Err(failure.into())
    }
}

fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_text() -> Result<Zeroizing<String>> {
    let mut bytes = Zeroizing::new([0_u8; 32]);
    aws_lc_rs::rand::fill(bytes.as_mut()).map_err(|_| Error::Closed)?;
    Ok(Zeroizing::new(URL_SAFE_NO_PAD.encode(bytes.as_ref())))
}

fn grant_ended(reply: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(reply)
        .is_ok_and(|reply| reply["error"] == "invalid_grant")
}

fn tokens(bytes: &[u8], previous_refresh: Option<&str>) -> Result<Tokens> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::loopback::{self, Loopback};
    use serde_json::json;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    fn login(redirect_uri: &str) -> BrowserLogin {
        BrowserLogin {
            page: String::new(),
            redirect_uri: redirect_uri.to_owned(),
            state: Zeroizing::new("state-1".to_owned()),
            verifier: Zeroizing::new("verifier".to_owned()),
            token_endpoint: String::new(),
            issuer: "https://auth.example/realms/kodosi".to_owned(),
        }
    }

    fn answer(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    async fn request(stream: &mut tokio::net::TcpStream) -> (String, String) {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let read = stream.read(&mut chunk).await.unwrap();
            bytes.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let length = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")?
                            .parse()
                            .ok()
                    })
                    .unwrap_or(0_usize);
                if body.len() >= length || read == 0 {
                    return (head.to_owned(), body.to_owned());
                }
            }
        }
    }

    fn token() -> serde_json::Value {
        json!({"access_token":"access","refresh_token":"refresh","expires_in":60,"token_type":"Bearer"})
    }

    async fn issuer(
        replies: Vec<serde_json::Value>,
    ) -> (String, tokio::task::JoinHandle<Vec<Url>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let discovery = json!({
            "issuer": address,
            "authorization_endpoint": format!("{address}/auth"),
            "token_endpoint": format!("{address}/token"),
            "device_authorization_endpoint": format!("{address}/device"),
        });
        let server = tokio::spawn(async move {
            let mut forms = Vec::new();
            for reply in std::iter::once(discovery).chain(replies) {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (head, body) = request(&mut stream).await;
                if head.starts_with("POST") {
                    forms.push(answer(&format!("http://form/?{body}")));
                }
                let reply = reply.to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            forms
        });
        (address, server)
    }

    fn query(url: &Url, name: &str) -> String {
        url.query_pairs()
            .find(|(key, _)| key == name)
            .unwrap()
            .1
            .into_owned()
    }

    #[tokio::test]
    async fn a_browser_sign_in_proves_its_challenge_when_it_trades_the_code() {
        let (address, server) = issuer(vec![token()]).await;
        let oidc = Oidc::new(&address, "kodosi-app".into(), &["openid".to_owned()]).unwrap();
        let login = oidc.browser("http://127.0.0.1:9/callback").await.unwrap();
        let page = answer(&login.page);
        assert_eq!(page.path(), "/auth");
        assert_eq!(query(&page, "client_id"), "kodosi-app");
        assert_eq!(query(&page, "response_type"), "code");
        assert_eq!(query(&page, "redirect_uri"), "http://127.0.0.1:9/callback");
        assert_eq!(query(&page, "code_challenge_method"), "S256");
        let reply = answer(&format!(
            "http://127.0.0.1:9/callback?state={}&code=the-code&iss={address}",
            query(&page, "state")
        ));
        let tokens = oidc.redeem(&login, &reply).await.unwrap();
        assert_eq!(tokens.access_token, "access");
        let forms = server.await.unwrap();
        let form = &forms[0];
        assert_eq!(query(form, "grant_type"), "authorization_code");
        assert_eq!(query(form, "code"), "the-code");
        assert_eq!(query(form, "redirect_uri"), "http://127.0.0.1:9/callback");
        assert_eq!(
            challenge(&query(form, "code_verifier")),
            query(&page, "code_challenge")
        );
    }

    #[tokio::test]
    async fn a_device_sign_in_proves_its_challenge_when_it_asks_for_tokens() {
        let device = json!({
            "device_code": "device", "user_code": "ABCD-EFGH", "expires_in": 60, "interval": 1,
            "verification_uri": "http://127.0.0.1:9/device",
        });
        let (address, server) = issuer(vec![device, token()]).await;
        let oidc = Oidc::new(&address, "kodosi-app".into(), &["openid".to_owned()]).unwrap();
        let login = oidc.start().await.unwrap();
        let tokens = oidc.poll(login, &CancellationToken::new()).await.unwrap();
        assert_eq!(tokens.access_token, "access");
        let forms = server.await.unwrap();
        assert_eq!(query(&forms[0], "code_challenge_method"), "S256");
        assert_eq!(
            challenge(&query(&forms[1], "code_verifier")),
            query(&forms[0], "code_challenge")
        );
    }

    #[test]
    fn an_answer_belongs_only_to_the_address_and_state_of_its_sign_in() {
        let app = login(APP_REDIRECT);
        let login = login("http://127.0.0.1:9/callback");
        assert!(login.owns(&answer("http://127.0.0.1:9/callback?state=state-1&code=c")));
        assert!(login.owns(&answer(
            "http://127.0.0.1:9/callback?code=c&state=state-1#end"
        )));
        assert!(!login.owns(&answer("http://127.0.0.1:9/callback?state=state-2&code=c")));
        assert!(!login.owns(&answer("http://127.0.0.1:9/callback?code=c")));
        assert!(!login.owns(&answer("http://127.0.0.1:8/callback?state=state-1&code=c")));
        assert!(!login.owns(&answer("http://127.0.0.1:9/other?state=state-1&code=c")));
        assert!(app.owns(&answer("com.kodosi:/auth?state=state-1&code=c")));
        assert!(!app.owns(&answer("com.other:/auth?state=state-1&code=c")));
    }

    #[test]
    fn a_declined_or_foreign_answer_gives_no_code() {
        let login = login(APP_REDIRECT);
        let code = |text: &str| login.code(&answer(text)).map(|code| code.to_string());
        assert_eq!(
            code("com.kodosi:/auth?state=state-1&code=c&iss=https%3A%2F%2Fauth.example%2Frealms%2Fkodosi").unwrap(),
            "c"
        );
        assert_eq!(
            code("com.kodosi:/auth?state=state-1&error=access_denied")
                .unwrap_err()
                .to_string(),
            "Sign-in was declined."
        );
        assert!(
            code("com.kodosi:/auth?state=state-1&code=c&iss=https%3A%2F%2Fevil.example").is_err()
        );
        assert!(code("com.kodosi:/auth?state=state-1").is_err());
        assert!(code("com.kodosi:/auth?state=state-2&code=c").is_err());
    }

    async fn get(address: &str, target: &str) -> String {
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        stream
            .write_all(format!("GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes())
            .await
            .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).await.unwrap();
        reply
    }

    #[tokio::test]
    async fn the_loopback_takes_only_the_answer_of_its_sign_in() {
        let loopback = Loopback::bind().await.unwrap();
        let login = login(loopback.redirect_uri());
        let address = loopback
            .redirect_uri()
            .trim_start_matches("http://")
            .trim_end_matches("/callback")
            .to_owned();
        let waiting = async {
            let (answer, stream) = loopback.answer(&login).await.unwrap();
            loopback::finish(stream, Ok(())).await;
            answer
        };
        let browser = async {
            let _idle = tokio::net::TcpStream::connect(&address).await.unwrap();
            assert!(
                get(&address, "/favicon.ico")
                    .await
                    .starts_with("HTTP/1.1 404")
            );
            assert!(
                get(&address, "/callback?state=state-2&code=c")
                    .await
                    .starts_with("HTTP/1.1 404")
            );
            get(&address, "/callback?state=state-1&code=c").await
        };
        let (answer, page) = tokio::join!(waiting, browser);
        assert_eq!(query(&answer, "code"), "c");
        assert!(page.starts_with("HTTP/1.1 200 OK"));
        assert!(page.contains("You are signed in to Kodosi"));
    }

    #[test]
    fn the_code_page_carries_the_code_after_the_hash_only() {
        let login: DeviceLogin = serde_json::from_str(
            r#"{"device_code":"d","user_code":"WDJB-MJHT","verification_uri":"https://auth.example/realms/kodosi/device","verification_uri_complete":"https://auth.example/realms/kodosi/device?user_code=WDJB-MJHT","expires_in":900}"#,
        )
        .unwrap();
        assert_eq!(
            login.code_page(),
            "https://auth.example/realms/kodosi/device#WDJB-MJHT"
        );
    }

    #[test]
    fn refresh_retains_previous_refresh_token_when_server_omits_it() {
        let reply = br#"{"access_token":"new-access","expires_in":60,"token_type":"Bearer"}"#;
        let refreshed = tokens(reply, Some("retained-refresh")).unwrap();
        assert_eq!(refreshed.access_token, "new-access");
        assert_eq!(refreshed.refresh_token.as_deref(), Some("retained-refresh"));
        assert!(refreshed.expires_at > super::super::now_ms());
    }

    #[test]
    fn only_a_rejected_grant_ends_the_sign_in() {
        assert!(grant_ended(
            br#"{"error":"invalid_grant","error_description":"Token is not active"}"#
        ));
        assert!(!grant_ended(br#"{"error":"invalid_client"}"#));
        assert!(!grant_ended(b"<html>Bad Gateway</html>"));
    }
}
