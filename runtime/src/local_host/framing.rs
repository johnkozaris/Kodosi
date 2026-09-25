use super::*;

pub(super) const CHECKPOINT: u8 = 1;
pub(super) const DATA: u8 = 2;
pub(super) const CONTROL: u8 = 3;
pub(super) const INPUT: u8 = 4;
pub(super) const RESIZE: u8 = 5;
pub(super) const INPUT_ACK: u8 = 6;

#[derive(Debug)]
pub enum TerminalFrame {
    InputAck {
        accepted: bool,
        message: Option<String>,
    },
    Checkpoint {
        bytes: Vec<u8>,
        rows: u16,
        cols: u16,
        next_sequence: u64,
    },
    Data {
        bytes: Bytes,
        sequence: u64,
    },
    Resize {
        rows: u16,
        cols: u16,
        at_sequence: u64,
    },
    Closed {
        reason: String,
        final_sequence: u64,
    },
}
pub(super) fn checkpoint_frame(checkpoint: &terminal::Checkpoint, next: u64) -> Result<Vec<u8>> {
    if checkpoint.semantic_checkpoint.len() > 8 * 1024 * 1024 {
        return Err(Error::Invalid("checkpoint exceeds bound".into()));
    }
    let mut bytes = Vec::with_capacity(13 + checkpoint.semantic_checkpoint.len());
    bytes.push(CHECKPOINT);
    bytes.extend(checkpoint.rows().to_be_bytes());
    bytes.extend(checkpoint.cols().to_be_bytes());
    bytes.extend(next.to_be_bytes());
    bytes.extend(&checkpoint.semantic_checkpoint);
    Ok(bytes)
}
pub(super) fn control_frame(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = vec![CONTROL];
    bytes.extend(serde_json::to_vec(value)?);
    Ok(bytes)
}
pub async fn read_terminal<R: AsyncRead + Unpin>(
    reader: &mut FrameReader<R>,
) -> Result<TerminalFrame> {
    let bytes = read_frame(reader).await?;
    match bytes.first() {
        Some(&CHECKPOINT) if bytes.len() > 13 => Ok(TerminalFrame::Checkpoint {
            rows: u16::from_be_bytes([bytes[1], bytes[2]]),
            cols: u16::from_be_bytes([bytes[3], bytes[4]]),
            next_sequence: u64::from_be_bytes(
                bytes[5..13]
                    .try_into()
                    .map_err(|_| Error::Invalid("invalid checkpoint sequence".into()))?,
            ),
            bytes: bytes[13..].to_vec(),
        }),
        Some(&DATA) if bytes.len() >= 9 => Ok(TerminalFrame::Data {
            sequence: u64::from_be_bytes(
                bytes[1..9]
                    .try_into()
                    .map_err(|_| Error::Invalid("invalid terminal sequence".into()))?,
            ),
            bytes: Bytes::copy_from_slice(&bytes[9..]),
        }),
        Some(&INPUT_ACK) => {
            let value: Value = serde_json::from_slice(&bytes[1..])?;
            Ok(TerminalFrame::InputAck {
                accepted: value
                    .get("accepted")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| Error::Invalid("invalid input acknowledgement".into()))?,
                message: value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        }
        Some(&CONTROL) => {
            let value: Value = serde_json::from_slice(&bytes[1..])?;
            match value.get("type").and_then(Value::as_str) {
                Some("resize") => Ok(TerminalFrame::Resize {
                    rows: number(&value, "rows")?,
                    cols: number(&value, "cols")?,
                    at_sequence: number(&value, "atSequence")?,
                }),
                Some("closed") => Ok(TerminalFrame::Closed {
                    reason: value
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("terminal closed")
                        .to_owned(),
                    final_sequence: number(&value, "finalSequence")?,
                }),
                _ => Err(Error::Invalid("unsupported terminal control".into())),
            }
        }
        _ => Err(Error::Invalid("malformed terminal frame".into())),
    }
}
pub(super) fn number<T: TryFrom<u64>>(value: &Value, key: &str) -> Result<T> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| Error::Invalid(format!("invalid {key}")))
}
pub async fn write_input<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    if bytes.len() > 1024 * 1024 {
        return Err(Error::Invalid("terminal input exceeds 1 MiB".into()));
    }
    let mut frame = vec![INPUT];
    frame.extend(bytes);
    write_frame(writer, &frame).await
}
pub async fn write_resize<W: AsyncWrite + Unpin>(
    writer: &mut W,
    cols: u16,
    rows: u16,
) -> Result<()> {
    if cols == 0 || rows == 0 {
        return Err(Error::Invalid("terminal size must be nonzero".into()));
    }
    let mut frame = vec![RESIZE];
    frame.extend(cols.to_be_bytes());
    frame.extend(rows.to_be_bytes());
    write_frame(writer, &frame).await
}
pub(super) async fn read_json<R: AsyncRead + Unpin, T: serde::de::DeserializeOwned>(
    reader: &mut FrameReader<R>,
) -> Result<T> {
    serde_json::from_slice(&read_frame(reader).await?).map_err(Into::into)
}
pub(super) async fn write_json<W: AsyncWrite + Unpin + Send, T: Serialize + Sync>(
    writer: &mut W,
    value: &T,
) -> Result<()> {
    write_frame(writer, &serde_json::to_vec(value)?).await
}
pub struct FrameReader<R> {
    pub(super) input: R,
    pub(super) prefix: [u8; 4],
    pub(super) prefix_read: usize,
    pub(super) body: Vec<u8>,
    pub(super) body_read: usize,
    pub(super) maximum: usize,
}
impl<R> FrameReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input,
            prefix: [0; 4],
            prefix_read: 0,
            body: Vec::new(),
            body_read: 0,
            maximum: MAX_FRAME,
        }
    }
}
pub(super) async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut FrameReader<R>,
) -> Result<Vec<u8>> {
    while reader.prefix_read < 4 {
        let count = reader
            .input
            .read(&mut reader.prefix[reader.prefix_read..])
            .await?;
        if count == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        reader.prefix_read += count;
    }
    if reader.body.is_empty() {
        let size = u32::from_be_bytes(reader.prefix) as usize;
        if size == 0 || size > reader.maximum {
            return Err(Error::Invalid("local frame exceeds its bound".into()));
        }
        reader.body = vec![0; size];
    }
    while reader.body_read < reader.body.len() {
        let count = reader
            .input
            .read(&mut reader.body[reader.body_read..])
            .await?;
        if count == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        reader.body_read += count;
    }
    reader.prefix_read = 0;
    reader.body_read = 0;
    Ok(std::mem::take(&mut reader.body))
}
pub(super) async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(Error::Invalid("local frame exceeds its bound".into()));
    }
    let size = u32::try_from(bytes.len())
        .map_err(|_| Error::Invalid("local frame length overflow".into()))?;
    tokio::time::timeout(WRITE_TIMEOUT, async {
        writer.write_u32(size).await?;
        writer.write_all(bytes).await?;
        writer.flush().await
    })
    .await
    .map_err(|_| Error::Other("local client stopped receiving".into()))??;
    Ok(())
}
