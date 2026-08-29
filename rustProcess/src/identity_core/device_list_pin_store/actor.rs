use std::{
    sync::{Arc, Mutex},
    thread,
};

use tokio::sync::{mpsc, oneshot};

use kodosi_domain::user::IdentityLifecycleState;

use crate::{AppError, Result};

use super::{
    IdentityBundleInput, IdentityBundleView, PinContext, PinVerdict,
    store::{DeviceListPinStore, IdentityLifecycleApplyOutcome},
};

const PIN_STORE_COMMAND_CAPACITY: usize = 64;
const PIN_STORE_CHANNEL_NAME: &str = "device-list-pin-store";

type PinStoreReply<T> = oneshot::Sender<Result<T>>;

enum DeviceListPinStoreCommand {
    BindToUser {
        user_id: String,
        reply: PinStoreReply<bool>,
    },
    Reset {
        user_id: String,
        reply: PinStoreReply<bool>,
    },
    ResetIfIdentityReceipt {
        owner_user_id: String,
        view: Box<IdentityBundleView>,
        reply: PinStoreReply<bool>,
    },
    ApplyIdentityLifecycle {
        user_id: String,
        revision: u64,
        state: IdentityLifecycleState,
        reply: PinStoreReply<IdentityLifecycleApplyOutcome>,
    },
    ResetAll {
        reply: PinStoreReply<()>,
    },
    VerifyOrPin {
        view: Box<IdentityBundleView>,
        context: PinContext,
        reply: PinStoreReply<PinVerdict>,
    },
    VerifyOrPinForOwner {
        owner_user_id: String,
        view: Box<IdentityBundleView>,
        context: PinContext,
        reply: PinStoreReply<PinVerdict>,
    },
    VerifyAndResolveSigner {
        view: Box<IdentityBundleView>,
        context: PinContext,
        sender_device_id: String,
        reply: PinStoreReply<ResolvedPinnedSigner>,
    },
    IdentityBundle {
        user_id: String,
        reply: PinStoreReply<Option<IdentityBundleInput>>,
    },
    ListPins {
        reply: PinStoreReply<Vec<PinSnapshot>>,
    },
}

pub(crate) struct ResolvedPinnedSigner {
    pub(crate) verdict: PinVerdict,
    pub(crate) public_key: Option<Vec<u8>>,
    pub(crate) identity_bundle: Option<IdentityBundleInput>,
}

#[derive(Debug, Clone)]
pub(crate) struct PinSnapshot {
    pub(crate) user_id: String,
    pub(crate) generation: u64,
    pub(crate) signer_device_id: String,
    pub(crate) device_count: u32,
    pub(crate) pinned_at_ms: i64,
}

pub(crate) struct DeviceListPinStoreHandle {
    tx: mpsc::Sender<DeviceListPinStoreCommand>,
    lifetime: Arc<DeviceListPinStoreLifetime>,
}

impl std::fmt::Debug for DeviceListPinStoreHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceListPinStoreHandle")
            .field("channel", &PIN_STORE_CHANNEL_NAME)
            .finish_non_exhaustive()
    }
}

impl Clone for DeviceListPinStoreHandle {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            lifetime: Arc::clone(&self.lifetime),
        }
    }
}

struct DeviceListPinStoreLifetime {
    join_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl DeviceListPinStoreLifetime {
    #[cfg(test)]
    fn join_blocking(&self) {
        let handle = match self.join_handle.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => {
                tracing::warn!("pin-store actor join guard was poisoned; recovering");
                poisoned.into_inner().take()
            }
        };
        if let Some(handle) = handle
            && handle.join().is_err()
        {
            tracing::warn!("pin-store actor thread panicked during shutdown");
        }
    }
}

impl Drop for DeviceListPinStoreLifetime {
    fn drop(&mut self) {
        let handle = match self.join_handle.lock() {
            Ok(mut guard) => guard.take(),
            Err(poisoned) => {
                tracing::warn!("pin-store actor join guard was poisoned during drop; recovering");
                poisoned.into_inner().take()
            }
        };

        let Some(handle) = handle else {
            return;
        };
        if handle.is_finished() {
            if handle.join().is_err() {
                tracing::warn!("pin-store actor thread panicked during shutdown");
            }
            return;
        }

        let spawn_result = thread::Builder::new()
            .name("kodosi-pin-store-join".to_owned())
            .spawn(move || {
                if handle.join().is_err() {
                    tracing::warn!("pin-store actor thread panicked during async shutdown");
                }
            });
        if let Err(error) = spawn_result {
            tracing::warn!(%error, "failed to spawn pin-store joiner thread; actor detached");
        }
    }
}

impl DeviceListPinStoreHandle {
    #[cfg_attr(test, allow(dead_code))]
    pub(crate) fn load_default() -> Result<Self> {
        Self::from_store(DeviceListPinStore::load_default()?)
    }

    pub(crate) fn from_store(store: DeviceListPinStore) -> Result<Self> {
        let (tx, rx) = mpsc::channel(PIN_STORE_COMMAND_CAPACITY);
        let join_handle = thread::Builder::new()
            .name("kodosi-pin-store".to_owned())
            .spawn(move || run_pin_store_actor(store, rx))
            .map_err(AppError::Io)?;
        Ok(Self {
            tx,
            lifetime: Arc::new(DeviceListPinStoreLifetime {
                join_handle: Mutex::new(Some(join_handle)),
            }),
        })
    }

    #[cfg(test)]
    pub(crate) fn shutdown_blocking(self) {
        let Self { tx, lifetime } = self;
        drop(tx);
        lifetime.join_blocking();
    }

    pub(crate) async fn bind_to_user(&self, user_id: &str) -> Result<bool> {
        self.request(|reply| DeviceListPinStoreCommand::BindToUser {
            user_id: user_id.to_owned(),
            reply,
        })
        .await
    }

    pub(crate) async fn reset(&self, user_id: &str) -> Result<bool> {
        self.request(|reply| DeviceListPinStoreCommand::Reset {
            user_id: user_id.to_owned(),
            reply,
        })
        .await
    }

    pub(crate) async fn reset_if_identity_receipt(
        &self,
        owner_user_id: &str,
        view: IdentityBundleView,
    ) -> Result<bool> {
        self.request(|reply| DeviceListPinStoreCommand::ResetIfIdentityReceipt {
            owner_user_id: owner_user_id.to_owned(),
            view: Box::new(view),
            reply,
        })
        .await
    }

    pub(crate) async fn apply_identity_lifecycle(
        &self,
        user_id: &str,
        revision: u64,
        state: IdentityLifecycleState,
    ) -> Result<IdentityLifecycleApplyOutcome> {
        self.request(|reply| DeviceListPinStoreCommand::ApplyIdentityLifecycle {
            user_id: user_id.to_owned(),
            revision,
            state,
            reply,
        })
        .await
    }

    pub(crate) async fn reset_all(&self) -> Result<()> {
        self.request(|reply| DeviceListPinStoreCommand::ResetAll { reply })
            .await
    }

    pub(crate) async fn verify_or_pin(
        &self,
        view: IdentityBundleView,
        context: PinContext,
    ) -> Result<PinVerdict> {
        self.request(|reply| DeviceListPinStoreCommand::VerifyOrPin {
            view: Box::new(view),
            context,
            reply,
        })
        .await
    }

    pub(crate) async fn verify_or_pin_for_owner(
        &self,
        owner_user_id: &str,
        view: IdentityBundleView,
        context: PinContext,
    ) -> Result<PinVerdict> {
        self.request(|reply| DeviceListPinStoreCommand::VerifyOrPinForOwner {
            owner_user_id: owner_user_id.to_owned(),
            view: Box::new(view),
            context,
            reply,
        })
        .await
    }

    pub(crate) async fn verify_and_resolve_signer(
        &self,
        view: IdentityBundleView,
        context: PinContext,
        sender_device_id: &str,
    ) -> Result<ResolvedPinnedSigner> {
        self.request(|reply| DeviceListPinStoreCommand::VerifyAndResolveSigner {
            view: Box::new(view),
            context,
            sender_device_id: sender_device_id.to_owned(),
            reply,
        })
        .await
    }

    pub(crate) async fn identity_bundle(
        &self,
        user_id: &str,
    ) -> Result<Option<IdentityBundleInput>> {
        self.request(|reply| DeviceListPinStoreCommand::IdentityBundle {
            user_id: user_id.to_owned(),
            reply,
        })
        .await
    }

    pub(crate) async fn list_pins(&self) -> Result<Vec<PinSnapshot>> {
        self.request(|reply| DeviceListPinStoreCommand::ListPins { reply })
            .await
    }

    async fn request<T>(
        &self,
        build_command: impl FnOnce(PinStoreReply<T>) -> DeviceListPinStoreCommand,
    ) -> Result<T> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(build_command(reply_tx))
            .await
            .map_err(|_| pin_store_actor_closed())?;
        reply_rx.await.map_err(|_| pin_store_actor_closed())?
    }
}

fn run_pin_store_actor(
    mut store: DeviceListPinStore,
    mut rx: mpsc::Receiver<DeviceListPinStoreCommand>,
) {
    while let Some(command) = rx.blocking_recv() {
        match command {
            DeviceListPinStoreCommand::BindToUser { user_id, reply } => {
                drop(reply.send(store.bind_to_user(&user_id)));
            }
            DeviceListPinStoreCommand::Reset { user_id, reply } => {
                drop(reply.send(store.reset(&user_id)));
            }
            DeviceListPinStoreCommand::ResetIfIdentityReceipt {
                owner_user_id,
                view,
                reply,
            } => {
                let result = if store.owner_user_id.as_deref() == Some(owner_user_id.as_str()) {
                    store.reset_if_identity_receipt(&view)
                } else {
                    Ok(false)
                };
                drop(reply.send(result));
            }
            DeviceListPinStoreCommand::ApplyIdentityLifecycle {
                user_id,
                revision,
                state,
                reply,
            } => {
                drop(reply.send(store.apply_identity_lifecycle(&user_id, revision, state)));
            }
            DeviceListPinStoreCommand::ResetAll { reply } => {
                drop(reply.send(store.reset_all()));
            }
            DeviceListPinStoreCommand::VerifyOrPin {
                view,
                context,
                reply,
            } => {
                drop(reply.send(store.verify_or_pin(&view, context)));
            }
            DeviceListPinStoreCommand::VerifyOrPinForOwner {
                owner_user_id,
                view,
                context,
                reply,
            } => {
                let result = if store.owner_user_id.as_deref() == Some(owner_user_id.as_str()) {
                    store.verify_or_pin(&view, context)
                } else {
                    Err(AppError::InvalidBackendData {
                        field: "pin_file.last_bound_owner_user_id".to_owned(),
                        reason: "pin store account changed before trust mutation committed"
                            .to_owned(),
                    })
                };
                drop(reply.send(result));
            }
            DeviceListPinStoreCommand::VerifyAndResolveSigner {
                view,
                context,
                sender_device_id,
                reply,
            } => {
                let result = store.verify_or_pin(&view, context).and_then(|verdict| {
                    let public_key = if matches!(verdict, PinVerdict::Reject { .. }) {
                        None
                    } else {
                        store.pinned_signing_pubkey(&view.user_id, &sender_device_id)?
                    };
                    let identity_bundle = if matches!(verdict, PinVerdict::Reject { .. }) {
                        None
                    } else {
                        store.identity_bundle(&view.user_id)?
                    };
                    Ok(ResolvedPinnedSigner {
                        verdict,
                        public_key,
                        identity_bundle,
                    })
                });
                drop(reply.send(result));
            }
            DeviceListPinStoreCommand::IdentityBundle { user_id, reply } => {
                drop(reply.send(store.identity_bundle(&user_id)));
            }
            DeviceListPinStoreCommand::ListPins { reply } => {
                drop(reply.send(pin_snapshots(&store)));
            }
        }
    }
    drop(store);
}

fn pin_snapshots(store: &DeviceListPinStore) -> Result<Vec<PinSnapshot>> {
    store
        .iter_pins()
        .map(|pin| {
            let summary = pin.proof_summary()?;
            Ok(PinSnapshot {
                user_id: pin.user_id.clone(),
                generation: summary.generation,
                signer_device_id: summary.signer_device_id,
                device_count: u32::try_from(summary.active_device_ids.len()).unwrap_or(u32::MAX),
                pinned_at_ms: pin.pinned_at_ms,
            })
        })
        .collect()
}

fn pin_store_actor_closed() -> AppError {
    AppError::ChannelClosed {
        session: PIN_STORE_CHANNEL_NAME.to_owned(),
    }
}
