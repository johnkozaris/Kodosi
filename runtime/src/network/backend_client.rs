use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::Method;
use serde_json::{Value, json};
#[cfg(feature = "host")]
use tokio::sync::mpsc;
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{
    BackendConfig, BackendEvent, BackendReply, Error, Identity, RemoteConnection, Result, crypto,
    http::{Credentials, Http},
    invalid, terminal_connections,
    wire::{self, SessionDto},
};
#[cfg(feature = "host")]
use super::{HostRequest, LocalPublication, PublicationOutput};
use crate::identity::{
    self,
    device_cert::{build_cert_for, build_self_cert},
    friends::FriendLists,
    keys::DeviceKeys,
    loopback::{self, Loopback},
    oidc::{APP_REDIRECT, BrowserLogin, Oidc, Tokens},
    pins::{IdentityBundle, Pins, VerifiedIdentity},
    signed_device_list::{DeviceListEntry, build_bootstrap_list, build_replacement_list},
    storage::Secrets,
};

#[derive(Clone)]
pub struct BackendClient {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) http: Http,
    pub(crate) config: BackendConfig,
    pub(crate) generation: AtomicU64,
    pub(crate) credentials: RwLock<Option<Credentials>>,
    pub(crate) state: Mutex<State>,
    pub(crate) operations: Mutex<()>,
    pub(crate) events: broadcast::Sender<BackendEvent>,
    pub(crate) shutdown: CancellationToken,
    initialized: AtomicBool,
    #[cfg(feature = "host")]
    pub(crate) publications: Mutex<BTreeMap<Uuid, Arc<terminal_connections::Publication>>>,
    pub(crate) connections: Mutex<Vec<CancellationToken>>,
    pub(crate) link: Arc<terminal_connections::Link>,
    pub(crate) calls: terminal_connections::Lines,
    channel_keys: std::sync::Mutex<Option<Arc<terminal_connections::ChannelKeys>>>,
    identities: std::sync::Mutex<BTreeMap<String, device_identity::KeptIdentity>>,
    sessions: std::sync::Mutex<BTreeMap<Uuid, SessionDto>>,
    notifications: Mutex<Option<CancellationToken>>,
    restore_pending: AtomicBool,
    enrollment_checked: AtomicU64,
    renew_connections: AtomicBool,
    identity_settled: AtomicBool,
    update_required: AtomicBool,
    pub(crate) friends_changed: tokio::sync::Notify,
    login_interrupt: std::sync::Mutex<CancellationToken>,
    app_login: std::sync::Mutex<Option<AppLogin>>,
    rooms: Mutex<rooms::RoomCache>,
}

struct AppLogin {
    login: BrowserLogin,
    cancel: CancellationToken,
    generation: u64,
    until: tokio::time::Instant,
    request: String,
}

pub(crate) const SIGN_IN_LIFE: Duration = Duration::from_mins(15);

pub(crate) struct State {
    tokens: Option<Tokens>,
    secrets: Secrets,
    pub(crate) pins: Arc<std::sync::Mutex<Pins>>,
    blocked_devices: BTreeSet<(String, String)>,
    account_cancel: CancellationToken,
    link: Option<enrollment::PendingLink>,
    deletion: Option<tokio::time::Instant>,
    friend_lists: FriendLists,
    changed_friends: BTreeSet<String>,
}

impl BackendClient {
    pub fn new(config: BackendConfig) -> Result<Self> {
        let http = Http::new(config.api_url.clone())?;
        let pins = Pins::load(config.data_root.join("network/device-pins.json"))?;
        let friend_lists = FriendLists::load(config.data_root.join("network/friend-lists.json"))?;
        let blocked_devices = match std::fs::read(
            config
                .data_root
                .join("network/pending-device-removals.json"),
        ) {
            Ok(bytes) => {
                if bytes.len() > 1024 * 1024 {
                    return Err(invalid(
                        "Pending device removals exceed their storage limit.",
                    ));
                }
                serde_json::from_slice(&bytes)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeSet::new(),
            Err(error) => return Err(error.into()),
        };
        let secrets = Secrets::new(
            config.data_root.join("secrets"),
            &config.secret_service,
            config.isolated,
        );
        let (events, _) = broadcast::channel(128);
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                config,
                generation: AtomicU64::new(1),
                credentials: RwLock::new(None),
                state: Mutex::new(State {
                    tokens: None,
                    secrets,
                    pins: Arc::new(std::sync::Mutex::new(pins)),
                    blocked_devices,
                    account_cancel: CancellationToken::new(),
                    link: None,
                    friend_lists,
                    changed_friends: BTreeSet::new(),
                    deletion: None,
                }),
                operations: Mutex::new(()),
                events,
                shutdown: CancellationToken::new(),
                initialized: AtomicBool::new(false),
                #[cfg(feature = "host")]
                publications: Mutex::new(BTreeMap::new()),
                connections: Mutex::new(Vec::new()),
                link: Arc::default(),
                calls: terminal_connections::Lines::default(),
                channel_keys: std::sync::Mutex::new(None),
                identities: std::sync::Mutex::default(),
                sessions: std::sync::Mutex::default(),
                notifications: Mutex::new(None),
                restore_pending: AtomicBool::new(true),
                enrollment_checked: AtomicU64::new(0),
                renew_connections: AtomicBool::new(false),
                identity_settled: AtomicBool::new(false),
                update_required: AtomicBool::new(false),
                friends_changed: tokio::sync::Notify::new(),
                login_interrupt: std::sync::Mutex::new(CancellationToken::new()),
                app_login: std::sync::Mutex::new(None),
                rooms: Mutex::new(rooms::RoomCache::default()),
            }),
        })
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    fn seen_rooms(&self) -> (std::path::PathBuf, BTreeMap<String, BTreeMap<String, u64>>) {
        let path = self.inner.config.data_root.join("network/room-seen.json");
        let seen = std::fs::read(&path)
            .ok()
            .filter(|bytes| bytes.len() <= 1024 * 1024)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        (path, seen)
    }

    pub(crate) fn rooms_seen(&self, user: &str) -> BTreeMap<String, u64> {
        self.seen_rooms().1.remove(user).unwrap_or_default()
    }

    pub(crate) fn remember_seen(&self, user: &str, rooms: &BTreeMap<String, u64>) -> Result<()> {
        let (path, mut seen) = self.seen_rooms();
        seen.insert(user.to_owned(), rooms.clone());
        kodosi_os::fs::write_private(&path, &serde_json::to_vec(&seen)?)?;
        Ok(())
    }
    pub(crate) fn ready_event(&self, user_id: &str, enrolled: bool) -> Value {
        let display_name = self.inner.credentials.read().ok().and_then(|credentials| {
            credentials
                .as_ref()
                .filter(|credentials| credentials.user_id == user_id)
                .map(|credentials| credentials.display_name.clone())
        });
        json!({"type":"auth.ready","userId":user_id,"displayName":display_name,"enrolled":enrolled})
    }

    pub fn identity(&self) -> Option<Identity> {
        if !self.inner.identity_settled.load(Ordering::Acquire) {
            return None;
        }
        let credentials = self.inner.credentials.read().ok()?;
        credentials.as_ref().map(|credentials| Identity {
            user_id: credentials.user_id.clone(),
            device_id: credentials.keys.device_id.clone(),
            display_name: credentials.display_name.clone(),
            enrolled: credentials.enrolled,
        })
    }
    pub fn events(&self) -> broadcast::Receiver<BackendEvent> {
        self.inner.events.subscribe()
    }

    pub async fn initialize(&self) -> Result<()> {
        let generation = self.generation();
        if self.inner.initialized.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        let this = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(10));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            tick.tick().await;
            loop {
                tokio::select! {
                    ()=this.inner.shutdown.cancelled()=>break,
                    _=tick.tick()=>{
                        let Ok(_operation)=this.inner.operations.try_lock() else {continue;};
                        let result=if this.identity().is_some() {
                            this.refresh().await
                        }else if this.inner.restore_pending.load(Ordering::Acquire) {
                            this.restore_saved(this.generation()).await
                        }else{
                            Ok(())
                        };
                        if let Err(error)=this.sign_in_kept(result).await
                            && error.user_facing()
                        {
                            tracing::warn!(%error, "the account was not refreshed; it will be tried again");
                        }
                    }
                }
            }
        });
        let _operation = self.inner.operations.lock().await;
        terminal_connections::check_generation(self, generation)?;
        let restored = self.restore_saved(generation).await;
        match self.sign_in_kept(restored).await {
            Err(error) if error.unanswered() => {
                tracing::warn!(%error, "saved sign-in was not restored; it will be tried again");
                Ok(())
            }
            result => result,
        }
    }

    async fn complete_login(
        &self,
        tokens: impl Future<Output = Result<Tokens>>,
        cancel: CancellationToken,
        generation: u64,
        request: String,
    ) {
        let result = async {
            let tokens = tokens.await?;
            let _operation = self.inner.operations.lock().await;
            if cancel.is_cancelled() || self.generation() != generation {
                return Err(Error::Stale);
            }
            self.emit_for(generation, None, json!({"type":"auth.finalizing"}));
            tokio::select! {
                () = cancel.cancelled() => Err(Error::Stale),
                () = self.inner.shutdown.cancelled() => Err(Error::Closed),
                result = self.finish_login(tokens, generation) => result,
            }
        }
        .await;
        let answer = match result {
            Ok(()) => {
                json!({"type":"auth.result","requestId":request,"operation":"auth.login.start"})
            }
            Err(Error::Closed) => return,
            Err(error) if error.user_facing() && !cancel.is_cancelled() => {
                json!({"type":"auth.error","requestId":request,"operation":"auth.login.start","message":error.to_string()})
            }
            Err(_) => {
                json!({"type":"auth.error","requestId":request,"operation":"auth.login.start","message":"The sign-in was cancelled."})
            }
        };
        self.emit_for(
            self.generation(),
            self.identity().map(|identity| identity.user_id),
            answer,
        );
    }

    async fn saved_sign_in_returns(&self, generation: u64) -> Result<bool> {
        if !self.inner.restore_pending.load(Ordering::Acquire) {
            return Ok(false);
        }
        self.new_login_interrupt()?;
        let restored = self.restore_saved(generation).await;
        self.sign_in_kept(restored).await?;
        Ok(self.identity().is_some())
    }

    fn new_login_interrupt(&self) -> Result<CancellationToken> {
        let cancel = CancellationToken::new();
        *self
            .inner
            .login_interrupt
            .lock()
            .map_err(|_| Error::Closed)? = cancel.clone();
        Ok(cancel)
    }

    async fn sign_in_kept(&self, result: Result<()>) -> Result<()> {
        let reason = match result {
            Err(Error::AccountDeleted) => "accountDeleted",
            Err(Error::SignedOut) if self.deletion_confirmed().await => "accountDeleted",
            Err(Error::SignedOut) => "expired",
            result => return result,
        };
        if reason == "accountDeleted" {
            self.forget_deleted_account().await?;
        } else {
            self.logout().await?;
        }
        self.emit_for(
            self.generation(),
            None,
            json!({"type":"auth.required","reason":reason}),
        );
        Ok(())
    }

    pub async fn execute(&self, operation: &str, args: Value) -> Result<BackendReply> {
        match self.dispatch(operation, args).await {
            Err(Error::AccountDeleted) => {
                let _operation = self.inner.operations.lock().await;
                self.sign_in_kept(Err(Error::AccountDeleted)).await?;
                Err(Error::Stale)
            }
            Err(Error::UpdateRequired) => {
                self.update_required();
                Err(Error::UpdateRequired)
            }
            result => result,
        }
    }

    pub(crate) fn update_required(&self) {
        if !self.inner.update_required.swap(true, Ordering::AcqRel) {
            self.emit_for(
                self.generation(),
                self.identity().map(|identity| identity.user_id),
                json!({"type":"system.updateRequired"}),
            );
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "closed command dispatch keeps retained surface mapping in one place"
    )]
    async fn dispatch(&self, operation: &str, args: Value) -> Result<BackendReply> {
        if matches!(
            operation,
            "auth.logout" | "auth.login.start" | "auth.login.cancel"
        ) {
            if operation != "auth.login.start"
                && let Some(credentials) = self
                    .inner
                    .credentials
                    .read()
                    .map_err(|_| Error::Closed)?
                    .as_ref()
            {
                credentials.cancel.cancel();
            }
            self.inner
                .login_interrupt
                .lock()
                .map_err(|_| Error::Closed)?
                .cancel();
            self.inner
                .app_login
                .lock()
                .map_err(|_| Error::Closed)?
                .take();
        }
        let before = self.generation();
        let _operation = self.inner.operations.lock().await;
        terminal_connections::check_generation(self, before)?;
        let request = wire::text(&args, "requestId")?.to_owned();
        let mut events = Vec::new();
        let mut answered_later = false;
        match operation {
            "auth.login.start" if self.saved_sign_in_returns(before).await? => {}
            "auth.login.start" => match args.get("browser").and_then(Value::as_str) {
                None => {
                    let oidc = self.oidc()?;
                    let login = oidc.start().await?;
                    self.logout().await?;
                    let cancel = self.new_login_interrupt()?;
                    let generation = self.generation();
                    events.push(json!({"type":"auth.device_code","requestId":request,"userCode":login.user_code,"verificationUri":login.code_page()}));
                    answered_later = true;
                    let this = self.clone();
                    tokio::spawn(async move {
                        let waiting = cancel.clone();
                        let tokens = oidc.poll(login, &waiting);
                        this.complete_login(tokens, cancel, generation, request)
                            .await;
                    });
                }
                Some("system") => {
                    let oidc = self.oidc()?;
                    let loopback = Loopback::bind().await?;
                    let login = oidc.browser(loopback.redirect_uri()).await?;
                    self.logout().await?;
                    let cancel = self.new_login_interrupt()?;
                    let generation = self.generation();
                    events
                        .push(json!({"type":"auth.browser","requestId":request,"url":login.page}));
                    answered_later = true;
                    let this = self.clone();
                    let waiting = cancel.clone();
                    tokio::spawn(async move {
                        let tokens = async {
                            let (answer, stream) = tokio::select! {
                                () = waiting.cancelled() => return Err(Error::Stale),
                                () = this.inner.shutdown.cancelled() => return Err(Error::Closed),
                                () = tokio::time::sleep(SIGN_IN_LIFE) => {
                                    return Err(invalid("The sign-in took too long. Try again."));
                                }
                                answer = loopback.answer(&login) => answer?,
                            };
                            let tokens = oidc.redeem(&login, &answer).await;
                            let outcome = tokens.as_ref().map(|_| ()).map_err(ToString::to_string);
                            loopback::finish(stream, outcome).await;
                            tokens
                        };
                        this.complete_login(tokens, cancel, generation, request)
                            .await;
                    });
                }
                Some("app") => {
                    let login = self.oidc()?.browser(APP_REDIRECT).await?;
                    self.logout().await?;
                    let cancel = self.new_login_interrupt()?;
                    events
                        .push(json!({"type":"auth.browser","requestId":request,"url":login.page}));
                    answered_later = true;
                    *self.inner.app_login.lock().map_err(|_| Error::Closed)? = Some(AppLogin {
                        login,
                        cancel,
                        generation: self.generation(),
                        until: tokio::time::Instant::now() + SIGN_IN_LIFE,
                        request,
                    });
                }
                Some(_) => return Err(invalid("Unknown sign-in browser.")),
            },
            "auth.login.finish" => {
                let answer = reqwest::Url::parse(wire::text(&args, "url")?)
                    .map_err(|_| invalid("The sign-in answer is not an address."))?;
                let pending = {
                    let mut pending = self.inner.app_login.lock().map_err(|_| Error::Closed)?;
                    if !pending.as_ref().is_some_and(|app| app.login.owns(&answer)) {
                        return Err(invalid("This answer does not belong to the sign-in."));
                    }
                    pending.take().ok_or(Error::Stale)?
                };
                if tokio::time::Instant::now() > pending.until {
                    return Err(invalid("The sign-in took too long. Try again."));
                }
                let oidc = self.oidc()?;
                let this = self.clone();
                tokio::spawn(async move {
                    let tokens = oidc.redeem(&pending.login, &answer);
                    this.complete_login(
                        tokens,
                        pending.cancel,
                        pending.generation,
                        pending.request,
                    )
                    .await;
                });
            }
            "auth.logout" => {
                self.logout().await?;
                events.push(json!({"type":"auth.required","reason":"signedOut"}));
            }
            "auth.login.cancel" => {
                self.logout().await?;
                events.push(json!({"type":"auth.required","reason":"cancelled"}));
            }
            "auth.deleteAccount" => {
                let credentials = self.credentials()?;
                let profile: Value = self
                    .inner
                    .http
                    .bearer(Method::GET, "api/me", &credentials.token, None)
                    .await?;
                if let Some(page) = profile.get("deletionUri").and_then(Value::as_str) {
                    let page = self.deletion_page(page)?;
                    events.push(json!({"type":"auth.deletion_pending","requestId":request,"confirmationUri":page}));
                    answered_later = true;
                    self.watch_deletion(credentials, request).await;
                } else {
                    let _response: Value = self
                        .inner
                        .http
                        .bearer(Method::DELETE, "api/me", &credentials.token, None)
                        .await?;
                    credentials.cancel.cancel();
                    self.forget_deleted_account().await?;
                    events.push(json!({"type":"auth.required","reason":"accountDeleted"}));
                }
            }
            "auth.deleteAccount.cancel" => {
                self.inner.state.lock().await.deletion = None;
            }
            "auth.refresh" => {
                let refreshed = self.refresh().await;
                self.sign_in_kept(refreshed).await?;
                if let Some(identity) = self.identity() {
                    events.push(self.ready_event(&identity.user_id, identity.enrolled));
                }
            }
            "devices.refresh" => {
                events.extend(self.device_events().await?);
            }
            "devices.reset" => {
                events.extend(self.reset_devices().await?);
            }
            "devices.link.startSelf" => {
                let code = self.start_link().await?;
                events.push(json!({"type":"devices.result","requestId":request,"operation":operation,"code":code}));
                self.watch_link().await;
            }
            "devices.link.cancelSelf" => {
                self.cancel_link().await?;
                events.push(json!({"type":"devices.link.selfResolved","outcome":"cancelled"}));
            }
            "devices.link.approve" => {
                let label = self.approve_link(wire::text(&args, "code")?).await?;
                events.push(
                    json!({"type":"devices.link.resolved","deviceLabel":label,"outcome":"approved"}),
                );
                events.extend(self.device_events().await?);
            }
            "devices.revoke" => {
                self.revoke_device(wire::text(&args, "deviceId")?).await?;
                events.extend(self.device_events().await?);
            }
            "devices.recovery.create" => {
                let key = self.create_recovery_key().await?;
                events.extend(self.device_events().await?);
                events.push(json!({"type":"devices.result","requestId":request,"operation":operation,"key":key}));
            }
            "devices.recovery.use" => {
                events.extend(self.use_recovery_key(wire::text(&args, "key")?).await?);
            }
            "room.command" => events.extend(self.room_command(&args).await?),
            "friends.refresh" => events.push(self.friend_event().await?),
            operation if operation.starts_with("friends.") => {
                events.extend(self.friend_command(operation, &args).await?);
            }
            "session.list" => events.push(self.session_event().await?),
            #[cfg(feature = "host")]
            "session.share" => {
                let id = wire::id(&args, "sessionId")?;
                let expected = wire::id(&args, "expectedRuntimeIncarnationId")?;
                let users = args
                    .get("userIds")
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid("Choose friends to share with."))?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or_else(|| invalid("Invalid friend identity."))
                    })
                    .collect::<Result<BTreeSet<_>>>()?;
                if let Some(expected_users) =
                    args.get("expectedUserIds").filter(|value| !value.is_null())
                {
                    let expected_users: BTreeSet<String> =
                        serde_json::from_value(expected_users.clone())?;
                    let publication = self
                        .inner
                        .publications
                        .lock()
                        .await
                        .get(&id)
                        .cloned()
                        .ok_or(Error::Stale)?;
                    if publication.info.read().await.shared_with != expected_users {
                        return Err(invalid(crate::runtime::SHARING_CHANGED));
                    }
                }
                self.require_trusted_friends(&self.credentials()?, &users)
                    .await?;
                self.set_shares(id, expected, users).await?;
                events.push(self.session_event().await?);
                events.push(session_result(operation, &args));
            }
            "session.leave" => {
                let credentials = self.credentials()?;
                let id = wire::id(&args, "sessionId")?;
                let current: SessionDto = self
                    .inner
                    .http
                    .device(
                        Method::GET,
                        &format!("api/sessions/{id}"),
                        &credentials,
                        None,
                    )
                    .await?;
                if current.incarnation_id != wire::id(&args, "expectedRuntimeIncarnationId")? {
                    return Err(Error::Stale);
                }
                let _response:Value=self.inner.http.device(Method::DELETE,&format!("api/sessions/{id}/members/me"),&credentials,Some(json!({"incarnationId":current.incarnation_id,"expectedRevision":current.authorization_revision}))).await?;
                events.push(self.session_event().await?);
                events.push(session_result(operation, &args));
            }
            "session.rename" | "session.attachMission" => {
                let credentials = self.credentials()?;
                let id = wire::id(&args, "sessionId")?;
                let current: SessionDto = self
                    .inner
                    .http
                    .device(
                        Method::GET,
                        &format!("api/sessions/{id}"),
                        &credentials,
                        None,
                    )
                    .await?;
                if current.incarnation_id != wire::id(&args, "expectedRuntimeIncarnationId")? {
                    return Err(Error::Stale);
                }
                if operation == "session.rename" {
                    let _response:Value=self.inner.http.device(Method::PATCH,&format!("api/sessions/{id}"),&credentials,Some(json!({"incarnationId":current.incarnation_id,"expectedRevision":current.authorization_revision,"name":wire::text(&args,"name")?}))).await?;
                } else {
                    let room = args
                        .get("missionId")
                        .and_then(Value::as_str)
                        .map(|room| {
                            Uuid::parse_str(room).map_err(|_| invalid("Invalid room identity."))
                        })
                        .transpose()?;
                    if let Some(room) = room {
                        self.refresh_room_keys(
                            &credentials,
                            room,
                            false,
                            &mut rooms::Lookups::default(),
                        )
                        .await?;
                    }
                    let choice = Self::room_choice(&credentials, &current, room)?;
                    let _response:Value=self.inner.http.device(Method::PUT,&format!("api/sessions/{id}/mission"),&credentials,Some(json!({"incarnationId":current.incarnation_id,"missionId":room,"choice":choice}))).await?;
                }
                events.push(self.session_event().await?);
                events.push(session_result(operation, &args));
            }
            "mission.list" => events.push(self.mission_list().await?),
            "mission.open" => {
                let credentials = self.credentials()?;
                let id = wire::id(&args, "missionId")?;
                let value: Value = self
                    .inner
                    .http
                    .device(
                        Method::GET,
                        &format!("api/missions/{id}"),
                        &credentials,
                        None,
                    )
                    .await?;
                events.push(json!({"type":"mission.result","requestId":args["requestId"],"operation":operation,
                    "missionId":id,"mission":value["mission"],"members":value["members"]}));
            }
            "mission.create"
            | "mission.rename"
            | "mission.delete"
            | "mission.invite"
            | "mission.invitation.accept"
            | "mission.invitation.reject"
            | "mission.removeMember"
            | "mission.leave" => {
                self.mission_mutation(operation, &args).await?;
                events.push(self.mission_list().await?);
                events.push(json!({"type":"mission.result","requestId":args["requestId"],"missionId":args.get("missionId"),"operation":operation}));
            }
            _ => return Err(invalid("Unsupported network command.")),
        }
        if !matches!(
            operation,
            "auth.logout" | "auth.login.start" | "auth.login.cancel" | "auth.deleteAccount"
        ) && before != self.generation()
        {
            return Err(Error::Stale);
        }
        Ok(BackendReply {
            generation: self.generation(),
            user_id: self.identity().map(|value| value.user_id),
            events,
            answered_later,
        })
    }

    pub(crate) fn emit_for(&self, generation: u64, user_id: Option<String>, event: Value) {
        let _outcome = self.inner.events.send(BackendEvent {
            generation,
            user_id,
            event,
        });
    }

    pub(crate) fn check_credentials(&self, credentials: &Credentials) -> Result<()> {
        if credentials.generation != self.generation()
            || credentials.cancel.is_cancelled()
            || self.inner.shutdown.is_cancelled()
        {
            return Err(Error::Stale);
        }
        Ok(())
    }

    pub(crate) fn credentials(&self) -> Result<Credentials> {
        let credentials = self
            .inner
            .credentials
            .read()
            .map_err(|_| Error::Closed)?
            .clone()
            .ok_or(Error::SignedOut)?;
        self.check_credentials(&credentials)?;
        Ok(credentials)
    }

    async fn session_event(&self) -> Result<Value> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Ok(json!({"type":"network.sessions","sessions":[]}));
        }
        let sessions: Vec<SessionDto> = self
            .inner
            .http
            .device(Method::GET, "api/sessions", &credentials, None)
            .await?;
        if let Ok(mut kept) = self.inner.sessions.lock() {
            *kept = sessions.iter().map(|dto| (dto.id, dto.clone())).collect();
        }
        #[cfg(feature = "host")]
        {
            let owned = {
                let publications = self.inner.publications.lock().await;
                sessions
                    .iter()
                    .filter_map(|dto| Some((Arc::clone(publications.get(&dto.id)?), dto)))
                    .collect::<Vec<_>>()
            };
            for (publication, dto) in owned {
                if publication.changing.load(Ordering::Acquire)
                    || publication.pending_shares.lock().await.is_some()
                {
                    continue;
                }
                let local = {
                    let info = publication.info.read().await;
                    (info.mission_id, info.room_sequence)
                };
                let change = self.room_change(&credentials, local, dto).await;
                let mut info = publication.info.write().await;
                if info.incarnation_id != dto.incarnation_id
                    || dto.host_device_id != credentials.keys.device_id
                {
                    continue;
                }
                info.name.clone_from(&dto.name);
                if let Some((room, sequence)) =
                    change.filter(|_| (info.mission_id, info.room_sequence) == local)
                {
                    info.mission_id = room;
                    info.room_sequence = sequence;
                }
                let reported = dto.shared_with.iter().cloned().collect::<BTreeSet<_>>();
                let removed = !info.shared_with.is_subset(&reported);
                if info.mission_id.is_none() {
                    info.shared_with.retain(|user| reported.contains(user));
                } else if info.mission_id == dto.mission_id {
                    info.shared_with.clone_from(&reported);
                }
                let sharing = json!({
                    "type":"network.sharing", "sessionId":dto.id, "incarnationId":dto.incarnation_id,
                    "sharedWith":info.shared_with, "missionId":info.mission_id, "roomSequence":info.room_sequence
                });
                drop(info);
                let previous = publication.dto.read().await.clone();
                *publication.dto.write().await = dto.clone();
                if removed || previous.authorization_revision != dto.authorization_revision {
                    publication.invalidate();
                }
                self.emit_for(
                    credentials.generation,
                    Some(credentials.user_id.clone()),
                    sharing,
                );
            }
        }
        Ok(
            json!({"type":"network.sessions","sessions":sessions.into_iter().map(super::RemoteSession::from).collect::<Vec<_>>()}),
        )
    }

    #[cfg(feature = "host")]
    async fn exclude_friend(&self, credentials: &Credentials, username: &str) -> Result<()> {
        let friends: Vec<Value> = self
            .inner
            .http
            .device(Method::GET, "api/friends", credentials, None)
            .await?;
        if let Some(friend) = friends.iter().find(|friend| {
            friend
                .get("handle")
                .and_then(Value::as_str)
                .is_some_and(|handle| handle.eq_ignore_ascii_case(username))
        }) {
            let user = wire::text(friend, "userId")?;
            for publication in self.inner.publications.lock().await.values() {
                let mut info = publication.info.write().await;
                if info.mission_id.is_none() && info.shared_with.remove(user) {
                    *publication.pending_shares.lock().await = Some(info.shared_with.clone());
                    drop(info);
                    publication.invalidate();
                }
            }
        }
        Ok(())
    }

    async fn mission_list(&self) -> Result<Value> {
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Ok(json!({"type":"missions.snapshot","missions":[],"invitations":[]}));
        }
        let mut missions: Value = self
            .inner
            .http
            .device(Method::GET, "api/missions", &credentials, None)
            .await?;
        self.learn_dosi(&credentials, &mut missions).await;
        missions["type"] = json!("missions.snapshot");
        Ok(missions)
    }

    async fn mission_mutation(&self, operation: &str, args: &Value) -> Result<()> {
        let credentials = self.credentials()?;
        let request_id = wire::id(args, "requestId")?;
        let (method, path, body) = match operation {
            "mission.create" => (
                Method::POST,
                "api/missions".to_owned(),
                Some(json!({"id":request_id,"name":wire::text(args,"name")?})),
            ),
            "mission.invitation.accept" | "mission.invitation.reject" => (
                Method::POST,
                format!(
                    "api/missions/invitations/{}/{}",
                    wire::id(args, "invitationId")?,
                    operation.rsplit('.').next().unwrap_or_default()
                ),
                None,
            ),
            other => {
                let id = wire::id(args, "missionId")?;
                match other {
                    "mission.rename" => (
                        Method::PATCH,
                        format!("api/missions/{id}"),
                        Some(json!({"name":wire::text(args,"name")?})),
                    ),
                    "mission.delete" => (Method::DELETE, format!("api/missions/{id}"), None),
                    "mission.invite" => {
                        let user = wire::text(args, "userId")?;
                        let keys = self
                            .prepare_room_membership(&credentials, id, Some(user), None)
                            .await?;
                        (
                            Method::POST,
                            format!("api/missions/{id}/invitations"),
                            Some(json!({"id":request_id,"userId":user,"keys":keys})),
                        )
                    }
                    "mission.removeMember" => {
                        let user = wire::text(args, "userId")?;
                        let keys = self
                            .prepare_room_membership(&credentials, id, None, Some(user))
                            .await?;
                        (
                            Method::DELETE,
                            format!("api/missions/{id}/members/{user}"),
                            Some(json!({"keys":keys})),
                        )
                    }
                    "mission.leave" => {
                        let keys = self
                            .prepare_room_membership(
                                &credentials,
                                id,
                                None,
                                Some(&credentials.user_id),
                            )
                            .await?;
                        (
                            Method::DELETE,
                            format!("api/missions/{id}/members/{}", credentials.user_id),
                            Some(json!({"keys":keys})),
                        )
                    }
                    _ => return Err(invalid("Unsupported room operation.")),
                }
            }
        };
        let body = if operation == "mission.invitation.reject" {
            let list = self.mission_list().await?;
            let invitation = wire::text(args, "invitationId")?;
            let room = list["invitations"]
                .as_array()
                .and_then(|items| {
                    items
                        .iter()
                        .find(|item| item["id"].as_str() == Some(invitation))
                })
                .and_then(|item| item["missionId"].as_str())
                .ok_or_else(|| invalid("This invitation is unavailable."))?;
            let room = Uuid::parse_str(room).map_err(|_| invalid("Invalid room identity."))?;
            Some(
                json!({"keys":self.prepare_room_membership(&credentials, room, None, Some(&credentials.user_id)).await?}),
            )
        } else {
            body
        };
        let _response: Value = self
            .inner
            .http
            .device(method, &path, &credentials, body)
            .await?;
        Ok(())
    }

    #[cfg(feature = "host")]
    pub async fn publish(
        &self,
        info: LocalPublication,
        requests: mpsc::Sender<HostRequest>,
        output: PublicationOutput,
    ) -> Result<()> {
        let generation = self.generation();
        let _operation = self.inner.operations.lock().await;
        terminal_connections::check_generation(self, generation)?;
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Err(Error::EnrollmentRequired);
        }
        if let Some(room) = info.mission_id {
            self.refresh_room_keys(&credentials, room, false, &mut rooms::Lookups::default())
                .await?;
        }
        if let Some(publication) = self.inner.publications.lock().await.get(&info.session_id) {
            if publication.info.read().await.incarnation_id != info.incarnation_id {
                return Err(Error::Stale);
            }
            return Ok(());
        }
        let mut dto:SessionDto=self.inner.http.device(Method::POST,"api/sessions",&credentials,Some(json!({"id":info.session_id,"incarnationId":info.incarnation_id,"name":info.name,"hostDeviceId":credentials.keys.device_id,"hostName":host_label(),"missionId":info.mission_id}))).await?;
        if info.mission_id.is_none()
            && dto.shared_with.iter().cloned().collect::<BTreeSet<_>>() != info.shared_with
        {
            dto=self.inner.http.device(Method::PUT,&format!("api/sessions/{}/members",info.session_id),&credentials,Some(json!({"incarnationId":info.incarnation_id,"expectedRevision":dto.authorization_revision,"userIds":info.shared_with}))).await?;
        }
        self.check_credentials(&credentials)?;
        let publication = Arc::new(terminal_connections::Publication::new(
            info, requests, output, dto,
        ));
        self.inner.publications.lock().await.insert(
            publication.info.read().await.session_id,
            Arc::clone(&publication),
        );
        terminal_connections::spawn_host(self.clone(), publication, credentials);
        Ok(())
    }

    #[cfg(feature = "host")]
    pub async fn unpublish(&self, id: Uuid) -> Result<()> {
        let generation = self.generation();
        let draining = self.inner.publications.lock().await.get(&id).cloned();
        if let Some(publication) = draining {
            let _drained =
                tokio::time::timeout(Duration::from_secs(12), publication.drained.cancelled())
                    .await;
        }
        let _operation = self.inner.operations.lock().await;
        terminal_connections::check_generation(self, generation)?;
        let publication = self.inner.publications.lock().await.get(&id).cloned();
        if let Some(publication) = publication {
            publication.cancel.cancel();
            publication.invalidate();
            let info = publication.info.read().await.clone();
            self.delete_session(id, info.incarnation_id).await?;
            let mut publications = self.inner.publications.lock().await;
            if publications
                .get(&id)
                .is_some_and(|current| Arc::ptr_eq(current, &publication))
            {
                publications.remove(&id);
            }
        }
        Ok(())
    }

    pub async fn end_hosted(&self, id: Uuid, incarnation: Uuid) -> Result<()> {
        let generation = self.generation();
        let _operation = self.inner.operations.lock().await;
        terminal_connections::check_generation(self, generation)?;
        self.delete_session(id, incarnation).await
    }

    async fn delete_session(&self, id: Uuid, incarnation: Uuid) -> Result<()> {
        let credentials = self.credentials()?;
        let _response: Value = self
            .inner
            .http
            .device(
                Method::DELETE,
                &format!("api/sessions/{id}?incarnationId={incarnation}"),
                &credentials,
                None,
            )
            .await?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn assume_identity(&self, identity: Identity) {
        let mut keys = DeviceKeys::generate().expect("device keys");
        keys.device_id = identity.device_id;
        *self.inner.credentials.write().expect("credentials") = Some(Credentials {
            user_id: identity.user_id,
            display_name: identity.display_name,
            token: Zeroizing::new(String::new()),
            keys: Arc::new(keys),
            enrolled: identity.enrolled,
            notice: None,
            generation: self.generation(),
            cancel: CancellationToken::new(),
        });
        self.inner.identity_settled.store(true, Ordering::Release);
    }

    #[cfg(feature = "host")]
    async fn set_shares(&self, id: Uuid, incarnation: Uuid, users: BTreeSet<String>) -> Result<()> {
        let publication = self
            .inner
            .publications
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| invalid(crate::runtime::SHARE_ON_HOST))?;
        if publication.info.read().await.incarnation_id != incarnation {
            return Err(Error::Stale);
        }
        let credentials = self.credentials()?;
        let previous = publication
            .info
            .read()
            .await
            .shared_with
            .intersection(
                &publication
                    .dto
                    .read()
                    .await
                    .shared_with
                    .iter()
                    .cloned()
                    .collect(),
            )
            .cloned()
            .collect::<BTreeSet<_>>();
        publication.changing.store(true, Ordering::Release);
        publication.invalidate();
        publication.info.write().await.shared_with = users.clone();
        *publication.pending_shares.lock().await = Some(users.clone());
        let result=async {
            let current:SessionDto=self.inner.http.device(Method::GET,&format!("api/sessions/{id}"),&credentials,None).await?;
            if current.incarnation_id != incarnation { return Err(Error::Stale); }
            if current.shared_with.iter().cloned().collect::<BTreeSet<_>>() == users { return Ok(current); }
            if !users.is_subset(&previous) && (current.authorization_revision != publication.dto.read().await.authorization_revision
                || current.shared_with.iter().cloned().collect::<BTreeSet<_>>() != previous) {
                return Err(Error::Backend { status: 409, message: "The sharing of this terminal changed. Refresh before you add people.".into() });
            }
            self.inner.http.device::<SessionDto>(Method::PUT,&format!("api/sessions/{id}/members"),&credentials,Some(json!({"incarnationId":incarnation,"expectedRevision":current.authorization_revision,"userIds":users}))).await
        }.await;
        if let Ok(dto) = &result {
            *publication.dto.write().await = dto.clone();
            *publication.pending_shares.lock().await = None;
        } else if matches!(
            &result,
            Err(Error::Backend {
                status: 400 | 401 | 403 | 404 | 409,
                ..
            })
        ) {
            let retained = previous
                .intersection(&users)
                .cloned()
                .collect::<BTreeSet<_>>();
            *publication.pending_shares.lock().await =
                (retained != previous).then_some(retained.clone());
            publication.info.write().await.shared_with = retained;
        }
        publication.changing.store(false, Ordering::Release);
        publication.refresh.notify_one();
        result.map(|_| ())
    }

    #[cfg(feature = "host")]
    async fn reconcile_shares(&self) -> Result<()> {
        let publications = self
            .inner
            .publications
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for publication in publications {
            let pending = publication.pending_shares.lock().await.clone();
            if let Some(users) = pending {
                let info = publication.info.read().await.clone();
                self.set_shares(info.session_id, info.incarnation_id, users)
                    .await?;
                let credentials = self.credentials()?;
                let event = self.session_event().await?;
                self.emit_for(credentials.generation, Some(credentials.user_id), event);
            }
        }
        Ok(())
    }

    #[cfg(feature = "host")]
    async fn review_access(&self) -> Result<()> {
        for publication in self.inner.publications.lock().await.values() {
            publication.invalidate();
        }
        Ok(())
    }

    pub(crate) fn channel_keys(
        &self,
        credentials: &Credentials,
    ) -> Result<Arc<terminal_connections::ChannelKeys>> {
        let kept = self
            .inner
            .channel_keys
            .lock()
            .map_err(|_| Error::Closed)?
            .as_ref()
            .filter(|keys| keys.belongs_to(credentials.keys.signing_public()))
            .map(Arc::clone);
        if let Some(keys) = kept {
            return Ok(keys);
        }
        let keys = Arc::new(terminal_connections::ChannelKeys::new(
            credentials.keys.signing_pkcs8(),
            credentials.keys.signing_public(),
        )?);
        *self.inner.channel_keys.lock().map_err(|_| Error::Closed)? = Some(Arc::clone(&keys));
        Ok(keys)
    }

    pub(crate) async fn known_session(
        &self,
        credentials: &Credentials,
        id: Uuid,
    ) -> Result<(SessionDto, bool)> {
        let kept = self
            .inner
            .sessions
            .lock()
            .map_err(|_| Error::Closed)?
            .get(&id)
            .cloned();
        match kept {
            Some(dto) => Ok((dto, true)),
            None => Ok((self.fetch_session(credentials, id).await?, false)),
        }
    }

    pub(crate) async fn fetch_session(
        &self,
        credentials: &Credentials,
        id: Uuid,
    ) -> Result<SessionDto> {
        let dto: SessionDto = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/sessions/{id}"),
                credentials,
                None,
            )
            .await?;
        if let Ok(mut kept) = self.inner.sessions.lock() {
            kept.insert(id, dto.clone());
        }
        Ok(dto)
    }

    pub async fn connect_remote(&self, id: Uuid) -> Result<RemoteConnection> {
        let generation = self.generation();
        let _operation = self.inner.operations.lock().await;
        terminal_connections::check_generation(self, generation)?;
        self.renew_token().await?;
        terminal_connections::connect_remote(self.clone(), id).await
    }

    pub(crate) async fn ask_computer(&self, device: &str, request: Value) -> Result<Value> {
        terminal_connections::ask_computer(self, device, request).await
    }

    #[cfg(feature = "host")]
    pub(crate) fn answer_calls(&self, answering: terminal_connections::Answering) {
        terminal_connections::answer_calls(self.clone(), answering);
    }
}

fn path_segment(value: &str) -> Result<String> {
    if value.is_empty() || value.len() > 256 {
        return Err(invalid("Invalid resource identity."));
    }
    let mut url =
        reqwest::Url::parse("https://path.invalid/").map_err(|_| invalid("Invalid URL."))?;
    url.path_segments_mut()
        .map_err(|()| invalid("Invalid URL."))?
        .push(value);
    Ok(url.path().trim_start_matches('/').to_owned())
}
pub(crate) fn host_label() -> String {
    let label = device_label().or_else(|| {
        std::env::var("HOSTNAME")
            .ok()
            .and_then(|value| normalize_host_label(&value))
    });
    #[cfg(feature = "host")]
    let label = label.or_else(|| {
        kodosi_os::process::computer_name().and_then(|value| normalize_host_label(&value))
    });
    label.unwrap_or_else(|| "Unnamed computer".to_owned())
}

pub(crate) fn device_label() -> Option<String> {
    std::env::var("KODOSI_DEVICE_LABEL")
        .ok()
        .and_then(|value| normalize_host_label(&value))
}

fn normalize_host_label(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(value[..value.floor_char_boundary(128)].to_owned())
}
fn session_result(operation: &str, args: &Value) -> Value {
    json!({"type":"session.result","requestId":args["requestId"],"sessionId":args["sessionId"],"operation":operation})
}

mod account;
pub(crate) use account::DELETION_LIFE;
mod device_identity;
mod enrollment;
mod friends;
mod notifications;
mod recovery;
mod rooms;
mod terminal_rooms;
#[cfg(test)]
mod tests;
