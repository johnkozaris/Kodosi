use super::{
    FRAME_LIMIT, RAW_BATCH_LIMIT, RAW_BODY_LIMIT, RAW_CHUNK_LIMIT, Result, SNAPSHOT_LIMIT, crypto,
    decode_checkpoint, decode_metadata, invalid, packing, read_u64,
};
use crate::network::RemoteUpdate;
use bytes::Bytes;
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct FrameDecoder {
    pub next_sequence: Option<u64>,
    checkpoint_counter: Option<u64>,
    checkpoint_revision: Option<u64>,
    raw_counter: Option<u64>,
    notice_counter: Option<u64>,
    output: packing::OutputUnpacker,
}

impl FrameDecoder {
    pub(crate) const fn restart(&mut self) {
        self.next_sequence = None;
        self.output.stop();
    }

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
            3 => self.checkpoint(key, generation, counter, first, next, frame),
            4 => self.raw(key, generation, counter, first, next, frame),
            5 => self.notice(key, generation, counter, first, frame),
            _ => Err(invalid("Unsupported terminal frame type.")),
        }
    }

    fn checkpoint(
        &mut self,
        key: &crypto::SessionKey,
        generation: u32,
        counter: u64,
        revision: u64,
        next: u64,
        frame: &[u8],
    ) -> Result<Vec<RemoteUpdate>> {
        if self
            .checkpoint_counter
            .is_some_and(|previous| counter <= previous)
            || self
                .checkpoint_revision
                .is_some_and(|previous| revision <= previous)
        {
            return Err(invalid("Remote checkpoint was replayed or regressed."));
        }
        let subkey = crypto::derive_stream_key(key, crypto::TrafficStream::Checkpoint)?;
        let packed = Zeroizing::new(crypto::decrypt_frame(
            &subkey,
            generation,
            counter,
            &frame[..29],
            &frame[29..],
        )?);
        let plaintext = Zeroizing::new(packing::unpack_snapshot(&packed, SNAPSHOT_LIMIT)?);
        let checkpoint = decode_checkpoint(&plaintext)?;
        self.checkpoint_counter = Some(counter);
        self.checkpoint_revision = Some(revision);
        if self.next_sequence.is_none() {
            self.next_sequence = Some(next);
        }
        Ok(vec![RemoteUpdate::Checkpoint {
            checkpoint,
            next_sequence: next,
            fresh: false,
        }])
    }

    fn raw(
        &mut self,
        key: &crypto::SessionKey,
        generation: u32,
        counter: u64,
        first: u64,
        next: u64,
        frame: &[u8],
    ) -> Result<Vec<RemoteUpdate>> {
        if self.raw_counter.is_some_and(|previous| counter <= previous) {
            return Err(invalid("Remote output was replayed."));
        }
        let expected = self
            .next_sequence
            .ok_or_else(|| invalid("Remote output arrived before a checkpoint."))?;
        if first != expected || next <= first || next - first > RAW_BATCH_LIMIT as u64 {
            return Err(invalid(
                "Remote output has a sequence gap or oversized batch.",
            ));
        }
        let subkey = crypto::derive_stream_key(key, crypto::TrafficStream::TerminalRaw)?;
        let packed = Zeroizing::new(crypto::decrypt_frame(
            &subkey,
            generation,
            counter,
            &frame[..29],
            &frame[29..],
        )?);
        let plaintext = Zeroizing::new(self.output.unpack(&packed, RAW_BODY_LIMIT)?);
        let mut updates = Vec::new();
        let mut offset = 0;
        for sequence in first..next {
            let prefix = plaintext
                .get(offset..offset + 4)
                .ok_or_else(|| invalid("Truncated output chunk."))?;
            let len = u32::from_be_bytes(prefix.try_into().map_err(|_| invalid("Invalid chunk."))?)
                as usize;
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

    fn notice(
        &mut self,
        key: &crypto::SessionKey,
        generation: u32,
        counter: u64,
        revision: u64,
        frame: &[u8],
    ) -> Result<Vec<RemoteUpdate>> {
        if self
            .notice_counter
            .is_some_and(|previous| counter <= previous)
            || self
                .checkpoint_revision
                .is_none_or(|current| revision < current)
        {
            return Err(invalid("Remote terminal notice was replayed."));
        }
        let subkey = crypto::derive_stream_key(key, crypto::TrafficStream::Notice)?;
        let plaintext = Zeroizing::new(crypto::decrypt_frame(
            &subkey,
            generation,
            counter,
            &frame[..29],
            &frame[29..],
        )?);
        let update = match plaintext.split_first() {
            Some((0, metadata)) => RemoteUpdate::Metadata(decode_metadata(metadata)?),
            Some((1, resize)) if resize.len() == 12 => RemoteUpdate::Resize {
                rows: u16::from_be_bytes([resize[0], resize[1]]),
                cols: u16::from_be_bytes([resize[2], resize[3]]),
                at_sequence: read_u64(resize, 4)?,
            },
            _ => return Err(invalid("Unsupported terminal notice.")),
        };
        self.notice_counter = Some(counter);
        Ok(vec![update])
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        Notice, OutputPacker, checkpoint_frame, encode_checkpoint, notice_frame, raw_frame,
    };
    use super::*;
    fn raw(key: &crypto::SessionKey, counter: u64, sequence: u64, chunks: &[&[u8]]) -> Vec<u8> {
        raw_frame(key, 1, counter, sequence, chunks, &mut OutputPacker::new()).unwrap()
    }
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
        decoder.decode(&key, 1, &raw(&key, 0, 10, &[b"a"])).unwrap();
        decoder.decode(&key, 1, &raw(&key, 1, 11, &[b"b"])).unwrap();
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
    fn notices_are_small_and_cannot_be_replayed_or_come_from_before_the_snapshot() {
        let key = [7; 32];
        let mut decoder = FrameDecoder::default();
        let metadata = crate::terminal::TerminalMetadata {
            connected_users: vec![],
            directory: Some("/work/project".into()),
            title: Some("Build 42".into()),
            program: Some("claude".into()),
        };
        let frame = notice_frame(&key, 1, 0, 2, &Notice::Metadata(&metadata)).unwrap();
        assert!(frame.len() < 256);
        assert!(decoder.decode(&key, 1, &frame).is_err());
        decoder
            .decode(
                &key,
                1,
                &checkpoint_frame(&key, 1, 0, 2, 10, &checkpoint()).unwrap(),
            )
            .unwrap();
        let updates = decoder.decode(&key, 1, &frame).unwrap();
        assert!(matches!(&updates[..], [RemoteUpdate::Metadata(current)] if *current == metadata));
        assert!(decoder.decode(&key, 1, &frame).is_err());
        let earlier = notice_frame(&key, 1, 1, 1, &Notice::Metadata(&metadata)).unwrap();
        assert!(decoder.decode(&key, 1, &earlier).is_err());
        let resize = Notice::Resize {
            rows: 40,
            cols: 120,
            at_sequence: 10,
        };
        let resize = notice_frame(&key, 1, 1, 2, &resize).unwrap();
        assert_eq!(resize.len(), 29 + 13 + 16);
        assert!(matches!(
            decoder.decode(&key, 1, &resize).unwrap()[..],
            [RemoteUpdate::Resize {
                rows: 40,
                cols: 120,
                at_sequence: 10
            }]
        ));
        assert_eq!(decoder.next_sequence, Some(10));
    }
    #[test]
    fn a_snapshot_travels_packed() {
        let key = [7; 32];
        let checkpoint = crate::terminal::Checkpoint::new(
            crate::terminal::TerminalSize::new(24, 80).unwrap(),
            crate::terminal::TerminalScreen::Primary,
            br#"{"cell":{"text":" ","style":0}},"#.repeat(32_768),
            0,
            0,
            false,
        )
        .unwrap();
        let frame = checkpoint_frame(&key, 1, 0, 1, 10, &checkpoint).unwrap();
        assert!(frame.len() < checkpoint.semantic_checkpoint.len() / 100);
        let mut decoder = FrameDecoder::default();
        assert!(matches!(
            &decoder.decode(&key, 1, &frame).unwrap()[..],
            [RemoteUpdate::Checkpoint { checkpoint: restored, .. }] if *restored == checkpoint
        ));
    }
    #[test]
    fn output_is_one_packed_stream_that_starts_at_the_snapshot() {
        let key = [7; 32];
        let mut packer = OutputPacker::new();
        let line: &[u8] = b"\x1b[32mok\x1b[0m terminal_connections::host::barrier_tests\r\n";
        let first = raw_frame(&key, 1, 0, 10, &[line, line, line], &mut packer).unwrap();
        let second = raw_frame(&key, 1, 1, 13, &[line], &mut packer).unwrap();
        assert!(second.len() < 29 + 16 + line.len() / 2);
        let mut decoder = FrameDecoder::default();
        decoder
            .decode(
                &key,
                1,
                &checkpoint_frame(&key, 1, 0, 1, 10, &checkpoint()).unwrap(),
            )
            .unwrap();
        let sequences = decoder
            .decode(&key, 1, &first)
            .unwrap()
            .into_iter()
            .filter_map(|update| match update {
                RemoteUpdate::Raw { sequence, .. } => Some(sequence),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(sequences, [10, 11, 12]);
        assert!(matches!(
            &decoder.decode(&key, 1, &second).unwrap()[..],
            [RemoteUpdate::Raw { sequence: 13, bytes }] if bytes == line
        ));
        assert_eq!(decoder.next_sequence, Some(14));

        let mut late = FrameDecoder::default();
        late.decode(
            &key,
            1,
            &checkpoint_frame(&key, 1, 1, 2, 13, &checkpoint()).unwrap(),
        )
        .unwrap();
        assert!(late.decode(&key, 1, &second).is_err());
        assert!(
            raw_frame(
                &key,
                1,
                2,
                14,
                &vec![b"x"; RAW_BATCH_LIMIT + 1],
                &mut packer
            )
            .is_err()
        );
    }
    #[test]
    fn replay_gap_and_wrong_generation_are_rejected() {
        let key = [7; 32];
        let mut decoder = FrameDecoder::default();
        let cp = checkpoint_frame(&key, 1, 0, 1, 10, &checkpoint()).unwrap();
        let output = raw(&key, 0, 10, &[b"a"]);
        assert!(decoder.decode(&key, 1, &output).is_err());
        decoder.decode(&key, 1, &cp).unwrap();
        assert!(decoder.decode(&key, 1, &cp).is_err());
        assert!(decoder.decode(&key, 2, &output).is_err());
        assert!(
            decoder
                .decode(&key, 1, &raw(&key, 0, 11, &[b"gap"]))
                .is_err()
        );
        decoder.decode(&key, 1, &output).unwrap();
        assert!(decoder.decode(&key, 1, &output).is_err());
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
        let output = raw(&key, 0, 10, &[b"a"]);
        for offset in [0, 5, 13, 21, 29, output.len() - 1] {
            let mut changed = output.clone();
            changed[offset] ^= 1;
            assert!(decoder.decode(&key, 1, &changed).is_err());
            assert_eq!(decoder.next_sequence, Some(10));
        }
        assert!(
            raw_frame(
                &key,
                1,
                0,
                10,
                &[vec![0; RAW_CHUNK_LIMIT + 1]],
                &mut OutputPacker::new()
            )
            .is_err()
        );
        decoder.decode(&key, 1, &output).unwrap();
    }
}
