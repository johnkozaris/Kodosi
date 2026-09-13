use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Result, TerminalControl, crypto, invalid};

mod decoder;
mod freshness;
pub(crate) use decoder::FrameDecoder;
pub(crate) use freshness::{CaptureIdentity, FreshFrames, frame_hash};

pub(crate) const FRAME_LIMIT: usize = 8 * 1024 * 1024 + 65_536;
pub(crate) const INPUT_LIMIT: usize = 1024 * 1024;
pub(crate) const RAW_CHUNK_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionDto {
    pub id: Uuid,
    pub incarnation_id: Uuid,
    pub name: String,
    pub owner_user_id: String,
    pub owner_name: String,
    pub host_device_id: String,
    pub host_name: String,
    pub room_id: Option<Uuid>,
    pub room_name: Option<String>,
    pub shared_with: Vec<String>,
    pub authorization_revision: u64,
    pub key_generation: u32,
    pub ready: bool,
    pub host_online: bool,
}

impl From<SessionDto> for super::RemoteSession {
    fn from(dto: SessionDto) -> Self {
        Self {
            id: dto.id,
            incarnation_id: dto.incarnation_id,
            name: dto.name,
            owner_user_id: dto.owner_user_id,
            owner_name: dto.owner_name,
            host_device_id: dto.host_device_id,
            host_name: dto.host_name,
            room_id: dto.room_id,
            room_name: dto.room_name,
            shared_with: dto.shared_with,
            online: dto.host_online,
        }
    }
}

#[derive(Clone)]
pub(crate) struct ControlIdentity {
    pub session_id: Uuid,
    pub incarnation_id: Uuid,
    pub authorization_revision: u64,
    pub key_generation: u32,
    pub connection_id: Uuid,
    pub user_id: String,
    pub device_id: String,
    pub sequence: u64,
    pub request_id: Uuid,
}

impl ControlIdentity {
    fn fields(&self) -> Vec<String> {
        vec![
            self.session_id.to_string(),
            self.incarnation_id.to_string(),
            self.authorization_revision.to_string(),
            self.key_generation.to_string(),
            self.connection_id.to_string(),
            self.user_id.clone(),
            self.device_id.clone(),
            self.sequence.to_string(),
            self.request_id.to_string(),
        ]
    }
    pub(crate) fn aad(&self) -> Result<Vec<u8>> {
        let fields = self.fields();
        crypto::signed_fields(
            b"kodosi-terminal-control-v1",
            &fields.iter().map(String::as_bytes).collect::<Vec<_>>(),
        )
    }
    pub(crate) fn signature(&self, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        crypto::signed_fields(
            b"kodosi-terminal-control-signature-v1",
            &[&self.aad()?, nonce, ciphertext],
        )
    }
    pub(crate) fn result_signature(&self, accepted: bool, message: &str) -> Result<Vec<u8>> {
        let mut fields = self.fields();
        fields.push(accepted.to_string());
        fields.push(message.to_owned());
        crypto::signed_fields(
            b"kodosi-terminal-result-v1",
            &fields.iter().map(String::as_bytes).collect::<Vec<_>>(),
        )
    }
}

pub(crate) fn encode_control(control: &TerminalControl) -> Result<Vec<u8>> {
    let value = match control {
        TerminalControl::Input { bytes } => {
            if bytes.len() > INPUT_LIMIT {
                return Err(invalid("Terminal input exceeds its chunk limit."));
            }
            json!({"type":"input","bytes":BASE64.encode(bytes)})
        }
        TerminalControl::Resize {
            rows,
            cols,
            width_pixels,
            height_pixels,
            cell_width_pixels,
            cell_height_pixels,
            claim,
            ..
        } => {
            let size = crate::terminal::TerminalSize::new(*rows, *cols)
                .map_err(|error| invalid(error.to_string()))?;
            let zero = [
                *width_pixels,
                *height_pixels,
                *cell_width_pixels,
                *cell_height_pixels,
            ]
            .iter()
            .all(|v| *v == 0);
            if !zero {
                crate::terminal::TerminalPixelGeometry::new(
                    *width_pixels,
                    *height_pixels,
                    *cell_width_pixels,
                    *cell_height_pixels,
                )
                .and_then(|geometry| geometry.validate_for_size(size))
                .map_err(|error| invalid(error.to_string()))?;
            }
            json!({"type":"resize","rows":rows,"cols":cols,"widthPixels":width_pixels,"heightPixels":height_pixels,
                "cellWidthPixels":cell_width_pixels,"cellHeightPixels":cell_height_pixels,"claim":claim})
        }
        TerminalControl::Focus { focused } => json!({"type":"focus","focused":focused}),
        TerminalControl::Interrupt => json!({"type":"interrupt"}),
        TerminalControl::Stop => json!({"type":"stop"}),
    };
    serde_json::to_vec(&value).map_err(Into::into)
}

pub(crate) fn decode_control(value: &[u8], request_id: Uuid) -> Result<TerminalControl> {
    let value: Value = serde_json::from_slice(value)?;
    let kind = text(&value, "type")?;
    match kind {
        "input" => {
            let bytes = decode_b64(&value, "bytes", INPUT_LIMIT)?;
            Ok(TerminalControl::Input { bytes })
        }
        "resize" => {
            let control = TerminalControl::Resize {
                request_id: request_id.to_string(),
                rows: u16::try_from(number(&value, "rows")?)
                    .map_err(|_| invalid("Invalid rows."))?,
                cols: u16::try_from(number(&value, "cols")?)
                    .map_err(|_| invalid("Invalid cols."))?,
                width_pixels: u32::try_from(number(&value, "widthPixels")?)
                    .map_err(|_| invalid("Invalid width."))?,
                height_pixels: u32::try_from(number(&value, "heightPixels")?)
                    .map_err(|_| invalid("Invalid height."))?,
                cell_width_pixels: u32::try_from(number(&value, "cellWidthPixels")?)
                    .map_err(|_| invalid("Invalid cell width."))?,
                cell_height_pixels: u32::try_from(number(&value, "cellHeightPixels")?)
                    .map_err(|_| invalid("Invalid cell height."))?,
                claim: value
                    .get("claim")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| invalid("Invalid size claim."))?,
            };
            encode_control(&control)?;
            Ok(control)
        }
        "focus" => Ok(TerminalControl::Focus {
            focused: value
                .get("focused")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("Invalid focus."))?,
        }),
        "interrupt" => Ok(TerminalControl::Interrupt),
        "stop" => Ok(TerminalControl::Stop),
        _ => Err(invalid("Unsupported terminal operation.")),
    }
}

pub(crate) fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("Missing {field}.")))
}
pub(crate) fn number(value: &Value, field: &str) -> Result<u64> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(format!("Invalid {field}.")))
}
pub(crate) fn id(value: &Value, field: &str) -> Result<Uuid> {
    Uuid::parse_str(text(value, field)?).map_err(|_| invalid(format!("Invalid {field}.")))
}
pub(crate) fn decode_b64(value: &Value, field: &str, limit: usize) -> Result<Vec<u8>> {
    let encoded = text(value, field)?;
    if encoded.len() > limit.saturating_add(2) / 3 * 4 {
        return Err(invalid(format!("{field} exceeds its limit.")));
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| invalid(format!("Invalid {field} encoding.")))?;
    if bytes.len() > limit {
        return Err(invalid(format!("{field} exceeds its limit.")));
    }
    Ok(bytes)
}

pub(crate) fn checkpoint_frame(
    key: &crypto::SessionKey,
    generation: u32,
    counter: u64,
    revision: u64,
    next_sequence: u64,
    checkpoint: &crate::terminal::Checkpoint,
) -> Result<Vec<u8>> {
    let body = encode_checkpoint(checkpoint)?;
    let mut header = vec![3];
    header.extend_from_slice(&generation.to_be_bytes());
    header.extend_from_slice(&counter.to_be_bytes());
    header.extend_from_slice(&revision.to_be_bytes());
    header.extend_from_slice(&next_sequence.to_be_bytes());
    let derived = crypto::derive_stream_key(key, crypto::RelayStream::Checkpoint)?;
    let ciphertext = crypto::encrypt_frame(&derived, generation, counter, &header, &body)?;
    header.extend_from_slice(&ciphertext);
    Ok(header)
}

pub(crate) fn raw_frame(
    key: &crypto::SessionKey,
    generation: u32,
    counter: u64,
    sequence: u64,
    bytes: &[u8],
) -> Result<Vec<u8>> {
    if bytes.len() > RAW_CHUNK_LIMIT {
        return Err(invalid("Terminal output batch exceeds its bound."));
    }
    let next = sequence
        .checked_add(1)
        .ok_or_else(|| invalid("Terminal sequence exhausted."))?;
    let mut header = vec![4];
    header.extend_from_slice(&generation.to_be_bytes());
    header.extend_from_slice(&counter.to_be_bytes());
    header.extend_from_slice(&sequence.to_be_bytes());
    header.extend_from_slice(&next.to_be_bytes());
    let mut body = Vec::with_capacity(bytes.len() + 4);
    body.extend_from_slice(
        &u32::try_from(bytes.len())
            .map_err(|_| invalid("Output too large."))?
            .to_be_bytes(),
    );
    body.extend_from_slice(bytes);
    let derived = crypto::derive_stream_key(key, crypto::RelayStream::TerminalRaw)?;
    header.extend_from_slice(&crypto::encrypt_frame(
        &derived, generation, counter, &header, &body,
    )?);
    Ok(header)
}

fn encode_checkpoint(checkpoint: &crate::terminal::Checkpoint) -> Result<Vec<u8>> {
    let bytes = &checkpoint.semantic_checkpoint;
    if bytes.is_empty() || bytes.len() > 8 * 1024 * 1024 {
        return Err(invalid("Terminal checkpoint exceeds its bound."));
    }
    let mut encoded = Vec::with_capacity(bytes.len() + 18);
    encoded.extend_from_slice(&checkpoint.schema_version().to_be_bytes());
    encoded.extend_from_slice(&checkpoint.rows().to_be_bytes());
    encoded.extend_from_slice(&checkpoint.cols().to_be_bytes());
    encoded.push(match checkpoint.active_screen {
        crate::terminal::TerminalScreen::Primary => 0,
        crate::terminal::TerminalScreen::Alternate => 1,
    });
    encoded.extend_from_slice(&checkpoint.cursor_x().to_be_bytes());
    encoded.extend_from_slice(&checkpoint.cursor_y().to_be_bytes());
    encoded.push(u8::from(checkpoint.cursor_hidden()));
    encoded.extend_from_slice(
        &u32::try_from(bytes.len())
            .map_err(|_| invalid("Checkpoint is too large."))?
            .to_be_bytes(),
    );
    encoded.extend_from_slice(bytes);
    if let Some(metadata) = &checkpoint.metadata {
        if !metadata.is_valid() {
            return Err(invalid("Invalid terminal metadata."));
        }
        let metadata = serde_json::to_vec(metadata)?;
        if metadata.len() > 16_384 {
            return Err(invalid("Terminal metadata exceeds its bound."));
        }
        encoded.extend_from_slice(&metadata);
    }
    Ok(encoded)
}

pub(crate) fn decode_checkpoint(encoded: &[u8]) -> Result<crate::terminal::Checkpoint> {
    use crate::terminal::{Checkpoint, TerminalScreen, TerminalSize};
    if encoded.len() < 18 || encoded.len() > 8 * 1024 * 1024 + 18 + 16_384 {
        return Err(invalid("Checkpoint plaintext has invalid length."));
    }
    if u32::from_be_bytes(
        encoded[..4]
            .try_into()
            .map_err(|_| invalid("Invalid schema."))?,
    ) != 2
    {
        return Err(invalid("Unsupported terminal checkpoint schema."));
    }
    let read16 = |at: usize| u16::from_be_bytes([encoded[at], encoded[at + 1]]);
    let size =
        TerminalSize::new(read16(4), read16(6)).map_err(|error| invalid(error.to_string()))?;
    let screen = match encoded[8] {
        0 => TerminalScreen::Primary,
        1 => TerminalScreen::Alternate,
        _ => return Err(invalid("Invalid terminal screen.")),
    };
    let hidden = match encoded[13] {
        0 => false,
        1 => true,
        _ => return Err(invalid("Invalid cursor state.")),
    };
    let len = u32::from_be_bytes(
        encoded[14..18]
            .try_into()
            .map_err(|_| invalid("Invalid checkpoint length."))?,
    ) as usize;
    if len == 0
        || len > 8 * 1024 * 1024
        || encoded.len() < 18 + len
        || encoded.len() - 18 - len > 16_384
    {
        return Err(invalid("Checkpoint length disagrees with envelope."));
    }
    let mut checkpoint = Checkpoint::new(
        size,
        screen,
        encoded[18..18 + len].to_vec(),
        read16(9),
        read16(11),
        hidden,
    )
    .map_err(|error| invalid(error.to_string()))?;
    if encoded.len() > 18 + len {
        let metadata: crate::terminal::TerminalMetadata =
            serde_json::from_slice(&encoded[18 + len..])?;
        if !metadata.is_valid() {
            return Err(invalid("Invalid terminal metadata."));
        }
        checkpoint.metadata = Some(metadata);
    }
    Ok(checkpoint)
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize) -> Result<u64> {
    Ok(u64::from_be_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(|| invalid("Truncated terminal frame."))?
            .try_into()
            .map_err(|_| invalid("Truncated terminal frame."))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_proof_changes_with_connection_and_revision() {
        let mut identity = ControlIdentity {
            session_id: Uuid::now_v7(),
            incarnation_id: Uuid::now_v7(),
            authorization_revision: 1,
            key_generation: 1,
            connection_id: Uuid::now_v7(),
            user_id: "user".into(),
            device_id: "device".into(),
            sequence: 1,
            request_id: Uuid::now_v7(),
        };
        let a = identity.aad().unwrap();
        identity.connection_id = Uuid::now_v7();
        assert_ne!(a, identity.aad().unwrap());
        let b = identity.aad().unwrap();
        identity.authorization_revision += 1;
        assert_ne!(b, identity.aad().unwrap());
    }
    #[test]
    fn current_signed_domains_match_the_manifest() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/crypto-domain-tags.json");
        let manifest: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let tags = &manifest["tags"];
        let identity = ControlIdentity {
            session_id: Uuid::nil(),
            incarnation_id: Uuid::nil(),
            authorization_revision: 1,
            key_generation: 1,
            connection_id: Uuid::nil(),
            user_id: "user".into(),
            device_id: "device".into(),
            sequence: 1,
            request_id: Uuid::nil(),
        };
        assert!(
            identity
                .aad()
                .unwrap()
                .starts_with(tags["TERMINAL_CONTROL_V1"].as_str().unwrap().as_bytes())
        );
        assert!(
            identity.signature(&[0; 12], &[]).unwrap().starts_with(
                tags["TERMINAL_CONTROL_SIGNATURE_V1"]
                    .as_str()
                    .unwrap()
                    .as_bytes()
            )
        );
        assert!(
            identity
                .result_signature(true, "")
                .unwrap()
                .starts_with(tags["TERMINAL_RESULT_V1"].as_str().unwrap().as_bytes())
        );
        assert!(tags.get("SESSION_KEY_V2").is_none());
    }

    #[test]
    fn retired_controls_do_not_decode() {
        for kind in [
            "suggest",
            "semanticSend",
            "permissionDecision",
            "queue",
            "steer",
        ] {
            assert!(
                decode_control(
                    &serde_json::to_vec(&json!({"type":kind})).unwrap(),
                    Uuid::now_v7()
                )
                .is_err()
            );
        }
    }
}
