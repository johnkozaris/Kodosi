use super::{FRAME_LIMIT, RAW_CHUNK_LIMIT, Result, crypto, decode_checkpoint, invalid, read_u64};
use crate::network::RemoteUpdate;
use bytes::Bytes;
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct FrameDecoder {
    pub next_sequence: Option<u64>,
    checkpoint_counter: Option<u64>,
    checkpoint_revision: Option<u64>,
    raw_counter: Option<u64>,
}

impl FrameDecoder {
    pub(crate) fn decode(
        &mut self,
        key: &crypto::SessionKey,
        generation: u32,
        frame: &[u8],
    ) -> Result<Vec<RemoteUpdate>> {
        if frame.len() < 45 || frame.len() > FRAME_LIMIT {
            return Err(invalid("Remote terminal frame is outside bounds."));
        }
        let encoded_generation = u32::from_be_bytes(
            frame[1..5]
                .try_into()
                .map_err(|_| invalid("Invalid frame generation."))?,
        );
        if encoded_generation != generation {
            return Err(crate::network::Error::Stale);
        }
        let counter = read_u64(frame, 5)?;
        let first = read_u64(frame, 13)?;
        let next = read_u64(frame, 21)?;
        match frame[0] {
            3 => {
                if self
                    .checkpoint_counter
                    .is_some_and(|previous| counter <= previous)
                    || self
                        .checkpoint_revision
                        .is_some_and(|previous| first <= previous)
                {
                    return Err(invalid("Remote checkpoint was replayed or regressed."));
                }
                let subkey = crypto::derive_stream_key(key, crypto::RelayStream::Checkpoint)?;
                let plaintext = Zeroizing::new(crypto::decrypt_frame(
                    &subkey,
                    generation,
                    counter,
                    &frame[..29],
                    &frame[29..],
                )?);
                let checkpoint = decode_checkpoint(&plaintext)?;
                self.checkpoint_counter = Some(counter);
                self.checkpoint_revision = Some(first);
                if self.next_sequence.is_none() {
                    self.next_sequence = Some(next);
                }
                Ok(vec![RemoteUpdate::Checkpoint {
                    checkpoint,
                    next_sequence: next,
                    fresh: false,
                }])
            }
            4 => {
                if self.raw_counter.is_some_and(|previous| counter <= previous) {
                    return Err(invalid("Remote output was replayed."));
                }
                let expected = self
                    .next_sequence
                    .ok_or_else(|| invalid("Remote output arrived before a checkpoint."))?;
                if first != expected || next <= first || next - first > 128 {
                    return Err(invalid(
                        "Remote output has a sequence gap or oversized batch.",
                    ));
                }
                let subkey = crypto::derive_stream_key(key, crypto::RelayStream::TerminalRaw)?;
                let plaintext = Zeroizing::new(crypto::decrypt_frame(
                    &subkey,
                    generation,
                    counter,
                    &frame[..29],
                    &frame[29..],
                )?);
                let mut updates = Vec::new();
                let mut offset = 0;
                for sequence in first..next {
                    let prefix = plaintext
                        .get(offset..offset + 4)
                        .ok_or_else(|| invalid("Truncated output chunk."))?;
                    let len = u32::from_be_bytes(
                        prefix.try_into().map_err(|_| invalid("Invalid chunk."))?,
                    ) as usize;
                    offset += 4;
                    if len > RAW_CHUNK_LIMIT {
                        return Err(invalid("Output chunk exceeds bound."));
                    }
                    let chunk = plaintext
                        .get(offset..offset + len)
                        .ok_or_else(|| invalid("Truncated output chunk."))?;
                    updates.push(RemoteUpdate::Raw {
                        sequence,
                        bytes: Bytes::copy_from_slice(chunk),
                    });
                    offset += len;
                }
                if offset != plaintext.len() {
                    return Err(invalid("Trailing remote output bytes."));
                }
                self.raw_counter = Some(counter);
                self.next_sequence = Some(next);
                Ok(updates)
            }
            _ => Err(invalid("Unsupported terminal frame type.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{checkpoint_frame, encode_checkpoint, raw_frame};
    use super::*;
    fn checkpoint() -> crate::terminal::Checkpoint {
        crate::terminal::Checkpoint::new(
            crate::terminal::TerminalSize::new(24, 80).unwrap(),
            crate::terminal::TerminalScreen::Primary,
            b"{}".to_vec(),
            0,
            0,
            false,
        )
        .unwrap()
    }
    #[test]
    fn private_terminal_metadata_roundtrips_with_the_encrypted_checkpoint() {
        let mut checkpoint = checkpoint();
        checkpoint.metadata = Some(crate::terminal::TerminalMetadata {
            connected_users: vec![],
            directory: Some("/work/project".into()),
            title: Some("My work".into()),
            program: Some("claude".into()),
        });
        let encoded = encode_checkpoint(&checkpoint).unwrap();
        assert_eq!(decode_checkpoint(&encoded).unwrap(), checkpoint);
        checkpoint.metadata.as_mut().unwrap().program = Some("unsupported".into());
        assert!(encode_checkpoint(&checkpoint).is_err());
    }

    #[test]
    fn current_checkpoint_binary_is_exact_and_roundtrips() {
        let encoded = encode_checkpoint(&checkpoint()).unwrap();
        assert_eq!(
            &encoded[..18],
            &[0, 0, 0, 2, 0, 24, 0, 80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]
        );
        assert_eq!(decode_checkpoint(&encoded).unwrap(), checkpoint());
    }
    #[test]
    fn targeted_capture_does_not_skip_raw_for_existing_views() {
        let key = [7; 32];
        let mut decoder = FrameDecoder::default();
        decoder
            .decode(
                &key,
                1,
                &checkpoint_frame(&key, 1, 0, 1, 10, &checkpoint()).unwrap(),
            )
            .unwrap();
        decoder
            .decode(
                &key,
                1,
                &checkpoint_frame(&key, 1, 1, 2, 12, &checkpoint()).unwrap(),
            )
            .unwrap();
        assert_eq!(decoder.next_sequence, Some(10));
        decoder
            .decode(&key, 1, &raw_frame(&key, 1, 0, 10, b"a").unwrap())
            .unwrap();
        decoder
            .decode(&key, 1, &raw_frame(&key, 1, 1, 11, b"b").unwrap())
            .unwrap();
        decoder
            .decode(
                &key,
                1,
                &checkpoint_frame(&key, 1, 2, 3, 10, &checkpoint()).unwrap(),
            )
            .unwrap();
        assert_eq!(decoder.next_sequence, Some(12));
    }
    #[test]
    fn replay_gap_and_wrong_generation_are_rejected() {
        let key = [7; 32];
        let mut decoder = FrameDecoder::default();
        let cp = checkpoint_frame(&key, 1, 0, 1, 10, &checkpoint()).unwrap();
        let raw = raw_frame(&key, 1, 0, 10, b"a").unwrap();
        assert!(decoder.decode(&key, 1, &raw).is_err());
        decoder.decode(&key, 1, &cp).unwrap();
        assert!(decoder.decode(&key, 1, &cp).is_err());
        assert!(decoder.decode(&key, 2, &raw).is_err());
        assert!(
            decoder
                .decode(&key, 1, &raw_frame(&key, 1, 0, 11, b"gap").unwrap())
                .is_err()
        );
        decoder.decode(&key, 1, &raw).unwrap();
        assert!(decoder.decode(&key, 1, &raw).is_err());
    }
    #[test]
    fn authenticated_header_and_payload_tampering_rejects_without_advancing() {
        let key = [7; 32];
        let mut decoder = FrameDecoder::default();
        decoder
            .decode(
                &key,
                1,
                &checkpoint_frame(&key, 1, 0, 1, 10, &checkpoint()).unwrap(),
            )
            .unwrap();
        let raw = raw_frame(&key, 1, 0, 10, b"a").unwrap();
        for offset in [0, 5, 13, 21, 29, raw.len() - 1] {
            let mut changed = raw.clone();
            changed[offset] ^= 1;
            assert!(decoder.decode(&key, 1, &changed).is_err());
            assert_eq!(decoder.next_sequence, Some(10));
        }
        assert!(raw_frame(&key, 1, 0, 10, &vec![0; RAW_CHUNK_LIMIT + 1]).is_err());
    }
}
