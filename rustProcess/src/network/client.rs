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
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{
    Error, HostRequest, Identity, LocalPublication, NetworkConfig, NetworkEvent, NetworkReply,
    PublicationOutput, RemoteConnection, Result, crypto,
    http::{Credentials, Http},
    invalid, relay,
    wire::{self, SessionDto},
};
use crate::identity::{
    self,
    device_cert::{build_cert_for, build_self_cert},
    keys::DeviceKeys,
    oidc::{Oidc, Tokens},
    pins::{IdentityBundle, Pins, VerifiedIdentity},
    signed_device_list::{DeviceListEntry, build_bootstrap_list, build_replacement_list},
    storage::Secrets,
};

#[derive(Clone)]
pub struct Network {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub(crate) http: Http,
    pub(crate) config: NetworkConfig,
    pub(crate) generation: AtomicU64,
    pub(crate) identity: RwLock<Option<Identity>>,
    pub(crate) credentials: RwLock<Option<Credentials>>,
    pub(crate) state: Mutex<State>,
    pub(crate) operations: Mutex<()>,
    pub(crate) events: broadcast::Sender<NetworkEvent>,
    pub(crate) shutdown: CancellationToken,
    initialized: AtomicBool,
    pub(crate) publications: Mutex<BTreeMap<Uuid, Arc<relay::Publication>>>,
    pub(crate) connections: Mutex<Vec<CancellationToken>>,
    notifications: Mutex<Option<CancellationToken>>,
    restore_pending: AtomicBool,
    identity_settled: AtomicBool,
    login_interrupt: std::sync::Mutex<CancellationToken>,
}

pub(crate) struct State {
    tokens: Option<Tokens>,
    secrets: Secrets,
    pub(crate) pins: Arc<std::sync::Mutex<Pins>>,
    blocked_devices: BTreeSet<(String, String)>,
    account_cancel: CancellationToken,
    link: Option<Value>,
}

impl Network {
    pub fn new(config: NetworkConfig) -> Result<Self> {
        let http = Http::new(config.api_url.clone())?;
        for legacy in [
            config.data_root.join("device-list-pins.json"),
            config.data_root.join("network/device-list-pins.json"),
        ] {
            match std::fs::symlink_metadata(&legacy) {
                Ok(_) => return Err(Error::Trust("Existing device trust uses an older store. Preserve it and migrate its pins before enabling remote access.".into())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(error.into()),
            }
        }
        let pins = Pins::load(config.data_root.join("network/device-pins.json"))?;
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
            config.secret_service.clone(),
            config.isolated,
        );
        let (events, _) = broadcast::channel(128);
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                config,
                generation: AtomicU64::new(1),
                identity: RwLock::new(None),
                credentials: RwLock::new(None),
                state: Mutex::new(State {
                    tokens: None,
                    secrets,
                    pins: Arc::new(std::sync::Mutex::new(pins)),
                    blocked_devices,
                    account_cancel: CancellationToken::new(),
                    link: None,
                }),
                operations: Mutex::new(()),
                events,
                shutdown: CancellationToken::new(),
                initialized: AtomicBool::new(false),
                publications: Mutex::new(BTreeMap::new()),
                connections: Mutex::new(Vec::new()),
                notifications: Mutex::new(None),
                restore_pending: AtomicBool::new(true),
                identity_settled: AtomicBool::new(false),
                login_interrupt: std::sync::Mutex::new(CancellationToken::new()),
            }),
        })
    }

    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }
    pub fn identity(&self) -> Option<Identity> {
        if !self.inner.identity_settled.load(Ordering::Acquire) {
            return None;
        }
        self.inner
            .identity
            .read()
            .ok()
            .and_then(|value| value.clone())
    }
    pub fn events(&self) -> broadcast::Receiver<NetworkEvent> {
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
                        if let Some(identity)=this.identity() {
                            let generation=this.generation();
                            if let Err(error)=this.refresh().await {
                                this.emit_for(generation,Some(identity.user_id),json!({"type":"auth.notice","message":error.to_string()}));
                            }
                        }else if this.inner.restore_pending.load(Ordering::Acquire) {
                            let generation=this.generation();
                            if let Err(error)=this.restore_saved(generation).await {
                                this.emit_for(generation,None,json!({"type":"auth.notice","message":error.to_string()}));
                            }
                        }
                    }
                }
            }
        });
        let _operation = self.inner.operations.lock().await;
        relay::check_generation(self, generation)?;
        self.restore_saved(generation).await
    }

    #[expect(
        clippy::too_many_lines,
        reason = "closed command dispatch keeps retained surface mapping in one place"
    )]
    pub async fn execute(&self, operation: &str, args: Value) -> Result<NetworkReply> {
        if matches!(operation, "auth.logout" | "auth.login.start") {
            if let Some(credentials) = self
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
        }
        let before = self.generation();
        let _operation = self.inner.operations.lock().await;
        relay::check_generation(self, before)?;
        let mut events = Vec::new();
        match operation {
            "auth.login.start" => {
                self.logout().await?;
                let login = self.oidc()?.start().await?;
                let cancel = CancellationToken::new();
                *self
                    .inner
                    .login_interrupt
                    .lock()
                    .map_err(|_| Error::Closed)? = cancel.clone();
                let generation = self.generation();
                events.push(json!({"type":"auth.device_code","userCode":login.user_code,"verificationUri":login.verification_uri_complete.as_ref().unwrap_or(&login.verification_uri)}));
                let this = self.clone();
                tokio::spawn(async move {
                    let result = async {
                        let tokens = this.oidc()?.poll(login, &cancel).await?;
                        let _operation = this.inner.operations.lock().await;
                        if cancel.is_cancelled() || this.generation() != generation {
                            return Err(Error::Stale);
                        }
                        this.emit_for(generation, None, json!({"type":"auth.finalizing"}));
                        tokio::select! {
                            () = cancel.cancelled() => Err(Error::Stale),
                            () = this.inner.shutdown.cancelled() => Err(Error::Closed),
                            result = this.finish_login(tokens, generation) => result,
                        }
                    }
                    .await;
                    if let Err(error) = result
                        && !cancel.is_cancelled()
                    {
                        this.emit_for(generation,None,json!({"type":"auth.error","operation":"login.start","message":error.to_string()}));
                    }
                });
            }
            "auth.logout" => {
                self.logout().await?;
                events.push(json!({"type":"auth.required","reason":"signedOut"}));
            }
            "auth.refresh" => {
                self.refresh().await?;
                if let Some(identity) = self.identity() {
                    events.push(json!({"type":"auth.ready","userId":identity.user_id}));
                }
            }
            "devices.refresh" => {
                events.extend(self.device_events().await?);
            }
            "devices.link.startSelf" => {
                events.push(self.start_link().await?);
            }
            "devices.link.cancelSelf" => {
                self.cancel_link().await?;
                events.push(json!({"type":"devices.link.selfResolved","outcome":"cancelled"}));
            }
            "devices.link.approve" => {
                let code = wire::text(&args, "userCode")?;
                self.approve_link(code).await?;
                events.push(
                    json!({"type":"devices.link.resolved","userCode":code,"outcome":"approved"}),
                );
                events.extend(self.device_events().await?);
            }
            "devices.revoke" => {
                self.revoke_device(wire::text(&args, "deviceId")?).await?;
                events.extend(self.device_events().await?);
            }
            "friends.refresh" => events.push(self.friend_event().await?),
            "friends.request.send"
            | "friends.request.accept"
            | "friends.request.reject"
            | "friends.request.cancel"
            | "friends.remove" => {
                let credentials = self.credentials()?;
                let username = wire::text(&args, "username")?;
                let segment = path_segment(username)?;
                let (method, path, body) = match operation {
                    "friends.request.send" => (
                        Method::POST,
                        "api/friends/requests".to_owned(),
                        Some(json!({"username":username})),
                    ),
                    "friends.remove" => (Method::DELETE, format!("api/friends/{segment}"), None),
                    other => (
                        Method::POST,
                        format!(
                            "api/friends/requests/{segment}/{}",
                            other.rsplit('.').next().unwrap_or_default()
                        ),
                        None,
                    ),
                };
                if operation == "friends.remove" {
                    self.exclude_friend(&credentials, username).await?;
                }
                let _response: Value = self
                    .inner
                    .http
                    .device(method, &path, &credentials, body)
                    .await?;
                events.push(self.friend_event().await?);
                if operation == "friends.remove" {
                    self.rekey_all().await?;
                }
            }
            "session.list" => events.push(self.session_event().await?),
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
                        return Err(invalid(
                            "Session sharing changed. Reopen sharing before saving.",
                        ));
                    }
                }
                self.set_shares(id, expected, users).await?;
                events.push(session_result(operation, &args));
                events.push(self.session_event().await?);
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
                events.push(session_result(operation, &args));
                events.push(self.session_event().await?);
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
                    let _response:Value=self.inner.http.device(Method::PUT,&format!("api/sessions/{id}/mission"),&credentials,Some(json!({"incarnationId":current.incarnation_id,"roomId":args.get("roomId").cloned().unwrap_or(Value::Null)}))).await?;
                }
                events.push(session_result(operation, &args));
                events.push(self.session_event().await?);
            }
            "room.list" => events.push(self.room_list().await?),
            "room.open" => {
                let credentials = self.credentials()?;
                let id = wire::id(&args, "roomId")?;
                let mut value: Value = self
                    .inner
                    .http
                    .device(Method::GET, &format!("api/rooms/{id}"), &credentials, None)
                    .await?;
                value["type"] = json!("room.snapshot");
                value["requestId"] = args["requestId"].clone();
                events.push(value);
            }
            "room.create"
            | "room.rename"
            | "room.delete"
            | "room.invite"
            | "room.invitation.accept"
            | "room.invitation.reject"
            | "room.removeMember"
            | "room.leave" => {
                self.room_mutation(operation, &args).await?;
                events.push(json!({"type":"room.result","requestId":args["requestId"],"roomId":args.get("roomId"),"operation":operation}));
                events.push(self.room_list().await?);
            }
            _ => return Err(invalid("Unsupported network command.")),
        }
        if !matches!(operation, "auth.logout" | "auth.login.start") && before != self.generation() {
            return Err(Error::Stale);
        }
        Ok(NetworkReply {
            generation: self.generation(),
            user_id: self.identity().map(|value| value.user_id),
            events,
        })
    }

    pub(crate) fn emit_for(&self, generation: u64, user_id: Option<String>, event: Value) {
        let _outcome = self.inner.events.send(NetworkEvent {
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
        {
            let publications = self.inner.publications.lock().await;
            for dto in &sessions {
                if let Some(publication) = publications.get(&dto.id) {
                    if publication.changing.load(Ordering::Acquire)
                        || publication.pending_shares.lock().await.is_some()
                    {
                        continue;
                    }
                    let mut info = publication.info.write().await;
                    if info.incarnation_id != dto.incarnation_id
                        || dto.host_device_id != credentials.keys.device_id
                    {
                        continue;
                    }
                    info.name.clone_from(&dto.name);
                    info.room_id = dto.room_id;
                    let reported = dto.shared_with.iter().cloned().collect::<BTreeSet<_>>();
                    let removed = !info.shared_with.is_subset(&reported);
                    info.shared_with.retain(|user| reported.contains(user));
                    drop(info);
                    let previous = publication.dto.read().await.clone();
                    *publication.dto.write().await = dto.clone();
                    if removed || previous.authorization_revision != dto.authorization_revision {
                        publication.invalidate().await;
                    }
                    self.emit_for(credentials.generation, Some(credentials.user_id.clone()), json!({
                        "type":"network.sharing", "sessionId":dto.id, "incarnationId":dto.incarnation_id,
                        "sharedWith":publication.info.read().await.shared_with
                    }));
                }
            }
        }
        Ok(
            json!({"type":"network.sessions","sessions":sessions.into_iter().map(super::RemoteSession::from).collect::<Vec<_>>()}),
        )
    }

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
                if info.shared_with.remove(user) {
                    *publication.pending_shares.lock().await = Some(info.shared_with.clone());
                    drop(info);
                    publication.invalidate().await;
                }
            }
        }
        Ok(())
    }

    async fn friend_event(&self) -> Result<Value> {
        let credentials = self.credentials()?;
        let friends: Value = self
            .inner
            .http
            .device(Method::GET, "api/friends", &credentials, None)
            .await?;
        let requests: Value = self
            .inner
            .http
            .device(Method::GET, "api/friends/requests", &credentials, None)
            .await?;
        Ok(
            json!({"type":"friends.snapshot","friends":friends,"incoming":requests["incoming"],"outgoing":requests["outgoing"]}),
        )
    }

    async fn room_list(&self) -> Result<Value> {
        let credentials = self.credentials()?;
        let mut rooms: Value = self
            .inner
            .http
            .device(Method::GET, "api/rooms", &credentials, None)
            .await?;
        rooms["type"] = json!("rooms.snapshot");
        Ok(rooms)
    }

    async fn room_mutation(&self, operation: &str, args: &Value) -> Result<()> {
        let credentials = self.credentials()?;
        let request_id = wire::id(args, "requestId")?;
        let (method, path, body) = match operation {
            "room.create" => (
                Method::POST,
                "api/rooms".to_owned(),
                Some(
                    json!({"id":request_id,"name":wire::text(args,"name")?,"slug":wire::text(args,"slug")?}),
                ),
            ),
            "room.invitation.accept" | "room.invitation.reject" => (
                Method::POST,
                format!(
                    "api/rooms/invitations/{}/{}",
                    wire::id(args, "invitationId")?,
                    operation.rsplit('.').next().unwrap_or_default()
                ),
                None,
            ),
            other => {
                let id = wire::id(args, "roomId")?;
                match other {
                    "room.rename" => (
                        Method::PATCH,
                        format!("api/rooms/{id}"),
                        Some(json!({"name":wire::text(args,"name")?})),
                    ),
                    "room.delete" => (Method::DELETE, format!("api/rooms/{id}"), None),
                    "room.invite" => (
                        Method::POST,
                        format!("api/rooms/{id}/invitations"),
                        Some(json!({"id":request_id,"userId":wire::text(args,"userId")?})),
                    ),
                    "room.removeMember" => (
                        Method::DELETE,
                        format!("api/rooms/{id}/members/{}", wire::id(args, "userId")?),
                        None,
                    ),
                    "room.leave" => (
                        Method::DELETE,
                        format!("api/rooms/{id}/members/{}", credentials.user_id),
                        None,
                    ),
                    _ => return Err(invalid("Unsupported Mission operation.")),
                }
            }
        };
        let _response: Value = self
            .inner
            .http
            .device(method, &path, &credentials, body)
            .await?;
        Ok(())
    }

    pub async fn publish(
        &self,
        info: LocalPublication,
        requests: mpsc::Sender<HostRequest>,
        output: PublicationOutput,
    ) -> Result<()> {
        let generation = self.generation();
        let _operation = self.inner.operations.lock().await;
        relay::check_generation(self, generation)?;
        let credentials = self.credentials()?;
        if !credentials.enrolled {
            return Err(Error::EnrollmentRequired);
        }
        if let Some(publication) = self.inner.publications.lock().await.get(&info.session_id) {
            if publication.info.read().await.incarnation_id != info.incarnation_id {
                return Err(Error::Stale);
            }
            return Ok(());
        }
        let mut dto:SessionDto=self.inner.http.device(Method::POST,"api/sessions",&credentials,Some(json!({"id":info.session_id,"incarnationId":info.incarnation_id,"name":info.name,"hostDeviceId":credentials.keys.device_id,"hostName":host_label(),"roomId":info.room_id}))).await?;
        if !info.shared_with.is_empty() {
            dto=self.inner.http.device(Method::PUT,&format!("api/sessions/{}/members",info.session_id),&credentials,Some(json!({"incarnationId":info.incarnation_id,"expectedRevision":dto.authorization_revision,"userIds":info.shared_with}))).await?;
        }
        self.check_credentials(&credentials)?;
        let publication = Arc::new(relay::Publication::new(info, requests, output, dto));
        self.inner.publications.lock().await.insert(
            publication.info.read().await.session_id,
            Arc::clone(&publication),
        );
        relay::spawn_host(self.clone(), publication, credentials);
        Ok(())
    }

    pub async fn unpublish(&self, id: Uuid) -> Result<()> {
        let generation = self.generation();
        let _operation = self.inner.operations.lock().await;
        relay::check_generation(self, generation)?;
        let publication = self.inner.publications.lock().await.get(&id).cloned();
        if let Some(publication) = publication {
            let _drained =
                tokio::time::timeout(Duration::from_secs(12), publication.drained.cancelled())
                    .await;
            publication.cancel.cancel();
            publication.invalidate().await;
            let info = publication.info.read().await.clone();
            let credentials = self.credentials()?;
            let _response: Value = self
                .inner
                .http
                .device(
                    Method::DELETE,
                    &format!("api/sessions/{id}?incarnationId={}", info.incarnation_id),
                    &credentials,
                    None,
                )
                .await?;
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

    async fn set_shares(&self, id: Uuid, incarnation: Uuid, users: BTreeSet<String>) -> Result<()> {
        let publication = self
            .inner
            .publications
            .lock()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| invalid("Change sharing on the computer hosting this session."))?;
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
        publication.invalidate().await;
        publication.info.write().await.shared_with = users.clone();
        *publication.pending_shares.lock().await = Some(users.clone());
        let result=async {
            let current:SessionDto=self.inner.http.device(Method::GET,&format!("api/sessions/{id}"),&credentials,None).await?;
            if current.incarnation_id != incarnation { return Err(Error::Stale); }
            if current.shared_with.iter().cloned().collect::<BTreeSet<_>>() == users { return Ok(current); }
            if !users.is_subset(&previous) && (current.authorization_revision != publication.dto.read().await.authorization_revision
                || current.shared_with.iter().cloned().collect::<BTreeSet<_>>() != previous) {
                return Err(Error::Backend { status: 409, message: "Session sharing changed. Refresh before adding access.".into() });
            }
            self.inner.http.device::<SessionDto>(Method::PUT,&format!("api/sessions/{id}/members"),&credentials,Some(json!({"incarnationId":incarnation,"expectedRevision":current.authorization_revision,"userIds":users}))).await
        }.await;
        if let Ok(dto) = &result {
            *publication.dto.write().await = dto.clone();
            *publication.pending_shares.lock().await = None;
        } else if matches!(
            &result,
            Err(Error::Backend {
                status: 400 | 401 | 403 | 404 | 409 | 422,
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

    async fn rekey_all(&self) -> Result<()> {
        for publication in self.inner.publications.lock().await.values() {
            publication.invalidate().await;
        }
        Ok(())
    }

    pub async fn connect_remote(&self, id: Uuid) -> Result<RemoteConnection> {
        let generation = self.generation();
        let _operation = self.inner.operations.lock().await;
        relay::check_generation(self, generation)?;
        relay::connect_remote(self.clone(), id).await
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
fn host_label() -> String {
    [
        std::env::var("HOSTNAME").ok(),
        std::env::var("COMPUTERNAME").ok(),
    ]
    .into_iter()
    .flatten()
    .find_map(|value| normalize_host_label(&value))
    .unwrap_or_else(|| "This computer".to_owned())
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
mod device_identity;
mod enrollment;
mod notifications;
#[cfg(test)]
mod tests;
