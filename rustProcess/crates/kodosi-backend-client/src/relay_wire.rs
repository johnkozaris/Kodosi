use std::borrow::Cow;

use serde::{Deserialize, Serialize};

pub(crate) const KEY_ROTATION_TYPE: &str = "key.rotation";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KeyRotationMessage<'a> {
    #[serde(rename = "type", borrow)]
    pub message_type: Cow<'a, str>,
    #[serde(borrow)]
    pub session_id: Cow<'a, str>,
    pub key_generation: u32,
}

impl<'a> KeyRotationMessage<'a> {
    pub(crate) fn borrowed(session_id: &'a str, key_generation: u32) -> Self {
        Self {
            message_type: Cow::Borrowed(KEY_ROTATION_TYPE),
            session_id: Cow::Borrowed(session_id),
            key_generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{KEY_ROTATION_TYPE, KeyRotationMessage};

    #[test]
    fn key_rotation_message_serializes_and_deserializes_authority_shape() {
        let message = KeyRotationMessage::borrowed("session-1", 7);

        let value = serde_json::to_value(&message).expect("key rotation should serialize");
        assert_eq!(
            value,
            json!({
                "type": "key.rotation",
                "sessionId": "session-1",
                "keyGeneration": 7
            })
        );

        let json = value.to_string();
        let parsed: KeyRotationMessage<'_> =
            serde_json::from_str(&json).expect("key rotation should deserialize");
        assert_eq!(parsed.message_type, KEY_ROTATION_TYPE);
        assert_eq!(parsed.session_id, "session-1");
        assert_eq!(parsed.key_generation, 7);
    }
}
