use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Result, TerminalControl, invalid};

mod frame;
mod packing;
pub(crate) use frame::{
    Accept, ControlResult, End, Frame, Hello, KEYFRAME_PART, OUTPUT_FRAME, Refuse,
};

pub(crate) const INPUT_LIMIT: usize = 1024 * 1024;
pub(crate) const RAW_CHUNK_LIMIT: usize = 64 * 1024;
pub(crate) const RAW_BATCH_LIMIT: usize = 128;
const METADATA_LIMIT: usize = 16_384;
const SNAPSHOT_LIMIT: usize = 8 * 1024 * 1024 + 18 + METADATA_LIMIT;
const SNAPSHOT_LEVEL: u32 = 6;

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
    pub mission_id: Option<Uuid>,
    pub mission_name: Option<String>,
    pub shared_with: Vec<String>,
    pub authorization_revision: u64,
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
            mission_id: dto.mission_id,
            mission_name: dto.mission_name,
            shared_with: dto.shared_with,
            online: dto.host_online,
        }
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
        TerminalControl::Close => json!({"type":"close"}),
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
        "close" => Ok(TerminalControl::Close),
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

pub(crate) fn snapshot_parts(
    checkpoint: &crate::terminal::Checkpoint,
) -> Result<Vec<bytes::Bytes>> {
    let packed = bytes::Bytes::from(packing::pack(
        &encode_checkpoint(checkpoint)?,
        SNAPSHOT_LEVEL,
    )?);
    Ok((0..packed.len())
        .step_by(KEYFRAME_PART)
        .map(|at| packed.slice(at..packed.len().min(at + KEYFRAME_PART)))
        .collect())
}

pub(crate) fn snapshot(packed: &[u8]) -> Result<crate::terminal::Checkpoint> {
    decode_checkpoint(&packing::unpack(packed, SNAPSHOT_LIMIT)?)
}

fn encode_metadata(metadata: &crate::terminal::TerminalMetadata) -> Result<Vec<u8>> {
    if !metadata.is_valid() {
        return Err(invalid("Invalid terminal metadata."));
    }
    let encoded = serde_json::to_vec(metadata)?;
    if encoded.len() > METADATA_LIMIT {
        return Err(invalid("Terminal metadata exceeds its bound."));
    }
    Ok(encoded)
}

fn decode_metadata(encoded: &[u8]) -> Result<crate::terminal::TerminalMetadata> {
    if encoded.len() > METADATA_LIMIT {
        return Err(invalid("Terminal metadata exceeds its bound."));
    }
    let metadata: crate::terminal::TerminalMetadata = serde_json::from_slice(encoded)?;
    if !metadata.is_valid() {
        return Err(invalid("Invalid terminal metadata."));
    }
    Ok(metadata)
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
        encoded.extend_from_slice(&encode_metadata(metadata)?);
    }
    Ok(encoded)
}

fn decode_checkpoint(encoded: &[u8]) -> Result<crate::terminal::Checkpoint> {
    use crate::terminal::{Checkpoint, TerminalScreen, TerminalSize};
    if encoded.len() < 18 || encoded.len() > 8 * 1024 * 1024 + 18 + METADATA_LIMIT {
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
        || encoded.len() - 18 - len > METADATA_LIMIT
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
        checkpoint.metadata = Some(decode_metadata(&encoded[18 + len..])?);
    }
    Ok(checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_large_snapshot_goes_in_parts_and_comes_back_whole() {
        use crate::terminal::{Checkpoint, TerminalScreen, TerminalSize};
        let body = (0..700_000u32)
            .flat_map(|value| value.wrapping_mul(2_654_435_761).to_be_bytes())
            .collect::<Vec<_>>();
        let checkpoint = Checkpoint::new(
            TerminalSize::new(30, 100).unwrap(),
            TerminalScreen::Primary,
            body.clone(),
            3,
            4,
            false,
        )
        .unwrap();
        let parts = snapshot_parts(&checkpoint).unwrap();
        assert!(parts.len() > 1 && parts.iter().all(|part| part.len() <= KEYFRAME_PART));
        let restored = snapshot(&parts.concat()).unwrap();
        assert_eq!(restored.semantic_checkpoint, body);
        assert!(snapshot(&parts[0]).is_err());
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
