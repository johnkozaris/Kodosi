use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use aws_lc_rs::{
    kem::{DecapsulationKey, EncapsulationKey, ML_KEM_768},
    signature::{ML_DSA_65, PqdsaKeyPair, VerificationAlgorithm},
};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{BackendClientError, Result};

pub type SessionKey = [u8; 32];

const KEM_CIPHERTEXT_LEN: usize = 1088;

const KEY_BLOB_VERSION: u8 = 0x01;

const ML_DSA_65_SIG_LEN: usize = 3309;
pub const LEGACY_KEY_BLOB_SIGNATURE_VERSION: u32 = 1;
pub const CURRENT_KEY_BLOB_SIGNATURE_VERSION: u32 = 2;

pub fn generate_session_key() -> Result<SessionKey> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|error| {
        tracing::error!(%error, "getrandom failed while sampling session key");
        BackendClientError::Crypto {
            reason: "platform RNG unavailable; cannot mint session key".to_owned(),
        }
    })?;
    Ok(key)
}

pub fn wrap_session_key(
    viewer_kem_public: &[u8],
    session_key: &SessionKey,
    session_id: &str,
    recipient_device_id: &str,
    key_generation: u32,
) -> Result<Vec<u8>> {
    let encaps_key = EncapsulationKey::new(&ML_KEM_768, viewer_kem_public).map_err(|_| {
        BackendClientError::Crypto {
            reason: "invalid ML-KEM-768 encapsulation key".to_owned(),
        }
    })?;

    let (ciphertext, shared_secret) =
        encaps_key
            .encapsulate()
            .map_err(|_| BackendClientError::Crypto {
                reason: "ML-KEM-768 encapsulation failed".to_owned(),
            })?;

    let wrap_key = derive_wrap_key(shared_secret.as_ref(), session_id)?;

    let cipher =
        Aes256Gcm::new_from_slice(wrap_key.as_ref()).map_err(|e| BackendClientError::Crypto {
            reason: format!("invalid wrap key: {e}"),
        })?;

    let mut nonce_bytes = [0u8; 12];
    getrandom::fill(&mut nonce_bytes).map_err(|error| {
        tracing::error!(%error, "getrandom failed while sampling wrap nonce");
        BackendClientError::Crypto {
            reason: "platform RNG unavailable; cannot mint wrap nonce".to_owned(),
        }
    })?;
    let nonce = Nonce::from(nonce_bytes);

    let aad = build_wrap_aad(session_id, recipient_device_id, key_generation)?;

    let aes_ciphertext = cipher
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: session_key.as_slice(),
                aad: &aad,
            },
        )
        .map_err(|e| BackendClientError::Crypto {
            reason: format!("session key encryption failed: {e}"),
        })?;

    let kem_ct_bytes = ciphertext.as_ref();
    let mut blob = Vec::with_capacity(1 + kem_ct_bytes.len() + 12 + aes_ciphertext.len());
    blob.push(KEY_BLOB_VERSION);
    blob.extend_from_slice(kem_ct_bytes);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&aes_ciphertext);
    Ok(blob)
}

pub fn unwrap_session_key(
    viewer_kem_secret: &DecapsulationKey,
    encrypted_blob: &[u8],
    session_id: &str,
    recipient_device_id: &str,
    key_generation: u32,
) -> Result<SessionKey> {
    let min_len = 1 + KEM_CIPHERTEXT_LEN + 12 + 32 + 16;
    if encrypted_blob.len() < min_len {
        return Err(BackendClientError::Crypto {
            reason: "encrypted session key blob too short".to_owned(),
        });
    }

    if encrypted_blob[0] != KEY_BLOB_VERSION {
        return Err(BackendClientError::Crypto {
            reason: format!(
                "unsupported key blob version {} (expected {})",
                encrypted_blob[0], KEY_BLOB_VERSION
            ),
        });
    }

    let body = &encrypted_blob[1..];
    let kem_ct_bytes = &body[..KEM_CIPHERTEXT_LEN];
    let aes_blob = &body[KEM_CIPHERTEXT_LEN..];

    let kem_ciphertext = aws_lc_rs::kem::Ciphertext::from(kem_ct_bytes);
    let shared_secret =
        viewer_kem_secret
            .decapsulate(kem_ciphertext)
            .map_err(|_| BackendClientError::Crypto {
                reason: "ML-KEM-768 decapsulation failed".to_owned(),
            })?;

    let wrap_key = derive_wrap_key(shared_secret.as_ref(), session_id)?;

    let cipher =
        Aes256Gcm::new_from_slice(wrap_key.as_ref()).map_err(|e| BackendClientError::Crypto {
            reason: format!("invalid wrap key: {e}"),
        })?;

    let nonce = Nonce::try_from(&aes_blob[..12]).map_err(|_| BackendClientError::Protocol {
        reason: "wrapped key nonce must be 12 bytes".to_owned(),
    })?;
    let ciphertext = &aes_blob[12..];
    let aad = build_wrap_aad(session_id, recipient_device_id, key_generation)?;

    let plaintext = cipher
        .decrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|e| BackendClientError::Crypto {
            reason: format!("session key decryption failed: {e}"),
        })?;

    let key: SessionKey = plaintext
        .try_into()
        .map_err(|_| BackendClientError::Crypto {
            reason: "decrypted session key wrong length".to_owned(),
        })?;
    Ok(key)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayStream {
    Checkpoint,

    TerminalRaw,

    TerminalPresentation,

    PendingPermissions,

    Control,

    SemanticRequest,
}

impl RelayStream {
    const fn hkdf_info(self) -> &'static [u8] {
        match self {
            Self::Checkpoint => b"kodosi-terminal-checkpoint-v1",
            Self::TerminalRaw => b"kodosi-terminal-raw-v1",
            Self::TerminalPresentation => b"kodosi-terminal-presentation-v1",
            Self::PendingPermissions => b"kodosi-pending-permissions-v1",
            Self::Control => b"kodosi-control-v1",
            Self::SemanticRequest => b"kodosi-semantic-request-key-v1",
        }
    }
}

pub fn derive_stream_key(session_key: &SessionKey, stream: RelayStream) -> Result<SessionKey> {
    let hkdf = Hkdf::<Sha256>::new(None, session_key);
    let mut okm = [0u8; 32];
    hkdf.expand(stream.hkdf_info(), &mut okm)
        .map_err(|e| BackendClientError::Crypto {
            reason: format!("stream subkey HKDF expansion failed: {e}"),
        })?;
    Ok(okm)
}

pub fn encrypt_frame(
    session_key: &SessionKey,
    key_generation: u32,
    nonce_counter: u64,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(session_key).map_err(|e| BackendClientError::Crypto {
            reason: format!("invalid session key: {e}"),
        })?;

    let nonce = build_frame_nonce(key_generation, nonce_counter);
    let nonce = Nonce::from(nonce);

    cipher
        .encrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|e| BackendClientError::Crypto {
            reason: format!("frame encryption failed: {e}"),
        })
}

pub fn decrypt_frame(
    session_key: &SessionKey,
    key_generation: u32,
    nonce_counter: u64,
    associated_data: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let cipher =
        Aes256Gcm::new_from_slice(session_key).map_err(|e| BackendClientError::Crypto {
            reason: format!("invalid session key: {e}"),
        })?;

    let nonce = build_frame_nonce(key_generation, nonce_counter);
    let nonce = Nonce::from(nonce);

    cipher
        .decrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|e| BackendClientError::Crypto {
            reason: format!("frame decryption failed: {e}"),
        })
}

fn build_frame_nonce(key_generation: u32, counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[..4].copy_from_slice(&key_generation.to_be_bytes());
    nonce[4..12].copy_from_slice(&counter.to_be_bytes());
    nonce
}

pub struct EncryptedControlPayload {
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

pub fn control_associated_data(kind: &str, session_id: &str, action_id: &str) -> Result<Vec<u8>> {
    let kind = kind.as_bytes();
    let session_id = session_id.as_bytes();
    let action_id = action_id.as_bytes();
    let mut aad = Vec::with_capacity(kind.len() + session_id.len() + action_id.len() + 12);
    aad.extend_from_slice(&len_prefix_u32("control kind", kind.len())?);
    aad.extend_from_slice(kind);
    aad.extend_from_slice(&len_prefix_u32("control session_id", session_id.len())?);
    aad.extend_from_slice(session_id);
    aad.extend_from_slice(&len_prefix_u32("control action_id", action_id.len())?);
    aad.extend_from_slice(action_id);
    Ok(aad)
}

pub fn encrypt_control_payload(
    session_key: &SessionKey,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<EncryptedControlPayload> {
    let control_key = derive_stream_key(session_key, RelayStream::Control)?;
    let cipher =
        Aes256Gcm::new_from_slice(&control_key).map_err(|error| BackendClientError::Crypto {
            reason: format!("invalid control key: {error}"),
        })?;
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|error| BackendClientError::Crypto {
        reason: format!("platform RNG unavailable for control nonce: {error}"),
    })?;
    let aes_nonce = Nonce::from(nonce);
    let ciphertext = cipher
        .encrypt(
            &aes_nonce,
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|error| BackendClientError::Crypto {
            reason: format!("control encryption failed: {error}"),
        })?;
    Ok(EncryptedControlPayload { nonce, ciphertext })
}

pub fn decrypt_control_payload(
    session_key: &SessionKey,
    associated_data: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    if nonce.len() != 12 {
        return Err(BackendClientError::Protocol {
            reason: "control nonce must be 12 bytes".to_owned(),
        });
    }

    let control_key = derive_stream_key(session_key, RelayStream::Control)?;
    let cipher =
        Aes256Gcm::new_from_slice(&control_key).map_err(|error| BackendClientError::Crypto {
            reason: format!("invalid control key: {error}"),
        })?;
    let nonce = Nonce::try_from(nonce).map_err(|_| BackendClientError::Protocol {
        reason: "control nonce must be 12 bytes".to_owned(),
    })?;
    cipher
        .decrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|error| BackendClientError::Crypto {
            reason: format!("control decryption failed: {error}"),
        })
}

pub fn encrypt_semantic_request(
    session_key: &SessionKey,
    associated_data: &[u8],
    plaintext: &[u8],
) -> Result<EncryptedControlPayload> {
    encrypt_payload_with_stream_key(
        session_key,
        RelayStream::SemanticRequest,
        associated_data,
        plaintext,
        "semantic request",
    )
}

pub fn decrypt_semantic_request(
    session_key: &SessionKey,
    associated_data: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    decrypt_payload_with_stream_key(
        session_key,
        RelayStream::SemanticRequest,
        associated_data,
        nonce,
        ciphertext,
        "semantic request",
    )
}

fn encrypt_payload_with_stream_key(
    session_key: &SessionKey,
    stream: RelayStream,
    associated_data: &[u8],
    plaintext: &[u8],
    label: &'static str,
) -> Result<EncryptedControlPayload> {
    let key = derive_stream_key(session_key, stream)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|error| BackendClientError::Crypto {
        reason: format!("invalid {label} key: {error}"),
    })?;
    let mut nonce = [0u8; 12];
    getrandom::fill(&mut nonce).map_err(|error| BackendClientError::Crypto {
        reason: format!("platform RNG unavailable for {label} nonce: {error}"),
    })?;
    let ciphertext = cipher
        .encrypt(
            &Nonce::from(nonce),
            aes_gcm::aead::Payload {
                msg: plaintext,
                aad: associated_data,
            },
        )
        .map_err(|error| BackendClientError::Crypto {
            reason: format!("{label} encryption failed: {error}"),
        })?;
    Ok(EncryptedControlPayload { nonce, ciphertext })
}

fn decrypt_payload_with_stream_key(
    session_key: &SessionKey,
    stream: RelayStream,
    associated_data: &[u8],
    nonce: &[u8],
    ciphertext: &[u8],
    label: &'static str,
) -> Result<Vec<u8>> {
    if nonce.len() != 12 {
        return Err(BackendClientError::Protocol {
            reason: format!("{label} nonce must be 12 bytes"),
        });
    }
    let key = derive_stream_key(session_key, stream)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|error| BackendClientError::Crypto {
        reason: format!("invalid {label} key: {error}"),
    })?;
    let nonce = Nonce::try_from(nonce).map_err(|_| BackendClientError::Protocol {
        reason: format!("{label} nonce must be 12 bytes"),
    })?;
    cipher
        .decrypt(
            &nonce,
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad: associated_data,
            },
        )
        .map_err(|error| BackendClientError::Crypto {
            reason: format!("{label} decryption failed: {error}"),
        })
}

pub fn control_message_preimage(
    kind: &str,
    session_id: &str,
    action_id: &str,
    sender_user_id: &str,
    sender_device_id: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let aad = control_associated_data(kind, session_id, action_id)?;
    let sender_user_id = sender_user_id.as_bytes();
    let sender_device_id = sender_device_id.as_bytes();
    let mut preimage = Vec::with_capacity(
        kodosi_domain::domain_tags::CONTROL_MESSAGE_V1.len()
            + aad.len()
            + sender_user_id.len()
            + sender_device_id.len()
            + nonce.len()
            + ciphertext.len()
            + 16,
    );
    preimage.extend_from_slice(kodosi_domain::domain_tags::CONTROL_MESSAGE_V1);
    preimage.extend_from_slice(&aad);
    preimage.extend_from_slice(&len_prefix_u32(
        "control sender_user_id",
        sender_user_id.len(),
    )?);
    preimage.extend_from_slice(sender_user_id);
    preimage.extend_from_slice(&len_prefix_u32(
        "control sender_device_id",
        sender_device_id.len(),
    )?);
    preimage.extend_from_slice(sender_device_id);
    preimage.extend_from_slice(&len_prefix_u32("control nonce", nonce.len())?);
    preimage.extend_from_slice(nonce);
    preimage.extend_from_slice(&len_prefix_u32("control ciphertext", ciphertext.len())?);
    preimage.extend_from_slice(ciphertext);
    Ok(preimage)
}

pub fn sha256_hex(value: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let digest = Sha256::digest(value);
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

pub fn semantic_request_associated_data(
    session_id: &Uuid,
    incarnation_id: &Uuid,
    request_id: &Uuid,
    mode: &str,
    payload_sha256: &str,
    requester_user_id: &Uuid,
    requester_device_id: &str,
) -> Result<Vec<u8>> {
    validate_semantic_metadata(
        incarnation_id,
        request_id,
        mode,
        payload_sha256,
        requester_device_id,
    )?;
    semantic_text_fields_preimage(
        kodosi_domain::domain_tags::SEMANTIC_REQUEST_V1,
        &[
            session_id.hyphenated().to_string(),
            incarnation_id.hyphenated().to_string(),
            request_id.hyphenated().to_string(),
            mode.to_owned(),
            payload_sha256.to_owned(),
            requester_user_id.hyphenated().to_string(),
            requester_device_id.to_owned(),
        ],
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "arguments mirror the signed wire tuple"
)]
pub fn semantic_request_signature_preimage(
    session_id: &Uuid,
    incarnation_id: &Uuid,
    request_id: &Uuid,
    mode: &str,
    payload_sha256: &str,
    requester_user_id: &Uuid,
    requester_device_id: &str,
    nonce: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>> {
    let mut preimage = semantic_request_associated_data(
        session_id,
        incarnation_id,
        request_id,
        mode,
        payload_sha256,
        requester_user_id,
        requester_device_id,
    )?;
    append_length_prefixed(&mut preimage, "semantic request nonce", nonce)?;
    append_length_prefixed(&mut preimage, "semantic request ciphertext", ciphertext)?;
    Ok(preimage)
}

pub fn semantic_cancel_preimage(
    session_id: &Uuid,
    incarnation_id: &Uuid,
    request_id: &Uuid,
    mode: &str,
    payload_sha256: &str,
    requester_user_id: &Uuid,
    requester_device_id: &str,
) -> Result<Vec<u8>> {
    validate_semantic_metadata(
        incarnation_id,
        request_id,
        mode,
        payload_sha256,
        requester_device_id,
    )?;
    semantic_text_fields_preimage(
        kodosi_domain::domain_tags::SEMANTIC_CANCEL_V1,
        &[
            session_id.hyphenated().to_string(),
            incarnation_id.hyphenated().to_string(),
            request_id.hyphenated().to_string(),
            mode.to_owned(),
            payload_sha256.to_owned(),
            requester_user_id.hyphenated().to_string(),
            requester_device_id.to_owned(),
        ],
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "arguments mirror the signed wire tuple"
)]
pub fn semantic_receipt_preimage(
    session_id: &Uuid,
    incarnation_id: &Uuid,
    request_id: &Uuid,
    mode: &str,
    payload_sha256: &str,
    outcome: &str,
    requester_user_id: &Uuid,
    requester_device_id: &str,
    owner_user_id: &Uuid,
    owner_device_id: &str,
) -> Result<Vec<u8>> {
    validate_semantic_metadata(
        incarnation_id,
        request_id,
        mode,
        payload_sha256,
        requester_device_id,
    )?;
    if !matches!(outcome, "injected" | "cancelled" | "deliveryUnknown")
        || owner_device_id.trim().is_empty()
    {
        return Err(BackendClientError::Protocol {
            reason: "semantic receipt outcome or owner device is invalid".to_owned(),
        });
    }
    semantic_text_fields_preimage(
        kodosi_domain::domain_tags::SEMANTIC_RECEIPT_V1,
        &[
            session_id.hyphenated().to_string(),
            incarnation_id.hyphenated().to_string(),
            request_id.hyphenated().to_string(),
            mode.to_owned(),
            payload_sha256.to_owned(),
            outcome.to_owned(),
            requester_user_id.hyphenated().to_string(),
            requester_device_id.to_owned(),
            owner_user_id.hyphenated().to_string(),
            owner_device_id.to_owned(),
        ],
    )
}

pub fn semantic_receipt_ack_preimage(
    session_id: &Uuid,
    incarnation_id: &Uuid,
    request_id: &Uuid,
    requester_user_id: &Uuid,
    requester_device_id: &str,
) -> Result<Vec<u8>> {
    if incarnation_id.is_nil() || request_id.is_nil() || requester_device_id.trim().is_empty() {
        return Err(BackendClientError::Protocol {
            reason: "semantic receipt acknowledgement fields are invalid".to_owned(),
        });
    }
    semantic_text_fields_preimage(
        kodosi_domain::domain_tags::SEMANTIC_RECEIPT_ACK_V1,
        &[
            session_id.hyphenated().to_string(),
            incarnation_id.hyphenated().to_string(),
            request_id.hyphenated().to_string(),
            requester_user_id.hyphenated().to_string(),
            requester_device_id.to_owned(),
        ],
    )
}

fn validate_semantic_metadata(
    incarnation_id: &Uuid,
    request_id: &Uuid,
    mode: &str,
    payload_sha256: &str,
    requester_device_id: &str,
) -> Result<()> {
    if incarnation_id.is_nil()
        || request_id.get_version() != Some(uuid::Version::SortRand)
        || !matches!(mode, "queue" | "steer" | "stopAndSend")
        || payload_sha256.len() != 64
        || !payload_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || requester_device_id.trim().is_empty()
    {
        return Err(BackendClientError::Protocol {
            reason: "semantic request metadata is invalid".to_owned(),
        });
    }
    Ok(())
}

fn semantic_text_fields_preimage(domain: &[u8], fields: &[String]) -> Result<Vec<u8>> {
    let capacity = domain.len() + fields.iter().map(|field| field.len() + 4).sum::<usize>();
    let mut preimage = Vec::with_capacity(capacity);
    preimage.extend_from_slice(domain);
    for field in fields {
        append_length_prefixed(&mut preimage, "semantic preimage field", field.as_bytes())?;
    }
    Ok(preimage)
}

fn append_length_prefixed(target: &mut Vec<u8>, label: &'static str, value: &[u8]) -> Result<()> {
    target.extend_from_slice(&len_prefix_u32(label, value.len())?);
    target.extend_from_slice(value);
    Ok(())
}

pub fn device_connection_proof_preimage(
    user_id: &str,
    device_id: &str,
    connection_id: &str,
    purpose: &str,
    session_id: Option<&str>,
    incarnation_id: Option<&Uuid>,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    if user_id.trim().is_empty()
        || device_id.trim().is_empty()
        || connection_id.trim().is_empty()
        || purpose.trim().is_empty()
        || challenge.len() != 32
    {
        return Err(BackendClientError::Protocol {
            reason: "device connection proof identity and challenge must be complete".to_owned(),
        });
    }
    let mut preimage = Vec::new();
    preimage.extend_from_slice(kodosi_domain::domain_tags::DEVICE_CONNECTION_PROOF_V1);
    for value in [
        user_id,
        device_id,
        connection_id,
        purpose,
        session_id.unwrap_or(""),
    ] {
        append_length_prefixed(
            &mut preimage,
            "device connection proof field",
            value.as_bytes(),
        )?;
    }
    let incarnation = incarnation_id.map(|id| id.hyphenated().to_string());
    append_length_prefixed(
        &mut preimage,
        "device connection proof incarnation",
        incarnation.as_deref().unwrap_or("").as_bytes(),
    )?;
    preimage.extend_from_slice(challenge);
    Ok(preimage)
}

pub fn sign_device_connection_proof(
    signing_pkcs8: &[u8],
    user_id: &str,
    device_id: &str,
    connection_id: &str,
    purpose: &str,
    session_id: Option<&str>,
    incarnation_id: Option<&Uuid>,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    let preimage = device_connection_proof_preimage(
        user_id,
        device_id,
        connection_id,
        purpose,
        session_id,
        incarnation_id,
        challenge,
    )?;
    sign_control_message(signing_pkcs8, &preimage)
}

pub fn device_http_request_proof_preimage(
    user_id: &str,
    device_id: &str,
    challenge_id: &Uuid,
    method: &str,
    canonical_path_and_query: &str,
    body_sha256: &str,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    if user_id.trim().is_empty()
        || device_id.trim().is_empty()
        || challenge_id.is_nil()
        || method.trim().is_empty()
        || canonical_path_and_query.trim().is_empty()
        || body_sha256.len() != 64
        || !body_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || challenge.len() != 32
    {
        return Err(BackendClientError::Protocol {
            reason: "device HTTP request proof tuple is invalid".to_owned(),
        });
    }
    let mut preimage = Vec::new();
    preimage.extend_from_slice(kodosi_domain::domain_tags::DEVICE_HTTP_REQUEST_PROOF_V1);
    for value in [
        user_id,
        device_id,
        &challenge_id.hyphenated().to_string(),
        &method.to_ascii_uppercase(),
        canonical_path_and_query,
        body_sha256,
    ] {
        append_length_prefixed(
            &mut preimage,
            "device HTTP request proof field",
            value.as_bytes(),
        )?;
    }
    preimage.extend_from_slice(challenge);
    Ok(preimage)
}

pub fn sign_device_http_request_proof(
    signing_pkcs8: &[u8],
    user_id: &str,
    device_id: &str,
    challenge_id: &Uuid,
    method: &str,
    canonical_path_and_query: &str,
    body_sha256: &str,
    challenge: &[u8],
) -> Result<Vec<u8>> {
    let preimage = device_http_request_proof_preimage(
        user_id,
        device_id,
        challenge_id,
        method,
        canonical_path_and_query,
        body_sha256,
        challenge,
    )?;
    sign_control_message(signing_pkcs8, &preimage)
}

pub fn verify_control_message(
    verifying_key_bytes: &[u8],
    preimage: &[u8],
    signature_bytes: &[u8],
) -> Result<()> {
    ML_DSA_65
        .verify_sig(verifying_key_bytes, preimage, signature_bytes)
        .map_err(|_| BackendClientError::Crypto {
            reason: "control message signature verification failed".to_owned(),
        })
}

pub fn sign_control_message(signing_pkcs8: &[u8], preimage: &[u8]) -> Result<Vec<u8>> {
    let signing_key =
        PqdsaKeyPair::from_pkcs8(&aws_lc_rs::signature::ML_DSA_65_SIGNING, signing_pkcs8).map_err(
            |_| BackendClientError::Crypto {
                reason: "invalid control signing key".to_owned(),
            },
        )?;
    let mut signature = vec![0u8; ML_DSA_65_SIG_LEN];
    let length =
        signing_key
            .sign(preimage, &mut signature)
            .map_err(|_| BackendClientError::Crypto {
                reason: "control message signing failed".to_owned(),
            })?;
    signature.truncate(length);
    Ok(signature)
}

#[cfg(test)]
pub(crate) fn sign_key_blob_v1(
    signing_key: &PqdsaKeyPair,
    session_id: &str,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    key_generation: u32,
    issued_at_ms: u64,
) -> Result<Vec<u8>> {
    let digest = key_blob_digest_v1(
        session_id,
        recipient_device_id,
        encrypted_blob,
        key_generation,
        issued_at_ms,
    )?;
    let mut signature_buf = vec![0u8; ML_DSA_65_SIG_LEN];
    let sig_len =
        signing_key
            .sign(&digest, &mut signature_buf)
            .map_err(|_| BackendClientError::Crypto {
                reason: "ML-DSA-65 signing failed".to_owned(),
            })?;
    signature_buf.truncate(sig_len);
    Ok(signature_buf)
}

pub fn sign_key_blob_v2(
    signing_key: &PqdsaKeyPair,
    session_id: &str,
    incarnation_id: &Uuid,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    key_generation: u32,
    issued_at_ms: u64,
) -> Result<Vec<u8>> {
    let digest = key_blob_digest_v2(
        session_id,
        incarnation_id,
        recipient_device_id,
        encrypted_blob,
        key_generation,
        issued_at_ms,
    )?;
    let mut signature_buf = vec![0u8; ML_DSA_65_SIG_LEN];
    let sig_len =
        signing_key
            .sign(&digest, &mut signature_buf)
            .map_err(|_| BackendClientError::Crypto {
                reason: "ML-DSA-65 signing failed".to_owned(),
            })?;
    signature_buf.truncate(sig_len);
    Ok(signature_buf)
}

fn pop_signed_preimage(challenge_bytes: &[u8]) -> Vec<u8> {
    use kodosi_domain::domain_tags::DEVICE_POP_V1;
    let mut preimage = Vec::with_capacity(DEVICE_POP_V1.len() + challenge_bytes.len());
    preimage.extend_from_slice(DEVICE_POP_V1);
    preimage.extend_from_slice(challenge_bytes);
    preimage
}

pub fn sign_pop_challenge(signing_key: &PqdsaKeyPair, challenge_bytes: &[u8]) -> Result<Vec<u8>> {
    let preimage = pop_signed_preimage(challenge_bytes);
    let mut signature_buf = vec![0u8; ML_DSA_65_SIG_LEN];
    let sig_len = signing_key
        .sign(&preimage, &mut signature_buf)
        .map_err(|_| BackendClientError::Crypto {
            reason: "ML-DSA-65 PoP signing failed".to_owned(),
        })?;
    signature_buf.truncate(sig_len);
    Ok(signature_buf)
}

#[cfg(test)]
pub(crate) fn verify_key_blob_signature_v1(
    verifying_key_bytes: &[u8],
    session_id: &str,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    key_generation: u32,
    issued_at_ms: u64,
    signature_bytes: &[u8],
) -> Result<()> {
    let digest = key_blob_digest_v1(
        session_id,
        recipient_device_id,
        encrypted_blob,
        key_generation,
        issued_at_ms,
    )?;
    ML_DSA_65
        .verify_sig(verifying_key_bytes, &digest, signature_bytes)
        .map_err(|_| BackendClientError::Crypto {
            reason: "key blob signature verification failed".to_owned(),
        })
}

#[allow(clippy::too_many_arguments)]
pub fn verify_key_blob_signature_for_incarnation(
    verifying_key_bytes: &[u8],
    session_id: &str,
    incarnation_id: &Uuid,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    key_generation: u32,
    issued_at_ms: u64,
    signature_version: u32,
    signature_bytes: &[u8],
) -> Result<()> {
    let digest = match signature_version {
        LEGACY_KEY_BLOB_SIGNATURE_VERSION => key_blob_digest_v1(
            session_id,
            recipient_device_id,
            encrypted_blob,
            key_generation,
            issued_at_ms,
        )?,
        CURRENT_KEY_BLOB_SIGNATURE_VERSION => key_blob_digest_v2(
            session_id,
            incarnation_id,
            recipient_device_id,
            encrypted_blob,
            key_generation,
            issued_at_ms,
        )?,
        version => {
            return Err(BackendClientError::Protocol {
                reason: format!("unsupported key blob signature version {version}"),
            });
        }
    };
    ML_DSA_65
        .verify_sig(verifying_key_bytes, &digest, signature_bytes)
        .map_err(|_| BackendClientError::Crypto {
            reason: "key blob signature verification failed".to_owned(),
        })
}

fn len_prefix_u32(label: &'static str, len: usize) -> Result<[u8; 4]> {
    u32::try_from(len)
        .map(u32::to_be_bytes)
        .map_err(|_| BackendClientError::Crypto {
            reason: format!("{label} exceeds u32 length prefix ({len} bytes)"),
        })
}

fn key_blob_digest_v1(
    session_id: &str,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    key_generation: u32,
    issued_at_ms: u64,
) -> Result<Vec<u8>> {
    let mut hasher = Sha256::new();
    hasher.update(kodosi_domain::domain_tags::SESSION_KEY_V2);

    hasher.update(8u32.to_be_bytes());
    hasher.update(issued_at_ms.to_be_bytes());

    hasher.update(4u32.to_be_bytes());
    hasher.update(key_generation.to_be_bytes());

    let sid = session_id.as_bytes();
    let did = recipient_device_id.as_bytes();
    hasher.update(len_prefix_u32("session_id", sid.len())?);
    hasher.update(sid);
    hasher.update(len_prefix_u32("recipient_device_id", did.len())?);
    hasher.update(did);
    hasher.update(len_prefix_u32("encrypted_blob", encrypted_blob.len())?);
    hasher.update(encrypted_blob);
    Ok(hasher.finalize().to_vec())
}

fn key_blob_digest_v2(
    session_id: &str,
    incarnation_id: &Uuid,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    key_generation: u32,
    issued_at_ms: u64,
) -> Result<Vec<u8>> {
    let mut hasher = Sha256::new();
    hasher.update(kodosi_domain::domain_tags::SESSION_KEY_BLOB_V2);

    hasher.update(8u32.to_be_bytes());
    hasher.update(issued_at_ms.to_be_bytes());

    hasher.update(4u32.to_be_bytes());
    hasher.update(key_generation.to_be_bytes());

    let sid = session_id.as_bytes();
    let incarnation = incarnation_id.hyphenated().to_string();
    let incarnation = incarnation.as_bytes();
    let did = recipient_device_id.as_bytes();
    hasher.update(len_prefix_u32("session_id", sid.len())?);
    hasher.update(sid);
    hasher.update(len_prefix_u32("incarnation_id", incarnation.len())?);
    hasher.update(incarnation);
    hasher.update(len_prefix_u32("recipient_device_id", did.len())?);
    hasher.update(did);
    hasher.update(len_prefix_u32("encrypted_blob", encrypted_blob.len())?);
    hasher.update(encrypted_blob);
    Ok(hasher.finalize().to_vec())
}

fn build_wrap_aad(
    session_id: &str,
    recipient_device_id: &str,
    key_generation: u32,
) -> Result<Vec<u8>> {
    let sid = session_id.as_bytes();
    let did = recipient_device_id.as_bytes();
    let sid_len = len_prefix_u32("session_id", sid.len())?;
    let did_len = len_prefix_u32("recipient_device_id", did.len())?;
    let mut aad = Vec::with_capacity(4 + sid.len() + 4 + did.len() + 4);
    aad.extend_from_slice(&sid_len);
    aad.extend_from_slice(sid);
    aad.extend_from_slice(&did_len);
    aad.extend_from_slice(did);
    aad.extend_from_slice(&key_generation.to_be_bytes());
    Ok(aad)
}

fn derive_wrap_key(shared_secret: &[u8], session_id: &str) -> Result<zeroize::Zeroizing<[u8; 32]>> {
    let hkdf = Hkdf::<Sha256>::new(Some(session_id.as_bytes()), shared_secret);
    let mut okm = zeroize::Zeroizing::new([0u8; 32]);
    hkdf.expand(b"kodosi-session-key-wrap", okm.as_mut())
        .map_err(|e| BackendClientError::Crypto {
            reason: format!("HKDF expansion failed: {e}"),
        })?;
    Ok(okm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING};

    #[test]
    fn device_http_request_proof_rejects_cross_request_replay() {
        let key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("key");
        let pkcs8 = key.to_pkcs8v1().expect("pkcs8");
        let challenge_id = Uuid::parse_str("01900000-0000-7000-8000-000000000002").unwrap();
        let challenge = [9_u8; 32];
        let preimage = device_http_request_proof_preimage(
            "01900000-0000-7000-8000-000000000001",
            "device-a",
            &challenge_id,
            "GET",
            "/api/me/semantic-receipts?limit=32",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            &challenge,
        )
        .unwrap();
        let signature = sign_control_message(pkcs8.as_ref(), &preimage).unwrap();
        verify_control_message(key.public_key().as_ref(), &preimage, &signature).unwrap();

        for changed in [
            device_http_request_proof_preimage(
                "01900000-0000-7000-8000-000000000001",
                "device-b",
                &challenge_id,
                "GET",
                "/api/me/semantic-receipts?limit=32",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                &challenge,
            ),
            device_http_request_proof_preimage(
                "01900000-0000-7000-8000-000000000001",
                "device-a",
                &challenge_id,
                "GET",
                "/api/sessions/x/keys/mine",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                &challenge,
            ),
            device_http_request_proof_preimage(
                "01900000-0000-7000-8000-000000000001",
                "device-a",
                &Uuid::now_v7(),
                "GET",
                "/api/me/semantic-receipts?limit=32",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                &challenge,
            ),
        ] {
            assert!(
                verify_control_message(key.public_key().as_ref(), &changed.unwrap(), &signature)
                    .is_err()
            );
        }
    }

    #[test]
    fn device_connection_proof_rejects_cross_context_replay() {
        let key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).expect("key");
        let pkcs8 = key.to_pkcs8v1().expect("pkcs8");
        let challenge = [7_u8; 32];
        let session = Uuid::parse_str("01900000-0000-7000-8000-000000000002").unwrap();
        let incarnation = Uuid::parse_str("01900000-0000-7000-8000-000000000003").unwrap();
        let preimage = device_connection_proof_preimage(
            "01900000-0000-7000-8000-000000000001",
            "device-a",
            "connection-a",
            "host",
            Some(&session.to_string()),
            Some(&incarnation),
            &challenge,
        )
        .unwrap();
        let signature = sign_control_message(pkcs8.as_ref(), &preimage).unwrap();
        verify_control_message(key.public_key().as_ref(), &preimage, &signature).unwrap();

        for changed in [
            device_connection_proof_preimage(
                "01900000-0000-7000-8000-000000000001",
                "device-b",
                "connection-a",
                "host",
                Some(&session.to_string()),
                Some(&incarnation),
                &challenge,
            ),
            device_connection_proof_preimage(
                "01900000-0000-7000-8000-000000000001",
                "device-a",
                "connection-a",
                "participant",
                Some(&session.to_string()),
                Some(&incarnation),
                &challenge,
            ),
            device_connection_proof_preimage(
                "01900000-0000-7000-8000-000000000001",
                "device-a",
                "connection-b",
                "host",
                Some(&session.to_string()),
                Some(&incarnation),
                &challenge,
            ),
        ] {
            assert!(
                verify_control_message(key.public_key().as_ref(), &changed.unwrap(), &signature)
                    .is_err()
            );
        }
    }

    #[test]
    fn stream_subkeys_differ_and_are_deterministic() {
        let session_key: SessionKey = [7u8; 32];
        let checkpoint =
            derive_stream_key(&session_key, RelayStream::Checkpoint).expect("checkpoint subkey");
        let raw = derive_stream_key(&session_key, RelayStream::TerminalRaw).expect("raw subkey");
        let presentation = derive_stream_key(&session_key, RelayStream::TerminalPresentation)
            .expect("presentation subkey");
        let pending_permissions = derive_stream_key(&session_key, RelayStream::PendingPermissions)
            .expect("pending-permissions subkey");
        let control =
            derive_stream_key(&session_key, RelayStream::Control).expect("control subkey");
        let semantic =
            derive_stream_key(&session_key, RelayStream::SemanticRequest).expect("semantic subkey");
        let keys = [
            checkpoint,
            raw,
            presentation,
            pending_permissions,
            control,
            semantic,
        ];
        for (index, key) in keys.iter().enumerate() {
            assert_ne!(*key, session_key);
            for other in &keys[index + 1..] {
                assert_ne!(key, other, "relay domains must never share a key");
            }
        }
        assert_eq!(
            checkpoint,
            derive_stream_key(&session_key, RelayStream::Checkpoint).expect("deterministic"),
        );
        let ciphertext = encrypt_frame(&checkpoint, 1, 42, b"header", b"frame").expect("encrypt");
        assert!(decrypt_frame(&raw, 1, 42, b"header", &ciphertext).is_err());
        assert!(decrypt_frame(&presentation, 1, 42, b"header", &ciphertext).is_err());
    }

    #[test]
    #[allow(clippy::print_stdout, reason = "opt-in dev emission, not CI output")]
    fn emit_wire_format_parity_vectors() {
        if std::env::var("KODOSI_EMIT_PQ_VECTORS").is_err() {
            return;
        }

        let digest = key_blob_digest_v1("sess", "dev", &[1, 2, 3], 1, 0).unwrap();
        let aad = build_wrap_aad("sess", "dev", 1).unwrap();
        let ss = vec![0x11u8; 32];
        let key = derive_wrap_key(&ss, "sess").unwrap();
        println!("DIGEST={}", hex_encode(&digest));
        println!("AAD={}", hex_encode(&aad));
        println!("WRAPKEY={}", hex_encode(&*key));
    }

    #[test]
    fn semantic_preimages_match_cross_language_vectors() {
        let session_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000001").expect("session UUID");
        let incarnation_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000002").expect("incarnation UUID");
        let request_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000003").expect("request UUID");
        let requester_user_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000004").expect("requester UUID");
        let owner_user_id =
            Uuid::parse_str("01900000-0000-7000-8000-000000000005").expect("owner UUID");
        let fingerprint = "a".repeat(64);
        let aad = semantic_request_associated_data(
            &session_id,
            &incarnation_id,
            &request_id,
            "stopAndSend",
            &fingerprint,
            &requester_user_id,
            "device-α",
        )
        .expect("request AAD");
        assert_eq!(
            hex_encode(&aad),
            "6b6f646f73692d73656d616e7469632d726571756573742d76310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030320000002430313930303030302d303030302d373030302d383030302d3030303030303030303030330000000b73746f70416e6453656e6400000040616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161610000002430313930303030302d303030302d373030302d383030302d303030303030303030303034000000096465766963652dceb1"
        );
        let request = semantic_request_signature_preimage(
            &session_id,
            &incarnation_id,
            &request_id,
            "stopAndSend",
            &fingerprint,
            &requester_user_id,
            "device-α",
            &(0_u8..12).collect::<Vec<_>>(),
            b"cipher",
        )
        .expect("request signature preimage");
        assert_eq!(
            hex_encode(&request),
            "6b6f646f73692d73656d616e7469632d726571756573742d76310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030320000002430313930303030302d303030302d373030302d383030302d3030303030303030303030330000000b73746f70416e6453656e6400000040616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161610000002430313930303030302d303030302d373030302d383030302d303030303030303030303034000000096465766963652dceb10000000c000102030405060708090a0b00000006636970686572"
        );
        let cancel = semantic_cancel_preimage(
            &session_id,
            &incarnation_id,
            &request_id,
            "stopAndSend",
            &fingerprint,
            &requester_user_id,
            "device-α",
        )
        .expect("cancel preimage");
        assert_eq!(
            hex_encode(&cancel),
            "6b6f646f73692d73656d616e7469632d63616e63656c2d76310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030320000002430313930303030302d303030302d373030302d383030302d3030303030303030303030330000000b73746f70416e6453656e6400000040616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161610000002430313930303030302d303030302d373030302d383030302d303030303030303030303034000000096465766963652dceb1"
        );
        let receipt = semantic_receipt_preimage(
            &session_id,
            &incarnation_id,
            &request_id,
            "stopAndSend",
            &fingerprint,
            "deliveryUnknown",
            &requester_user_id,
            "device-α",
            &owner_user_id,
            "owner-device",
        )
        .expect("receipt preimage");
        assert_eq!(
            hex_encode(&receipt),
            "6b6f646f73692d73656d616e7469632d726563656970742d76310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030320000002430313930303030302d303030302d373030302d383030302d3030303030303030303030330000000b73746f70416e6453656e6400000040616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161616161610000000f64656c6976657279556e6b6e6f776e0000002430313930303030302d303030302d373030302d383030302d303030303030303030303034000000096465766963652dceb10000002430313930303030302d303030302d373030302d383030302d3030303030303030303030350000000c6f776e65722d646576696365"
        );
    }

    #[test]
    fn semantic_request_uses_separate_key_and_round_trips() {
        let session_key = [7_u8; 32];
        let aad = b"semantic-aad";
        let sealed = encrypt_semantic_request(&session_key, aad, b"hello").expect("seal");
        assert_eq!(
            decrypt_semantic_request(&session_key, aad, &sealed.nonce, &sealed.ciphertext,)
                .expect("open"),
            b"hello"
        );
        assert!(
            decrypt_control_payload(&session_key, aad, &sealed.nonce, &sealed.ciphertext,).is_err()
        );
    }

    #[test]
    fn semantic_receipt_ack_preimage_matches_dotnet_parity_vector() {
        let preimage = semantic_receipt_ack_preimage(
            &Uuid::parse_str("01900000-0000-7000-8000-000000000001").expect("session UUID"),
            &Uuid::parse_str("01900000-0000-7000-8000-000000000002").expect("incarnation UUID"),
            &Uuid::parse_str("01900000-0000-7000-8000-000000000003").expect("request UUID"),
            &Uuid::parse_str("01900000-0000-7000-8000-000000000004").expect("user UUID"),
            "device-α",
        )
        .expect("preimage");

        assert_eq!(
            hex_encode(&preimage),
            "6b6f646f73692d73656d616e7469632d726563656970742d61636b2d76310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030310000002430313930303030302d303030302d373030302d383030302d3030303030303030303030320000002430313930303030302d303030302d373030302d383030302d3030303030303030303030330000002430313930303030302d303030302d373030302d383030302d303030303030303030303034000000096465766963652dceb1"
        );
    }

    #[test]
    fn key_blob_digest_v2_matches_dotnet_parity_vector() {
        let digest = key_blob_digest_v2(
            "sess",
            &Uuid::parse_str("01900000-0000-7000-8000-000000000002").expect("incarnation UUID"),
            "dev",
            &[1, 2, 3],
            1,
            0,
        )
        .expect("digest");

        assert_eq!(
            hex_encode(&digest),
            "f8cdc6ee91d6d3ea2502a71d3012e255c1b8ca232de151788530284fb4d4efdc"
        );
    }

    fn hex_encode(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            write!(s, "{b:02x}").expect("writing to String never fails");
        }
        s
    }

    #[test]
    fn session_key_wrap_unwrap_round_trip() {
        let viewer_dk = DecapsulationKey::generate(&ML_KEM_768).unwrap();
        let viewer_public = viewer_dk
            .encapsulation_key()
            .unwrap()
            .key_bytes()
            .unwrap()
            .as_ref()
            .to_vec();

        let session_key = generate_session_key().expect("test RNG");
        let session_id = "test-session-123";
        let device_id = "device-abc";
        let key_gen: u32 = 7;

        let blob = wrap_session_key(&viewer_public, &session_key, session_id, device_id, key_gen)
            .expect("wrap should succeed");

        let unwrapped = unwrap_session_key(&viewer_dk, &blob, session_id, device_id, key_gen)
            .expect("unwrap should succeed");

        assert_eq!(session_key, unwrapped);
    }

    #[test]
    fn wrong_viewer_key_fails_unwrap() {
        let viewer_dk = DecapsulationKey::generate(&ML_KEM_768).unwrap();
        let viewer_public = viewer_dk
            .encapsulation_key()
            .unwrap()
            .key_bytes()
            .unwrap()
            .as_ref()
            .to_vec();
        let wrong_dk = DecapsulationKey::generate(&ML_KEM_768).unwrap();

        let session_key = generate_session_key().expect("test RNG");
        let blob = wrap_session_key(&viewer_public, &session_key, "session-1", "device", 1)
            .expect("wrap should succeed");

        let result = unwrap_session_key(&wrong_dk, &blob, "session-1", "device", 1);
        assert!(result.is_err());
    }

    #[test]
    fn hostile_backend_blob_substitution_across_contexts_fails_unwrap() {
        let viewer_dk = DecapsulationKey::generate(&ML_KEM_768).unwrap();
        let viewer_public = viewer_dk
            .encapsulation_key()
            .unwrap()
            .key_bytes()
            .unwrap()
            .as_ref()
            .to_vec();

        let session_key = generate_session_key().expect("test RNG");
        let blob = wrap_session_key(&viewer_public, &session_key, "sessA", "dev1", 5)
            .expect("wrap should succeed");

        assert!(unwrap_session_key(&viewer_dk, &blob, "sessB", "dev1", 5).is_err());
        assert!(unwrap_session_key(&viewer_dk, &blob, "sessA", "dev2", 5).is_err());
        assert!(unwrap_session_key(&viewer_dk, &blob, "sessA", "dev1", 6).is_err());
        assert!(unwrap_session_key(&viewer_dk, &blob, "sessA", "dev1", 5).is_ok());
    }

    #[test]
    fn frame_encrypt_decrypt_round_trip() {
        let session_key = generate_session_key().expect("test RNG");
        let plaintext = b"hello terminal content with ANSI \x1b[31mred\x1b[0m";

        let ciphertext = encrypt_frame(&session_key, 1, 42, b"header", plaintext)
            .expect("encrypt should succeed");
        let decrypted = decrypt_frame(&session_key, 1, 42, b"header", &ciphertext)
            .expect("decrypt should succeed");

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn wrong_nonce_fails_decrypt() {
        let session_key = generate_session_key().expect("test RNG");
        let plaintext = b"secret data";

        let ciphertext = encrypt_frame(&session_key, 1, 42, b"header", plaintext)
            .expect("encrypt should succeed");

        let result = decrypt_frame(&session_key, 1, 43, b"header", &ciphertext);
        assert!(result.is_err());

        let result = decrypt_frame(&session_key, 2, 42, b"header", &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn frame_decryption_rejects_modified_associated_data() {
        let session_key = [7u8; 32];
        let ciphertext = encrypt_frame(&session_key, 1, 42, b"header-a", b"frame")
            .expect("encrypt should succeed");

        assert!(decrypt_frame(&session_key, 1, 42, b"header-b", &ciphertext).is_err());
    }

    #[test]
    fn control_payload_is_encrypted_and_device_authenticated() {
        let session_key = [7u8; 32];
        let signing_key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let signing_pkcs8 = signing_key.to_pkcs8v1().unwrap();
        let aad = control_associated_data("inject", "session-1", "action-1").unwrap();
        let sealed = encrypt_control_payload(&session_key, &aad, b"secret prompt").unwrap();
        let preimage = control_message_preimage(
            "inject",
            "session-1",
            "action-1",
            "user-1",
            "device-1",
            &sealed.nonce,
            &sealed.ciphertext,
        )
        .unwrap();
        let signature = sign_control_message(signing_pkcs8.as_ref(), &preimage).unwrap();
        let trust = crate::control::ControlTrustStore::default();
        trust.replace(std::collections::HashMap::from([(
            ("user-1".to_owned(), "device-1".to_owned()),
            crate::control::ControlTrustEntry::new(
                signing_key.public_key().as_ref().to_vec(),
                kodosi_domain::permissions::SessionCapabilities::from_access(
                    kodosi_domain::permissions::AccessLevel::Inject,
                    false,
                ),
            ),
        )]));

        assert_eq!(
            trust.authorize(
                "user-1",
                "device-1",
                &preimage,
                &signature,
                kodosi_domain::permissions::SessionCapabilities::SEND_INPUT,
            ),
            crate::control::ControlAuthorization::Authorized
        );
        assert_eq!(
            trust.authorize(
                "user-2",
                "device-1",
                &preimage,
                &signature,
                kodosi_domain::permissions::SessionCapabilities::SEND_INPUT,
            ),
            crate::control::ControlAuthorization::Untrusted
        );
        assert_eq!(
            decrypt_control_payload(&session_key, &aad, &sealed.nonce, &sealed.ciphertext).unwrap(),
            b"secret prompt",
        );
    }

    #[test]
    fn nonce_uniqueness() {
        let n1 = build_frame_nonce(1, 0);
        let n2 = build_frame_nonce(1, 1);
        let n3 = build_frame_nonce(2, 0);
        assert_ne!(n1, n2);
        assert_ne!(n1, n3);
        assert_ne!(n2, n3);
    }

    #[test]
    fn sign_verify_key_blob_v1_round_trip() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let session_id = "session-xyz";
        let device_id = "device-abc";
        let blob = b"encrypted session key data";
        let key_gen: u32 = 3;
        let issued_at_ms: u64 = 1_700_000_000_000;

        let signature =
            sign_key_blob_v1(&kp, session_id, device_id, blob, key_gen, issued_at_ms).unwrap();

        let pub_bytes = kp.public_key().as_ref();
        verify_key_blob_signature_v1(
            pub_bytes,
            session_id,
            device_id,
            blob,
            key_gen,
            issued_at_ms,
            &signature,
        )
        .unwrap();
    }

    #[test]
    fn wrong_signature_fails_verification() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let other_kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let blob = b"encrypted data";

        let signature =
            sign_key_blob_v1(&kp, "session", "device", blob, 1, 1_700_000_000_000).unwrap();

        let result = verify_key_blob_signature_v1(
            other_kp.public_key().as_ref(),
            "session",
            "device",
            blob,
            1,
            1_700_000_000_000,
            &signature,
        );
        assert!(result.is_err());
    }

    #[test]
    fn signature_rejects_replay_at_different_key_generation() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let blob = b"encrypted data";
        let signature = sign_key_blob_v1(&kp, "s", "d", blob, 5, 1_700_000_000_000).unwrap();

        let pub_bytes = kp.public_key().as_ref();
        let result = verify_key_blob_signature_v1(
            pub_bytes,
            "s",
            "d",
            blob,
            6,
            1_700_000_000_000,
            &signature,
        );
        assert!(result.is_err());
    }

    #[test]
    fn signature_rejects_replay_at_different_issued_at() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let blob = b"encrypted data";
        let signature = sign_key_blob_v1(&kp, "s", "d", blob, 1, 1_700_000_000_000).unwrap();

        let pub_bytes = kp.public_key().as_ref();
        let result = verify_key_blob_signature_v1(
            pub_bytes,
            "s",
            "d",
            blob,
            1,
            1_800_000_000_000,
            &signature,
        );
        assert!(result.is_err());
    }

    #[test]
    fn sign_pop_challenge_round_trip() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let challenge = b"\xaa\xbb\xcc\xdd\xee\xff\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99\
                          \xaa\xbb\xcc\xdd\xee\xff\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99";

        let signature = sign_pop_challenge(&kp, challenge).unwrap();
        let preimage = pop_signed_preimage(challenge);
        let pub_bytes = kp.public_key().as_ref();
        ML_DSA_65
            .verify_sig(pub_bytes, &preimage, &signature)
            .expect("PoP signature must verify against the domain-tagged preimage");
    }

    #[test]
    fn sign_pop_challenge_wrong_message_fails() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let challenge = [0u8; 32];
        let tampered = [0xFFu8; 32];

        let signature = sign_pop_challenge(&kp, &challenge).unwrap();
        let pub_bytes = kp.public_key().as_ref();
        let tampered_preimage = pop_signed_preimage(&tampered);
        let result = ML_DSA_65.verify_sig(pub_bytes, &tampered_preimage, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn sign_pop_challenge_rejects_unprefixed_verification() {
        use aws_lc_rs::signature::{KeyPair, ML_DSA_65, ML_DSA_65_SIGNING};
        let kp = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let challenge = [0x42u8; 32];

        let signature = sign_pop_challenge(&kp, &challenge).unwrap();
        let pub_bytes = kp.public_key().as_ref();
        let result = ML_DSA_65.verify_sig(pub_bytes, &challenge, &signature);
        assert!(result.is_err());
    }
}
