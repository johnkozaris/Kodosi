use std::{
    collections::HashSet,
    io::Read as _,
    path::{Component, Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;

use super::{Agent, AgentAccount, AgentCatalog, AgentInfo, shell::Facts, shell_word};

const MAX_ACCOUNT_FILE: u64 = 8 * 1024 * 1024;
const MAX_HOME_ENTRIES: usize = 8_192;

pub(super) fn catalog(home: &Path, facts: &Facts) -> AgentCatalog {
    AgentCatalog {
        agents: Agent::ALL
            .into_iter()
            .map(|agent| {
                let installed =
                    facts.executable(agent.program()).is_some() || facts.defines(agent.program());
                AgentInfo {
                    agent,
                    program: agent.program().to_owned(),
                    installed,
                    accounts: if installed {
                        accounts(home, facts, agent)
                    } else {
                        Vec::new()
                    },
                    install: install(agent).to_owned(),
                    adds_accounts: cfg!(unix) && agent.home().is_some(),
                }
            })
            .collect(),
    }
}

/// The usual account, then an account for each alias or function that moves
/// the agent's sign-in folder, then each sibling of the usual folder that the
/// agent has used or that is still empty.
fn accounts(home: &Path, facts: &Facts, agent: Agent) -> Vec<AgentAccount> {
    let program = agent.program();
    let Some((folder, variable)) = agent.home() else {
        return vec![account(program, program, None)];
    };
    let usual = home.join(folder);
    let mut accounts = vec![account(program, program, identity(agent, home, &usual))];
    let mut homes = HashSet::from([usual]);
    let mut definitions = facts
        .definitions
        .iter()
        .filter(|definition| definition.name != program && definition.name.starts_with(program))
        .collect::<Vec<_>>();
    definitions
        .sort_by(|left, right| (left.name.len(), &left.name).cmp(&(right.name.len(), &right.name)));
    for definition in definitions {
        if let Some(path) = referenced_home(&definition.text, home, folder, variable)
            && homes.insert(path.clone())
        {
            accounts.push(account(
                &definition.name,
                &definition.name,
                identity(agent, home, &path),
            ));
        }
    }
    for (name, path) in sibling_homes(home, folder) {
        if homes.contains(&path) {
            continue;
        }
        if !is_empty(&path)
            && !records(agent)
                .iter()
                .any(|record| path.join(record).is_file())
        {
            continue;
        }
        if let Some(command) = home_command(variable, &path, program) {
            accounts.push(account(&name, &command, identity(agent, home, &path)));
            homes.insert(path);
        }
    }
    accounts
}

fn account(name: &str, command: &str, identity: Option<String>) -> AgentAccount {
    AgentAccount {
        name: name.to_owned(),
        command: command.to_owned(),
        identity,
    }
}

fn home_command(variable: &str, path: &Path, program: &str) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let assignment = shell_word(&format!("{variable}={}", path.to_str()?))?;
    Some(format!("env {assignment} {program}"))
}

/// The sign-in folder that a definition gives the agent: the value it sets
/// for the agent's variable, or a sibling of the usual folder that it names.
pub(super) fn referenced_home(
    text: &str,
    home: &Path,
    folder: &str,
    variable: &str,
) -> Option<PathBuf> {
    let key = format!("{variable}=");
    text.match_indices(&key)
        .find_map(|(index, _)| {
            let value = &text[index + key.len()..];
            let value = match text[..index].chars().next_back() {
                Some(quote @ ('"' | '\'')) => value.split(quote).next(),
                _ => word(value),
            };
            expand(value?, home)
        })
        .or_else(|| {
            let prefixes = home
                .to_str()
                .map(|home| format!("{home}/"))
                .into_iter()
                .chain(["$HOME/", "${HOME}/", "~/"].map(str::to_owned))
                .collect::<Vec<_>>();
            text.match_indices(folder).find_map(|(index, _)| {
                if !prefixes
                    .iter()
                    .any(|prefix| text[..index].ends_with(prefix.as_str()))
                {
                    return None;
                }
                let rest = &text[index + folder.len()..];
                let separator = rest.chars().next().filter(|c| matches!(c, '-' | '_'))?;
                let suffix = rest[1..]
                    .split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
                    .next()?
                    .trim_end_matches('.');
                (!suffix.is_empty()).then(|| home.join(format!("{folder}{separator}{suffix}")))
            })
        })
}

fn word(text: &str) -> Option<&str> {
    let value = match text.chars().next()? {
        quote @ ('"' | '\'') => text[1..].split(quote).next()?,
        _ => text
            .split(|c: char| c.is_whitespace() || matches!(c, ';' | ')' | '&' | '|'))
            .next()?,
    };
    (!value.is_empty()).then_some(value)
}

fn expand(value: &str, home: &Path) -> Option<PathBuf> {
    let path = if let Some(rest) = ["~/", "$HOME/", "${HOME}/"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
    {
        home.join(rest)
    } else if value.starts_with('/') {
        PathBuf::from(value)
    } else {
        return None;
    };
    (!path.to_str()?.contains('$')
        && path
            .components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_))))
    .then_some(path)
}

/// Folders such as `~/.claude-work` or `~/.codex_personal`, by name.
fn sibling_homes(home: &Path, folder: &str) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(home) else {
        return Vec::new();
    };
    let mut homes = entries
        .take(MAX_HOME_ENTRIES)
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let suffix = name
                .strip_prefix(folder)?
                .strip_prefix(['-', '_'])
                .filter(|suffix| {
                    !suffix.is_empty()
                        && suffix.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                        })
                })?
                .to_owned();
            entry.path().is_dir().then(|| (suffix, home.join(&name)))
        })
        .collect::<Vec<_>>();
    homes.sort();
    homes
}

/// Files that the agent writes in its sign-in folder when it first starts.
const fn records(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &[".claude.json"],
        Agent::Codex => &["auth.json", "config.toml", "installation_id"],
        Agent::Copilot | Agent::Cursor | Agent::Gemini | Agent::Opencode => &["config.json"],
    }
}

fn is_empty(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

/// Who signed in to the agent in sign-in folder `path`, as the agent records it.
pub(super) fn identity(agent: Agent, home: &Path, path: &Path) -> Option<String> {
    let identity = match agent {
        Agent::Claude => {
            let file = if path == home.join(".claude") {
                home.join(".claude.json")
            } else {
                path.join(".claude.json")
            };
            let value: Value = serde_json::from_str(&read(&file)?).ok()?;
            value
                .pointer("/oauthAccount/emailAddress")?
                .as_str()?
                .to_owned()
        }
        Agent::Codex => {
            let value: Value = serde_json::from_str(&read(&path.join("auth.json"))?).ok()?;
            let token = value.pointer("/tokens/id_token")?.as_str()?;
            let claims = URL_SAFE_NO_PAD
                .decode(token.split('.').nth(1)?.trim_end_matches('='))
                .ok()?;
            let claims: Value = serde_json::from_slice(&claims).ok()?;
            claims.get("email")?.as_str()?.to_owned()
        }
        Agent::Copilot => {
            let text = read(&path.join("config.json"))?;
            let json = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            let value: Value = serde_json::from_str(&json).ok()?;
            value
                .pointer("/lastLoggedInUser/login")?
                .as_str()?
                .to_owned()
        }
        Agent::Cursor | Agent::Gemini | Agent::Opencode => return None,
    };
    (!identity.is_empty() && identity.len() <= 256 && !identity.chars().any(char::is_control))
        .then_some(identity)
}

fn read(path: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_ACCOUNT_FILE)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

const fn install(agent: Agent) -> &'static str {
    if cfg!(windows) {
        return match agent {
            Agent::Claude => "irm https://claude.ai/install.ps1 | iex",
            Agent::Codex => "irm https://chatgpt.com/codex/install.ps1 | iex",
            Agent::Copilot => "winget install GitHub.Copilot",
            Agent::Cursor => "irm 'https://cursor.com/install?win32=true' | iex",
            Agent::Gemini => "npm install -g @google/gemini-cli",
            Agent::Opencode => "npm install -g opencode-ai",
        };
    }
    match agent {
        Agent::Claude => "curl -fsSL https://claude.ai/install.sh | bash",
        Agent::Codex => "curl -fsSL https://chatgpt.com/codex/install.sh | sh",
        Agent::Copilot => "curl -fsSL https://gh.io/copilot-install | bash",
        Agent::Cursor => "curl https://cursor.com/install -fsS | bash",
        Agent::Gemini => "npm install -g @google/gemini-cli",
        Agent::Opencode => "curl -fsSL https://opencode.ai/install | bash",
    }
}

/// Makes `~/.claude-<name>` (or the folder of another agent) for an account
/// that signs in on its first start.
pub(super) fn add(home: &Path, agent: Agent, name: &str) -> Result<AgentAccount, String> {
    let (folder, variable) = agent
        .home()
        .filter(|_| cfg!(unix))
        .ok_or_else(|| "Kodosi cannot add accounts for this agent.".to_owned())?;
    let name = name
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if name.is_empty()
        || name.len() > 32
        || name.starts_with('-')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("Name the account with letters, digits and hyphens.".to_owned());
    }
    let path = home.join(format!("{folder}-{name}"));
    create_folder(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!("There is already an account named {name}.")
        } else {
            format!("Kodosi could not make {}: {error}", path.display())
        }
    })?;
    let command = home_command(variable, &path, agent.program())
        .ok_or_else(|| "Kodosi cannot start an agent from this folder.".to_owned())?;
    Ok(account(&name, &command, None))
}

#[cfg(unix)]
fn create_folder(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new().mode(0o700).create(path)
}

#[cfg(windows)]
fn create_folder(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir(path)
}
