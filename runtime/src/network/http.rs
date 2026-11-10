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
use crate::text::Text;

#[derive(Clone)]
pub(crate) struct Http {
    pub(crate) client: Client,
    pub(crate) websocket_tls: Arc<rustls::ClientConfig>,
    base: Url,
    compatibility: Arc<tokio::sync::Mutex<Option<tokio::time::Instant>>>,
    device_session: Arc<tokio::sync::Mutex<Option<DeviceSession>>>,
}

struct DeviceSession {
    user_id: String,
    device_id: String,
    token: Zeroizing<String>,
}

#[derive(Clone)]
pub(crate) struct Credentials {
    pub(crate) user_id: String,
    pub(crate) display_name: String,
    pub(crate) token: Zeroizing<String>,
    pub(crate) keys: Arc<DeviceKeys>,
    pub(crate) enrolled: bool,
    pub(crate) notice: Option<Text>,
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
        let websocket_tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::aws_lc_rs::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .and_then(rustls_platform_verifier::BuilderVerifierExt::with_platform_verifier)
        .map_err(|_| invalid("Kodosi could not load the system certificate verifier."))?
        .with_no_client_auth();
        Ok(Self {
            client,
            websocket_tls: Arc::new(websocket_tls),
            base,
            compatibility: Arc::new(tokio::sync::Mutex::new(None)),
            device_session: Arc::new(tokio::sync::Mutex::new(None)),
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
            .map_err(|()| invalid("Invalid connection service URL."))?;
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
        *checked = None;
        let reported = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("The server did not report its Kodosi versions."))
        };
        supported(
            crate::protocol::BACKEND_API_VERSION,
            reported("oldestApiContractVersion")?,
            reported("apiContractVersion")?,
        )?;
        let auth = reported("authContractVersion")?;
        supported(crate::protocol::AUTH_VERSION, auth, auth)?;
        supported(
            crate::protocol::TERMINAL_CONNECTION_VERSION,
            reported("oldestTerminalConnectionProtocolVersion")?,
            reported("terminalConnectionProtocolVersion")?,
        )?;
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

    pub(crate) async fn tagged<T: DeserializeOwned>(
        &self,
        path: &str,
        credentials: &Credentials,
        device: bool,
        known: Option<&str>,
    ) -> Result<Option<(T, String)>> {
        self.compatible().await?;
        let mut session = if device {
            Some(self.device_session(credentials).await?)
        } else {
            None
        };
        let mut retried = false;
        loop {
            let mut request = self
                .client
                .get(self.url(path)?)
                .bearer_auth(credentials.token.as_str());
            if let Some(session) = &session {
                request = request
                    .header("X-Kodosi-Device-Id", &credentials.keys.device_id)
                    .header("X-Kodosi-Device-Session", session.as_str());
            }
            if let Some(known) = known {
                request = request.header(reqwest::header::IF_NONE_MATCH, known);
            }
            let response = request.send().await?;
            if response.status() == reqwest::StatusCode::NOT_MODIFIED {
                return Ok(None);
            }
            if !retried
                && response.status().as_u16() == 428
                && let Some(refused) = session.take()
            {
                retried = true;
                self.forget_device_session(&refused).await;
                session = Some(self.device_session(credentials).await?);
                continue;
            }
            let tag = response
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|tag| tag.to_str().ok())
                .unwrap_or_default()
                .to_owned();
            return Ok(Some((decode(response).await?, tag)));
        }
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
        self.compatible().await?;
        let session = self.device_session(credentials).await?;
        match self
            .with_session(&method, path, credentials, body.as_ref(), &session)
            .await
        {
            Err(Error::Backend { status: 428, .. }) => {
                self.forget_device_session(&session).await;
                let session = self.device_session(credentials).await?;
                self.with_session(&method, path, credentials, body.as_ref(), &session)
                    .await
            }
            result => result,
        }
    }

    async fn with_session<T: DeserializeOwned>(
        &self,
        method: &Method,
        path: &str,
        credentials: &Credentials,
        body: Option<&Value>,
        session: &str,
    ) -> Result<T> {
        let mut request = self
            .client
            .request(method.clone(), self.url(path)?)
            .bearer_auth(credentials.token.as_str())
            .header("X-Kodosi-Device-Id", &credentials.keys.device_id)
            .header("X-Kodosi-Device-Session", session);
        if let Some(body) = body {
            request = request.json(body);
        }
        decode(request.send().await?).await
    }

    pub(crate) async fn device_session(
        &self,
        credentials: &Credentials,
    ) -> Result<Zeroizing<String>> {
        let mut held = self.device_session.lock().await;
        if let Some(session) = held.as_ref().filter(|session| {
            session.user_id == credentials.user_id
                && session.device_id == credentials.keys.device_id
        }) {
            return Ok(session.token.clone());
        }
        let challenge: Value = self
            .bearer(
                Method::POST,
                "api/me/devices/challenge",
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
            .and_then(|bytes| BASE64.decode(bytes).ok())
            .ok_or_else(|| invalid("Backend device challenge is absent."))?;
        let preimage = crypto::device_session_proof_preimage(
            &credentials.user_id,
            &credentials.keys.device_id,
            &challenge_id,
            &challenge_bytes,
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &preimage)?;
        let opened: Value = self
            .bearer(
                Method::POST,
                "api/me/device-sessions",
                &credentials.token,
                Some(serde_json::json!({
                    "deviceId": credentials.keys.device_id,
                    "challengeId": challenge_id,
                    "signature": BASE64.encode(signature),
                })),
            )
            .await?;
        let token = Zeroizing::new(
            opened
                .get("session")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("Backend device session is absent."))?
                .to_owned(),
        );
        *held = Some(DeviceSession {
            user_id: credentials.user_id.clone(),
            device_id: credentials.keys.device_id.clone(),
            token: token.clone(),
        });
        drop(held);
        Ok(token)
    }

    pub(crate) async fn forget_device_session(&self, token: &str) {
        let mut held = self.device_session.lock().await;
        if held
            .as_ref()
            .is_some_and(|session| session.token.as_str() == token)
        {
            *held = None;
        }
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
    if status == reqwest::StatusCode::GONE {
        return Err(Error::AccountDeleted);
    }
    if status == reqwest::StatusCode::LOCKED {
        return Err(Error::AccountBanned);
    }
    if !status.is_success() {
        let body = serde_json::from_slice::<Value>(&bytes).unwrap_or_default();
        return Err(Error::Backend {
            status: status.as_u16(),
            message: backend_message(status.as_u16(), &body),
        });
    }
    if bytes.is_empty() {
        return serde_json::from_value(Value::Null).map_err(Error::from);
    }
    serde_json::from_slice(&bytes).map_err(Error::from)
}

pub(crate) fn supported(own: u32, oldest: u64, current: u64) -> Result<()> {
    if u64::from(own) < oldest {
        Err(Error::UpdateRequired)
    } else if u64::from(own) > current {
        Err(Error::ServerOutdated)
    } else {
        Ok(())
    }
}

fn backend_message(status: u16, body: &Value) -> Text {
    let known = || {
        body.get("error")
            .and_then(Value::as_str)
            .and_then(known_refusal)
    };
    match status {
        401 => text!("Your sign-in has expired. Sign in again."),
        404 => known().unwrap_or_else(|| text!("That no longer exists on the Kodosi server.")),
        429 => known().unwrap_or_else(|| text!("Too many requests. Wait a minute and try again.")),
        500..=599 => text!("The Kodosi server had a problem. Try again in a moment."),
        _ => backend_text(body)
            .unwrap_or_else(|| text!("The Kodosi server could not complete the request.")),
    }
}

pub(crate) fn backend_text(body: &Value) -> Option<Text> {
    if let (Some(limit), Some(most)) = (
        body.get("limit").and_then(Value::as_str),
        body.get("most").and_then(Value::as_u64),
    ) && let Some(text) = plan_limit(limit, most)
    {
        return Some(text);
    }
    let words: String = body.get("error")?.as_str()?.chars().take(512).collect();
    if words.is_empty() {
        return None;
    }
    Some(known_refusal(&words).unwrap_or_else(|| Text::plain(words)))
}

fn known_refusal(words: &str) -> Option<Text> {
    backend_refusals()
        .into_iter()
        .find(|text| text.words() == words)
}

fn plan_limit(limit: &str, most: u64) -> Option<Text> {
    Some(match limit {
        "rooms" => text!(
            "You own the most rooms that you can own ({most}). Delete a room to make a new one.",
            most = most
        ),
        "roomPeople" => text!(
            "This room has the most people that it can have ({most}), with its open invitations.",
            most = most
        ),
        "sharePeople" => text!(
            "You chose more people than a terminal can have ({most}).",
            most = most
        ),
        "friends" => text!(
            "You have the most friends that you can have ({most}). Remove a friend first.",
            most = most
        ),
        "friendRequests" => text!(
            "You have the most open friend requests that you can have ({most}). Wait for an answer, or cancel a request.",
            most = most
        ),
        "devices" => text!(
            "This account has the most devices that it can have ({most}). Remove a device in Settings on one of your devices, then try again.",
            most = most
        ),
        "devicesRecovering" => text!(
            "This account has the most devices that it can have ({most}). Remove a device in Settings on one of your devices, then try again. If you have no other device, start fresh on this device.",
            most = most
        ),
        _ => return None,
    })
}

fn backend_refusals() -> Vec<Text> {
    vec![
        text!("An existing trusted device must approve this device."),
        text!("Another Kodosi account has this user name."),
        text!("Approve this device before you use shared terminals."),
        text!("Choose another person."),
        text!("Delete your account on the account page of the sign-in service."),
        text!("Dosi already has a conversation."),
        text!("Invalid device code."),
        text!("Invite an accepted friend."),
        text!("Leave a room before you join or make another."),
        text!("Leave the room to leave its shared terminals."),
        text!("No Kodosi account has this user name."),
        text!("Only the computer that hosts a terminal can add it."),
        text!("Only the computer that hosts this terminal can change it."),
        text!("Only the owner can change this terminal."),
        text!("Refresh the room before changing its members."),
        text!("Remove the current recovery key first."),
        text!("Room keys changed; refresh before posting."),
        text!("Room keys changed; refresh before retrying."),
        text!("Room membership changed; refresh before retrying."),
        text!("Share a terminal only with accepted friends."),
        text!("Sign in again to delete your account."),
        text!("Sign in again to start fresh on this device."),
        text!("That computer is busy. Try again in a moment."),
        text!("That computer is not online."),
        text!("The Kodosi server is busy. Open the terminal again in a moment."),
        text!("The approval has already changed."),
        text!("The connection is too slow."),
        text!("The current state changed; refresh before retrying."),
        text!("The device challenge expired or was already used."),
        text!("The device connection closed."),
        text!("The friend list changed on another device."),
        text!("The host is offline."),
        text!("The owner cannot leave the room. Delete it instead."),
        text!("The requested change conflicts with current state."),
        text!("The room has reached its item limit."),
        text!("The room of this terminal changed; refresh before retrying."),
        text!("The server could not check this terminal; try again."),
        text!("The server has reached its limit of terminal connections."),
        text!("The sharing of this terminal changed; refresh before retrying."),
        text!("The sign-in service gave no user name that Kodosi can use."),
        text!("There are too many terminals to list."),
        text!("This Kodosi account was deleted."),
        text!("This account has no recovery key."),
        text!("This device was removed."),
        text!("This invitation is no longer available."),
        text!("This operation is not authorized."),
        text!("This person does not take friend requests from you."),
        text!("This person has the most friends that they can have."),
        text!("This person has too many open friend requests. Try again later."),
        text!("This person has too many open room invitations."),
        text!(
            "This room is full, because its tasks and repositories use all of its space. Make a new room to continue."
        ),
        text!("This room item changed; refresh before retrying."),
        text!("This terminal is no longer available."),
        text!("This terminal is not shared with you."),
        text!(
            "This terminal is shared with its room. Remove it from the room to share it with friends."
        ),
        text!("This terminal was replaced."),
        text!("Too many connected participants."),
        text!("Too many device connections."),
        text!(
            "Too many of your terminals wait to close on offline computers. Remove the computers that do not come back."
        ),
        text!("Too many open views."),
        text!("Too many pending device approvals."),
        text!("Unblock this person first."),
        text!("You cannot leave your own terminal."),
        text!("You have too many terminals."),
        text!("You tried too many user names. You can try another one in 30 minutes."),
    ]
}

#[cfg(test)]
pub(crate) fn current_versions() -> Value {
    serde_json::json!({
        "apiContractVersion": crate::protocol::BACKEND_API_VERSION,
        "oldestApiContractVersion": crate::protocol::BACKEND_API_VERSION,
        "authContractVersion": crate::protocol::AUTH_VERSION,
        "terminalConnectionProtocolVersion": crate::protocol::TERMINAL_CONNECTION_VERSION,
        "oldestTerminalConnectionProtocolVersion": crate::protocol::TERMINAL_CONNECTION_VERSION,
    })
}

#[cfg(test)]
mod refusal_tests {
    use super::*;
    use crate::text::Language;

    #[test]
    fn a_refusal_of_the_server_reads_in_the_language_of_the_app() {
        let known =
            backend_text(&serde_json::json!({"error": "This terminal is no longer available."}))
                .unwrap();
        assert_eq!(
            known.in_language(Language::German),
            "Dieses Terminal ist nicht mehr verfügbar."
        );
        let unknown =
            backend_text(&serde_json::json!({"error": "Invalid room signature."})).unwrap();
        assert!(
            unknown
                .in_language(Language::German)
                .ends_with(" (Invalid room signature.)")
        );
    }

    #[test]
    fn a_missing_or_limited_request_keeps_only_the_server_words_that_the_app_knows() {
        let missing = backend_message(
            404,
            &serde_json::json!({"error": "No Kodosi account has this user name."}),
        );
        assert_eq!(missing.words(), "No Kodosi account has this user name.");
        let limited = backend_message(
            429,
            &serde_json::json!({"error": "Too many requests. Wait a moment and try again."}),
        );
        assert_eq!(
            limited.words(),
            "Too many requests. Wait a minute and try again."
        );
    }
}
