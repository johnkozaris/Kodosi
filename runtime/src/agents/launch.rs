use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    Agent, AgentAccount, AgentCatalog, AgentChoice, AgentChoiceChange, AgentInfo, AgentModel,
    AgentModels, AgentModelsReply, AgentOption, AgentPick, AgentSettings, AgentStart,
    AgentUsageReply, Installed, Notes, Output, Permissions, as_shell_word,
    quiet::{self, QuietSettings},
    usage,
};

const FILE: &str = "launcher.json";
const CATALOG_LIFE: Duration = Duration::from_secs(30);

pub struct Launcher {
    file: PathBuf,
    home: PathBuf,
    shell: PathBuf,
    notes: PathBuf,
    state: Mutex<State>,
    writing: Mutex<()>,
    unsaved: AtomicBool,
    usage: usage::Readings,
    quiet: QuietSettings,
    codex_servers: OnceLock<String>,
}

pub(crate) struct Launch {
    pub(crate) line: String,
    pub(crate) quiet_settings: Option<PathBuf>,
    /// The account folder of a Copilot that Kodosi starts, where Kodosi allows its extension.
    pub(crate) copilot_home: Option<PathBuf>,
}

pub(crate) struct RunningAgent {
    pub(crate) agent: Agent,
    pub(crate) account_folder: Option<PathBuf>,
    pub(crate) settings: Option<AgentSettings>,
    pub(crate) pending: Option<AgentSettings>,
}

impl RunningAgent {
    /// What Kodosi does not know of the running agent stays the launcher's last choice.
    pub(super) fn start(self, accounts: &[AgentAccount], home: &Path) -> AgentStart {
        let pending = self.pending.unwrap_or_default();
        let known = self.settings.is_some();
        let settings = self.settings.unwrap_or_default();
        let known_or_own = |value: Option<String>| known.then(|| value.unwrap_or_default());
        let effort = if pending.model.is_some() {
            Some(pending.effort.unwrap_or_default())
        } else {
            pending.effort.or_else(|| known_or_own(settings.effort))
        };
        AgentStart {
            agent: self.agent,
            account: self
                .account_folder
                .and_then(|folder| accounts.get(super::accounts::place(accounts, home, &folder)))
                .map(|account| account.name.clone()),
            model: pending.model.or_else(|| known_or_own(settings.model)),
            effort,
            permissions: pending.permissions.or(settings.permissions),
            output: pending.output.or(settings.output),
            fast: pending
                .fast
                .or_else(|| known.then(|| settings.fast.unwrap_or_default())),
            chat: None,
        }
    }
}

#[derive(Default)]
struct State {
    saved: Saved,
    catalog: Option<(Instant, AgentCatalog)>,
    installed: Installed,
    models: HashMap<Agent, AgentModels>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    #[serde(flatten)]
    choice: AgentChoice,
    plain_terminal: bool,
}

impl Saved {
    fn apply(&mut self, change: &AgentChoiceChange) {
        if let AgentChoiceChange::Tab { agent } = change {
            self.plain_terminal = agent.is_none();
        }
        self.choice.apply(change);
    }
}

impl Launcher {
    pub fn load(data_root: &Path, home: &Path, shell: &Path) -> Self {
        let file = data_root.join(FILE);
        let saved = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            file,
            home: home.to_owned(),
            shell: shell.to_owned(),
            notes: super::notes_folder(data_root),
            state: Mutex::new(State {
                saved,
                ..State::default()
            }),
            writing: Mutex::new(()),
            unsaved: AtomicBool::new(false),
            usage: usage::Readings::default(),
            quiet: QuietSettings::load(data_root),
            codex_servers: OnceLock::new(),
        }
    }

    /// From now on Codex starts through the server of its terminal.
    #[cfg(unix)]
    pub(crate) fn serve_codex(&self, servers: String) {
        drop(self.codex_servers.set(servers));
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn notes(&self) -> Notes {
        Notes {
            shell: self.shell.clone(),
            folder: self.notes.clone(),
            installed: self.state().installed,
            codex_servers: self.codex_servers.get().cloned(),
        }
    }

    pub async fn catalog(&self) -> Result<AgentCatalog, String> {
        let (catalog, installed) = super::catalog(&self.home, &self.shell).await?;
        let mut state = self.state();
        state.catalog = Some((Instant::now(), catalog.clone()));
        state.installed = installed;
        Ok(AgentCatalog {
            choice: state.resolved(),
            ..catalog
        })
    }

    pub async fn add_account(
        &self,
        agent: Agent,
        name: &str,
    ) -> Result<AgentAccount, crate::text::Text> {
        let catalog = self.catalog().await.map_err(crate::text::Text::plain)?;
        let taken = agent_info(&catalog, agent).map_or_default(|info| {
            info.accounts
                .iter()
                .map(|account| account.name.clone())
                .collect()
        });
        let account = super::add_account(&self.home, agent, name, taken).await;
        self.state().catalog = None;
        account
    }

    pub async fn models(&self, agent: Agent) -> AgentModelsReply {
        let models = super::models(&self.shell, agent).await;
        let mut state = self.state();
        state.models.insert(agent, models.clone());
        AgentModelsReply {
            models: models.models,
            efforts: models.efforts,
            choice: state.resolved(),
        }
    }

    pub fn choice(&self) -> AgentChoice {
        self.state().resolved()
    }

    pub async fn usage(&self) -> Result<AgentUsageReply, String> {
        let catalog = self.known_catalog(false).await?;
        Ok(self.usage.refresh(&catalog, &self.shell, &self.home).await)
    }

    pub fn choose(&self, change: &AgentChoiceChange) {
        self.state().saved.apply(change);
    }

    pub async fn save(self: Arc<Self>) {
        if self.unsaved.swap(true, Ordering::AcqRel) {
            return;
        }
        let saved = tokio::task::spawn_blocking(move || {
            let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
            self.unsaved.store(false, Ordering::Release);
            let bytes = serde_json::to_vec(&self.state().saved)?;
            kodosi_os::fs::write_private(&self.file, &bytes)
        })
        .await
        .map_err(std::io::Error::from)
        .and_then(|saved| saved);
        if let Err(error) = saved {
            tracing::warn!(%error, "the launch choice was not saved");
        }
    }

    /// A model or effort that Kodosi cannot start, such as one that a person typed, stays the
    /// launcher's last choice.
    pub(crate) async fn start_like(&self, running: RunningAgent) -> AgentStart {
        let catalog = self.known_catalog(false).await.ok();
        let accounts = catalog
            .as_ref()
            .and_then(|catalog| agent_info(catalog, running.agent))
            .map_or(&[][..], |info| info.accounts.as_slice());
        let mut start = running.start(accounts, &self.home);
        let models = self.known_models(start.agent, start.model.as_deref()).await;
        let listed = |id: &str| models.models.iter().find(|model| model.id == id);
        if start
            .model
            .as_deref()
            .is_some_and(|id| !id.is_empty() && listed(id).is_none())
        {
            start.model = None;
        }
        let model = match start.model.as_deref() {
            Some(id) => listed(id),
            None => self
                .state()
                .saved
                .choice
                .pick(start.agent)
                .model
                .as_deref()
                .and_then(listed),
        };
        if start.effort.as_deref().is_some_and(|id| {
            !id.is_empty() && !efforts(&models, model).any(|effort| effort.id == id)
        }) {
            start.effort = None;
        }
        start
    }

    pub(crate) async fn line(self: &Arc<Self>, start: &AgentStart) -> Result<Launch, String> {
        let mut catalog = self.known_catalog(false).await?;
        let fresh = |catalog: &AgentCatalog| {
            agent_info(catalog, start.agent).is_some_and(|info| {
                info.installed
                    && start.account.as_ref().is_none_or(|name| {
                        info.accounts.iter().any(|account| &account.name == name)
                    })
            })
        };
        if !fresh(&catalog) {
            catalog = self.known_catalog(true).await?;
        }
        let info = agent_info(&catalog, start.agent)
            .ok_or_else(|| format!("{} is not known.", start.agent.label()))?;
        if !info.installed {
            return Err(format!(
                "{} is not installed. Install it with: {}",
                start.agent.label(),
                info.install
            ));
        }
        let saved = self.state().saved.choice.pick(start.agent);
        let wanted = |explicit: &Option<String>, saved: &Option<String>| {
            explicit
                .as_ref()
                .map_or_else(|| saved.is_some(), |value| !value.is_empty())
        };
        let models = if wanted(&start.model, &saved.model) || wanted(&start.effort, &saved.effort) {
            Some(self.known_models(start.agent, start.model.as_deref()).await)
        } else {
            None
        };
        let (line, pick) = plan(info, models.as_ref(), &saved, start, &self.shell)?;
        let account = info
            .accounts
            .iter()
            .find(|account| pick.account.as_ref() == Some(&account.name))
            .and_then(|account| account.folder_in(&self.home));
        let quiet_settings = account
            .clone()
            .zip(pick.output)
            .and_then(|(folder, output)| quiet::settings_file(start.agent, output, &folder));
        let copilot_home = (start.agent == Agent::Copilot)
            .then(|| account.unwrap_or_else(|| self.home.join(".copilot")));
        {
            let mut state = self.state();
            state.saved.apply(&AgentChoiceChange::Tab {
                agent: Some(start.agent),
            });
            state.saved.apply(&AgentChoiceChange::Pick(pick));
        }
        Arc::clone(self).save().await;
        Ok(Launch {
            line: start.agent.with_context(&line, &self.notes()),
            quiet_settings,
            copilot_home,
        })
    }

    pub(crate) fn quiet_settings_of(&self, terminal: Uuid) -> Option<PathBuf> {
        self.quiet.held_by(terminal)
    }

    pub(crate) async fn hold_quiet(self: &Arc<Self>, terminal: Uuid, settings: PathBuf) -> bool {
        let launcher = Arc::clone(self);
        tokio::task::spawn_blocking(move || launcher.quiet.hold(terminal, &settings))
            .await
            .unwrap_or(false)
    }

    pub(crate) async fn release_quiet_hold(self: &Arc<Self>, terminal: Uuid, settings: PathBuf) {
        let launcher = Arc::clone(self);
        drop(
            tokio::task::spawn_blocking(move || launcher.quiet.release_hold(terminal, &settings))
                .await,
        );
    }

    pub(crate) fn release_quiet(self: &Arc<Self>, terminal: Uuid) {
        if self.quiet.held_by(terminal).is_some() {
            let launcher = Arc::clone(self);
            drop(tokio::task::spawn_blocking(move || {
                launcher.quiet.release(terminal);
            }));
        }
    }

    pub(crate) async fn release_all_quiet(self: &Arc<Self>) {
        let launcher = Arc::clone(self);
        drop(tokio::task::spawn_blocking(move || launcher.quiet.release_all()).await);
    }

    pub(crate) async fn account_of(&self, agent: Agent, folder: &Path) -> Option<u8> {
        let mut place = None;
        for refresh in [false, true] {
            let catalog = self.known_catalog(refresh).await.ok()?;
            let accounts = &agent_info(&catalog, agent)?.accounts;
            place = Some(super::accounts::place(accounts, &self.home, folder));
            if place.is_some_and(|place| place < accounts.len()) {
                break;
            }
        }
        place.map(|place| u8::try_from(place).unwrap_or(u8::MAX))
    }

    async fn known_catalog(&self, refresh: bool) -> Result<AgentCatalog, String> {
        if !refresh
            && let Some((at, catalog)) = &self.state().catalog
            && at.elapsed() < CATALOG_LIFE
        {
            return Ok(catalog.clone());
        }
        self.catalog().await
    }

    async fn known_models(&self, agent: Agent, model: Option<&str>) -> AgentModels {
        if let Some(models) = self.state().models.get(&agent)
            && model.is_none_or(|id| id.is_empty() || models.models.iter().any(|m| m.id == id))
        {
            return models.clone();
        }
        let models = super::models(&self.shell, agent).await;
        self.state().models.insert(agent, models.clone());
        models
    }
}

impl AgentChoice {
    pub(crate) fn pick(&self, agent: Agent) -> AgentPick {
        self.picks
            .iter()
            .find(|pick| pick.agent == agent)
            .cloned()
            .unwrap_or(AgentPick {
                agent,
                account: None,
                model: None,
                effort: None,
                permissions: None,
                output: None,
                fast: false,
                chat: false,
            })
    }

    pub(crate) fn apply(&mut self, change: &AgentChoiceChange) {
        match change {
            AgentChoiceChange::Tab { agent } => self.agent = *agent,
            AgentChoiceChange::Pick(pick) => self.set(pick.clone()),
        }
    }

    fn set(&mut self, pick: AgentPick) {
        match self
            .picks
            .iter_mut()
            .find(|saved| saved.agent == pick.agent)
        {
            Some(saved) => *saved = pick,
            None => self.picks.push(pick),
        }
    }
}

impl State {
    fn resolved(&self) -> AgentChoice {
        AgentChoice {
            agent: self.saved.choice.agent.or_else(|| self.first_agent()),
            picks: Agent::ALL
                .into_iter()
                .map(|agent| self.resolved_pick(agent))
                .collect(),
        }
    }

    fn first_agent(&self) -> Option<Agent> {
        let (_, catalog) = self
            .catalog
            .as_ref()
            .filter(|_| !self.saved.plain_terminal)?;
        Agent::ALL
            .into_iter()
            .find(|agent| agent_info(catalog, *agent).is_some_and(|info| info.installed))
    }

    fn resolved_pick(&self, agent: Agent) -> AgentPick {
        let mut saved = self.saved.choice.pick(agent);
        saved.permissions = Some(permissions_of(agent, saved.permissions));
        saved.output = Some(output_of(agent, saved.output));
        saved.fast = fast_of(agent, saved.fast);
        let account = match self
            .catalog
            .as_ref()
            .and_then(|(_, catalog)| agent_info(catalog, agent))
        {
            Some(info) => saved
                .account
                .as_ref()
                .and_then(|name| info.accounts.iter().find(|account| &account.name == name))
                .or_else(|| info.accounts.first())
                .map(|account| account.name.clone()),
            None => saved.account.clone(),
        };
        let Some(models) = self.models.get(&agent) else {
            return AgentPick { account, ..saved };
        };
        let model = saved
            .model
            .as_deref()
            .and_then(|id| models.models.iter().find(|model| model.id == id));
        let effort = saved
            .effort
            .as_deref()
            .and_then(|id| efforts(models, model).find(|effort| effort.id == id));
        AgentPick {
            agent,
            account,
            model: model.map(|model| model.id.clone()),
            effort: effort.map(|effort| effort.id.clone()),
            ..saved
        }
    }
}

fn permissions_of(agent: Agent, saved: Option<Permissions>) -> Permissions {
    saved
        .filter(|permissions| agent.permission_words(*permissions).is_some())
        .unwrap_or_else(|| agent.default_permissions())
}

fn output_of(agent: Agent, saved: Option<Output>) -> Output {
    saved
        .filter(|output| agent.output_words(*output).is_some())
        .unwrap_or_else(|| agent.default_output())
}

const fn fast_of(agent: Agent, saved: bool) -> bool {
    saved && agent.has_fast()
}

fn agent_info(catalog: &AgentCatalog, agent: Agent) -> Option<&AgentInfo> {
    catalog.agents.iter().find(|info| info.agent == agent)
}

fn efforts<'a>(
    models: &'a AgentModels,
    model: Option<&'a AgentModel>,
) -> impl Iterator<Item = &'a AgentOption> {
    let allowed = model
        .map(|model| &model.efforts)
        .filter(|ids| !ids.is_empty());
    models
        .efforts
        .iter()
        .filter(move |effort| allowed.is_none_or(|ids| ids.contains(&effort.id)))
}

fn listed<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let names = items.collect::<Vec<_>>();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

pub(super) fn plan(
    info: &AgentInfo,
    models: Option<&AgentModels>,
    saved: &AgentPick,
    start: &AgentStart,
    shell: &Path,
) -> Result<(String, AgentPick), String> {
    let label = info.agent.label();
    let account = match start.account.as_deref() {
        Some(name) => info
            .accounts
            .iter()
            .find(|account| account.name == name)
            .ok_or_else(|| {
                format!(
                    "{label} has no account named {name}. Accounts: {}.",
                    listed(info.accounts.iter().map(|account| account.name.as_str()))
                )
            })?,
        None => saved
            .account
            .as_deref()
            .and_then(|name| info.accounts.iter().find(|account| account.name == name))
            .or_else(|| info.accounts.first())
            .ok_or_else(|| format!("{label} has no account."))?,
    };
    let empty = AgentModels {
        models: Vec::new(),
        efforts: Vec::new(),
    };
    let models = models.unwrap_or(&empty);
    let model = match start.model.as_deref() {
        Some("") => None,
        Some(id) => Some(
            models
                .models
                .iter()
                .find(|model| model.id == id)
                .ok_or_else(|| {
                    format!(
                        "{label} has no model {id}. Models: {}.",
                        listed(models.models.iter().map(|model| model.id.as_str()))
                    )
                })?,
        ),
        None => saved
            .model
            .as_deref()
            .and_then(|id| models.models.iter().find(|model| model.id == id)),
    };
    let effort = match start.effort.as_deref() {
        Some("") => None,
        Some(id) => Some(
            efforts(models, model)
                .find(|effort| effort.id == id)
                .ok_or_else(|| {
                    format!(
                        "{label} has no effort {id} here. Efforts: {}.",
                        listed(efforts(models, model).map(|effort| effort.id.as_str()))
                    )
                })?,
        ),
        None => saved
            .effort
            .as_deref()
            .and_then(|id| efforts(models, model).find(|effort| effort.id == id)),
    };
    let chose = modes(info.agent, saved, start)?;
    let chat = info.chat && start.chat.unwrap_or(saved.chat);
    let chosen = [
        model.map(|model| model.argument.clone()),
        effort.map(|effort| effort.argument.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    Ok((
        start_line(info.agent, &account.command, chosen, &chose, chat, shell),
        AgentPick {
            agent: info.agent,
            account: Some(account.name.clone()),
            model: model.map(|model| model.id.clone()),
            effort: effort.map(|effort| effort.id.clone()),
            permissions: Some(chose.permissions),
            output: Some(chose.output),
            // Fast mode costs more, so each start chooses it again.
            fast: false,
            chat,
        },
    ))
}

/// In the chat, Kodosi's `acp` gives the session the choices of the line, where the empty mode is
/// the agent's mode that asks, and only the agent's server takes its own words for no
/// permissions. The terminal app's output has no chat.
fn start_line(
    agent: Agent,
    command: &str,
    chosen: Vec<String>,
    chose: &Modes,
    chat: bool,
    shell: &Path,
) -> String {
    let quoted = |words: &[&str]| {
        words
            .iter()
            .map(|word| as_shell_word(shell, word))
            .collect::<Vec<_>>()
    };
    let words = if chat {
        let (session, server): (&[&str], &[&str]) = match chose.permissions {
            Permissions::Ask => (&["--mode="], &[]),
            Permissions::Auto => (&["--mode", "auto"], &[]),
            Permissions::Unrestricted => (&[], chose.permission_words),
        };
        std::iter::once("kodosi acp".to_owned())
            .chain(chosen)
            .chain(quoted(session))
            .chain(["--".to_owned(), command.to_owned(), "acp".to_owned()])
            .chain(quoted(server))
            .collect::<Vec<_>>()
    } else {
        let modes = chose
            .permission_words
            .iter()
            .chain(chose.output_words)
            .chain(chose.fast_words)
            .map(|word| (*word).to_owned())
            .collect::<Vec<_>>();
        let modes = if agent == Agent::Antigravity {
            super::one_settings(modes)
        } else {
            modes
        };
        std::iter::once(command.to_owned())
            .chain(chosen)
            .chain(modes.iter().map(|word| as_shell_word(shell, word)))
            .collect::<Vec<_>>()
    };
    words.join(" ")
}

/// The modes of a start, and the agent's words for each.
struct Modes {
    permissions: Permissions,
    output: Output,
    permission_words: &'static [&'static str],
    output_words: &'static [&'static str],
    fast_words: &'static [&'static str],
}

fn modes(agent: Agent, saved: &AgentPick, start: &AgentStart) -> Result<Modes, String> {
    let label = agent.label();
    let permissions = start
        .permissions
        .unwrap_or_else(|| permissions_of(agent, saved.permissions));
    let permission_words = agent.permission_words(permissions).ok_or_else(|| {
        format!(
            "{label} has no permissions {}. Permissions: {}.",
            permissions.name(),
            listed(agent.permissions().into_iter().map(Permissions::name))
        )
    })?;
    let output = start
        .output
        .unwrap_or_else(|| output_of(agent, saved.output));
    let output_words = agent.output_words(output).ok_or_else(|| {
        format!(
            "{label} has no output {}. Outputs: {}.",
            output.name(),
            listed(agent.outputs().into_iter().map(Output::name))
        )
    })?;
    let fast = start.fast.unwrap_or_else(|| fast_of(agent, saved.fast));
    // An agent keeps a fast mode that it had turned on, so a start without it says so.
    let fast_words = match agent.fast_words(fast) {
        Some(words) => words,
        None if fast => return Err(format!("{label} has no fast mode.")),
        None => &[],
    };
    Ok(Modes {
        permissions,
        output,
        permission_words,
        output_words,
        fast_words,
    })
}
