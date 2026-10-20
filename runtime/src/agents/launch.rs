use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use super::{
    Agent, AgentAccount, AgentCatalog, AgentChoice, AgentChoiceChange, AgentInfo, AgentModel,
    AgentModels, AgentModelsReply, AgentOption, AgentPick, AgentStart,
};

const FILE: &str = "launcher.json";
const CATALOG_LIFE: Duration = Duration::from_secs(30);

pub struct Launcher {
    file: PathBuf,
    home: PathBuf,
    shell: PathBuf,
    state: Mutex<State>,
    writing: Mutex<()>,
    unsaved: AtomicBool,
}

#[derive(Default)]
struct State {
    saved: AgentChoice,
    catalog: Option<(Instant, AgentCatalog)>,
    models: HashMap<Agent, AgentModels>,
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
            state: Mutex::new(State {
                saved,
                ..State::default()
            }),
            writing: Mutex::new(()),
            unsaved: AtomicBool::new(false),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn catalog(&self) -> Result<AgentCatalog, String> {
        let catalog = super::catalog(&self.home, &self.shell).await?;
        let mut state = self.state();
        state.catalog = Some((Instant::now(), catalog.clone()));
        Ok(AgentCatalog {
            choice: state.resolved(),
            ..catalog
        })
    }

    pub async fn add_account(&self, agent: Agent, name: &str) -> Result<AgentAccount, String> {
        let catalog = self.catalog().await?;
        let taken = agent_info(&catalog, agent)
            .map(|info| {
                info.accounts
                    .iter()
                    .map(|account| account.name.clone())
                    .collect()
            })
            .unwrap_or_default();
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

    pub async fn line(self: &Arc<Self>, start: &AgentStart) -> Result<String, String> {
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
        let saved = self.state().saved.pick(start.agent);
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
        let (line, pick) = plan(info, models.as_ref(), &saved, start)?;
        {
            let mut state = self.state();
            state.saved.agent = Some(start.agent);
            state.saved.set(pick);
        }
        Arc::clone(self).save().await;
        Ok(line)
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
            agent: self.saved.agent,
            picks: Agent::ALL
                .into_iter()
                .map(|agent| self.resolved_pick(agent))
                .collect(),
        }
    }

    fn resolved_pick(&self, agent: Agent) -> AgentPick {
        let saved = self.saved.pick(agent);
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
            None => saved.account,
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
        }
    }
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
    let line = [
        Some(account.command.as_str()),
        model.map(|model| model.argument.as_str()),
        effort.map(|effort| effort.argument.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    Ok((
        line,
        AgentPick {
            agent: info.agent,
            account: Some(account.name.clone()),
            model: model.map(|model| model.id.clone()),
            effort: effort.map(|effort| effort.id.clone()),
        },
    ))
}
