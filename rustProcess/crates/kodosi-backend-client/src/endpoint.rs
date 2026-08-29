use std::{fmt, str::FromStr};

use crate::{
    BackendClientError, Result,
    relay::{CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES, raw_batch_encrypted_frame_max_bytes},
};
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{
        client::IntoClientRequest,
        handshake::client::Request as ClientRequest,
        http::{HeaderValue, header::AUTHORIZATION},
        protocol::WebSocketConfig,
    },
};

fn session_relay_max_message_bytes() -> usize {
    CHECKPOINT_ENCRYPTED_FRAME_MAX_BYTES.max(raw_batch_encrypted_frame_max_bytes())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BackendOrigin(String);

impl BackendOrigin {
    pub(crate) fn from_base_url(base_url: &Url) -> Result<Self> {
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err(invalid_origin(base_url, "scheme must be http or https"));
        }
        if !base_url.username().is_empty() || base_url.password().is_some() {
            return Err(invalid_origin(base_url, "credentials are not allowed"));
        }
        if base_url.query().is_some() {
            return Err(invalid_origin(base_url, "query parameters are not allowed"));
        }
        if base_url.fragment().is_some() {
            return Err(invalid_origin(base_url, "fragments are not allowed"));
        }

        let host = base_url
            .host_str()
            .ok_or_else(|| invalid_origin(base_url, "host is required"))?;
        let host = if host.starts_with('[') && host.ends_with(']') {
            host.to_ascii_lowercase()
        } else if host.contains(':') {
            format!("[{}]", host.to_ascii_lowercase())
        } else {
            host.to_ascii_lowercase()
        };
        let port = base_url
            .port_or_known_default()
            .ok_or_else(|| invalid_origin(base_url, "scheme has no effective port"))?;
        Ok(Self(format!(
            "{}://{host}:{port}{}",
            base_url.scheme().to_ascii_lowercase(),
            base_url.path()
        )))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BackendOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for BackendOrigin {
    type Err = BackendClientError;

    fn from_str(value: &str) -> Result<Self> {
        let parsed = parse_url(value)?;
        let origin = Self::from_base_url(&parsed)?;
        if origin.as_str() != value {
            return Err(BackendClientError::InvalidUrl {
                value: value.to_owned(),
                reason: format!(
                    "stored backend origin is not canonical; expected `{}`",
                    origin.as_str()
                ),
            });
        }
        Ok(origin)
    }
}

impl TryFrom<String> for BackendOrigin {
    type Error = BackendClientError;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl Serialize for BackendOrigin {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BackendOrigin {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

fn invalid_origin(base_url: &Url, reason: &str) -> BackendClientError {
    BackendClientError::InvalidUrl {
        value: base_url.as_str().to_owned(),
        reason: format!("backend API base URL {reason}"),
    }
}

const USER_EVENTS_MAX_MESSAGE_BYTES: usize = 64 * 1024;

const RELAY_READ_BUFFER_BYTES: usize = 32 * 1024;

const USER_EVENTS_READ_BUFFER_BYTES: usize = 4 * 1024;

pub(crate) fn session_relay_ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(RELAY_READ_BUFFER_BYTES)
        .max_message_size(Some(session_relay_max_message_bytes()))
        .max_frame_size(Some(session_relay_max_message_bytes()))
}

pub(crate) fn user_events_ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(USER_EVENTS_READ_BUFFER_BYTES)
        .max_message_size(Some(USER_EVENTS_MAX_MESSAGE_BYTES))
        .max_frame_size(Some(USER_EVENTS_MAX_MESSAGE_BYTES))
}

pub(crate) fn build_ws_bearer_request(url: &Url, access_token: &str) -> Result<ClientRequest> {
    let mut request =
        url.as_str()
            .into_client_request()
            .map_err(|error| BackendClientError::Protocol {
                reason: format!("failed to build websocket request: {error}"),
            })?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}")).map_err(|error| {
            BackendClientError::Protocol {
                reason: format!("invalid bearer token: {error}"),
            }
        })?,
    );
    Ok(request)
}

pub(crate) fn set_tcp_nodelay(stream: &WebSocketStream<MaybeTlsStream<TcpStream>>) -> Result<()> {
    stream.get_ref().get_ref().set_nodelay(true)?;
    Ok(())
}

pub(crate) fn parse_url(value: &str) -> Result<Url> {
    Url::parse(value).map_err(|error| BackendClientError::InvalidUrl {
        value: value.to_owned(),
        reason: error.to_string(),
    })
}

pub(crate) fn join_endpoint(
    base_url: Option<&Url>,
    path: &str,
    service: &'static str,
) -> Result<Url> {
    let base = base_url.ok_or(BackendClientError::MissingConfig { key: service })?;
    base.join(path)
        .map_err(|error| BackendClientError::InvalidUrl {
            value: path.to_owned(),
            reason: error.to_string(),
        })
}

pub(crate) fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(encoded, "%{byte:02X}");
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{
        BackendOrigin, parse_url, session_relay_max_message_bytes, session_relay_ws_config,
        user_events_ws_config,
    };
    use crate::{BackendClientError, session_relay_authority::load_session_relay_authority};

    #[test]
    fn backend_origin_canonicalizes_authority_and_preserves_base_path() {
        let cases = [
            (
                "HTTPS://API.Example.Test/team/api///",
                "https://api.example.test:443/team/api///",
            ),
            ("http://API.Example.Test:80/", "http://api.example.test:80/"),
            (
                "https://API.Example.Test:8443/v5/",
                "https://api.example.test:8443/v5/",
            ),
            ("http://[2001:db8::1]/api/", "http://[2001:db8::1]:80/api/"),
        ];

        for (input, expected) in cases {
            let parsed = parse_url(input).expect("base URL");
            let origin = BackendOrigin::from_base_url(&parsed).expect("backend origin");
            assert_eq!(origin.as_str(), expected, "input: {input}");
        }
    }

    #[test]
    fn backend_origin_preserves_join_significant_trailing_slash() {
        let without_slash = BackendOrigin::from_base_url(
            &parse_url("https://example.test/api").expect("URL without slash"),
        )
        .expect("origin without slash");
        let with_slash = BackendOrigin::from_base_url(
            &parse_url("https://example.test/api/").expect("URL with slash"),
        )
        .expect("origin with slash");

        assert_ne!(without_slash, with_slash);
        assert_eq!(without_slash.as_str(), "https://example.test:443/api");
        assert_eq!(with_slash.as_str(), "https://example.test:443/api/");
    }

    #[test]
    fn persisted_backend_origin_requires_canonical_form_and_round_trips() {
        let origin: BackendOrigin = "https://api.example.test:443/api/"
            .parse()
            .expect("canonical origin");
        let encoded = serde_json::to_string(&origin).expect("serialize origin");
        let decoded: BackendOrigin = serde_json::from_str(&encoded).expect("deserialize origin");
        assert_eq!(decoded, origin);

        for value in [
            "HTTPS://api.example.test:443/api/",
            "https://API.example.test:443/api/",
            "https://api.example.test/api/",
        ] {
            assert!(value.parse::<BackendOrigin>().is_err(), "{value}");
        }
    }

    #[test]
    fn backend_origin_rejects_unsafe_url_components() {
        for input in [
            "https://user@example.test/api",
            "https://user:secret@example.test/api",
            "https://example.test/api?tenant=one",
            "https://example.test/api#deployment",
            "ftp://example.test:21/api",
            "file:///tmp/backend",
        ] {
            let parsed = parse_url(input).expect("syntactically valid URL");
            assert!(matches!(
                BackendOrigin::from_base_url(&parsed),
                Err(BackendClientError::InvalidUrl { .. })
            ));
        }
    }

    #[test]
    fn relay_message_ceiling_matches_the_protocol_manifest() {
        let authority = load_session_relay_authority();
        let manifest_ceiling = authority
            .messages
            .values()
            .map(|message| message.max_bytes)
            .max()
            .expect("session relay authority declares at least one message");

        assert_eq!(
            session_relay_max_message_bytes(),
            manifest_ceiling,
            "relay socket cap must exactly match the largest backend-authorized frame"
        );
    }

    #[test]
    fn ws_configs_bound_message_and_frame_size_below_the_tungstenite_defaults() {
        let defaults = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();

        for config in [session_relay_ws_config(), user_events_ws_config()] {
            assert!(config.max_message_size < defaults.max_message_size);
            assert!(config.max_frame_size < defaults.max_frame_size);
            assert!(config.read_buffer_size < defaults.read_buffer_size);
        }
    }
}
