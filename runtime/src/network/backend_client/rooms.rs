use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm as _};
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::identity::keys::ROOM_PUBLIC_BYTES;
use crate::rooms::{
    self, Action, Payload, Snapshot,
    crypto::{self as room_crypto, Secret, SignedState, State as KeyState, VerifiedState},
};
use serde::{Deserialize, Serialize};

#[derive(Default)]
pub(crate) struct RoomCache {
    generation: u64,
    rooms: BTreeMap<Uuid, CachedRoom>,
    registered: bool,
}
#[derive(Clone, Default)]
struct CachedRoom {
    states: BTreeMap<u64, VerifiedState>,
    keys: BTreeMap<u64, Secret>,
    sequence: u64,
    issues_checked: Option<tokio::time::Instant>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: Uuid,
    kind: String,
    version: u64,
    sequence: u64,
    key_version: u64,
    user_id: String,
    device_id: String,
    created_at: String,
    body: String,
    signature: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyHistory {
    owner_user_id: String,
    #[serde(default)]
    members: Vec<String>,
    version: u64,
    states: Vec<SignedState>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentPage {
    sequence: u64,
    has_more: bool,
    items: Vec<Item>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecipientKey {
    user_id: String,
    device_id: String,
    public_key: String,
    signature: String,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Head {
    version: u64,
    hash: String,
    #[serde(default)]
    owner_root: Option<identity::pins::Root>,
}
type Heads = BTreeMap<String, BTreeMap<Uuid, Head>>;

struct Standing {
    members: BTreeMap<String, IdentityBundle>,
    identities: BTreeMap<String, VerifiedIdentity>,
    replaced: BTreeSet<String>,
}

#[cfg(test)]
#[path = "rooms_tests.rs"]
mod tests;

impl BackendClient {
    pub(super) async fn register_room_key(&self, credentials: &Credentials) -> Result<()> {
        {
            let mut cache = self.inner.rooms.lock().await;
            if cache.generation != credentials.generation {
                *cache = RoomCache {
                    generation: credentials.generation,
                    ..RoomCache::default()
                };
            }
            if cache.registered {
                return Ok(());
            }
        }
        let proof = crypto::signed_fields(
            b"kodosi-room-recipient-v1",
            &[
                credentials.user_id.as_bytes(),
                credentials.keys.device_id.as_bytes(),
                credentials.keys.room_public(),
            ],
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &proof)?;
        let _response: Value = self.inner.http.device(Method::PUT, "api/me/room-key", credentials,
            Some(json!({"publicKey":BASE64.encode(credentials.keys.room_public()),"signature":BASE64.encode(signature)}))).await?;
        self.inner.rooms.lock().await.registered = true;
        Ok(())
    }

    async fn room_identity_bundle(
        &self,
        credentials: &Credentials,
        user: &str,
        root: Option<&identity::pins::Root>,
    ) -> Result<IdentityBundle> {
        let root = match root {
            Some(root) => *root,
            None => self.fetch_identity_with(credentials, user).await?.root,
        };
        let bundle: IdentityBundle = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/users/{user}/identity"),
                credentials,
                None,
            )
            .await?;
        if bundle.user_id != user {
            return Err(invalid("The room identity belongs to another user."));
        }
        self.verify_bundle(&bundle, Some(root)).await?;
        Ok(bundle)
    }

    async fn room_standing(
        &self,
        credentials: &Credentials,
        previous: &VerifiedState,
        at_ms: u64,
    ) -> Result<Standing> {
        let mut standing = Standing {
            members: previous.state.members.clone(),
            identities: previous.identities.clone(),
            replaced: BTreeSet::new(),
        };
        for (user, known) in &previous.state.members {
            let fetched = self
                .inner
                .http
                .device::<IdentityBundle>(
                    Method::GET,
                    &format!("api/users/{user}/identity"),
                    credentials,
                    None,
                )
                .await;
            let current = match fetched {
                Ok(current) if current.user_id == *user => current,
                Ok(_) | Err(Error::Backend { status: 400, .. }) => continue,
                Err(Error::Backend {
                    status: 404 | 409, ..
                }) => {
                    standing.replaced.insert(user.clone());
                    continue;
                }
                Err(error) => return Err(error),
            };
            if current == *known {
                continue;
            }
            let root = known.root()?;
            if current.root().is_ok_and(|current| current != root) {
                standing.replaced.insert(user.clone());
                continue;
            }
            let earlier = Some((known, previous.state.created_at_ms));
            match Pins::historical(&current, &root, at_ms, earlier) {
                Ok(identity) => {
                    standing.members.insert(user.clone(), current);
                    standing.identities.insert(user.clone(), identity);
                }
                Err(error) if *user == credentials.user_id => return Err(error),
                Err(error) => {
                    tracing::warn!(%error, "the room keeps the last identity of a member whose devices are not current");
                }
            }
        }
        Ok(standing)
    }

    async fn room_recipients(
        &self,
        credentials: &Credentials,
        standing: &Standing,
    ) -> Result<Vec<RecipientKey>> {
        let mut recipients = Vec::new();
        for (user, identity) in &standing.identities {
            if standing.replaced.contains(user) {
                continue;
            }
            let keys: Vec<RecipientKey> = self
                .inner
                .http
                .device(
                    Method::GET,
                    &format!("api/users/{user}/room-keys"),
                    credentials,
                    None,
                )
                .await?;
            for key in keys {
                if key.user_id != *user {
                    return Err(invalid("A room key belongs to another user."));
                }
                let Some(device) = identity.devices.get(&key.device_id) else {
                    continue;
                };
                let public = room_crypto::decode(&key.public_key, ROOM_PUBLIC_BYTES)?;
                if public.len() != ROOM_PUBLIC_BYTES {
                    continue;
                }
                let preimage = crypto::signed_fields(
                    b"kodosi-room-recipient-v1",
                    &[user.as_bytes(), key.device_id.as_bytes(), &public],
                )?;
                let signature = room_crypto::decode(&key.signature, 3309)?;
                ML_DSA_65
                    .verify_sig(&device.sig_public_key, &preimage, &signature)
                    .map_err(|_| invalid("The room recipient key is not authentic."))?;
                recipients.push(key);
            }
        }
        Ok(recipients)
    }

    fn wrap_room_state(
        credentials: &Credentials,
        state: &mut KeyState,
        recipients: &[RecipientKey],
        key: &Secret,
    ) -> Result<Value> {
        state.recipients = recipients
            .iter()
            .map(|recipient| {
                room_crypto::wrap(
                    state,
                    &recipient.user_id,
                    &recipient.device_id,
                    &room_crypto::decode(&recipient.public_key, ROOM_PUBLIC_BYTES)?,
                    key,
                )
            })
            .collect::<Result<_>>()?;
        let (body, signature) =
            room_crypto::sign(room_crypto::STATE_DOMAIN, &credentials.keys, state)?;
        Ok(json!({"deviceId":credentials.keys.device_id,"body":body,"signature":signature}))
    }

    async fn initialize_room(
        &self,
        credentials: &Credentials,
        room: Uuid,
        owner: &str,
        participants: &[String],
    ) -> Result<()> {
        if owner != credentials.user_id {
            return Err(invalid(
                "The room owner needs to open this room once to prepare its conversation.",
            ));
        }
        let detail: Value = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/missions/{room}"),
                credentials,
                None,
            )
            .await?;
        let mut users = participants.to_vec();
        if users.is_empty() {
            for member in detail["members"]
                .as_array()
                .ok_or_else(|| invalid("Room members are missing."))?
            {
                users.push(wire::text(member, "userId")?.to_owned());
            }
        }
        let created = identity::now_ms();
        let mut standing = Standing {
            members: BTreeMap::new(),
            identities: BTreeMap::new(),
            replaced: BTreeSet::new(),
        };
        for user in users {
            let bundle = self.room_identity_bundle(credentials, &user, None).await?;
            let identity = Pins::historical(&bundle, &bundle.root()?, created, None)?;
            standing.identities.insert(user.clone(), identity);
            standing.members.insert(user, bundle);
        }
        let recipients = self.room_recipients(credentials, &standing).await?;
        let mut state = KeyState {
            room_id: room,
            owner_user_id: owner.to_owned(),
            author_id: credentials.user_id.clone(),
            device_id: credentials.keys.device_id.clone(),
            version: 1,
            epoch: 1,
            created_at_ms: created,
            previous_hash: String::new(),
            members: standing.members,
            recipients: Vec::new(),
            previous_key: None,
        };
        let body = Self::wrap_room_state(
            credentials,
            &mut state,
            &recipients,
            &room_crypto::secret()?,
        )?;
        let result = self
            .inner
            .http
            .device::<Value>(
                Method::PUT,
                &format!("api/missions/{room}/keys"),
                credentials,
                Some(body),
            )
            .await;
        if !matches!(result, Err(Error::Backend { status: 409, .. })) {
            result?;
        }
        Ok(())
    }

    async fn room_anchor(
        &self,
        credentials: &Credentials,
        room: Uuid,
        cached: &CachedRoom,
        owner: &str,
    ) -> Result<identity::pins::Root> {
        if let Some((_, first)) = cached.states.first_key_value() {
            return first
                .state
                .members
                .get(owner)
                .ok_or_else(|| invalid("Room owner identity is missing."))?
                .root();
        }
        if let Some(saved) = self
            .room_heads()?
            .1
            .get(&credentials.user_id)
            .and_then(|rooms| rooms.get(&room))
            .and_then(|head| head.owner_root)
        {
            return Ok(saved);
        }
        let pins = Arc::clone(&self.inner.state.lock().await.pins);
        let pinned = pins.lock().map_err(|_| Error::Closed)?.root(owner);
        match pinned {
            Some(pinned) => Ok(pinned),
            None => Ok(self.fetch_identity_with(credentials, owner).await?.root),
        }
    }

    async fn room_keys(&self, credentials: &Credentials, room: Uuid) -> Result<CachedRoom> {
        self.register_room_key(credentials).await?;
        let mut initialization_attempts = 0;
        let mut cached = self
            .inner
            .rooms
            .lock()
            .await
            .rooms
            .get(&room)
            .cloned()
            .unwrap_or_default();
        loop {
            let after = cached
                .states
                .last_key_value()
                .map_or(0, |(version, _)| *version);
            let history: KeyHistory = self
                .inner
                .http
                .device(
                    Method::GET,
                    &format!("api/missions/{room}/keys?afterVersion={after}"),
                    credentials,
                    None,
                )
                .await?;
            if history.version < after {
                return Err(invalid("The server returned an older room."));
            }
            if history.version == 0 {
                initialization_attempts += 1;
                if initialization_attempts > 3 {
                    return Err(invalid("The room changed during setup. Open it again."));
                }
                self.initialize_room(credentials, room, &history.owner_user_id, &history.members)
                    .await?;
                continue;
            }
            let anchor = self
                .room_anchor(credentials, room, &cached, &history.owner_user_id)
                .await?;
            if history.states.is_empty() && history.version > after {
                return Err(invalid("Room key history is incomplete."));
            }
            for signed in history.states {
                let verified = room_crypto::verify_state(
                    &signed,
                    room,
                    &history.owner_user_id,
                    &anchor,
                    cached.states.last_key_value().map(|(_, state)| state),
                )
                .map_err(|error| match error {
                    Error::Trust(_)
                        if cached.states.is_empty()
                            && history.owner_user_id == credentials.user_id =>
                    {
                        invalid(
                            "You made this room before you started fresh. Delete it and make a new room.",
                        )
                    }
                    error => error,
                })?;
                if let Ok(key) =
                    room_crypto::unwrap(&verified.state, &credentials.user_id, &credentials.keys)
                {
                    cached.keys.insert(verified.state.epoch, key);
                }
                cached.states.insert(signed.version, verified);
            }
            if cached
                .states
                .last_key_value()
                .is_some_and(|(version, _)| *version == history.version)
            {
                break;
            }
            if cached.states.len() > 16_384 {
                return Err(invalid("The room key history exceeds its limit."));
            }
        }
        self.remember_room(credentials, room, cached).await
    }

    fn room_heads(&self) -> Result<(std::path::PathBuf, Heads)> {
        let path = self.inner.config.data_root.join("network/room-heads.json");
        let heads = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes)?,
            Ok(_) => return Err(invalid("Saved room history exceeds its limit.")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok((path, heads))
    }

    async fn remember_room(
        &self,
        credentials: &Credentials,
        room: Uuid,
        mut cached: CachedRoom,
    ) -> Result<CachedRoom> {
        let mut cache = self.inner.rooms.lock().await;
        let current = cached
            .states
            .last_key_value()
            .ok_or_else(|| invalid("Room keys are missing."))?
            .1;
        let (path, mut heads) = self.room_heads()?;
        let known = heads.entry(credentials.user_id.clone()).or_default();
        if known.get(&room).is_some_and(|head| {
            head.version > current.state.version
                || cached
                    .states
                    .get(&head.version)
                    .is_some_and(|state| state.hash != head.hash)
        }) {
            return Err(invalid("The room history changed unexpectedly."));
        }
        known.insert(
            room,
            Head {
                version: current.state.version,
                hash: current.hash.clone(),
                owner_root: cached
                    .states
                    .first_key_value()
                    .and_then(|(_, first)| first.state.members.get(&first.state.owner_user_id))
                    .and_then(|owner| owner.root().ok()),
            },
        );
        identity::storage::private_write(&path, &serde_json::to_vec(&heads)?)?;
        for state in cached.states.values().rev() {
            if state.state.epoch > 1
                && let Some(current) = cached.keys.get(&state.state.epoch)
            {
                let previous = room_crypto::open_previous(&state.state, current)?;
                cached.keys.insert(state.state.epoch - 1, previous);
            }
        }
        cache.rooms.insert(room, cached.clone());
        drop(cache);
        Ok(cached)
    }

    pub(super) async fn prepare_room_membership(
        &self,
        credentials: &Credentials,
        room: Uuid,
        add: Option<&str>,
        remove: Option<&str>,
    ) -> Result<Value> {
        let cached = self.room_keys(credentials, room).await?;
        self.next_room_state(credentials, room, &cached, add, remove)
            .await?
            .ok_or_else(|| invalid("The room membership did not change."))
    }

    async fn next_room_state(
        &self,
        credentials: &Credentials,
        room: Uuid,
        cached: &CachedRoom,
        add: Option<&str>,
        remove: Option<&str>,
    ) -> Result<Option<Value>> {
        let previous = cached
            .states
            .last_key_value()
            .ok_or_else(|| invalid("Room keys are missing."))?
            .1;
        let prior_key = cached
            .keys
            .get(&previous.state.epoch)
            .ok_or_else(|| invalid("Room keys are still arriving on this device."))?;
        let created = identity::now_ms().max(previous.state.created_at_ms);
        let mut standing = self.room_standing(credentials, previous, created).await?;
        let mut returned = false;
        if previous.state.owner_user_id == credentials.user_id {
            for user in standing.replaced.clone() {
                if remove == Some(user.as_str()) {
                    continue;
                }
                match self.room_identity_bundle(credentials, &user, None).await {
                    Ok(bundle) => {
                        let identity = Pins::historical(&bundle, &bundle.root()?, created, None)?;
                        standing.identities.insert(user.clone(), identity);
                        standing.members.insert(user.clone(), bundle);
                        standing.replaced.remove(&user);
                        returned = true;
                    }
                    Err(error) if error.unanswered() => return Err(error),
                    Err(error) => {
                        tracing::info!(%error, "a room member with a new identity waits for the trust of the owner");
                    }
                }
            }
        }
        if let Some(user) = add {
            let bundle = self.room_identity_bundle(credentials, user, None).await?;
            let identity = Pins::historical(&bundle, &bundle.root()?, created, None)?;
            standing.identities.insert(user.to_owned(), identity);
            standing.members.insert(user.to_owned(), bundle);
        }
        if let Some(user) = remove {
            standing.identities.remove(user);
            standing.members.remove(user);
        }
        let recipients = self.room_recipients(credentials, &standing).await?;
        let old = previous
            .state
            .recipients
            .iter()
            .map(|r| (&r.user_id, &r.device_id))
            .collect::<BTreeSet<_>>();
        let new = recipients
            .iter()
            .map(|r| (&r.user_id, &r.device_id))
            .collect::<BTreeSet<_>>();
        if add.is_none() && remove.is_none() && !returned && old == new {
            return Ok(None);
        }
        let mut state = previous.state.clone();
        state.version += 1;
        state.previous_hash.clone_from(&previous.hash);
        state.author_id.clone_from(&credentials.user_id);
        state.device_id.clone_from(&credentials.keys.device_id);
        state.created_at_ms = created;
        state.members = standing.members;
        let key = if remove.is_some() || returned || old.difference(&new).next().is_some() {
            state.epoch += 1;
            let key = room_crypto::secret()?;
            state.previous_key = Some(room_crypto::seal_previous(
                room,
                state.epoch,
                &key,
                prior_key,
            )?);
            key
        } else {
            prior_key.clone()
        };
        Self::wrap_room_state(credentials, &mut state, &recipients, &key).map(Some)
    }

    pub(super) async fn refresh_room_keys(
        &self,
        credentials: &Credentials,
        room: Uuid,
    ) -> Result<()> {
        let mut attempts = 0;
        loop {
            let cached = self.room_keys(credentials, room).await?;
            let current = &cached
                .states
                .last_key_value()
                .ok_or_else(|| invalid("Room keys are missing."))?
                .1
                .state;
            if !current.members.contains_key(&credentials.user_id)
                || !cached.keys.contains_key(&current.epoch)
            {
                return Ok(());
            }
            let Some(body) = self
                .next_room_state(credentials, room, &cached, None, None)
                .await?
            else {
                return Ok(());
            };
            let stored = self
                .inner
                .http
                .device::<Value>(
                    Method::PUT,
                    &format!("api/missions/{room}/keys"),
                    credentials,
                    Some(body),
                )
                .await;
            attempts += 1;
            match stored {
                Err(Error::Backend { status: 409, .. }) if attempts < 3 => {}
                stored => return stored.map(|_| ()),
            }
        }
    }

    pub(crate) async fn room_identity(
        &self,
        credentials: &Credentials,
        room: Uuid,
        user: &str,
    ) -> Result<VerifiedIdentity> {
        let cached = self.room_keys(credentials, room).await?;
        let state = &cached
            .states
            .last_key_value()
            .ok_or_else(|| invalid("Room keys are missing."))?
            .1
            .state;
        let anchor = state
            .members
            .get(user)
            .ok_or_else(|| invalid("This person is no longer in the room."))?
            .root()?;
        if user != credentials.user_id
            && self
                .friend_record_root(credentials, user)
                .await?
                .is_some_and(|recorded| recorded != anchor)
        {
            return Err(Error::Trust(
                "This friend has a new identity. Trust it in Friends before you share or connect."
                    .into(),
            ));
        }
        let bundle = self
            .room_identity_bundle(credentials, user, Some(&anchor))
            .await?;
        self.verify_bundle(&bundle, Some(anchor)).await
    }

    fn room_payload(cached: &CachedRoom, room: Uuid, item: &Item) -> Result<Payload> {
        let bytes = room_crypto::decode(&item.body, 64 * 1024)?;
        let content: room_crypto::Content = serde_json::from_slice(&bytes)?;
        if content.room_id != room
            || content.id != item.id
            || content.kind != item.kind
            || content.version != item.version
            || content.key_version != item.key_version
            || content.author_id != item.user_id
            || content.device_id != item.device_id
        {
            return Err(invalid("Room content has inconsistent identity."));
        }
        let state = cached
            .states
            .get(&content.key_version)
            .ok_or_else(|| invalid("Room content keys are missing."))?;
        if state.state.epoch != content.epoch {
            return Err(invalid("Room content refers to another key."));
        }
        let author = state
            .identities
            .get(&content.author_id)
            .and_then(|identity| identity.devices.get(&content.device_id))
            .ok_or_else(|| invalid("The room content author is unavailable."))?;
        room_crypto::verify_signature(
            room_crypto::CONTENT_DOMAIN,
            &bytes,
            &item.signature,
            &author.sig_public_key,
        )?;
        let key = cached
            .keys
            .get(&content.epoch)
            .ok_or_else(|| invalid("Room history keys are still arriving."))?;
        Ok(serde_json::from_slice(&content.decrypt(key)?)?)
    }

    async fn room_snapshot(
        &self,
        credentials: &Credentials,
        room: Uuid,
        before: Option<u64>,
    ) -> Result<Snapshot> {
        let mut cached = self.room_keys(credentials, room).await?;
        let mut messages = Vec::new();
        let mut tasks = Vec::new();
        let mut repositories = Vec::new();
        let mut has_older = false;
        let known_sequence = cached.sequence;
        for kind in ["message", "task", "repository"] {
            let mut after = 0;
            let mut before_cursor = before.unwrap_or(i64::MAX as u64);
            loop {
                let position = if kind == "message" {
                    format!("&before={before_cursor}")
                } else {
                    format!("&after={after}")
                };
                let page: ContentPage = self
                    .inner
                    .http
                    .device(
                        Method::GET,
                        &format!("api/missions/{room}/content?kind={kind}&limit=100{position}"),
                        credentials,
                        None,
                    )
                    .await?;
                cached.sequence = cached.sequence.max(page.sequence);
                if kind == "message" {
                    has_older = page.has_more;
                }
                let has_more = page.has_more;
                let next = page.items.last().map_or(after, |item| item.sequence);
                let earliest = page.items.first().map_or(0, |item| item.sequence);
                for item in page.items {
                    match Self::room_payload(&cached, room, &item)? {
                        Payload::Message {
                            text,
                            author_name,
                            agent,
                            terminal_id,
                        } if item.kind == "message" => messages.push(rooms::Message {
                            id: item.id.to_string(),
                            sequence: item.sequence,
                            author_id: item.user_id.clone(),
                            author_name,
                            agent,
                            terminal_id,
                            text,
                            created_at: item.created_at.clone(),
                        }),
                        Payload::Task { mut task } if item.kind == "task" => {
                            task.id = item.id.to_string();
                            task.version = item.version;
                            tasks.push(task);
                        }
                        Payload::Repository { mut repository } if item.kind == "repository" => {
                            repository.id = item.id.to_string();
                            repositories.push(repository);
                        }
                        _ => return Err(invalid("Room content has a different type.")),
                    }
                }
                if kind == "message" {
                    if before.is_some()
                        || known_sequence == 0
                        || !has_more
                        || earliest <= known_sequence
                    {
                        break;
                    }
                    if earliest >= before_cursor {
                        return Err(invalid("Room history did not advance."));
                    }
                    before_cursor = earliest;
                    continue;
                }
                if !has_more {
                    break;
                }
                if next <= after {
                    return Err(invalid("Room history did not advance."));
                }
                after = next;
            }
        }
        messages.sort_by_key(|message| message.sequence);
        let sequence = cached.sequence;
        self.inner.rooms.lock().await.rooms.insert(room, cached);
        Ok(Snapshot {
            room_id: room.to_string(),
            messages,
            tasks,
            repositories,
            sequence,
            has_older,
            more_tasks: false,
        })
    }

    async fn room_write(
        &self,
        credentials: &Credentials,
        room: Uuid,
        id: Uuid,
        kind: &str,
        expected: u64,
        payload: &Payload,
    ) -> Result<()> {
        let cached = self.room_keys(credentials, room).await?;
        let state = &cached
            .states
            .last_key_value()
            .ok_or_else(|| invalid("Room keys are missing."))?
            .1
            .state;
        let key = cached
            .keys
            .get(&state.epoch)
            .ok_or_else(|| invalid("Room keys are still arriving."))?;
        let mut content = room_crypto::Content {
            room_id: room,
            id,
            kind: kind.into(),
            version: expected + 1,
            key_version: state.version,
            epoch: state.epoch,
            author_id: credentials.user_id.clone(),
            device_id: credentials.keys.device_id.clone(),
            nonce: String::new(),
            ciphertext: String::new(),
        };
        content.encrypt(key, &serde_json::to_vec(payload)?)?;
        let (body, signature) =
            room_crypto::sign(room_crypto::CONTENT_DOMAIN, &credentials.keys, &content)?;
        let _response: Item = self.inner.http.device(Method::PUT, &format!("api/missions/{room}/content/{id}"), credentials,
            Some(json!({"expectedVersion":expected,"deviceId":credentials.keys.device_id,"body":body,"signature":signature}))).await?;
        Ok(())
    }

    #[expect(clippy::too_many_lines, reason = "one typed room action dispatch")]
    pub(super) async fn room_command(&self, args: &Value) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let room = wire::id(args, "roomId")?;
        let request = wire::text(args, "requestId")?;
        let action: Action = serde_json::from_value(args["action"].clone())?;
        self.refresh_room_keys(&credentials, room).await?;
        let before = match action {
            Action::Read { before } => before,
            _ => None,
        };
        let mut snapshot = self.room_snapshot(&credentials, room, before).await?;
        if matches!(action, Action::Read { .. }) {
            self.sync_linked_issues(&credentials, room, &mut snapshot)
                .await?;
        }
        let name = self.identity().map_or_else(
            || credentials.user_id.clone(),
            |identity| identity.display_name,
        );
        let mut item_id = None;
        match action {
            Action::Read { .. } => {}
            Action::Post {
                text,
                terminal_id,
                agent,
            } => {
                let id =
                    Uuid::parse_str(request).map_err(|_| invalid("Invalid message identity."))?;
                self.room_write(
                    &credentials,
                    room,
                    id,
                    "message",
                    0,
                    &Payload::Message {
                        text,
                        terminal_id,
                        agent,
                        author_name: name,
                    },
                )
                .await?;
                item_id = Some(id.to_string());
            }
            Action::CreateTask {
                title,
                description,
                repository_ids,
                terminal_id,
            } => {
                let id = Uuid::parse_str(request).map_err(|_| invalid("Invalid task identity."))?;
                let task = rooms::Task {
                    id: id.to_string(),
                    version: 1,
                    title,
                    description,
                    closed: false,
                    assigned_to: None,
                    assigned_name: None,
                    terminal_id,
                    repository_ids,
                    note: None,
                    issue: None,
                };
                self.room_write(&credentials, room, id, "task", 0, &Payload::Task { task })
                    .await?;
                item_id = Some(id.to_string());
            }
            Action::UpdateTask {
                task_id,
                change,
                note,
                terminal_id,
            } => {
                let mut task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .cloned()
                    .ok_or_else(|| invalid("This task is unavailable."))?;
                match change {
                    rooms::TaskChange::Claim => {
                        task.assigned_to = Some(credentials.user_id.clone());
                        task.assigned_name = Some(name);
                        task.terminal_id = terminal_id;
                    }
                    rooms::TaskChange::Release => {
                        task.assigned_to = None;
                        task.assigned_name = None;
                        task.terminal_id = None;
                    }
                    rooms::TaskChange::Close => task.closed = true,
                    rooms::TaskChange::Reopen => task.closed = false,
                }
                if let Some(note) = note {
                    task.note = Some(note);
                }
                self.update_linked_issue(&snapshot.repositories, &mut task, change)
                    .await?;
                let id =
                    Uuid::parse_str(&task_id).map_err(|_| invalid("Invalid task identity."))?;
                self.room_write(
                    &credentials,
                    room,
                    id,
                    "task",
                    task.version,
                    &Payload::Task { task },
                )
                .await?;
                item_id = Some(task_id);
            }
            Action::AddRepository { url, provider } => {
                let mut repository = rooms::providers::repository(&url, provider.as_deref())?;
                if let Some(existing) = snapshot
                    .repositories
                    .iter()
                    .find(|existing| existing.url.eq_ignore_ascii_case(&repository.url))
                {
                    item_id = Some(existing.id.clone());
                } else {
                    let id = Uuid::parse_str(request)
                        .map_err(|_| invalid("Invalid repository identity."))?;
                    repository.id = id.to_string();
                    self.room_write(
                        &credentials,
                        room,
                        id,
                        "repository",
                        0,
                        &Payload::Repository { repository },
                    )
                    .await?;
                    item_id = Some(id.to_string());
                }
            }
            Action::Issues { repository_id } => {
                let repository = snapshot
                    .repositories
                    .iter()
                    .find(|repo| repo.id == repository_id)
                    .ok_or_else(|| invalid("This repository is unavailable."))?;
                let issues = rooms::providers::issues(repository, None).await?;
                return Ok(vec![
                    json!({"type":"room.issues","requestId":request,"roomId":room,"repositoryId":repository_id,"issues":issues}),
                    json!({"type":"room.result","requestId":request,"operation":"room.command","roomId":room,"itemId":null,"action":args["action"]["type"]}),
                ]);
            }
            Action::ImportIssue {
                repository_id,
                number,
            } => {
                let repository = snapshot
                    .repositories
                    .iter()
                    .find(|repo| repo.id == repository_id)
                    .ok_or_else(|| invalid("This repository is unavailable."))?;
                let issue = rooms::providers::issues(repository, Some(number))
                    .await?
                    .into_iter()
                    .next()
                    .ok_or_else(|| invalid("This issue is unavailable."))?;
                if let Some(existing) = snapshot.tasks.iter().find(|task| {
                    task.issue
                        .as_ref()
                        .is_some_and(|linked| linked.url == issue.url)
                }) {
                    item_id = Some(existing.id.clone());
                } else {
                    let id =
                        Uuid::parse_str(request).map_err(|_| invalid("Invalid task identity."))?;
                    let mut task = rooms::Task {
                        id: id.to_string(),
                        version: 1,
                        title: issue.title.clone(),
                        description: issue.body.clone(),
                        closed: issue.closed,
                        assigned_to: None,
                        assigned_name: None,
                        terminal_id: None,
                        repository_ids: vec![repository_id],
                        note: None,
                        issue: None,
                    };
                    Self::apply_linked_issue(&mut task, repository, issue);
                    self.room_write(&credentials, room, id, "task", 0, &Payload::Task { task })
                        .await?;
                    item_id = Some(id.to_string());
                }
            }
        }
        let current = self.room_snapshot(&credentials, room, before).await?;
        Ok(vec![
            json!({"type":"room.snapshot","room":current}),
            json!({"type":"room.result","requestId":request,"operation":"room.command","roomId":room,"itemId":item_id,"action":args["action"]["type"]}),
        ])
    }

    async fn sync_linked_issues(
        &self,
        credentials: &Credentials,
        room: Uuid,
        snapshot: &mut Snapshot,
    ) -> Result<()> {
        {
            let mut cache = self.inner.rooms.lock().await;
            let Some(cached) = cache.rooms.get_mut(&room) else {
                return Ok(());
            };
            if cached
                .issues_checked
                .is_some_and(|checked| checked.elapsed() < Duration::from_secs(30))
            {
                return Ok(());
            }
            cached.issues_checked = Some(tokio::time::Instant::now());
            drop(cache);
        }
        for task in &mut snapshot.tasks {
            let Some(linked) = &task.issue else {
                continue;
            };
            let Some(repository) = snapshot
                .repositories
                .iter()
                .find(|repo| repo.id == linked.repository_id)
            else {
                continue;
            };
            let latest = match rooms::providers::issues(repository, Some(linked.number)).await {
                Ok(mut issues) if !issues.is_empty() => issues.remove(0),
                Ok(_) => continue,
                Err(error) => {
                    tracing::debug!(%error, "linked issue refresh will retry");
                    continue;
                }
            };
            if task.issue.as_ref() == Some(&latest) {
                continue;
            }
            Self::apply_linked_issue(task, repository, latest);
            let id = Uuid::parse_str(&task.id).map_err(|_| invalid("Invalid task identity."))?;
            match self
                .room_write(
                    credentials,
                    room,
                    id,
                    "task",
                    task.version,
                    &Payload::Task { task: task.clone() },
                )
                .await
            {
                Ok(()) => task.version += 1,
                Err(Error::Backend { status: 409, .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    fn apply_linked_issue(
        task: &mut rooms::Task,
        repository: &rooms::Repository,
        issue: rooms::Issue,
    ) {
        task.title.clone_from(&issue.title);
        task.description.clone_from(&issue.body);
        task.closed = issue.closed;
        task.assigned_name = (!issue.assignees.is_empty()).then(|| issue.assignees.join(", "));
        task.assigned_to = issue
            .assignees
            .first()
            .map(|login| format!("{}:{}:{login}", repository.provider, repository.host));
        task.issue = Some(issue);
    }

    async fn update_linked_issue(
        &self,
        repositories: &[rooms::Repository],
        task: &mut rooms::Task,
        change: rooms::TaskChange,
    ) -> Result<()> {
        if let Some(issue) = &task.issue {
            let repository = repositories
                .iter()
                .find(|repo| repo.id == issue.repository_id)
                .ok_or_else(|| invalid("The task's repository is unavailable."))?;
            let updated = rooms::providers::update_issue(repository, issue.number, change).await?;
            Self::apply_linked_issue(task, repository, updated);
        }
        Ok(())
    }

    pub(super) async fn refresh_rooms(&self, credentials: &Credentials) -> Result<Vec<Value>> {
        let mut ids = self
            .inner
            .rooms
            .lock()
            .await
            .rooms
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        let catalog = self.mission_list().await?;
        for room in catalog["missions"].as_array().into_iter().flatten() {
            ids.insert(wire::id(room, "id")?);
        }
        let mut events = Vec::new();
        for id in ids {
            if let Err(error) = self.refresh_room_keys(credentials, id).await {
                if matches!(
                    error,
                    Error::Backend {
                        status: 403 | 404,
                        ..
                    }
                ) {
                    self.inner.rooms.lock().await.rooms.remove(&id);
                    continue;
                }
                tracing::debug!(%error, "room key refresh will retry");
                continue;
            }
            match self.room_snapshot(credentials, id, None).await {
                Ok(room) => events.push(json!({"type":"room.snapshot","room":room})),
                Err(error) => tracing::debug!(%error, "room refresh will retry"),
            }
        }
        Ok(events)
    }
}
