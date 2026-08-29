use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async_with_config};

use crate::{
    BackendClientError, Result,
    endpoint::{
        build_ws_bearer_request, join_endpoint, parse_url, session_relay_ws_config, set_tcp_nodelay,
    },
};

pub type HostWebSocketStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Clone)]
pub struct HostWsClient {
    base_url: Option<Url>,
}

impl HostWsClient {
    pub fn new(base_url: Option<&str>) -> Result<Self> {
        Ok(Self {
            base_url: base_url.map(parse_url).transpose()?,
        })
    }

    pub async fn connect(
        &self,
        session_id: &str,
        access_token: &str,
        user_id: &str,
        device_id: &str,
        signing_pkcs8: &[u8],
        incarnation_id: &uuid::Uuid,
    ) -> Result<HostWebSocketStream> {
        let url = join_endpoint(
            self.base_url.as_ref(),
            &format!("hosts/{session_id}"),
            "backend.host_relay",
        )?;
        let request = build_ws_bearer_request(&url, access_token)?;
        let (mut stream, _) =
            connect_async_with_config(request, Some(session_relay_ws_config()), false)
                .await
                .map_err(BackendClientError::WebSocket)?;
        set_tcp_nodelay(&stream)?;
        prove_device_connection(
            &mut stream,
            user_id,
            device_id,
            signing_pkcs8,
            "host",
            Some(session_id),
            Some(incarnation_id),
        )
        .await?;
        Ok(stream)
    }
}

pub(crate) async fn prove_device_connection(
    stream: &mut HostWebSocketStream,
    user_id: &str,
    device_id: &str,
    signing_pkcs8: &[u8],
    purpose: &str,
    session_id: Option<&str>,
    incarnation_id: Option<&uuid::Uuid>,
) -> Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use tokio_tungstenite::tungstenite::Message;

    let message = stream
        .next()
        .await
        .ok_or_else(|| BackendClientError::Protocol {
            reason: "device proof challenge stream closed".to_owned(),
        })?
        .map_err(BackendClientError::WebSocket)?;
    let text = message.into_text().map_err(BackendClientError::WebSocket)?;
    let challenge: serde_json::Value = serde_json::from_str(&text)?;
    if challenge["type"] != "device.proofChallenge"
        || challenge["purpose"] != purpose
        || challenge["sessionId"].as_str() != session_id
    {
        return Err(BackendClientError::Protocol {
            reason: "device proof challenge context mismatch".to_owned(),
        });
    }
    let connection_id =
        challenge["connectionId"]
            .as_str()
            .ok_or_else(|| BackendClientError::Protocol {
                reason: "device proof challenge omitted connectionId".to_owned(),
            })?;
    let challenge_bytes = BASE64
        .decode(challenge["challenge"].as_str().unwrap_or_default())
        .map_err(|_| BackendClientError::Protocol {
            reason: "device proof challenge is not valid base64".to_owned(),
        })?;
    let signature = crate::crypto::sign_device_connection_proof(
        signing_pkcs8,
        user_id,
        device_id,
        connection_id,
        purpose,
        session_id,
        incarnation_id,
        &challenge_bytes,
    )?;
    stream
        .send(Message::Text(
            serde_json::json!({
                "type": "device.proof",
                "deviceId": device_id,
                "sessionId": session_id,
                "expectedIncarnationId": incarnation_id,
                "signature": BASE64.encode(signature),
            })
            .to_string()
            .into(),
        ))
        .await
        .map_err(BackendClientError::WebSocket)
}
