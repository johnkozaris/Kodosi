use std::collections::{BTreeMap, BTreeSet};

use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm};
use base64::Engine;
use subtle::ConstantTimeEq;

use crate::{AppError, Result, identity_core::identity_bundle_view::build_bundle_view};
use kodosi_domain::domain_tags::DEVICE_LIST_V1;

use super::{
    BASE64, BreakReason, DeviceListPin, IdentityBundleView, PinContext, PinVerdict,
    schema::is_legacy_identity_incarnation,
};

#[expect(
    clippy::too_many_lines,
    reason = "Single pin-update state machine; splitting arms obscures the verify-then-tombstone flow."
)]
pub(super) fn verify_or_pin_pending(
    pins: &BTreeMap<String, DeviceListPin>,
    mut pending: BTreeMap<String, DeviceListPin>,
    clock: fn() -> i64,
    view: &IdentityBundleView,
    context: PinContext,
) -> Result<(PinVerdict, Option<BTreeMap<String, DeviceListPin>>)> {
    let now_ms = u64::try_from(clock()).unwrap_or(0);
    let Some(existing) = pins.get(&view.user_id).cloned() else {
        if !view.is_valid_at(now_ms) {
            return Ok((
                PinVerdict::Reject {
                    reason: BreakReason::IdentityExpired,
                },
                None,
            ));
        }
        if !matches!(
            context,
            PinContext::ExplicitShare | PinContext::PeerEnrollment
        ) {
            return Ok((
                PinVerdict::Reject {
                    reason: BreakReason::NoExplicitShareContext,
                },
                None,
            ));
        }
        let pin = DeviceListPin::from_view(
            view,
            i64::try_from(now_ms).unwrap_or(i64::MAX),
            BTreeSet::new(),
        );
        pending.insert(view.user_id.clone(), pin);
        return Ok((PinVerdict::FirstShare, Some(pending)));
    };

    let existing_input =
        existing
            .identity_bundle_input()?
            .ok_or_else(|| AppError::InvalidBackendData {
                field: "pin.device_proofs".to_owned(),
                reason: "pinned identity is missing an active certificate proof".to_owned(),
            })?;
    let existing_view = build_bundle_view(&existing_input)?;
    let existing_summary = existing.proof_summary()?;

    let legacy_incarnation = is_legacy_identity_incarnation(existing.identity_incarnation_id);
    if !legacy_incarnation
        && (view.identity_revision < existing.identity_revision
            || view.identity_incarnation_id != existing.identity_incarnation_id)
    {
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::IdentityLifecycleChanged,
            },
            None,
        ));
    }

    if view.signed_list.generation < existing_summary.generation {
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::StaleGeneration,
            },
            None,
        ));
    }
    if view.signed_list.generation == existing_summary.generation {
        let pinned_body = BASE64.decode(&existing.list_body_b64).map_err(|error| {
            AppError::InvalidBackendData {
                field: "pin.list_body_b64".to_owned(),
                reason: format!("corrupt pinned body base64: {error}"),
            }
        })?;
        let pinned_sig = BASE64
            .decode(&existing.list_signature_b64)
            .map_err(|error| AppError::InvalidBackendData {
                field: "pin.list_signature_b64".to_owned(),
                reason: format!("corrupt pinned signature base64: {error}"),
            })?;

        let body_match: bool = view.list_body.as_slice().ct_eq(&pinned_body).into();
        let sig_match: bool = view.list_signature.as_slice().ct_eq(&pinned_sig).into();
        if body_match && sig_match {
            if !view.is_valid_at(now_ms) {
                return Ok((
                    PinVerdict::Reject {
                        reason: BreakReason::IdentityExpired,
                    },
                    None,
                ));
            }
            let upgraded =
                DeviceListPin::from_view(view, existing.pinned_at_ms, existing.revoked_device_ids);
            let proofs_preserved =
                existing
                    .device_proofs
                    .iter()
                    .all(|(device_id, existing_proof)| {
                        upgraded.device_proofs.get(device_id) == Some(existing_proof)
                    });
            if !proofs_preserved {
                return Ok((
                    PinVerdict::Reject {
                        reason: BreakReason::KeyMaterialChanged,
                    },
                    None,
                ));
            }
            if upgraded.device_proofs != existing.device_proofs {
                pending.insert(view.user_id.clone(), upgraded);
                return Ok((PinVerdict::AlreadyPinned, Some(pending)));
            }
            return Ok((PinVerdict::AlreadyPinned, None));
        }
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::StaleGeneration,
            },
            None,
        ));
    }

    if existing
        .revoked_device_ids
        .contains(&view.signed_list.signer_device_id)
    {
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::SignerRevoked,
            },
            None,
        ));
    }
    for entry in &view.signed_list.entries {
        if existing.revoked_device_ids.contains(&entry.device_id) {
            return Ok((
                PinVerdict::Reject {
                    reason: BreakReason::RevokedDeviceReappeared,
                },
                None,
            ));
        }
    }
    if !view.is_valid_at(now_ms) {
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::IdentityExpired,
            },
            None,
        ));
    }

    let Some(pinned_signer_pub) = existing_view
        .devices
        .get(&view.signed_list.signer_device_id)
        .map(|device| device.sig_public_key.as_slice())
    else {
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::NoChainToPin,
            },
            None,
        ));
    };

    let mut payload = Vec::with_capacity(DEVICE_LIST_V1.len() + view.list_body.len());
    payload.extend_from_slice(DEVICE_LIST_V1);
    payload.extend_from_slice(&view.list_body);
    if ML_DSA_65
        .verify_sig(pinned_signer_pub, &payload, &view.list_signature)
        .is_err()
    {
        return Ok((
            PinVerdict::Reject {
                reason: BreakReason::NoChainToPin,
            },
            None,
        ));
    }

    let new_entry_ids: BTreeSet<String> = view
        .signed_list
        .entries
        .iter()
        .map(|entry| entry.device_id.clone())
        .collect();
    let proposed = DeviceListPin::from_view(
        view,
        i64::try_from(now_ms).unwrap_or(i64::MAX),
        BTreeSet::new(),
    );
    for retained_device_id in existing_summary
        .active_device_ids
        .iter()
        .filter(|device_id| new_entry_ids.contains(*device_id))
    {
        let proof_is_unchanged =
            existing
                .device_proofs
                .get(retained_device_id)
                .is_some_and(|existing_proof| {
                    proposed.device_proofs.get(retained_device_id) == Some(existing_proof)
                });
        if !proof_is_unchanged {
            return Ok((
                PinVerdict::Reject {
                    reason: BreakReason::KeyMaterialChanged,
                },
                None,
            ));
        }
    }

    let mut tombstones = existing.revoked_device_ids;
    for previously_pinned in &existing_summary.active_device_ids {
        if !new_entry_ids.contains(previously_pinned) {
            tombstones.insert(previously_pinned.clone());
        }
    }

    let pin = DeviceListPin::from_view(view, i64::try_from(now_ms).unwrap_or(i64::MAX), tombstones);
    pending.insert(view.user_id.clone(), pin);
    Ok((PinVerdict::AcceptUpdate, Some(pending)))
}
