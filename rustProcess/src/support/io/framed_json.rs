use bytes::{Bytes, BytesMut};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Framed, FramedRead, FramedWrite, LengthDelimitedCodec};

use crate::{AppError, MAX_FRAME_BYTES, Result};

pub(crate) type FramedJsonReader<R> = FramedRead<R, LengthDelimitedCodec>;
pub(crate) type FramedJsonWriter<W> = FramedWrite<W, LengthDelimitedCodec>;
pub(crate) type FramedJsonStream<T> = Framed<T, LengthDelimitedCodec>;

fn codec() -> LengthDelimitedCodec {
    LengthDelimitedCodec::builder()
        .length_field_type::<u32>()
        .big_endian()
        .max_frame_length(MAX_FRAME_BYTES)
        .new_codec()
}

pub(crate) fn reader<R: AsyncRead>(io: R) -> FramedJsonReader<R> {
    FramedRead::new(io, codec())
}

pub(crate) fn writer<W: AsyncWrite>(io: W) -> FramedJsonWriter<W> {
    FramedWrite::new(io, codec())
}

pub(crate) fn framed<T: AsyncRead + AsyncWrite>(io: T) -> FramedJsonStream<T> {
    Framed::new(io, codec())
}

async fn next_frame<S>(framed: &mut S) -> Result<Option<BytesMut>>
where
    S: Stream<Item = std::io::Result<BytesMut>> + Unpin,
{
    let Some(result) = framed.next().await else {
        return Ok(None);
    };
    result.map(Some).map_err(AppError::Io)
}

fn encode_json<T: Serialize>(message: &T) -> Result<Bytes> {
    let bytes = serde_json::to_vec(message).map_err(AppError::Json)?;
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(AppError::Unsupported {
            reason: format!(
                "serialized local protocol frame is {} bytes; maximum is {MAX_FRAME_BYTES}",
                bytes.len()
            ),
        });
    }
    Ok(Bytes::from(bytes))
}

async fn send_encoded<S>(framed: &mut S, bytes: Bytes) -> Result<()>
where
    S: Sink<Bytes, Error = std::io::Error> + Unpin,
{
    framed.send(bytes).await.map_err(AppError::Io)
}

pub(crate) async fn read_json<R, T>(framed: &mut FramedJsonReader<R>) -> Result<Option<T>>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let Some(bytes) = next_frame(framed).await? else {
        return Ok(None);
    };
    parse_json(&bytes).map(Some)
}

pub(crate) async fn read_frame<R>(framed: &mut FramedJsonReader<R>) -> Result<Option<BytesMut>>
where
    R: AsyncRead + Unpin,
{
    next_frame(framed).await
}

pub(crate) fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(AppError::Json)
}

pub(crate) async fn write_frame<W>(framed: &mut FramedJsonWriter<W>, bytes: Bytes) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(AppError::Unsupported {
            reason: format!(
                "local protocol frame is {} bytes; maximum is {MAX_FRAME_BYTES}",
                bytes.len()
            ),
        });
    }
    send_encoded(framed, bytes).await
}

#[expect(clippy::future_not_send, reason = "single-owner orchestrator")]
pub(crate) async fn write_json<W, T>(framed: &mut FramedJsonWriter<W>, message: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    send_encoded(framed, encode_json(message)?).await
}

#[expect(clippy::future_not_send, reason = "single-owner orchestrator")]
pub(crate) async fn send_json<T, M>(framed: &mut FramedJsonStream<T>, message: &M) -> Result<()>
where
    T: AsyncRead + AsyncWrite + Unpin,
    M: Serialize,
{
    send_encoded(framed, encode_json(message)?).await
}

pub(crate) async fn next_json<T, M>(framed: &mut FramedJsonStream<T>) -> Result<Option<M>>
where
    T: AsyncRead + AsyncWrite + Unpin,
    M: DeserializeOwned,
{
    let Some(bytes) = next_frame(framed).await? else {
        return Ok(None);
    };
    parse_json(&bytes).map(Some)
}

#[cfg(test)]
mod tests {
    use proptest::{
        collection::vec,
        prelude::{Strategy, any},
        prop_assert_eq, proptest,
    };
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct EchoMessage {
        body: String,
    }

    fn message_strategy() -> impl Strategy<Value = EchoMessage> {
        vec(any::<char>(), 0..256).prop_map(|chars| EchoMessage {
            body: chars.into_iter().collect(),
        })
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        #[test]
        fn write_json_round_trips_over_length_framing(message in message_strategy()) {
            let expected = message.clone();
            let decoded = tokio::runtime::Runtime::new()
                .unwrap_or_else(|error| panic!("test runtime should construct: {error}"))
                .block_on(async move {
                    let (client, server) = tokio::io::duplex(8 * 1024);
                    let mut writer = writer(client);
                    let mut reader = reader(server);

                    write_json(&mut writer, &message)
                        .await
                        .unwrap_or_else(|error| panic!("framed write should succeed: {error}"));

                    read_json::<_, EchoMessage>(&mut reader)
                        .await
                        .unwrap_or_else(|error| panic!("framed read should succeed: {error}"))
                });

            prop_assert_eq!(decoded, Some(expected));
        }

        #[test]
        fn send_json_round_trips_over_bidirectional_length_framing(message in message_strategy()) {
            let expected = message.clone();
            let decoded = tokio::runtime::Runtime::new()
                .unwrap_or_else(|error| panic!("test runtime should construct: {error}"))
                .block_on(async move {
                    let (client, server) = tokio::io::duplex(8 * 1024);
                    let mut client_stream = framed(client);
                    let mut server_stream = framed(server);

                    send_json(&mut client_stream, &message)
                        .await
                        .unwrap_or_else(|error| panic!("framed send should succeed: {error}"));

                    next_json::<_, EchoMessage>(&mut server_stream)
                        .await
                        .unwrap_or_else(|error| panic!("framed next should succeed: {error}"))
                });

            prop_assert_eq!(decoded, Some(expected));
        }
    }
}
