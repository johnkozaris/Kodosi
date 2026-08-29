use crate::{
    AppError, Result,
    identity_core::device_list_pin_store::{
        DeviceListPinStoreHandle, PinContext, PinVerdict, identity_bundle_fingerprint,
    },
};
use kodosi_backend_client::{
    http_client::BackendHttpClient,
    session_key_service::{
        SessionKeyTrustAnchor, SessionKeyTrustContext, SessionKeyTrustError,
        SessionKeyTrustFailure, SessionKeyTrustFuture, SessionKeyTrustedSigner,
    },
};

use super::backend_adapters;

#[derive(Debug, Clone)]
pub(crate) struct IdentitySessionKeyTrust {
    pin_store: DeviceListPinStoreHandle,
}

impl IdentitySessionKeyTrust {
    pub(crate) const fn new(pin_store: DeviceListPinStoreHandle) -> Self {
        Self { pin_store }
    }
}

impl SessionKeyTrustAnchor for IdentitySessionKeyTrust {
    fn resolve_signer_pubkey<'a>(
        &'a self,
        backend: &'a BackendHttpClient,
        owner_user_id: &'a str,
        sender_device_id: &'a str,
        context: SessionKeyTrustContext,
    ) -> SessionKeyTrustFuture<'a> {
        Box::pin(async move {
            self.resolve_signer_pubkey_inner(backend, owner_user_id, sender_device_id, context)
                .await
                .map_err(session_key_trust_error)
        })
    }
}

fn session_key_trust_error(error: AppError) -> SessionKeyTrustError {
    let failure = match &error {
        AppError::PeerIdentityChanged { .. } => SessionKeyTrustFailure::PeerIdentityChanged,
        AppError::Unauthorized => SessionKeyTrustFailure::Unauthorized,
        _ => SessionKeyTrustFailure::Unavailable,
    };
    SessionKeyTrustError::from_error(failure, error)
}

impl IdentitySessionKeyTrust {
    #[tracing::instrument(skip_all, fields(owner_user_id = %owner_user_id, sender_device_id = %sender_device_id, context = ?context), err)]
    async fn resolve_signer_pubkey_inner(
        &self,
        backend: &BackendHttpClient,
        owner_user_id: &str,
        sender_device_id: &str,
        context: SessionKeyTrustContext,
    ) -> Result<SessionKeyTrustedSigner> {
        let dto = backend.fetch_user_identity(owner_user_id).await?;
        let view = backend_adapters::identity_bundle_view(&dto)?;

        let resolved = self
            .pin_store
            .verify_and_resolve_signer(view, pin_context_for_session_key(context), sender_device_id)
            .await?;
        match resolved.verdict {
            PinVerdict::FirstShare | PinVerdict::AcceptUpdate | PinVerdict::AlreadyPinned => {}
            PinVerdict::Reject { reason } => {
                return Err(AppError::PeerIdentityChanged {
                    user_id: owner_user_id.to_string(),
                    detail: format!("{reason:?}"),
                });
            }
        }

        let public_key = resolved.public_key.ok_or_else(|| AppError::Unsupported {
            reason: format!(
                "sender device {sender_device_id} is not in the pinned device list for user {owner_user_id}"
            ),
        })?;
        let pinned_bundle = resolved
            .identity_bundle
            .ok_or_else(|| AppError::Unsupported {
                reason: format!("pinned identity bundle for user {owner_user_id} is unavailable"),
            })?;
        Ok(SessionKeyTrustedSigner {
            public_key,
            owner_user_id: owner_user_id.to_owned(),
            sender_device_id: sender_device_id.to_owned(),
            device_list_generation: pinned_bundle.device_list.generation,
            identity_fingerprint: identity_bundle_fingerprint(&pinned_bundle)?,
        })
    }
}

fn pin_context_for_session_key(context: SessionKeyTrustContext) -> PinContext {
    match context {
        SessionKeyTrustContext::ExplicitShare => PinContext::ExplicitShare,
        SessionKeyTrustContext::BackgroundFetch => PinContext::BackgroundFetch,
    }
}
