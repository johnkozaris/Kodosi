use std::{
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
        kodosi: facts.executable("kodosi").map(|path| tilde(&path, home)),
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
                    prompt_flag: agent.prompt_flag().map(str::to_owned),
                }
            })
            .collect(),
    }
}

fn accounts(home: &Path, facts: &Facts, agent: Agent) -> Vec<AgentAccount> {
    let program = agent.program();
    let Some((usual, variable)) = agent.home() else {
        return vec![AgentAccount {
            name: program.to_owned(),
            command: program.to_owned(),
            identity: None,
            folder: None,
        }];
    };
    let chosen = facts
        .variable(variable)
        .and_then(|value| expand(value, home))
        .unwrap_or_else(|| home.join(usual));
    let mut others = sibling_homes(home, agent, usual);
    others.extend(
        facts
            .definitions
            .iter()
            .filter_map(|definition| referenced_home(&definition.text, home, variable))
            .filter(|path| path.is_dir())
            .map(|path| (folder_name(&path, usual, program), path)),
    );
    let mut folders = vec![chosen.clone()];
    let mut accounts = Vec::new();
    for (name, path) in others {
        if folders.contains(&path) {
            continue;
        }
        if let Some(command) = home_command(variable, &path, program) {
            accounts.push(account(agent, home, &name, &command, &path));
            folders.push(path);
        }
    }
    accounts.sort_by(|left, right| left.name.cmp(&right.name));
    accounts.insert(0, account(agent, home, "default", program, &chosen));
    accounts
}

fn account(agent: Agent, home: &Path, name: &str, command: &str, folder: &Path) -> AgentAccount {
    AgentAccount {
        name: name.to_owned(),
        command: command.to_owned(),
        identity: identity(agent, home, folder),
        folder: Some(tilde(folder, home)),
    }
}

fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// The name that sets a sign-in folder apart; a folder named like the agent, such as
/// `~/work/.claude`, takes the name of the folder that holds it.
fn folder_name(path: &Path, usual: &str, program: &str) -> String {
    let short = |path: &Path| {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        name.strip_prefix(usual)
            .and_then(|rest| rest.strip_prefix(['-', '_']))
            .filter(|rest| !rest.is_empty())
            .unwrap_or_else(|| name.trim_start_matches('.'))
            .to_owned()
    };
    let name = short(path);
    if name.is_empty() || name == program || name == "default" {
        return path
            .parent()
            .map(short)
            .filter(|parent| !parent.is_empty())
            .unwrap_or(name);
    }
    name
}

fn home_command(variable: &str, path: &Path, program: &str) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let assignment = shell_word(&format!("{variable}={}", path.to_str()?))?;
    Some(format!("env {assignment} {program}"))
}

pub(super) fn referenced_home(text: &str, home: &Path, variable: &str) -> Option<PathBuf> {
    let key = format!("{variable}=");
    let names_another = |index: usize| {
        text[..index]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || matches!(before, '_' | '$'))
    };
    text.match_indices(&key).find_map(|(index, _)| {
        if names_another(index) {
            return None;
        }
        let value = &text[index + key.len()..];
        let value = match text[..index].chars().next_back() {
            Some(quote @ ('"' | '\'')) => value.split(quote).next(),
            _ => word(value),
        };
        expand(value?, home)
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

/// Other tools keep their own folders beside the agent's, such as `~/.claude-squad`.
fn sibling_homes(home: &Path, agent: Agent, folder: &str) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(home) else {
        return Vec::new();
    };
    entries
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
            let path = home.join(&name);
            (path.is_dir()
                && (is_empty(&path)
                    || records(agent)
                        .iter()
                        .any(|record| path.join(record).is_file())))
            .then_some((suffix, path))
        })
        .collect()
}

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
    Ok(account(agent, home, &name, &command, &path))
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
