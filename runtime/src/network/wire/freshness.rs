use super::{FrameDecoder, SessionDto, crypto, decode_b64, invalid, read_u64};
use crate::network::{Error, RemoteUpdate, Result};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct CaptureIdentity {
    pub session: Uuid,
    pub incarnation: Uuid,
    pub revision: u64,
    pub generation: u32,
    pub connection: Uuid,
    pub user: String,
    pub device: String,
}

impl CaptureIdentity {
    pub(crate) fn new(dto: &SessionDto, connection: Uuid, user: String, device: String) -> Self {
        Self {
            session: dto.id,
            incarnation: dto.incarnation_id,
            revision: dto.authorization_revision,
            generation: dto.key_generation,
            connection,
            user,
            device,
        }
    }
    pub(crate) fn preimage(
        &self,
        challenge: &[u8; 32],
        next: u64,
        digest: &[u8; 32],
    ) -> Result<Vec<u8>> {
        crypto::signed_fields(
            b"kodosi-terminal-checkpoint-proof-v1",
            &[
                challenge,
                self.session.to_string().as_bytes(),
                self.incarnation.to_string().as_bytes(),
                self.revision.to_string().as_bytes(),
                self.generation.to_string().as_bytes(),
                self.connection.to_string().as_bytes(),
                self.user.as_bytes(),
                self.device.as_bytes(),
                next.to_string().as_bytes(),
                digest,
            ],
        )
    }
}

struct Proof {
    digest: [u8; 32],
    signature: Vec<u8>,
}
struct Pending {
    challenge: [u8; 32],
    proof: Option<Proof>,
    deadline: tokio::time::Instant,
}

pub(crate) struct FreshFrames {
    decoder: FrameDecoder,
    identity: CaptureIdentity,
    public: Vec<u8>,
    pending: Option<Pending>,
    discarded_frame: Option<[u8; 32]>,
}

impl FreshFrames {
    pub(crate) fn new(identity: CaptureIdentity, public: Vec<u8>, challenge: [u8; 32]) -> Self {
        Self {
            decoder: FrameDecoder::default(),
            identity,
            public,
            pending: Some(Self::pending(challenge)),
            discarded_frame: None,
        }
    }
    fn pending(challenge: [u8; 32]) -> Pending {
        Pending {
            challenge,
            proof: None,
            deadline: tokio::time::Instant::now() + std::time::Duration::from_secs(10),
        }
    }
    pub(crate) fn next_sequence(&self) -> Option<u64> {
        self.decoder.next_sequence
    }

    pub(crate) fn ready(&self) -> bool {
        self.decoder.next_sequence.is_some()
    }
    pub(crate) fn request(&mut self) -> Result<Option<[u8; 32]>> {
        self.check_deadline()?;
        if self.pending.is_some() {
            return Ok(None);
        }
        let challenge = crypto::random_bytes()?;
        self.pending = Some(Self::pending(challenge));
        Ok(Some(challenge))
    }
    pub(crate) fn check_deadline(&mut self) -> Result<()> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| tokio::time::Instant::now() >= pending.deadline)
        {
            if !self.ready() {
                return Err(invalid(
                    "The host did not prove a fresh terminal snapshot in time.",
                ));
            }
            self.discarded_frame = self
                .pending
                .take()
                .and_then(|pending| pending.proof.map(|proof| proof.digest));
        }
        Ok(())
    }
    pub(crate) fn proof(&mut self, value: &Value) -> Result<()> {
        self.check_deadline()?;
        let challenge = decode_b64(value, "challenge", 32)?;
        let digest = decode_b64(value, "frameSha256", 32)?
            .try_into()
            .map_err(|_| invalid("Invalid snapshot ciphertext hash."))?;
        if self
            .pending
            .as_ref()
            .is_none_or(|pending| challenge.as_slice() != pending.challenge)
        {
            if !self.ready() {
                return Err(invalid("Unrequested terminal snapshot proof."));
            }
            self.discarded_frame = Some(digest);
            return Ok(());
        }
        let pending = self.pending.as_mut().ok_or(Error::Stale)?;
        if pending.proof.is_some() {
            return Err(invalid("Repeated terminal snapshot proof."));
        }
        let signature = decode_b64(value, "signature", 3309)?;
        if signature.len() != 3309 {
            return Err(invalid("Invalid snapshot proof signature."));
        }
        pending.proof = Some(Proof { digest, signature });
        Ok(())
    }
    pub(crate) fn decode(
        &mut self,
        key: &crypto::SessionKey,
        frame: &[u8],
    ) -> Result<Vec<RemoteUpdate>> {
        self.check_deadline()?;
        if frame.len() < 45 || frame.len() > super::FRAME_LIMIT {
            return Err(invalid("Remote terminal frame is outside bounds."));
        }
        let checkpoint = frame.first() == Some(&3);
        let digest: [u8; 32] = Sha256::digest(frame).into();
        if checkpoint && self.discarded_frame == Some(digest) {
            self.discarded_frame = None;
            return Ok(Vec::new());
        }
        let fresh = if checkpoint {
            if let Some(pending) = &self.pending {
                if let Some(proof) = &pending.proof {
                    if proof.digest == digest {
                        crypto::verify_control_message(
                            &self.public,
                            &self.identity.preimage(
                                &pending.challenge,
                                read_u64(frame, 21)?,
                                &digest,
                            )?,
                            &proof.signature,
                        )?;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !self.ready() && !fresh {
            return Err(Error::Trust(
                "The connection service supplied terminal content without a fresh host proof."
                    .into(),
            ));
        }
        let mut updates = self.decoder.decode(key, self.identity.generation, frame)?;
        if fresh {
            self.pending = None;
            for update in &mut updates {
                if let RemoteUpdate::Checkpoint { fresh, .. } = update {
                    *fresh = true;
                }
            }
        }
        Ok(updates)
    }
}

pub(crate) fn frame_hash(frame: &[u8]) -> [u8; 32] {
    Sha256::digest(frame).into()
}

#[cfg(test)]
mod tests {
    use super::super::{checkpoint_frame, raw_frame};
    use super::*;
    use crate::identity::keys::DeviceKeys;
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use serde_json::json;

    fn identity() -> CaptureIdentity {
        CaptureIdentity {
            session: Uuid::now_v7(),
            incarnation: Uuid::now_v7(),
            revision: 1,
            generation: 1,
            connection: Uuid::now_v7(),
            user: Uuid::now_v7().to_string(),
            device: Uuid::now_v7().to_string(),
        }
    }
    fn checkpoint(key: &crypto::SessionKey, counter: u64, next: u64) -> Vec<u8> {
        let cp = crate::terminal::Checkpoint::new(
            crate::terminal::TerminalSize::new(24, 80).unwrap(),
            crate::terminal::TerminalScreen::Primary,
            b"{}".to_vec(),
            0,
            0,
            false,
        )
        .unwrap();
        checkpoint_frame(key, 1, counter, counter + 1, next, &cp).unwrap()
    }
    fn proof(id: &CaptureIdentity, keys: &DeviceKeys, nonce: &[u8; 32], frame: &[u8]) -> Value {
        let digest = frame_hash(frame);
        let signature = crypto::sign_control_message(
            keys.signing_pkcs8(),
            &id.preimage(nonce, read_u64(frame, 21).unwrap(), &digest)
                .unwrap(),
        )
        .unwrap();
        json!({"challenge":BASE64.encode(nonce),"frameSha256":BASE64.encode(digest),"signature":BASE64.encode(signature)})
    }
    #[test]
    fn fresh_capture_domain_matches_current_manifest() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../protocol/crypto-domains.json");
        let manifest: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let domain = manifest["tags"]["TERMINAL_CHECKPOINT_PROOF_V1"]
            .as_str()
            .unwrap();
        assert!(
            identity()
                .preimage(&[0; 32], 0, &[0; 32])
                .unwrap()
                .starts_with(domain.as_bytes())
        );
    }

    #[tokio::test]
    async fn recorded_same_generation_checkpoint_cannot_bootstrap_a_new_viewer() {
        let keys = DeviceKeys::generate().unwrap();
        let key = [1; 32];
        let id = identity();
        let old_nonce = [2; 32];
        let new_nonce = [3; 32];
        let frame = checkpoint(&key, 0, 5);
        let old_proof = proof(&id, &keys, &old_nonce, &frame);
        let mut current = FreshFrames::new(id, keys.signing_public().to_vec(), new_nonce);
        assert!(current.decode(&key, &frame).is_err());
        assert!(
            current
                .decode(&key, &raw_frame(&key, 1, 0, 5, b"old").unwrap())
                .is_err()
        );
        assert!(current.proof(&old_proof).is_err());
        let mut substituted = old_proof;
        substituted["challenge"] = json!(BASE64.encode(new_nonce));
        current.proof(&substituted).unwrap();
        assert!(current.decode(&key, &frame).is_err());
        assert!(!current.ready());
    }
    #[tokio::test]
    async fn valid_capture_keeps_live_raw_and_does_not_consume_an_interleaved_checkpoint() {
        let keys = DeviceKeys::generate().unwrap();
        let key = [1; 32];
        let id = identity();
        let nonce = [2; 32];
        let frame = checkpoint(&key, 0, 5);
        let mut current = FreshFrames::new(id.clone(), keys.signing_public().to_vec(), nonce);
        current.proof(&proof(&id, &keys, &nonce, &frame)).unwrap();
        assert!(matches!(
            current.decode(&key, &frame).unwrap().as_slice(),
            [RemoteUpdate::Checkpoint { fresh: true, .. }]
        ));
        let challenge = current.request().unwrap().unwrap();
        assert!(current.request().unwrap().is_none());
        let capture = checkpoint(&key, 2, 6);
        current
            .proof(&proof(&id, &keys, &challenge, &capture))
            .unwrap();
        assert!(matches!(
            current
                .decode(&key, &checkpoint(&key, 1, 5))
                .unwrap()
                .as_slice(),
            [RemoteUpdate::Checkpoint { fresh: false, .. }]
        ));
        current
            .decode(&key, &raw_frame(&key, 1, 0, 5, b"live").unwrap())
            .unwrap();
        assert!(matches!(
            current.decode(&key, &capture).unwrap().as_slice(),
            [RemoteUpdate::Checkpoint { fresh: true, .. }]
        ));
        assert_eq!(current.decoder.next_sequence, Some(6));
        current
            .proof(&proof(&id, &keys, &challenge, &capture))
            .unwrap();
        assert!(current.decode(&key, &capture).unwrap().is_empty());
    }
    #[tokio::test(start_paused = true)]
    async fn failed_late_capture_does_not_disconnect_healthy_raw_or_complete_new_nonce() {
        let keys = DeviceKeys::generate().unwrap();
        let key = [1; 32];
        let id = identity();
        let nonce = [2; 32];
        let frame = checkpoint(&key, 0, 5);
        let mut current = FreshFrames::new(id.clone(), keys.signing_public().to_vec(), nonce);
        current.proof(&proof(&id, &keys, &nonce, &frame)).unwrap();
        current.decode(&key, &frame).unwrap();
        let old = current.request().unwrap().unwrap();
        tokio::time::advance(std::time::Duration::from_secs(11)).await;
        current.check_deadline().unwrap();
        current
            .decode(&key, &raw_frame(&key, 1, 0, 5, b"healthy").unwrap())
            .unwrap();
        let next = current.request().unwrap().unwrap();
        assert_ne!(old, next);
        let old_frame = checkpoint(&key, 1, 6);
        current.proof(&proof(&id, &keys, &old, &old_frame)).unwrap();
        assert!(current.decode(&key, &old_frame).unwrap().is_empty());
        assert_eq!(current.decoder.next_sequence, Some(6));
        let fresh = checkpoint(&key, 2, 6);
        current.proof(&proof(&id, &keys, &next, &fresh)).unwrap();
        assert!(matches!(
            current.decode(&key, &fresh).unwrap().as_slice(),
            [RemoteUpdate::Checkpoint { fresh: true, .. }]
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn pending_proof_has_fixed_deadline_and_connection_binding() {
        let keys = DeviceKeys::generate().unwrap();
        let key = [1; 32];
        let id = identity();
        let nonce = [2; 32];
        let frame = checkpoint(&key, 0, 5);
        let mut different = id.clone();
        different.connection = Uuid::now_v7();
        let mut current = FreshFrames::new(id, keys.signing_public().to_vec(), nonce);
        current
            .proof(&proof(&different, &keys, &nonce, &frame))
            .unwrap();
        assert!(current.decode(&key, &frame).is_err());
        tokio::time::advance(std::time::Duration::from_secs(11)).await;
        assert!(current.check_deadline().is_err());
        assert!(!current.ready());
    }
}
