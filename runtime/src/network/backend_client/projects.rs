use aws_lc_rs::signature::{ML_DSA_65, VerificationAlgorithm as _};
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::identity::keys::PROJECT_PUBLIC_BYTES;
use crate::projects::{
    self, Action, Payload, Snapshot,
    crypto::{self as project_crypto, Secret, SignedState, State as KeyState, VerifiedState},
};
use serde::{Deserialize, Serialize};

const MEMBER_CHECK: Duration = Duration::from_mins(5);
const PAGE_LIMIT: usize = 200;
const SHOWN_MESSAGES: usize = 100;
const KEPT_MESSAGES: usize = 1000;

#[derive(Default)]
pub(crate) struct ProjectCache {
    generation: u64,
    projects: BTreeMap<Uuid, CachedProject>,
    contents: BTreeMap<Uuid, Content>,
    registered: Option<String>,
    dosi: Option<Uuid>,
}
#[derive(Clone, Default)]
struct CachedProject {
    states: BTreeMap<u64, VerifiedState>,
    keys: BTreeMap<u64, Secret>,
    issues_checked: Option<tokio::time::Instant>,
    members_checked: Option<tokio::time::Instant>,
}
#[derive(Default)]
struct Content {
    sequence: u64,
    messages: BTreeMap<u64, projects::Message>,
    tasks: BTreeMap<String, projects::Task>,
    repositories: BTreeMap<String, projects::Repository>,
    has_older: bool,
    history_trimmed: bool,
    shown: u64,
}
#[derive(Default)]
pub(super) struct Lookups {
    bundles: BTreeMap<String, IdentityBundle>,
    keys: BTreeMap<String, Vec<RecipientKey>>,
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
    members: Vec<String>,
    version: u64,
    states: Vec<SignedState>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentPage {
    sequence: u64,
    key_version: u64,
    has_more: bool,
    history_trimmed: bool,
    items: Vec<Item>,
}
#[derive(Clone, Deserialize)]
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
    owner_root: Option<identity::pins::Root>,
}
type Heads = BTreeMap<String, BTreeMap<Uuid, Head>>;

struct Standing {
    members: BTreeMap<String, IdentityBundle>,
    identities: BTreeMap<String, VerifiedIdentity>,
    replaced: BTreeSet<String>,
}

impl CachedProject {
    fn key_maker_left(&self) -> bool {
        let Some((_, current)) = self.states.last_key_value() else {
            return false;
        };
        self.states
            .values()
            .find(|state| state.state.epoch == current.state.epoch)
            .is_some_and(|opener| !current.state.members.contains_key(&opener.state.author_id))
    }
}

impl ProjectCache {
    fn current(&self, credentials: &Credentials) -> Result<()> {
        if self.generation == credentials.generation {
            Ok(())
        } else {
            Err(Error::Stale)
        }
    }

    fn sequence(&self, credentials: &Credentials, project: Uuid) -> Result<Option<u64>> {
        self.current(credentials)?;
        Ok(self.contents.get(&project).map(|content| content.sequence))
    }

    fn decrypt(
        &self,
        credentials: &Credentials,
        project: Uuid,
        content: &mut Content,
        items: &[Item],
    ) -> Result<()> {
        self.current(credentials)?;
        let cached = self
            .projects
            .get(&project)
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?;
        items
            .iter()
            .try_for_each(|item| content.take(cached, project, item, &credentials.user_id))
    }

    fn merge(
        &mut self,
        credentials: &Credentials,
        project: Uuid,
        items: &[Item],
        sequence: u64,
        trimmed: bool,
    ) -> Result<()> {
        self.current(credentials)?;
        let cached = self
            .projects
            .get(&project)
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?;
        let content = self.contents.get_mut(&project).ok_or(Error::Stale)?;
        for item in items {
            content.take(cached, project, item, &credentials.user_id)?;
        }
        content.sequence = content.sequence.max(sequence);
        content.history_trimmed |= trimmed;
        content.trim();
        Ok(())
    }

    fn absorb(&mut self, credentials: &Credentials, project: Uuid, item: &Item) {
        if self.generation == credentials.generation
            && let (Some(cached), Some(content)) =
                (self.projects.get(&project), self.contents.get_mut(&project))
            && item.sequence == content.sequence + 1
            && content
                .take(cached, project, item, &credentials.user_id)
                .is_ok()
        {
            content.sequence = item.sequence;
        }
    }

    fn show(&mut self, credentials: &Credentials, project: Uuid) -> Result<Snapshot> {
        self.current(credentials)?;
        Ok(self
            .contents
            .get_mut(&project)
            .ok_or_else(|| Error::Text(text!("The project is not loaded.")))?
            .show(project))
    }

    fn view(&self, credentials: &Credentials, project: Uuid) -> Result<Snapshot> {
        self.current(credentials)?;
        let content = self
            .contents
            .get(&project)
            .ok_or_else(|| Error::Text(text!("The project is not loaded.")))?;
        Ok(content.view(project, Vec::new(), content.has_older))
    }

    fn key_maker_left(&self, project: Uuid) -> bool {
        self.projects
            .get(&project)
            .is_some_and(CachedProject::key_maker_left)
    }

    fn writing_key(&self, credentials: &Credentials, project: Uuid) -> Result<(u64, u64, Secret)> {
        self.current(credentials)?;
        self.projects
            .get(&project)
            .filter(|cached| !cached.key_maker_left())
            .and_then(|cached| {
                let (_, state) = cached.states.last_key_value()?;
                let key = cached.keys.get(&state.state.epoch)?.clone();
                Some((state.state.version, state.state.epoch, key))
            })
            .ok_or_else(|| Error::Text(text!("Project keys are still arriving.")))
    }
}

impl Content {
    fn take(
        &mut self,
        cached: &CachedProject,
        project: Uuid,
        item: &Item,
        own: &str,
    ) -> Result<()> {
        match BackendClient::project_payload(cached, project, item)? {
            Payload::Message {
                text,
                author_name,
                agent,
                terminal_id,
            } if item.kind == "message" => {
                self.messages.insert(
                    item.sequence,
                    projects::Message {
                        id: item.id.to_string(),
                        sequence: item.sequence,
                        author_id: item.user_id.clone(),
                        author_name,
                        agent,
                        terminal_id,
                        text,
                        created_at: item.created_at.clone(),
                    },
                );
            }
            Payload::Task { mut task } if item.kind == "task" => {
                task.id = item.id.to_string();
                task.version = item.version;
                if self
                    .tasks
                    .get(&task.id)
                    .is_none_or(|known| known.version <= task.version)
                {
                    self.tasks.insert(task.id.clone(), task);
                }
            }
            Payload::Repository { mut repository } if item.kind == "repository" => {
                repository.id = item.id.to_string();
                if projects::providers::linkable(&repository, item.user_id == own) {
                    self.repositories.insert(repository.id.clone(), repository);
                } else {
                    tracing::warn!(item = %item.id, "a project repository names another address than its own, so Kodosi leaves it out");
                }
            }
            Payload::Dosi { .. } | Payload::DosiInstructions { .. } => {}
            _ => return Err(invalid("Project content has a different type.")),
        }
        Ok(())
    }

    fn trim(&mut self) {
        while self.messages.len() > KEPT_MESSAGES
            && let Some((&oldest, _)) = self.messages.first_key_value()
            && oldest <= self.shown
        {
            self.messages.remove(&oldest);
            self.has_older = true;
        }
    }

    fn view(&self, project: Uuid, messages: Vec<projects::Message>, has_older: bool) -> Snapshot {
        Snapshot {
            project_id: project.to_string(),
            messages,
            tasks: self.tasks.values().cloned().collect(),
            repositories: self.repositories.values().cloned().collect(),
            sequence: self.sequence,
            has_older,
            history_trimmed: self.history_trimmed,
        }
    }

    fn show(&mut self, project: Uuid) -> Snapshot {
        let newest = self
            .messages
            .keys()
            .rev()
            .nth(SHOWN_MESSAGES - 1)
            .copied()
            .unwrap_or_default();
        let floor = if self.shown == 0 {
            newest
        } else {
            newest.min(self.shown + 1)
        };
        let messages = self
            .messages
            .range(floor..)
            .map(|(_, message)| message.clone())
            .collect();
        let has_older = self.has_older || self.messages.range(..floor).next().is_some();
        if let Some((&last, _)) = self.messages.last_key_value() {
            self.shown = self.shown.max(last);
        }
        self.view(project, messages, has_older)
    }
}

mod dosi;
#[cfg(test)]
#[path = "projects_tests.rs"]
mod tests;

pub(super) fn project_key_proof(user: &str, device: &str, public: &[u8]) -> Result<Vec<u8>> {
    crypto::signed_fields(
        project_crypto::RECIPIENT_DOMAIN,
        &[user.as_bytes(), device.as_bytes(), public],
    )
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Parting {
    own: Vec<Uuid>,
    leave: Vec<Uuid>,
    reject: Vec<Uuid>,
}

fn parting(list: &Value, own: &str, other: &str) -> Result<Parting> {
    let mut parting = Parting::default();
    for project in list["projects"].as_array().into_iter().flatten() {
        let owner = wire::text(project, "ownerUserId")?;
        if owner == own {
            parting.own.push(wire::id(project, "id")?);
        } else if owner == other {
            parting.leave.push(wire::id(project, "id")?);
        }
    }
    for invitation in list["invitations"].as_array().into_iter().flatten() {
        if wire::text(invitation, "inviterUserId")? == other {
            parting.reject.push(wire::id(invitation, "id")?);
        }
    }
    Ok(parting)
}

impl BackendClient {
    pub(super) async fn end_projects_with(
        &self,
        credentials: &Credentials,
        other: &str,
    ) -> Result<()> {
        let parting = parting(&self.project_list().await?, &credentials.user_id, other)?;
        for project in parting.own {
            let member = match self.project_keys(credentials, project).await {
                Ok(cached) => cached
                    .states
                    .last_key_value()
                    .is_some_and(|(_, current)| current.state.members.contains_key(other)),
                Err(error) if self.listed_member(credentials, project, other).await? => {
                    return Err(error);
                }
                Err(_) => false,
            };
            if member {
                self.project_mutation(
                    "project.removeMember",
                    &json!({"requestId": Uuid::now_v7(), "projectId": project, "userId": other}),
                )
                .await?;
            }
        }
        for project in parting.leave {
            self.project_mutation(
                "project.leave",
                &json!({"requestId": Uuid::now_v7(), "projectId": project}),
            )
            .await?;
        }
        for invitation in parting.reject {
            self.project_mutation(
                "project.invitation.reject",
                &json!({"requestId": Uuid::now_v7(), "invitationId": invitation}),
            )
            .await?;
        }
        Ok(())
    }

    async fn listed_member(
        &self,
        credentials: &Credentials,
        project: Uuid,
        user: &str,
    ) -> Result<bool> {
        let detail: Value = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/projects/{project}"),
                credentials,
                None,
            )
            .await?;
        Ok(detail["members"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|member| member["userId"] == user))
    }

    pub(super) async fn register_project_key(&self, credentials: &Credentials) -> Result<()> {
        {
            let mut cache = self.inner.projects.lock().await;
            if cache.generation != credentials.generation {
                *cache = ProjectCache {
                    generation: credentials.generation,
                    ..ProjectCache::default()
                };
            }
            if cache.registered.as_ref() == Some(&credentials.keys.device_id) {
                return Ok(());
            }
        }
        let proof = project_key_proof(
            &credentials.user_id,
            &credentials.keys.device_id,
            credentials.keys.project_public(),
        )?;
        let signature = crypto::sign_control_message(credentials.keys.signing_pkcs8(), &proof)?;
        let _response: Value = self.inner.http.device(Method::PUT, "api/me/project-key", credentials,
            Some(json!({"publicKey":BASE64.encode(credentials.keys.project_public()),"signature":BASE64.encode(signature)}))).await?;
        self.inner.projects.lock().await.registered = Some(credentials.keys.device_id.clone());
        Ok(())
    }

    async fn member_bundle(&self, credentials: &Credentials, user: &str) -> Result<IdentityBundle> {
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
            return Err(invalid("The project identity belongs to another user."));
        }
        Ok(bundle)
    }

    async fn trusted_member(
        &self,
        credentials: &Credentials,
        user: &str,
    ) -> Result<(IdentityBundle, VerifiedIdentity)> {
        let trusted = self.fetch_identity_with(credentials, user).await?.root;
        let bundle = self.member_bundle(credentials, user).await?;
        let identity = self.verify_bundle(&bundle, Some(trusted)).await?;
        Ok((bundle, identity))
    }

    async fn project_standing(
        &self,
        credentials: &Credentials,
        previous: &VerifiedState,
        at_ms: u64,
        lookups: &mut Lookups,
    ) -> Result<Standing> {
        let mut standing = Standing {
            members: previous.state.members.clone(),
            identities: previous.identities.clone(),
            replaced: BTreeSet::new(),
        };
        for (user, known) in &previous.state.members {
            let current = match lookups.bundles.get(user) {
                Some(current) => Ok(current.clone()),
                None => self.member_bundle(credentials, user).await,
            };
            let current = match current {
                Ok(current) => {
                    lookups.bundles.insert(user.clone(), current.clone());
                    current
                }
                Err(Error::Backend { status: 400, .. }) => continue,
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
                    tracing::warn!(%error, "the project keeps the last identity of a member whose devices are not current");
                }
            }
        }
        Ok(standing)
    }

    async fn project_recipients(
        &self,
        credentials: &Credentials,
        standing: &Standing,
        lookups: &mut Lookups,
    ) -> Result<Vec<RecipientKey>> {
        let mut recipients = Vec::new();
        for (user, identity) in &standing.identities {
            if standing.replaced.contains(user) {
                continue;
            }
            let keys = if let Some(keys) = lookups.keys.get(user) {
                keys.clone()
            } else {
                let keys: Vec<RecipientKey> = self
                    .inner
                    .http
                    .device(
                        Method::GET,
                        &format!("api/users/{user}/project-keys"),
                        credentials,
                        None,
                    )
                    .await?;
                lookups.keys.insert(user.clone(), keys.clone());
                keys
            };
            for key in keys {
                if key.user_id != *user {
                    return Err(invalid("A project key belongs to another user."));
                }
                let Some(device) = identity.devices.get(&key.device_id) else {
                    continue;
                };
                let public = identity::pins::decode(&key.public_key, PROJECT_PUBLIC_BYTES)?;
                if public.len() != PROJECT_PUBLIC_BYTES {
                    continue;
                }
                let preimage = project_key_proof(user, &key.device_id, &public)?;
                let signature = identity::pins::decode(&key.signature, 3309)?;
                ML_DSA_65
                    .verify_sig(&device.sig_public_key, &preimage, &signature)
                    .map_err(|_| invalid("The project recipient key is not authentic."))?;
                recipients.push(key);
            }
        }
        Ok(recipients)
    }

    fn wrap_project_state(
        credentials: &Credentials,
        state: &mut KeyState,
        recipients: &[RecipientKey],
        key: &Secret,
    ) -> Result<Value> {
        state.recipients = recipients
            .iter()
            .map(|recipient| {
                project_crypto::wrap(
                    state,
                    &recipient.user_id,
                    &recipient.device_id,
                    &identity::pins::decode(&recipient.public_key, PROJECT_PUBLIC_BYTES)?,
                    key,
                )
            })
            .collect::<Result<_>>()?;
        let (body, signature) =
            project_crypto::sign(project_crypto::STATE_DOMAIN, &credentials.keys, state)?;
        Ok(json!({"deviceId":credentials.keys.device_id,"body":body,"signature":signature}))
    }

    async fn initialize_project(
        &self,
        credentials: &Credentials,
        project: Uuid,
        owner: &str,
        participants: &[String],
    ) -> Result<()> {
        if owner != credentials.user_id {
            return Err(invalid(
                "The project owner needs to open this project once to prepare its conversation.",
            ));
        }
        let detail: Value = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/projects/{project}"),
                credentials,
                None,
            )
            .await?;
        let mut users = participants.to_vec();
        if users.is_empty() {
            for member in detail["members"]
                .as_array()
                .ok_or_else(|| invalid("Project members are missing."))?
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
            let (bundle, identity) = self.trusted_member(credentials, &user).await?;
            standing.identities.insert(user.clone(), identity);
            standing.members.insert(user, bundle);
        }
        let recipients = self
            .project_recipients(credentials, &standing, &mut Lookups::default())
            .await?;
        let mut state = KeyState {
            project_id: project,
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
        let body = Self::wrap_project_state(
            credentials,
            &mut state,
            &recipients,
            &project_crypto::secret()?,
        )?;
        let result = self
            .inner
            .http
            .device::<Value>(
                Method::PUT,
                &format!("api/projects/{project}/keys"),
                credentials,
                Some(body),
            )
            .await;
        if !matches!(result, Err(Error::Backend { status: 409, .. })) {
            result?;
        }
        Ok(())
    }

    async fn project_anchor(
        &self,
        credentials: &Credentials,
        project: Uuid,
        cached: &CachedProject,
        owner: &str,
    ) -> Result<identity::pins::Root> {
        if let Some((_, first)) = cached.states.first_key_value() {
            return first
                .state
                .members
                .get(owner)
                .ok_or_else(|| invalid("Project owner identity is missing."))?
                .root();
        }
        if let Some(saved) = self
            .project_heads()?
            .1
            .get(&credentials.user_id)
            .and_then(|projects| projects.get(&project))
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

    async fn project_keys(
        &self,
        credentials: &Credentials,
        project: Uuid,
    ) -> Result<CachedProject> {
        self.register_project_key(credentials).await?;
        let mut initialization_attempts = 0;
        let mut cached = self
            .inner
            .projects
            .lock()
            .await
            .projects
            .get(&project)
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
                    &format!("api/projects/{project}/keys?afterVersion={after}"),
                    credentials,
                    None,
                )
                .await?;
            if history.version < after {
                return Err(invalid("The server returned an older project."));
            }
            if history.version == 0 {
                initialization_attempts += 1;
                if initialization_attempts > 3 {
                    return Err(Error::Text(text!(
                        "The project changed during setup. Open it again."
                    )));
                }
                let members = if self.inner.projects.lock().await.dosi == Some(project) {
                    std::slice::from_ref(&credentials.user_id)
                } else {
                    &history.members
                };
                self.initialize_project(credentials, project, &history.owner_user_id, members)
                    .await?;
                continue;
            }
            let anchor = self
                .project_anchor(credentials, project, &cached, &history.owner_user_id)
                .await?;
            if history.states.is_empty() && history.version > after {
                return Err(invalid("Project key history is incomplete."));
            }
            for signed in history.states {
                let verified = project_crypto::verify_state(
                    &signed,
                    project,
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
                            "You made this project before you started fresh. Delete it and make a new project.",
                        )
                    }
                    error => error,
                })?;
                if let Ok(key) =
                    project_crypto::unwrap(&verified.state, &credentials.user_id, &credentials.keys)
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
                return Err(invalid("The project key history exceeds its limit."));
            }
        }
        self.remember_project(credentials, project, cached).await
    }

    fn project_heads(&self) -> Result<(std::path::PathBuf, Heads)> {
        let path = self.inner.config.data_root.join("network/room-heads.json");
        let heads = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => serde_json::from_slice(&bytes)?,
            Ok(_) => return Err(invalid("Saved project history exceeds its limit.")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.into()),
        };
        Ok((path, heads))
    }

    /// A history that forks from the one that this device saved, or ends before it, fails, unless
    /// this device keeps a newer history that continues it: a check that read the server before
    /// another one kept that history takes it.
    async fn remember_project(
        &self,
        credentials: &Credentials,
        project: Uuid,
        mut cached: CachedProject,
    ) -> Result<CachedProject> {
        let mut cache = self.inner.projects.lock().await;
        if cache.generation != credentials.generation {
            return Err(Error::Stale);
        }
        let current = cached
            .states
            .last_key_value()
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?
            .1;
        if let Some(newer) = cache.projects.get(&project).filter(|kept| {
            kept.states
                .last_key_value()
                .is_some_and(|(version, _)| *version > current.state.version)
                && kept
                    .states
                    .get(&current.state.version)
                    .is_some_and(|state| state.hash == current.hash)
        }) {
            return Ok(newer.clone());
        }
        let (path, mut heads) = self.project_heads()?;
        let known = heads.entry(credentials.user_id.clone()).or_default();
        if known.get(&project).is_some_and(|head| {
            head.version > current.state.version
                || cached
                    .states
                    .get(&head.version)
                    .is_some_and(|state| state.hash != head.hash)
        }) {
            return Err(invalid("The project history changed unexpectedly."));
        }
        known.insert(
            project,
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
        kodosi_os::fs::write_private(&path, &serde_json::to_vec(&heads)?)?;
        for state in cached.states.values().rev() {
            if state.state.epoch > 1
                && let Some(current) = cached.keys.get(&state.state.epoch)
            {
                let previous = project_crypto::open_previous(&state.state, current)?;
                cached.keys.insert(state.state.epoch - 1, previous);
            }
        }
        if let Some(existing) = cache.projects.get(&project) {
            cached.issues_checked = cached.issues_checked.max(existing.issues_checked);
            cached.members_checked = cached.members_checked.max(existing.members_checked);
        }
        cache.projects.insert(project, cached.clone());
        drop(cache);
        Ok(cached)
    }

    pub(super) async fn prepare_project_membership(
        &self,
        credentials: &Credentials,
        project: Uuid,
        add: Option<&str>,
        remove: Option<&str>,
    ) -> Result<Value> {
        let cached = self.project_keys(credentials, project).await?;
        self.next_project_state(
            credentials,
            project,
            &cached,
            add,
            remove,
            &mut Lookups::default(),
        )
        .await?
        .ok_or_else(|| invalid("The project membership did not change."))
    }

    /// Takes the project state that this device signed and the server accepted, so the server
    /// cannot keep serving the state before it.
    pub(super) async fn apply_own_state(
        &self,
        credentials: &Credentials,
        project: Uuid,
        written: &Value,
    ) -> Result<()> {
        let body = wire::text(written, "body")?;
        let state: KeyState =
            serde_json::from_slice(&identity::pins::decode(body, 3 * 1024 * 1024)?)?;
        let mut cached = self
            .inner
            .projects
            .lock()
            .await
            .projects
            .get(&project)
            .cloned()
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?;
        let anchor = self
            .project_anchor(credentials, project, &cached, &state.owner_user_id)
            .await?;
        let verified = project_crypto::verify_state(
            &SignedState {
                version: state.version,
                body: body.to_owned(),
                signature: wire::text(written, "signature")?.to_owned(),
            },
            project,
            &state.owner_user_id,
            &anchor,
            cached.states.last_key_value().map(|(_, previous)| previous),
        )?;
        if let Ok(key) =
            project_crypto::unwrap(&verified.state, &credentials.user_id, &credentials.keys)
        {
            cached.keys.insert(verified.state.epoch, key);
        }
        cached.states.insert(state.version, verified);
        self.remember_project(credentials, project, cached).await?;
        Ok(())
    }

    async fn next_project_state(
        &self,
        credentials: &Credentials,
        project: Uuid,
        cached: &CachedProject,
        add: Option<&str>,
        remove: Option<&str>,
        lookups: &mut Lookups,
    ) -> Result<Option<Value>> {
        let previous = cached
            .states
            .last_key_value()
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?
            .1;
        let prior_key = cached
            .keys
            .get(&previous.state.epoch)
            .ok_or_else(|| Error::Text(text!("Project keys are still arriving on this device.")))?;
        let created = identity::now_ms().max(previous.state.created_at_ms);
        let mut standing = self
            .project_standing(credentials, previous, created, lookups)
            .await?;
        let mut returned = false;
        if previous.state.owner_user_id == credentials.user_id {
            for user in standing.replaced.clone() {
                if remove == Some(user.as_str()) {
                    continue;
                }
                match self.trusted_member(credentials, &user).await {
                    Ok((bundle, identity)) => {
                        standing.identities.insert(user.clone(), identity);
                        standing.members.insert(user.clone(), bundle);
                        standing.replaced.remove(&user);
                        returned = true;
                    }
                    Err(error) if error.unanswered() => return Err(error),
                    Err(error) => {
                        tracing::info!(%error, "a project member with a new identity waits for the trust of the owner");
                    }
                }
            }
        }
        if let Some(user) = add {
            let (bundle, identity) = self.trusted_member(credentials, user).await?;
            standing.identities.insert(user.to_owned(), identity);
            standing.members.insert(user.to_owned(), bundle);
        }
        if let Some(user) = remove {
            standing.identities.remove(user);
            standing.members.remove(user);
        }
        let recipients = self
            .project_recipients(credentials, &standing, lookups)
            .await?;
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
        let rotate = cached.key_maker_left();
        if add.is_none() && remove.is_none() && !returned && !rotate && old == new {
            return Ok(None);
        }
        let mut state = previous.state.clone();
        state.version += 1;
        state.previous_hash.clone_from(&previous.hash);
        state.author_id.clone_from(&credentials.user_id);
        state.device_id.clone_from(&credentials.keys.device_id);
        state.created_at_ms = created;
        state.members = standing.members;
        let key = if remove.is_some() || returned || rotate || old.difference(&new).next().is_some()
        {
            state.epoch += 1;
            let key = project_crypto::secret()?;
            state.previous_key = Some(project_crypto::seal_previous(
                project,
                state.epoch,
                &key,
                prior_key,
            )?);
            key
        } else {
            prior_key.clone()
        };
        Self::wrap_project_state(credentials, &mut state, &recipients, &key).map(Some)
    }

    pub(super) async fn refresh_project_keys(
        &self,
        credentials: &Credentials,
        project: Uuid,
        force: bool,
        lookups: &mut Lookups,
    ) -> Result<()> {
        if !force && self.members_checked(credentials, project).await {
            self.ensure_project_keys(credentials, project, None).await?;
            if !self.inner.projects.lock().await.key_maker_left(project) {
                return Ok(());
            }
        }
        let mut attempts = 0;
        loop {
            let cached = self.project_keys(credentials, project).await?;
            let current = &cached
                .states
                .last_key_value()
                .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?
                .1
                .state;
            if !current.members.contains_key(&credentials.user_id)
                || !cached.keys.contains_key(&current.epoch)
            {
                return Ok(());
            }
            let Some(body) = self
                .next_project_state(credentials, project, &cached, None, None, lookups)
                .await?
            else {
                self.mark_members_checked(credentials, project).await;
                return Ok(());
            };
            let stored = self
                .inner
                .http
                .device::<Value>(
                    Method::PUT,
                    &format!("api/projects/{project}/keys"),
                    credentials,
                    Some(body),
                )
                .await;
            attempts += 1;
            match stored {
                Err(Error::Backend { status: 409, .. }) if attempts < 3 => {}
                Err(error) => return Err(error),
                Ok(_) => {
                    self.project_keys(credentials, project).await?;
                    self.mark_members_checked(credentials, project).await;
                    return Ok(());
                }
            }
        }
    }

    async fn members_checked(&self, credentials: &Credentials, project: Uuid) -> bool {
        let cache = self.inner.projects.lock().await;
        cache.generation == credentials.generation
            && cache
                .projects
                .get(&project)
                .and_then(|cached| cached.members_checked)
                .is_some_and(|checked| checked.elapsed() < MEMBER_CHECK)
    }

    async fn mark_members_checked(&self, credentials: &Credentials, project: Uuid) {
        let mut cache = self.inner.projects.lock().await;
        if cache.generation == credentials.generation
            && let Some(cached) = cache.projects.get_mut(&project)
        {
            cached.members_checked = Some(tokio::time::Instant::now());
        }
    }

    async fn ensure_project_keys(
        &self,
        credentials: &Credentials,
        project: Uuid,
        version: Option<u64>,
    ) -> Result<()> {
        {
            let cache = self.inner.projects.lock().await;
            if cache.generation == credentials.generation
                && cache.registered.as_ref() == Some(&credentials.keys.device_id)
                && let Some(cached) = cache.projects.get(&project)
                && let Some((&latest, _)) = cached.states.last_key_value()
                && version.is_none_or(|version| latest >= version)
            {
                return Ok(());
            }
        }
        self.project_keys(credentials, project).await.map(|_| ())
    }

    pub(super) async fn project_history(
        &self,
        credentials: &Credentials,
        project: Uuid,
    ) -> Result<String> {
        self.project_keys(credentials, project)
            .await?
            .states
            .last_key_value()
            .map(|(_, newest)| newest.hash.clone())
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))
    }

    #[cfg(feature = "host")]
    pub(super) async fn project_holds(
        &self,
        credentials: &Credentials,
        project: Uuid,
        hash: &str,
    ) -> Result<bool> {
        Ok(self
            .project_keys(credentials, project)
            .await?
            .states
            .values()
            .any(|state| state.hash == hash))
    }

    pub(crate) async fn project_identity(
        &self,
        credentials: &Credentials,
        project: Uuid,
        user: &str,
    ) -> Result<VerifiedIdentity> {
        let cached = self.project_keys(credentials, project).await?;
        let state = &cached
            .states
            .last_key_value()
            .ok_or_else(|| Error::Text(text!("Project keys are missing.")))?
            .1
            .state;
        if !state.members.contains_key(&credentials.user_id) {
            return Err(invalid("This device's person is no longer in the project."));
        }
        let anchor = state
            .members
            .get(user)
            .ok_or_else(|| Error::Text(text!("This person is no longer in the project.")))?
            .root()?;
        if user != credentials.user_id {
            let friend = self.fetch_identity_with(credentials, user).await?;
            if friend.root != anchor {
                return Err(Error::Trust(text!(
                    "This friend has a new identity. Trust it in Friends before you share or connect."
                )));
            }
            return Ok(friend);
        }
        let bundle = self.member_bundle(credentials, user).await?;
        self.verify_bundle(&bundle, Some(anchor)).await
    }

    fn project_payload(cached: &CachedProject, project: Uuid, item: &Item) -> Result<Payload> {
        let bytes = identity::pins::decode(&item.body, 64 * 1024)?;
        let content: project_crypto::Content = serde_json::from_slice(&bytes)?;
        if content.project_id != project
            || content.id != item.id
            || content.kind != item.kind
            || content.version != item.version
            || content.key_version != item.key_version
            || content.author_id != item.user_id
            || content.device_id != item.device_id
        {
            return Err(invalid("Project content has inconsistent identity."));
        }
        let state = cached
            .states
            .get(&content.key_version)
            .ok_or_else(|| invalid("Project content keys are missing."))?;
        if state.state.epoch != content.epoch {
            return Err(invalid("Project content refers to another key."));
        }
        let author = state
            .identities
            .get(&content.author_id)
            .and_then(|identity| identity.devices.get(&content.device_id))
            .ok_or_else(|| invalid("The project content author is unavailable."))?;
        project_crypto::verify_signature(
            project_crypto::CONTENT_DOMAIN,
            &bytes,
            &item.signature,
            &author.sig_public_key,
        )?;
        let key = cached
            .keys
            .get(&content.epoch)
            .ok_or_else(|| Error::Text(text!("Project history keys are still arriving.")))?;
        Ok(serde_json::from_slice(&content.decrypt(key)?)?)
    }

    async fn read_page(
        &self,
        credentials: &Credentials,
        project: Uuid,
        query: &str,
    ) -> Result<ContentPage> {
        let page: ContentPage = self
            .inner
            .http
            .device(
                Method::GET,
                &format!("api/projects/{project}/content?{query}"),
                credentials,
                None,
            )
            .await?;
        self.ensure_project_keys(credentials, project, Some(page.key_version))
            .await?;
        Ok(page)
    }

    async fn load_project(&self, credentials: &Credentials, project: Uuid) -> Result<()> {
        let mut content = Content::default();
        let mut first = None;
        for kind in ["message", "task", "repository"] {
            let mut after = 0;
            loop {
                let query = if kind == "message" {
                    format!("kind=message&limit={SHOWN_MESSAGES}&before={}", i64::MAX)
                } else {
                    format!("kind={kind}&limit={PAGE_LIMIT}&after={after}")
                };
                let page = self.read_page(credentials, project, &query).await?;
                first.get_or_insert(page.sequence);
                content.history_trimmed |= page.history_trimmed;
                self.inner.projects.lock().await.decrypt(
                    credentials,
                    project,
                    &mut content,
                    &page.items,
                )?;
                if kind == "message" {
                    content.has_older = page.has_more;
                    break;
                }
                let next = page.items.last().map_or(after, |item| item.sequence);
                if !page.has_more {
                    break;
                }
                if next <= after {
                    return Err(invalid("Project history did not advance."));
                }
                after = next;
            }
        }
        content.sequence = first.unwrap_or_default();
        let mut cache = self.inner.projects.lock().await;
        cache.current(credentials)?;
        cache.contents.insert(project, content);
        drop(cache);
        Ok(())
    }

    async fn sync_project(
        &self,
        credentials: &Credentials,
        project: Uuid,
        wanted: Option<u64>,
    ) -> Result<bool> {
        self.ensure_project_keys(credentials, project, None).await?;
        let known = self
            .inner
            .projects
            .lock()
            .await
            .sequence(credentials, project)?;
        let Some(mut after) = known else {
            self.load_project(credentials, project).await?;
            return Ok(true);
        };
        if wanted.is_some_and(|wanted| after >= wanted) {
            return Ok(false);
        }
        let mut changed = false;
        loop {
            let page = self
                .read_page(
                    credentials,
                    project,
                    &format!("limit={PAGE_LIMIT}&after={after}"),
                )
                .await?;
            if page.sequence < after {
                self.load_project(credentials, project).await?;
                return Ok(true);
            }
            let last = page.items.last().map(|item| item.sequence);
            let next = if page.has_more {
                last.filter(|last| *last > after)
                    .ok_or_else(|| invalid("Project history did not advance."))?
            } else {
                last.unwrap_or(after).max(page.sequence)
            };
            changed |= !page.items.is_empty();
            self.inner.projects.lock().await.merge(
                credentials,
                project,
                &page.items,
                next,
                page.history_trimmed,
            )?;
            if !page.has_more {
                return Ok(changed);
            }
            after = next;
        }
    }

    async fn show_project(&self, credentials: &Credentials, project: Uuid) -> Result<Snapshot> {
        self.inner.projects.lock().await.show(credentials, project)
    }

    async fn project_view(&self, credentials: &Credentials, project: Uuid) -> Result<Snapshot> {
        self.inner.projects.lock().await.view(credentials, project)
    }

    async fn older_messages(
        &self,
        credentials: &Credentials,
        project: Uuid,
        before: u64,
    ) -> Result<Snapshot> {
        let page = self
            .read_page(
                credentials,
                project,
                &format!("kind=message&limit={SHOWN_MESSAGES}&before={before}"),
            )
            .await?;
        let mut older = Content::default();
        self.inner
            .projects
            .lock()
            .await
            .decrypt(credentials, project, &mut older, &page.items)?;
        let mut view = self.project_view(credentials, project).await?;
        view.messages = older.messages.into_values().collect();
        view.has_older = page.has_more;
        view.history_trimmed |= page.history_trimmed;
        Ok(view)
    }

    pub(super) fn listed_projects(catalog: &Value) -> Result<(BTreeSet<Uuid>, bool)> {
        let mut listed = BTreeSet::new();
        for project in catalog["projects"].as_array().into_iter().flatten() {
            listed.insert(wire::id(project, "id")?);
        }
        Ok((listed, catalog["projectsTruncated"] != true))
    }

    async fn forget_project(&self, project: Uuid) {
        let mut cache = self.inner.projects.lock().await;
        cache.projects.remove(&project);
        cache.contents.remove(&project);
        if cache.dosi == Some(project) {
            cache.dosi = None;
        }
    }

    pub(super) async fn learn_dosi(&self, credentials: &Credentials, catalog: &mut Value) {
        let listed = catalog
            .as_object_mut()
            .and_then(|fields| fields.remove("dosi"))
            .and_then(|project| {
                project
                    .as_str()
                    .and_then(|project| Uuid::parse_str(project).ok())
            });
        let project = match self.pinned_dosi(credentials) {
            Ok(pinned) => pinned.or(listed),
            Err(error) => {
                tracing::warn!(%error, "Dosi could not read the project it keeps");
                None
            }
        };
        let mut cache = self.inner.projects.lock().await;
        if cache.generation == credentials.generation && project.is_some() {
            cache.dosi = project;
        }
    }

    async fn refresh_dosi(
        &self,
        credentials: &Credentials,
        project: Uuid,
        wanted: Option<u64>,
        force: bool,
        lookups: &mut Lookups,
    ) -> Result<Option<Value>> {
        match self
            .refresh_project_keys(credentials, project, force, lookups)
            .await
        {
            Ok(()) => Ok(Some(json!({"type":"network.dosi","sequence":wanted}))),
            Err(Error::Backend {
                status: 403 | 404, ..
            }) => {
                self.forget_project(project).await;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(super) async fn refresh_project(
        &self,
        credentials: &Credentials,
        project: Uuid,
        wanted: Option<u64>,
        force: bool,
        lookups: &mut Lookups,
    ) -> Result<Option<Value>> {
        if self.inner.projects.lock().await.dosi == Some(project) {
            return self
                .refresh_dosi(credentials, project, wanted, force, lookups)
                .await;
        }
        let refreshed = async {
            self.refresh_project_keys(credentials, project, force, lookups)
                .await?;
            self.sync_project(credentials, project, wanted).await
        }
        .await;
        match refreshed {
            Ok(changed) if changed || force => Ok(Some(
                json!({"type":"project.snapshot","content":self.show_project(credentials, project).await?}),
            )),
            Ok(_) => Ok(None),
            Err(Error::Backend {
                status: 403 | 404, ..
            }) => {
                self.forget_project(project).await;
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn project_write(
        &self,
        credentials: &Credentials,
        project: Uuid,
        id: Uuid,
        kind: &str,
        expected: u64,
        payload: &Payload,
    ) -> Result<u64> {
        self.ensure_project_keys(credentials, project, None).await?;
        let mut key = self.writing_key(credentials, project).await?;
        let mut written = self
            .put_project_item(credentials, project, id, kind, expected, payload, &key)
            .await;
        if matches!(written, Err(Error::Backend { status: 409, .. })) {
            self.project_keys(credentials, project).await?;
            let newer = self.writing_key(credentials, project).await?;
            if newer.0 > key.0 {
                key = newer;
                written = self
                    .put_project_item(credentials, project, id, kind, expected, payload, &key)
                    .await;
            }
        }
        let item = written?;
        self.inner
            .projects
            .lock()
            .await
            .absorb(credentials, project, &item);
        Ok(item.sequence)
    }

    async fn writing_key(
        &self,
        credentials: &Credentials,
        project: Uuid,
    ) -> Result<(u64, u64, Secret)> {
        if self.inner.projects.lock().await.key_maker_left(project) {
            self.refresh_project_keys(credentials, project, true, &mut Lookups::default())
                .await?;
        }
        self.inner
            .projects
            .lock()
            .await
            .writing_key(credentials, project)
    }

    async fn put_project_item(
        &self,
        credentials: &Credentials,
        project: Uuid,
        id: Uuid,
        kind: &str,
        expected: u64,
        payload: &Payload,
        (version, epoch, key): &(u64, u64, Secret),
    ) -> Result<Item> {
        let mut content = project_crypto::Content {
            project_id: project,
            id,
            kind: kind.into(),
            version: expected + 1,
            key_version: *version,
            epoch: *epoch,
            author_id: credentials.user_id.clone(),
            device_id: credentials.keys.device_id.clone(),
            nonce: String::new(),
            ciphertext: String::new(),
        };
        content.encrypt(key, &serde_json::to_vec(payload)?)?;
        let (body, signature) =
            project_crypto::sign(project_crypto::CONTENT_DOMAIN, &credentials.keys, &content)?;
        self.inner.http.device(Method::PUT, &format!("api/projects/{project}/content/{id}"), credentials,
            Some(json!({"expectedVersion":expected,"deviceId":credentials.keys.device_id,"body":body,"signature":signature}))).await
    }

    #[expect(clippy::too_many_lines, reason = "one typed project action dispatch")]
    pub(super) async fn act_in_project(&self, args: &Value) -> Result<Vec<Value>> {
        let credentials = self.credentials()?;
        let project = wire::id(args, "projectId")?;
        let request = wire::text(args, "requestId")?;
        let action: Action = serde_json::from_value(args["action"].clone())?;
        self.refresh_project_keys(&credentials, project, false, &mut Lookups::default())
            .await?;
        let before = match action {
            Action::Read { before } => before,
            _ => None,
        };
        if !matches!(action, Action::Post { .. })
            || self
                .inner
                .projects
                .lock()
                .await
                .sequence(&credentials, project)?
                .is_none()
        {
            self.sync_project(&credentials, project, None).await?;
        }
        let mut snapshot = self.project_view(&credentials, project).await?;
        let mut written = if cfg!(feature = "host") && matches!(action, Action::Read { .. }) {
            self.sync_linked_issues(&credentials, project, &mut snapshot)
                .await?
        } else {
            None
        };
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
                written = Some(
                    self.project_write(
                        &credentials,
                        project,
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
                    .await?,
                );
                item_id = Some(id.to_string());
            }
            Action::CreateTask {
                title,
                description,
                mut repository_ids,
                terminal_id,
                issue_repository_id,
            } => {
                let id = Uuid::parse_str(request).map_err(|_| invalid("Invalid task identity."))?;
                let created = match &issue_repository_id {
                    Some(repository_id) => {
                        let repository = snapshot
                            .repositories
                            .iter()
                            .find(|repo| &repo.id == repository_id)
                            .ok_or_else(|| Error::Text(text!("This repository is unavailable.")))?;
                        if !repository_ids.contains(repository_id) {
                            repository_ids.insert(0, repository_id.clone());
                        }
                        Some((
                            repository,
                            projects::providers::create_issue(repository, &title, &description)
                                .await?,
                        ))
                    }
                    None => None,
                };
                let mut task = projects::Task {
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
                if let Some((repository, issue)) = created {
                    Self::apply_linked_issue(&mut task, repository, issue);
                }
                written = Some(
                    self.project_write(
                        &credentials,
                        project,
                        id,
                        "task",
                        0,
                        &Payload::Task { task },
                    )
                    .await?,
                );
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
                    .ok_or_else(|| Error::Text(text!("This task is unavailable.")))?;
                match change {
                    projects::TaskChange::Claim => {
                        let live = |id: &str| {
                            Uuid::parse_str(id).is_ok_and(|id| {
                                self.inner
                                    .sessions
                                    .lock()
                                    .is_ok_and(|sessions| sessions.contains_key(&id))
                            })
                        };
                        task.claim(&credentials.user_id, name, terminal_id, live)
                            .map_err(Error::Text)?;
                    }
                    projects::TaskChange::Release => {
                        task.assigned_to = None;
                        task.assigned_name = None;
                        task.terminal_id = None;
                    }
                    projects::TaskChange::Close => task.closed = true,
                    projects::TaskChange::Reopen => task.closed = false,
                }
                if let Some(note) = note {
                    task.note = Some(note);
                }
                self.update_linked_issue(&snapshot.repositories, &mut task, change)
                    .await?;
                let id =
                    Uuid::parse_str(&task_id).map_err(|_| invalid("Invalid task identity."))?;
                written = Some(
                    self.project_write(
                        &credentials,
                        project,
                        id,
                        "task",
                        task.version,
                        &Payload::Task { task },
                    )
                    .await?,
                );
                item_id = Some(task_id);
            }
            Action::AddRepository { url, provider } => {
                let mut repository = projects::providers::repository(&url, provider.as_deref())?;
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
                    written = Some(
                        self.project_write(
                            &credentials,
                            project,
                            id,
                            "repository",
                            0,
                            &Payload::Repository { repository },
                        )
                        .await?,
                    );
                    item_id = Some(id.to_string());
                }
            }
            Action::Issues { repository_id } => {
                let repository = snapshot
                    .repositories
                    .iter()
                    .find(|repo| repo.id == repository_id)
                    .ok_or_else(|| Error::Text(text!("This repository is unavailable.")))?;
                let issues = projects::providers::issues(repository, None).await?;
                return Ok(vec![
                    json!({"type":"project.result","requestId":request,"operation":"project.act","projectId":project,"action":args["action"]["type"],"repositoryId":repository_id,"issues":issues}),
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
                    .ok_or_else(|| Error::Text(text!("This repository is unavailable.")))?;
                let issue = projects::providers::issues(repository, Some(number))
                    .await?
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::Text(text!("This issue is unavailable.")))?;
                if let Some(existing) = snapshot.tasks.iter().find(|task| {
                    task.issue
                        .as_ref()
                        .is_some_and(|linked| linked.url == issue.url)
                }) {
                    item_id = Some(existing.id.clone());
                } else {
                    let id =
                        Uuid::parse_str(request).map_err(|_| invalid("Invalid task identity."))?;
                    let mut task = projects::Task {
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
                    written = Some(
                        self.project_write(
                            &credentials,
                            project,
                            id,
                            "task",
                            0,
                            &Payload::Task { task },
                        )
                        .await?,
                    );
                    item_id = Some(id.to_string());
                }
            }
        }
        if let Some(sequence) = written {
            self.sync_project(&credentials, project, Some(sequence))
                .await?;
        }
        let answer = json!({"type":"project.result","requestId":request,"operation":"project.act","projectId":project,"itemId":item_id,"action":args["action"]["type"]});
        Ok(match before {
            Some(before) => {
                let mut answer = answer;
                answer["content"] = serde_json::to_value(
                    self.older_messages(&credentials, project, before).await?,
                )?;
                vec![answer]
            }
            None => vec![
                json!({"type":"project.snapshot","content":self.show_project(&credentials, project).await?}),
                answer,
            ],
        })
    }

    async fn sync_linked_issues(
        &self,
        credentials: &Credentials,
        project: Uuid,
        snapshot: &mut Snapshot,
    ) -> Result<Option<u64>> {
        {
            let mut cache = self.inner.projects.lock().await;
            let Some(cached) = cache.projects.get_mut(&project) else {
                return Ok(None);
            };
            if cached
                .issues_checked
                .is_some_and(|checked| checked.elapsed() < Duration::from_secs(30))
            {
                return Ok(None);
            }
            cached.issues_checked = Some(tokio::time::Instant::now());
            drop(cache);
        }
        let mut written = None;
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
            let latest = match projects::providers::issues(repository, Some(linked.number)).await {
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
                .project_write(
                    credentials,
                    project,
                    id,
                    "task",
                    task.version,
                    &Payload::Task { task: task.clone() },
                )
                .await
            {
                Ok(sequence) => {
                    task.version += 1;
                    written = written.max(Some(sequence));
                }
                Err(Error::Backend { status: 409, .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(written)
    }

    fn apply_linked_issue(
        task: &mut projects::Task,
        repository: &projects::Repository,
        issue: projects::Issue,
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
        repositories: &[projects::Repository],
        task: &mut projects::Task,
        change: projects::TaskChange,
    ) -> Result<()> {
        if let Some(issue) = &task.issue {
            let repository = repositories
                .iter()
                .find(|repo| repo.id == issue.repository_id)
                .ok_or_else(|| Error::Text(text!("The task's repository is unavailable.")))?;
            let updated =
                projects::providers::update_issue(repository, issue.number, change).await?;
            Self::apply_linked_issue(task, repository, updated);
        }
        Ok(())
    }

    pub(super) async fn projects_to_refresh(
        &self,
        listed: BTreeSet<Uuid>,
        scope: Option<&BTreeSet<Uuid>>,
    ) -> BTreeSet<Uuid> {
        let mut projects = self
            .inner
            .projects
            .lock()
            .await
            .projects
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();
        projects.extend(listed);
        if let Some(scope) = scope {
            projects.retain(|project| scope.contains(project));
        }
        projects
    }

    pub(super) async fn drop_recovery_project_key(&self, credentials: &Credentials) {
        if credentials.enrolled
            && credentials.keys.recovery().is_some()
            && let Err(error) = self.keep_recovery_project_key(credentials, None).await
        {
            tracing::debug!(%error, "the recovery project key stays until the next project refresh");
        }
    }
}
