use std::{error::Error, fmt, future::Future, pin::Pin};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

use crate::{
    BackendClientError, Result, crypto,
    dto::session_keys::{SessionKeyBlobDto, SessionKeyFetchStateDto},
    http_client::BackendHttpClient,
    session_key_access::{
        format_session_key_fetch_state_reason, session_key_fetch_access_state,
        session_key_fetch_failure, session_key_fetch_state_retryable,
    },
};
use kodosi_domain::lifecycle::{RemoteSessionAccessIssue, RemoteSessionAccessState};

const ENCRYPTED_SESSION_SETUP_FAILED_REASON: &str = "Encrypted session setup failed.";

pub type SessionKeyTrustFuture<'a> = Pin<
    Box<
        dyn Future<Output = std::result::Result<SessionKeyTrustedSigner, SessionKeyTrustError>>
            + Send
            + 'a,
    >,
>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKeyTrustedSigner {
    pub public_key: Vec<u8>,
    pub owner_user_id: String,
    pub sender_device_id: String,
    pub device_list_generation: u64,
    pub identity_fingerprint: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKeyTrustFailure {
    PeerIdentityChanged,
    Unauthorized,
    Unavailable,
}

#[derive(Debug)]
pub struct SessionKeyTrustError {
    failure: SessionKeyTrustFailure,
    source: Box<dyn Error + Send + Sync + 'static>,
}

impl SessionKeyTrustError {
    pub fn from_error(
        failure: SessionKeyTrustFailure,
        error: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            failure,
            source: Box::new(error),
        }
    }

    pub const fn failure(&self) -> SessionKeyTrustFailure {
        self.failure
    }

    pub fn source_message(&self) -> String {
        self.source.to_string()
    }

    pub fn into_source(self) -> Box<dyn Error + Send + Sync + 'static> {
        self.source
    }
}

impl fmt::Display for SessionKeyTrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "session key trust anchor failed: {}", self.source)
    }
}

impl Error for SessionKeyTrustError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKeyTrustContext {
    ExplicitShare,
    BackgroundFetch,
}

pub trait SessionKeyTrustAnchor: Send + Sync {
    fn resolve_signer_pubkey<'a>(
        &'a self,
        backend: &'a BackendHttpClient,
        owner_user_id: &'a str,
        sender_device_id: &'a str,
        context: SessionKeyTrustContext,
    ) -> SessionKeyTrustFuture<'a>;
}

pub enum SessionKeyFetchOutcome {
    Ready {
        key: crypto::SessionKey,
        trusted_signer: SessionKeyTrustedSigner,
    },
    Unavailable {
        state: RemoteSessionAccessState,
        reason: String,
        issue: Option<RemoteSessionAccessIssue>,
        retryable: bool,
    },
}

#[derive(Clone, Copy)]
struct ViewerKeyMaterial<'a> {
    kem_secret_bytes: &'a [u8],
    device_id: &'a str,
    user_id: &'a str,
    signing_pkcs8: &'a [u8],
}

#[derive(Clone, Copy)]
struct SessionKeyFetchRequest<'a> {
    backend_session_id: &'a str,
    expected_incarnation_id: &'a uuid::Uuid,
    owner_user_id: &'a str,
    viewer: ViewerKeyMaterial<'a>,
    trust_context: SessionKeyTrustContext,
}

struct ReadySessionKeyBlob {
    dto: SessionKeyBlobDto,
    encrypted_blob: Vec<u8>,
}

enum SessionKeyBlobFetch {
    Ready(ReadySessionKeyBlob),
    Unavailable {
        state: RemoteSessionAccessState,
        reason: String,
        retryable: bool,
    },
}

#[expect(
    clippy::too_many_arguments,
    reason = "session key fetch carries explicit trust and device possession inputs"
)]
#[tracing::instrument(skip_all, fields(backend_session_id = %backend_session_id, owner_user_id = %owner_user_id, trust_context = ?trust_context))]
pub async fn fetch_session_key(
    backend: &BackendHttpClient,
    trust_anchor: &dyn SessionKeyTrustAnchor,
    backend_session_id: &str,
    expected_incarnation_id: &uuid::Uuid,
    owner_user_id: &str,
    viewer_kem_secret_bytes: Option<&[u8]>,
    viewer_device_id: Option<&str>,
    viewer_user_id: Option<&str>,
    viewer_signing_pkcs8: Option<&[u8]>,
    trust_context: SessionKeyTrustContext,
) -> SessionKeyFetchOutcome {
    let attempt = require_viewer_key_material(
        viewer_kem_secret_bytes,
        viewer_device_id,
        viewer_user_id,
        viewer_signing_pkcs8,
    )
    .map(|viewer| SessionKeyFetchRequest {
        backend_session_id,
        expected_incarnation_id,
        owner_user_id,
        viewer,
        trust_context,
    });

    let attempt = match attempt {
        Ok(request) => prepare_session_key(backend, trust_anchor, request).await,
        Err(error) => Err(error),
    };

    match attempt {
        Ok(SessionKeyFetchOutcome::Ready {
            key,
            trusted_signer,
        }) => {
            tracing::info!("session key unwrapped successfully for session {backend_session_id}");
            SessionKeyFetchOutcome::Ready {
                key,
                trusted_signer,
            }
        }
        Ok(SessionKeyFetchOutcome::Unavailable {
            state,
            reason,
            issue,
            retryable,
        }) => {
            if state == RemoteSessionAccessState::AwaitingKey {
                tracing::info!(
                    session_id = backend_session_id,
                    %reason,
                    "viewer is waiting for an encrypted session key"
                );
            } else {
                tracing::warn!(
                    session_id = backend_session_id,
                    %reason,
                    "failed to prepare encrypted session key"
                );
            }
            SessionKeyFetchOutcome::Unavailable {
                state,
                reason,
                issue,
                retryable,
            }
        }
        Err(error) => {
            let failure = session_key_fetch_failure(&error);
            tracing::warn!(
                session_id = backend_session_id,
                reason = %failure.reason,
                %error,
                "failed to prepare encrypted session key"
            );
            SessionKeyFetchOutcome::Unavailable {
                state: failure.state,
                reason: failure.reason,
                issue: failure.issue,
                retryable: false,
            }
        }
    }
}

fn require_viewer_key_material<'a>(
    kem_secret_bytes: Option<&'a [u8]>,
    device_id: Option<&'a str>,
    user_id: Option<&'a str>,
    signing_pkcs8: Option<&'a [u8]>,
) -> Result<ViewerKeyMaterial<'a>> {
    Ok(ViewerKeyMaterial {
        kem_secret_bytes: kem_secret_bytes.ok_or_else(|| BackendClientError::Protocol {
            reason: ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned(),
        })?,
        device_id: device_id.ok_or_else(|| BackendClientError::Protocol {
            reason: ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned(),
        })?,
        user_id: user_id.ok_or_else(|| BackendClientError::Protocol {
            reason: ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned(),
        })?,
        signing_pkcs8: signing_pkcs8.ok_or_else(|| BackendClientError::Protocol {
            reason: ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned(),
        })?,
    })
}

async fn prepare_session_key(
    backend: &BackendHttpClient,
    trust_anchor: &dyn SessionKeyTrustAnchor,
    request: SessionKeyFetchRequest<'_>,
) -> Result<SessionKeyFetchOutcome> {
    let ready_blob = match fetch_ready_session_key_blob(backend, &request).await? {
        SessionKeyBlobFetch::Ready(ready_blob) => ready_blob,
        SessionKeyBlobFetch::Unavailable {
            state,
            reason,
            retryable,
        } => {
            return Ok(SessionKeyFetchOutcome::Unavailable {
                state,
                reason,
                issue: None,
                retryable,
            });
        }
    };

    let trusted_signer =
        verify_trusted_session_key_blob(backend, trust_anchor, &request, &ready_blob).await?;
    unwrap_viewer_session_key(&request, &ready_blob).map(|key| SessionKeyFetchOutcome::Ready {
        key,
        trusted_signer,
    })
}

async fn fetch_ready_session_key_blob(
    backend: &BackendHttpClient,
    request: &SessionKeyFetchRequest<'_>,
) -> Result<SessionKeyBlobFetch> {
    let fetch_dto = backend
        .fetch_my_session_key(
            request.backend_session_id,
            crate::dto::devices::DeviceHttpProof {
                user_id: request.viewer.user_id,
                device_id: request.viewer.device_id,
                signing_pkcs8: request.viewer.signing_pkcs8,
            },
        )
        .await?;

    let SessionKeyFetchStateDto::Ready = fetch_dto.state else {
        return Ok(SessionKeyBlobFetch::Unavailable {
            state: session_key_fetch_access_state(&fetch_dto.state),
            reason: format_session_key_fetch_state_reason(&fetch_dto.state).to_owned(),
            retryable: session_key_fetch_state_retryable(&fetch_dto.state),
        });
    };

    let dto = fetch_dto
        .key_blob
        .ok_or_else(|| BackendClientError::InvalidBackendData {
            field: "keyBlob".to_owned(),
            reason: "ready session key response is missing keyBlob".to_owned(),
        })?;
    let encrypted_blob = decode_encrypted_session_key(&dto)?;
    Ok(SessionKeyBlobFetch::Ready(ReadySessionKeyBlob {
        dto,
        encrypted_blob,
    }))
}

fn decode_encrypted_session_key(blob_dto: &SessionKeyBlobDto) -> Result<Vec<u8>> {
    BASE64
        .decode(&blob_dto.encrypted_session_key)
        .map_err(|error| BackendClientError::Protocol {
            reason: format!("encrypted session key blob is invalid: {error}"),
        })
}

async fn verify_trusted_session_key_blob(
    backend: &BackendHttpClient,
    trust_anchor: &dyn SessionKeyTrustAnchor,
    request: &SessionKeyFetchRequest<'_>,
    ready_blob: &ReadySessionKeyBlob,
) -> Result<SessionKeyTrustedSigner> {
    let trusted_signer = trust_anchor
        .resolve_signer_pubkey(
            backend,
            request.owner_user_id,
            &ready_blob.dto.sender_device_id,
            request.trust_context,
        )
        .await?;

    verify_signed_key_blob(
        &ready_blob.dto,
        request.backend_session_id,
        request.expected_incarnation_id,
        request.viewer.device_id,
        &ready_blob.encrypted_blob,
        &trusted_signer.public_key,
    )?;
    Ok(trusted_signer)
}

fn unwrap_viewer_session_key(
    request: &SessionKeyFetchRequest<'_>,
    ready_blob: &ReadySessionKeyBlob,
) -> Result<crypto::SessionKey> {
    let viewer_dk = aws_lc_rs::kem::DecapsulationKey::new(
        &aws_lc_rs::kem::ML_KEM_768,
        request.viewer.kem_secret_bytes,
    )
    .map_err(|_| BackendClientError::Crypto {
        reason: ENCRYPTED_SESSION_SETUP_FAILED_REASON.to_owned(),
    })?;

    crypto::unwrap_session_key(
        &viewer_dk,
        &ready_blob.encrypted_blob,
        request.backend_session_id,
        request.viewer.device_id,
        ready_blob.dto.key_generation,
    )
}

const SESSION_KEY_BLOB_MAX_FUTURE_SKEW_MS: u64 = 15 * 60 * 1000;

fn verify_signed_key_blob(
    blob_dto: &SessionKeyBlobDto,
    backend_session_id: &str,
    expected_incarnation_id: &uuid::Uuid,
    recipient_device_id: &str,
    encrypted_blob: &[u8],
    signer_sig_pubkey: &[u8],
) -> Result<()> {
    let signature_b64 = blob_dto
        .signature
        .as_deref()
        .filter(|signature| !signature.is_empty())
        .ok_or_else(|| BackendClientError::Protocol {
            reason: "session key blob is missing signature".to_owned(),
        })?;

    if signer_sig_pubkey.is_empty() {
        return Err(BackendClientError::Protocol {
            reason: "no pinned signing pubkey resolved for sender device".to_owned(),
        });
    }
    if blob_dto.incarnation_id != *expected_incarnation_id {
        return Err(BackendClientError::Protocol {
            reason: "session key blob belongs to a different session incarnation".to_owned(),
        });
    }
    if blob_dto.signature_version == crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION
        && blob_dto.incarnation_protocol_version != 1
    {
        return Err(BackendClientError::Protocol {
            reason: "legacy key blob signature replayed into a non-legacy incarnation".to_owned(),
        });
    }

    let signature = BASE64
        .decode(signature_b64)
        .map_err(|error| BackendClientError::Protocol {
            reason: format!("invalid base64 in key blob signature: {error}"),
        })?;

    let now_ms = current_epoch_ms();
    let max_issued_at = now_ms.saturating_add(SESSION_KEY_BLOB_MAX_FUTURE_SKEW_MS);
    if blob_dto.issued_at_ms > max_issued_at {
        return Err(BackendClientError::Protocol {
            reason: format!(
                "session key blob is issued too far in the future (issued={}, now={}, max future skew={}ms)",
                blob_dto.issued_at_ms, now_ms, SESSION_KEY_BLOB_MAX_FUTURE_SKEW_MS
            ),
        });
    }

    crypto::verify_key_blob_signature_for_incarnation(
        signer_sig_pubkey,
        backend_session_id,
        expected_incarnation_id,
        recipient_device_id,
        encrypted_blob,
        blob_dto.key_generation,
        blob_dto.issued_at_ms,
        blob_dto.signature_version,
        &signature,
    )?;
    tracing::info!("key blob signature verified for session {backend_session_id}");
    Ok(())
}

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use aws_lc_rs::signature::{KeyPair, ML_DSA_65_SIGNING, PqdsaKeyPair};
    use base64::{Engine, engine::general_purpose::STANDARD as BASE64};

    use super::{current_epoch_ms, verify_signed_key_blob};
    use crate::{crypto, dto::session_keys::SessionKeyBlobDto};

    fn incarnation_id() -> uuid::Uuid {
        uuid::Uuid::from_u128(2)
    }

    #[test]
    fn unsigned_key_blob_is_rejected() {
        let blob = SessionKeyBlobDto {
            incarnation_id: incarnation_id(),
            incarnation_protocol_version: 1,
            encrypted_session_key: BASE64.encode(b"encrypted"),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: None,
            key_generation: 1,
            issued_at_ms: current_epoch_ms(),
            signature_version: crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION,
        };

        let result = verify_signed_key_blob(
            &blob,
            "session-123",
            &incarnation_id(),
            "viewer-device",
            b"encrypted",
            &[7_u8; 32],
        );

        assert!(result.is_err());
    }

    #[test]
    fn signed_key_blob_is_verified() {
        let signing_key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("signing key generation should succeed: {error}"));
        let encrypted_blob = b"encrypted";
        let issued_at_ms = current_epoch_ms();
        let key_generation: u32 = 1;
        let signature = crypto::sign_key_blob_v1(
            &signing_key,
            "session-123",
            "viewer-device",
            encrypted_blob,
            key_generation,
            issued_at_ms,
        )
        .unwrap_or_else(|error| panic!("key blob signing should succeed: {error}"));

        let blob = SessionKeyBlobDto {
            incarnation_id: incarnation_id(),
            incarnation_protocol_version: 1,
            encrypted_session_key: BASE64.encode(encrypted_blob),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: Some(BASE64.encode(signature)),
            key_generation,
            issued_at_ms,
            signature_version: crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION,
        };

        let result = verify_signed_key_blob(
            &blob,
            "session-123",
            &incarnation_id(),
            "viewer-device",
            encrypted_blob,
            signing_key.public_key().as_ref(),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn legacy_signature_is_rejected_for_non_legacy_incarnation() {
        let signing_key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let encrypted_blob = b"encrypted";
        let issued_at_ms = current_epoch_ms();
        let signature = crypto::sign_key_blob_v1(
            &signing_key,
            "session-123",
            "viewer-device",
            encrypted_blob,
            1,
            issued_at_ms,
        )
        .unwrap();
        let blob = SessionKeyBlobDto {
            incarnation_id: incarnation_id(),
            incarnation_protocol_version: 2,
            encrypted_session_key: BASE64.encode(encrypted_blob),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: Some(BASE64.encode(signature)),
            key_generation: 1,
            issued_at_ms,
            signature_version: crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION,
        };

        assert!(
            verify_signed_key_blob(
                &blob,
                "session-123",
                &incarnation_id(),
                "viewer-device",
                encrypted_blob,
                signing_key.public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn v2_signature_cannot_replay_across_incarnations() {
        let signing_key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let signed_incarnation = uuid::Uuid::from_u128(2);
        let current_incarnation = uuid::Uuid::from_u128(3);
        let encrypted_blob = b"encrypted";
        let issued_at_ms = current_epoch_ms();
        let signature = crypto::sign_key_blob_v2(
            &signing_key,
            "session-123",
            &signed_incarnation,
            "viewer-device",
            encrypted_blob,
            1,
            issued_at_ms,
        )
        .unwrap();
        let blob = SessionKeyBlobDto {
            incarnation_id: current_incarnation,
            incarnation_protocol_version: 2,
            encrypted_session_key: BASE64.encode(encrypted_blob),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: Some(BASE64.encode(signature)),
            key_generation: 1,
            issued_at_ms,
            signature_version: crypto::CURRENT_KEY_BLOB_SIGNATURE_VERSION,
        };

        assert!(
            verify_signed_key_blob(
                &blob,
                "session-123",
                &current_incarnation,
                "viewer-device",
                encrypted_blob,
                signing_key.public_key().as_ref(),
            )
            .is_err()
        );
    }

    #[test]
    fn key_blob_verification_uses_caller_pubkey_not_dto_pubkey() {
        let real_signer = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let attacker_signer = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let encrypted_blob = b"encrypted";
        let issued_at_ms = current_epoch_ms();
        let key_generation: u32 = 1;
        let signature = crypto::sign_key_blob_v1(
            &real_signer,
            "session-123",
            "viewer-device",
            encrypted_blob,
            key_generation,
            issued_at_ms,
        )
        .unwrap();

        let blob = SessionKeyBlobDto {
            incarnation_id: incarnation_id(),
            incarnation_protocol_version: 1,
            encrypted_session_key: BASE64.encode(encrypted_blob),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: Some(BASE64.encode(signature)),
            key_generation,
            issued_at_ms,
            signature_version: crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION,
        };

        let ok = verify_signed_key_blob(
            &blob,
            "session-123",
            &incarnation_id(),
            "viewer-device",
            encrypted_blob,
            real_signer.public_key().as_ref(),
        );
        assert!(ok.is_ok(), "caller-supplied pubkey should verify");

        let bad = verify_signed_key_blob(
            &blob,
            "session-123",
            &incarnation_id(),
            "viewer-device",
            encrypted_blob,
            attacker_signer.public_key().as_ref(),
        );
        assert!(
            bad.is_err(),
            "caller-supplied non-matching pubkey must reject"
        );
    }

    #[test]
    fn old_current_generation_key_blob_remains_valid() {
        let signing_key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING)
            .unwrap_or_else(|error| panic!("signing key generation should succeed: {error}"));
        let encrypted_blob = b"encrypted";
        let key_generation: u32 = 5;
        let stale_ms = current_epoch_ms().saturating_sub(60 * 60 * 1000);
        let signature = crypto::sign_key_blob_v1(
            &signing_key,
            "session-123",
            "viewer-device",
            encrypted_blob,
            key_generation,
            stale_ms,
        )
        .unwrap();

        let blob = SessionKeyBlobDto {
            incarnation_id: incarnation_id(),
            incarnation_protocol_version: 1,
            encrypted_session_key: BASE64.encode(encrypted_blob),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: Some(BASE64.encode(signature)),
            key_generation,
            issued_at_ms: stale_ms,
            signature_version: crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION,
        };

        let result = verify_signed_key_blob(
            &blob,
            "session-123",
            &incarnation_id(),
            "viewer-device",
            encrypted_blob,
            signing_key.public_key().as_ref(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn future_dated_key_blob_is_rejected() {
        let signing_key = PqdsaKeyPair::generate(&ML_DSA_65_SIGNING).unwrap();
        let encrypted_blob = b"encrypted";
        let key_generation = 5;
        let future_ms = current_epoch_ms().saturating_add(60 * 60 * 1000);
        let signature = crypto::sign_key_blob_v1(
            &signing_key,
            "session-123",
            "viewer-device",
            encrypted_blob,
            key_generation,
            future_ms,
        )
        .unwrap();
        let blob = SessionKeyBlobDto {
            incarnation_id: incarnation_id(),
            incarnation_protocol_version: 1,
            encrypted_session_key: BASE64.encode(encrypted_blob),
            sender_device_id: "owner-dev-1".to_owned(),
            signature: Some(BASE64.encode(signature)),
            key_generation,
            issued_at_ms: future_ms,
            signature_version: crypto::LEGACY_KEY_BLOB_SIGNATURE_VERSION,
        };
        assert!(
            verify_signed_key_blob(
                &blob,
                "session-123",
                &incarnation_id(),
                "viewer-device",
                encrypted_blob,
                signing_key.public_key().as_ref(),
            )
            .is_err()
        );
    }
}
