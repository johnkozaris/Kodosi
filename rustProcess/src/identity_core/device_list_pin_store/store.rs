use std::{collections::BTreeMap, path::PathBuf};

use base64::Engine;
use kodosi_domain::user::IdentityLifecycleState;

use crate::{AppError, Result};

use super::{
    BASE64, DeviceListPin, IdentityBundleView, PinContext, PinVerdict,
    persistence::{default_clock_ms, load_pin_file, update_pin_file},
    schema::{IdentityLifecycleWatermark, PinFile, is_legacy_identity_incarnation},
    verifier::verify_or_pin_pending,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityLifecycleApplyOutcome {
    Stale,
    Recorded,
    PinCleared,
}

#[derive(Debug)]
pub(crate) struct DeviceListPinStore {
    pub(super) path: PathBuf,
    pub(super) pins: BTreeMap<String, DeviceListPin>,
    pub(super) owner_user_id: Option<String>,
    pub(super) clock: fn() -> i64,
}

impl DeviceListPinStore {
    pub(crate) fn load_default() -> Result<Self> {
        let path = crate::support::storage::paths::device_list_pins_path()?;
        Self::load_from(path)
    }

    pub(crate) fn load_from(path: impl Into<PathBuf>) -> Result<Self> {
        Self::load_from_with_clock(path, default_clock_ms)
    }

    pub(super) fn load_from_with_clock(
        path: impl Into<PathBuf>,
        clock: fn() -> i64,
    ) -> Result<Self> {
        let path = path.into();
        let file = load_pin_file(&path)?;
        let last_bound_owner_user_id = file.last_bound_owner_user_id.as_deref();
        let pins = file
            .pins_for(last_bound_owner_user_id)
            .cloned()
            .unwrap_or_default();
        Ok(Self {
            path,
            pins,

            owner_user_id: None,
            clock,
        })
    }

    pub(crate) fn bind_to_user(&mut self, user_id: &str) -> Result<bool> {
        let (committed, account_changed) = update_pin_file(&self.path, |current| {
            let mut pending = current.clone();
            let changed = current
                .last_bound_owner_user_id
                .as_deref()
                .is_some_and(|owner| owner != user_id);
            pending.accounts.entry(user_id.to_owned()).or_default();
            pending.last_bound_owner_user_id = Some(user_id.to_owned());
            Ok((pending, changed))
        })?;
        self.apply_snapshot(&committed);
        Ok(account_changed)
    }

    pub(super) fn is_bound(&self) -> bool {
        self.owner_user_id.is_some()
    }

    pub(crate) fn get(&self, user_id: &str) -> Option<&DeviceListPin> {
        self.pins.get(user_id)
    }

    pub(crate) fn identity_bundle(
        &self,
        user_id: &str,
    ) -> Result<Option<super::IdentityBundleInput>> {
        if !self.is_bound() {
            return Ok(None);
        }
        self.get(user_id)
            .map_or(Ok(None), DeviceListPin::identity_bundle_input)
    }

    pub(crate) fn iter_pins(&self) -> impl Iterator<Item = &DeviceListPin> {
        self.pins.values()
    }

    pub(crate) fn reset(&mut self, user_id: &str) -> Result<bool> {
        let selected_owner =
            self.owner_user_id
                .clone()
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "pin_file.last_bound_owner_user_id".to_owned(),
                    reason: "device-list pin store is not bound to an account".to_owned(),
                })?;
        let (committed, cleared) = update_pin_file(&self.path, |current| {
            let mut pending = current.clone();
            let cleared = pending
                .pins_for_mut(&selected_owner)
                .remove(user_id)
                .is_some();
            Ok((pending, cleared))
        })?;
        self.apply_snapshot(&committed);
        Ok(cleared)
    }

    pub(crate) fn reset_if_identity_receipt(&mut self, view: &IdentityBundleView) -> Result<bool> {
        let selected_owner =
            self.owner_user_id
                .clone()
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "pin_file.last_bound_owner_user_id".to_owned(),
                    reason: "device-list pin store is not bound to an account".to_owned(),
                })?;
        let expected = DeviceListPin::from_view(view, 0, std::collections::BTreeSet::new());
        let (committed, cleared) = update_pin_file(&self.path, |current| {
            let mut pending = current.clone();
            let pins = pending.pins_for_mut(&selected_owner);
            let cleared = pins.get(&view.user_id).is_some_and(|pin| {
                pin.identity_revision == expected.identity_revision
                    && pin.identity_incarnation_id == expected.identity_incarnation_id
                    && pin.list_body_b64 == expected.list_body_b64
                    && pin.list_signature_b64 == expected.list_signature_b64
                    && pin.device_proofs == expected.device_proofs
            });
            if cleared {
                pins.remove(&view.user_id);
            }
            Ok((pending, cleared))
        })?;
        self.apply_snapshot(&committed);
        Ok(cleared)
    }

    pub(crate) fn apply_identity_lifecycle(
        &mut self,
        user_id: &str,
        revision: u64,
        state: IdentityLifecycleState,
    ) -> Result<IdentityLifecycleApplyOutcome> {
        if revision == 0 {
            return Err(AppError::InvalidBackendData {
                field: "identityLifecycle.identityRevision".to_owned(),
                reason: "identity lifecycle revision must be positive".to_owned(),
            });
        }
        let incarnation_id = state.incarnation_id();
        let selected_owner =
            self.owner_user_id
                .clone()
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "pin_file.last_bound_owner_user_id".to_owned(),
                    reason: "device-list pin store is not bound to an account".to_owned(),
                })?;
        let (committed, outcome) = update_pin_file(&self.path, |current| {
            let mut pending = current.clone();
            let bucket = pending.accounts.entry(selected_owner.clone()).or_default();
            let existing = bucket.identity_lifecycles.get(user_id);
            if existing.is_some_and(|watermark| watermark.revision >= revision) {
                return Ok((pending, IdentityLifecycleApplyOutcome::Stale));
            }
            let legacy_pin_adoption =
                existing.is_none() && bucket.pins.contains_key(user_id) && incarnation_id.is_some();
            let clear_pin = incarnation_id.is_none()
                || existing.is_some_and(|watermark| {
                    watermark.incarnation_id.is_some() && watermark.incarnation_id != incarnation_id
                })
                || !legacy_pin_adoption
                    && bucket
                        .pins
                        .get(user_id)
                        .is_some_and(|pin| Some(pin.identity_incarnation_id) != incarnation_id);
            if legacy_pin_adoption
                && !clear_pin
                && let (Some(pin), Some(incarnation_id)) =
                    (bucket.pins.get_mut(user_id), incarnation_id)
            {
                pin.identity_revision = revision;
                pin.identity_incarnation_id = incarnation_id;
            }
            bucket.identity_lifecycles.insert(
                user_id.to_owned(),
                IdentityLifecycleWatermark {
                    revision,
                    incarnation_id,
                },
            );
            let outcome = if clear_pin && bucket.pins.remove(user_id).is_some() {
                IdentityLifecycleApplyOutcome::PinCleared
            } else {
                IdentityLifecycleApplyOutcome::Recorded
            };
            Ok((pending, outcome))
        })?;
        self.apply_snapshot(&committed);
        Ok(outcome)
    }

    pub(crate) fn reset_all(&mut self) -> Result<()> {
        let selected_owner =
            self.owner_user_id
                .clone()
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "pin_file.last_bound_owner_user_id".to_owned(),
                    reason: "device-list pin store is not bound to an account".to_owned(),
                })?;
        let (committed, ()) = update_pin_file(&self.path, |current| {
            let mut pending = current.clone();
            pending.pins_for_mut(&selected_owner).clear();
            Ok((pending, ()))
        })?;
        self.apply_snapshot(&committed);
        Ok(())
    }

    pub(crate) fn verify_or_pin(
        &mut self,
        view: &IdentityBundleView,
        context: PinContext,
    ) -> Result<PinVerdict> {
        let selected_owner =
            self.owner_user_id
                .clone()
                .ok_or_else(|| AppError::InvalidBackendData {
                    field: "pin_file.last_bound_owner_user_id".to_owned(),
                    reason: "device-list pin store is not bound to an account".to_owned(),
                })?;
        let (committed, verdict) = update_pin_file(&self.path, |current| {
            let mut pending = current.clone();
            let bucket = pending.accounts.entry(selected_owner.clone()).or_default();
            let watermark = bucket.identity_lifecycles.get(&view.user_id).cloned();
            if watermark.as_ref().is_some_and(|watermark| {
                view.identity_revision < watermark.revision
                    || (view.identity_revision == watermark.revision
                        && watermark.incarnation_id != Some(view.identity_incarnation_id))
            }) {
                return Ok((
                    pending,
                    PinVerdict::Reject {
                        reason: crate::identity_core::device_list_pin_store::BreakReason::IdentityLifecycleChanged,
                    },
                ));
            }

            let pinned_lifecycle = bucket
                .pins
                .get(&view.user_id)
                .map(|pin| (pin.identity_revision, pin.identity_incarnation_id));
            let lifecycle_advanced = watermark
                .as_ref()
                .is_some_and(|watermark| view.identity_revision > watermark.revision)
                || watermark.is_none()
                    && pinned_lifecycle.is_some_and(|(revision, incarnation)| {
                        !is_legacy_identity_incarnation(incarnation)
                            && view.identity_revision > revision
                            && view.identity_incarnation_id != incarnation
                    });
            let incarnation_changed = watermark.as_ref().is_some_and(|watermark| {
                watermark.incarnation_id != Some(view.identity_incarnation_id)
            }) || pinned_lifecycle.is_some_and(|(_, incarnation)| {
                !is_legacy_identity_incarnation(incarnation)
                    && incarnation != view.identity_incarnation_id
            });
            if lifecycle_advanced && incarnation_changed {
                bucket.pins.remove(&view.user_id);
                bucket.identity_lifecycles.insert(
                    view.user_id.clone(),
                    IdentityLifecycleWatermark {
                        revision: view.identity_revision,
                        incarnation_id: Some(view.identity_incarnation_id),
                    },
                );
                return Ok((
                    pending,
                    PinVerdict::Reject {
                        reason: crate::identity_core::device_list_pin_store::BreakReason::IdentityLifecycleChanged,
                    },
                ));
            }
            if lifecycle_advanced || watermark.is_none() {
                bucket.identity_lifecycles.insert(
                    view.user_id.clone(),
                    IdentityLifecycleWatermark {
                        revision: view.identity_revision,
                        incarnation_id: Some(view.identity_incarnation_id),
                    },
                );
            }

            let current_pins = bucket.pins.clone();
            let (verdict, pending_pins) = verify_or_pin_pending(
                &current_pins,
                current_pins.clone(),
                self.clock,
                view,
                context,
            )?;
            if let Some(pins) = pending_pins {
                bucket.pins = pins;
                bucket.identity_lifecycles.insert(
                    view.user_id.clone(),
                    IdentityLifecycleWatermark {
                        revision: view.identity_revision,
                        incarnation_id: Some(view.identity_incarnation_id),
                    },
                );
            }
            Ok((pending, verdict))
        })?;
        self.apply_snapshot(&committed);
        Ok(verdict)
    }

    pub(super) fn apply_snapshot(&mut self, file: &PinFile) {
        let owner_user_id = file.last_bound_owner_user_id.clone();
        self.pins = file
            .pins_for(owner_user_id.as_deref())
            .cloned()
            .unwrap_or_default();
        self.owner_user_id = owner_user_id;
    }

    pub(super) fn pinned_signing_pubkey(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<Vec<u8>>> {
        if !self.is_bound() {
            return Ok(None);
        }
        let Some(pin) = self.get(user_id) else {
            return Ok(None);
        };
        if pin.revoked_device_ids.contains(device_id) {
            return Ok(None);
        }
        let summary = pin.proof_summary()?;
        if !summary.active_device_ids.contains(device_id) {
            return Ok(None);
        }
        let Some(proof) = pin.device_proofs.get(device_id) else {
            return Ok(None);
        };
        let certificate =
            BASE64
                .decode(&proof.certificate)
                .map_err(|error| AppError::InvalidBackendData {
                    field: "pin.device_proofs.certificate".to_owned(),
                    reason: format!("base64 decode failed: {error}"),
                })?;
        crate::identity_core::device_cert::DeviceCertificate::parse_body(&certificate)
            .map(|certificate| Some(certificate.sig_public_key))
    }
}
