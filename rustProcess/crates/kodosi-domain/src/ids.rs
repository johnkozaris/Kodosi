use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
#[error("invalid {field}: {source}")]
pub struct SessionIdParseError {
    pub field: &'static str,
    pub source: uuid::Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SessionId(Uuid);

#[expect(
    clippy::new_without_default,
    reason = "session IDs should not be generated implicitly via Default"
)]
impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn short(self) -> String {
        let mut buffer = Uuid::encode_buffer();
        self.0.simple().encode_lower(&mut buffer)[..8].to_owned()
    }

    pub fn simple(self) -> String {
        let mut buffer = Uuid::encode_buffer();
        self.0.simple().encode_lower(&mut buffer).to_owned()
    }

    pub fn parse_field(value: &str, field: &'static str) -> Result<Self, SessionIdParseError> {
        Self::try_from(value).map_err(|source| SessionIdParseError { field, source })
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<&str> for SessionId {
    type Error = uuid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UserId(Uuid);

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<&str> for UserId {
    type Error = uuid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(Uuid::parse_str(value)?))
    }
}
