use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Result, TerminalControl, TerminalStatus, invalid};
use crate::terminal::{
    TERMINAL_CHECKPOINT_SCHEMA_VERSION, TERMINAL_INPUT_MAX_BYTES, TERMINAL_SNAPSHOT_MAX_BYTES,
};

mod frame;
mod packing;
#[cfg(feature = "host")]
pub(crate) use frame::{Accept, OUTPUT_FRAME, Refuse};
pub(crate) use frame::{ControlResult, Frame};
#[cfg(any(feature = "host", test))]
pub(crate) use frame::{End, KEYFRAME_PART};

pub(crate) const RAW_CHUNK_LIMIT: usize = 64 * 1024;
pub(crate) const RAW_BATCH_LIMIT: usize = 128;
const METADATA_LIMIT: usize = 16_384;
const CHECKPOINT_HEADER: usize = 9;
const SNAPSHOT_LIMIT: usize = TERMINAL_SNAPSHOT_MAX_BYTES + CHECKPOINT_HEADER + METADATA_LIMIT;
pub(crate) const PACKED_SNAPSHOT_LIMIT: usize = 2 * SNAPSHOT_LIMIT;
#[cfg(any(feature = "host", test))]
const SNAPSHOT_LEVEL: u32 = 6;

#[derive(Debug, Clone, Deserialize)]
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
    pub status: Option<TerminalStatus>,
    pub room_sequence: u64,
    #[cfg(feature = "host")]
    pub room_choice: Option<RoomChoice>,
    #[serde(default)]
    pub hidden: bool,
    #[cfg(feature = "host")]
    #[serde(default)]
    pub close_request: Option<CloseRequest>,
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoomChoice {
    pub device_id: String,
    pub signature: String,
}

#[cfg(feature = "host")]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CloseRequest {
    pub user_id: String,
    pub device_id: String,
    pub signature: String,
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
            status: dto.status.filter(TerminalStatus::is_valid),
            hidden: dto.hidden,
        }
    }
}

pub(crate) fn encode_control(control: &TerminalControl) -> Result<Vec<u8>> {
    let value = match control {
        TerminalControl::Input { .. } => {
            return Err(invalid("Terminal input is not a control."));
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
        TerminalControl::Message { bytes } => {
            let text =
                std::str::from_utf8(bytes).map_err(|_| invalid("A message must be text."))?;
            json!({"type":"message","text":text})
        }
        TerminalControl::Focus { focused } => json!({"type":"focus","focused":focused}),
        TerminalControl::Interrupt => json!({"type":"interrupt"}),
        TerminalControl::Close => json!({"type":"close"}),
    };
    serde_json::to_vec(&value).map_err(Into::into)
}

pub(crate) fn decode_control(value: &[u8]) -> Result<TerminalControl> {
    let value: Value = serde_json::from_slice(value)?;
    let kind = text(&value, "type")?;
    match kind {
        "resize" => {
            let control = TerminalControl::Resize {
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
        "message" => Ok(TerminalControl::Message {
            bytes: text(&value, "text")?.as_bytes().to_vec(),
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
pub(crate) fn hosts(value: &Value) -> Result<Vec<String>> {
    value
        .get("deviceIds")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("Invalid deviceIds."))?
        .iter()
        .map(|device| {
            device
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("Invalid deviceIds."))
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub surface: String,
    pub room: Option<Uuid>,
    pub sequence: Option<u64>,
}

impl Change {
    pub(crate) fn of(surface: &str) -> Self {
        Self {
            surface: surface.to_owned(),
            room: None,
            sequence: None,
        }
    }

    pub(crate) fn read(value: &Value) -> Result<Self> {
        let room = match value.get("roomId") {
            None | Some(Value::Null) => None,
            Some(_) => Some(id(value, "roomId")?),
        };
        Ok(Self {
            surface: text(value, "surface")?.to_owned(),
            room,
            sequence: value.get("sequence").and_then(Value::as_u64),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TerminalNotice {
    pub session: Uuid,
    pub incarnation: Uuid,
    pub online: bool,
    pub status: Option<TerminalStatus>,
}

impl TerminalNotice {
    pub(crate) fn read(value: &Value) -> Result<Self> {
        Ok(Self {
            session: id(value, "sessionId")?,
            incarnation: id(value, "incarnationId")?,
            online: value
                .get("online")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("Invalid online."))?,
            status: match value.get("status") {
                None | Some(Value::Null) => None,
                Some(status) => Some(serde_json::from_value::<TerminalStatus>(status.clone())?)
                    .filter(TerminalStatus::is_valid),
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Notice {
    Changed(Change),
    Terminal(TerminalNotice),
    Hosts(Vec<String>),
}
pub(crate) fn decode_b64(value: &Value, field: &str, limit: usize) -> Result<Vec<u8>> {
    crate::identity::pins::decode(text(value, field)?, limit)
}

#[cfg(any(feature = "host", test))]
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

#[cfg(any(feature = "host", test))]
fn encode_checkpoint(checkpoint: &crate::terminal::Checkpoint) -> Result<Vec<u8>> {
    let bytes = checkpoint.snapshot();
    let mut encoded = Vec::with_capacity(bytes.len() + CHECKPOINT_HEADER);
    encoded.extend_from_slice(&TERMINAL_CHECKPOINT_SCHEMA_VERSION.to_be_bytes());
    encoded.push(u8::from(checkpoint.console));
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
    use crate::terminal::Checkpoint;
    if encoded.len() < CHECKPOINT_HEADER || encoded.len() > SNAPSHOT_LIMIT {
        return Err(invalid("Checkpoint plaintext has invalid length."));
    }
    let schema = u32::from_be_bytes(
        encoded[..4]
            .try_into()
            .map_err(|_| invalid("Invalid schema."))?,
    );
    if schema != TERMINAL_CHECKPOINT_SCHEMA_VERSION {
        return Err(invalid("Unsupported terminal checkpoint schema."));
    }
    let console = match encoded[4] {
        0 => false,
        1 => true,
        _ => return Err(invalid("Invalid console state.")),
    };
    let len = u32::from_be_bytes(
        encoded[5..CHECKPOINT_HEADER]
            .try_into()
            .map_err(|_| invalid("Invalid checkpoint length."))?,
    ) as usize;
    if len == 0
        || len > TERMINAL_SNAPSHOT_MAX_BYTES
        || encoded.len() < CHECKPOINT_HEADER + len
        || encoded.len() - CHECKPOINT_HEADER - len > METADATA_LIMIT
    {
        return Err(invalid("Checkpoint length disagrees with envelope."));
    }
    let snapshot = encoded[CHECKPOINT_HEADER..CHECKPOINT_HEADER + len].to_vec();
    let mut checkpoint =
        Checkpoint::new(snapshot, console).map_err(|error| invalid(error.to_string()))?;
    if encoded.len() > CHECKPOINT_HEADER + len {
        checkpoint.metadata = Some(Box::new(decode_metadata(
            &encoded[CHECKPOINT_HEADER + len..],
        )?));
    }
    Ok(checkpoint)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_large_snapshot_goes_in_parts_and_comes_back_whole() {
        use crate::terminal::Checkpoint;
        let mut body = ghostty_vt::Terminal::new(100, 30, ghostty_vt::TerminalPolicy::default())
            .and_then(|mut terminal| {
                terminal.visible_snapshot(crate::terminal::snapshot_limits(u64::MAX))
            })
            .unwrap();
        body.extend(
            (0..700_000u32).flat_map(|value| value.wrapping_mul(2_654_435_761).to_be_bytes()),
        );
        let checkpoint = Checkpoint::new(body, true).unwrap();
        let parts = snapshot_parts(&checkpoint).unwrap();
        assert!(parts.len() > 1 && parts.iter().all(|part| part.len() <= KEYFRAME_PART));
        let restored = snapshot(&parts.concat()).unwrap();
        assert_eq!(restored, checkpoint);
        assert!(snapshot(&parts[0]).is_err());
    }
}
