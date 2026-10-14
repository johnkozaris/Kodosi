use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read as _,
    path::{Component, Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::Value;

use super::{
    Agent, AgentAccount, AgentCatalog, AgentInfo,
    learned::Learned,
    shell::{Definition, Facts},
    shell_word,
};

const MAX_ACCOUNT_FILE: u64 = 8 * 1024 * 1024;
const MAX_HOME_ENTRIES: usize = 8_192;
const MAX_SCRIPT: u64 = 64 * 1024;
const MAX_OPTIONS: usize = 80;
/// The words with which a function or a script hands on the words after it.
const PASSED_WORDS: [&str; 4] = ["$@", "${@}", "$*", "$argv"];

pub(super) fn catalog(home: &Path, facts: &Facts, learned: &Learned) -> AgentCatalog {
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
                        accounts(home, facts, agent, learned.get(&agent))
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

/// The usual account, then each alias or function that starts the agent, then
/// each sibling of the usual folder that the agent has used or that is still
/// empty. Each names its sign-in folder when Kodosi knows it, so the commands
/// that share a sign-in show together. A command that started the agent in a
/// terminal before has the folder that the agent got then, in `learned`.
fn accounts(
    home: &Path,
    facts: &Facts,
    agent: Agent,
    learned: Option<&BTreeMap<String, PathBuf>>,
) -> Vec<AgentAccount> {
    let program = agent.program();
    let usual = agent.home().map(|(folder, _)| home.join(folder));
    let mut accounts = vec![account(
        agent,
        home,
        program,
        program,
        usual.as_deref(),
        None,
    )];
    let mut homes = usual.iter().cloned().collect::<HashSet<_>>();
    let mut definitions = facts
        .definitions
        .iter()
        .filter(|definition| {
            definition.name != program
                && !definition.name.starts_with('_')
                && !gone(definition, home)
        })
        .collect::<Vec<_>>();
    definitions
        .sort_by(|left, right| (left.name.len(), &left.name).cmp(&(right.name.len(), &right.name)));
    // The commands found so far, with their sign-in folders. An alias of one of
    // them starts the agent too, so it is found on a later pass.
    let mut found = HashMap::new();
    for _ in 0..3 {
        let before = found.len();
        for definition in &definitions {
            if found.contains_key(definition.name.as_str()) {
                continue;
            }
            let seen = learned
                .and_then(|learned| learned.get(&definition.name))
                .filter(|path| path.is_dir());
            let Some(Launch { folder: path }) =
                launch(definition, agent, home, usual.as_deref(), &found, seen)
            else {
                continue;
            };
            homes.extend(path.clone());
            accounts.push(account(
                agent,
                home,
                &definition.name,
                &definition.name,
                path.as_deref(),
                options(definition, program),
            ));
            found.insert(definition.name.as_str(), path);
        }
        if found.len() == before {
            break;
        }
    }
    let Some((folder, variable)) = agent.home() else {
        return accounts;
    };
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
            accounts.push(account(agent, home, &name, &command, Some(&path), None));
            homes.insert(path);
        }
    }
    accounts
}

/// A definition that starts the agent, with the sign-in folder that it gives
/// the agent; none when Kodosi cannot read the folder from the definition.
struct Launch {
    folder: Option<PathBuf>,
}

/// Whether `definition` starts the agent, and with which sign-in folder: the
/// one that it names, the usual one when it runs the program and leaves the
/// folder alone, or that of a command in `found` that it runs. The folder that
/// the agent got when the definition started it, `seen`, comes first. A
/// definition that would drop the words after it does not count, because the
/// launcher puts the model, the effort and the first prompt there.
fn launch(
    definition: &Definition,
    agent: Agent,
    home: &Path,
    usual: Option<&Path>,
    found: &HashMap<&str, Option<PathBuf>>,
    seen: Option<&PathBuf>,
) -> Option<Launch> {
    let program = agent.program();
    let script = script(definition, home);
    let passes = |text: &str| PASSED_WORDS.iter().any(|words| text.contains(words));
    if !(if definition.alias {
        script.as_deref().is_none_or(passes)
    } else {
        passes(&definition.text)
    }) {
        return None;
    }
    if let Some(path) = seen {
        return Some(Launch {
            folder: Some(path.clone()),
        });
    }
    let texts = [Some(definition.text.as_str()), script.as_deref()];
    let texts = texts.iter().flatten();
    let runs = runs(definition, script.as_deref(), program);
    if let Some((folder, variable)) = agent.home()
        && let Some(path) = texts
            .clone()
            .find_map(|text| referenced_home(text, home, folder, variable))
    {
        return (definition.name.starts_with(program) || runs)
            .then_some(Launch { folder: Some(path) });
    }
    if definition.alias
        && let Some(path) =
            command_word(&definition.text).and_then(|command| found.get(command.as_str()))
    {
        return Some(Launch {
            folder: path.clone(),
        });
    }
    if !runs {
        return None;
    }
    let moves = agent.home().is_some_and(|(_, variable)| {
        texts
            .clone()
            .any(|text| text.contains(&format!("{variable}=")))
    });
    Some(Launch {
        folder: usual.filter(|_| !moves).map(Path::to_path_buf),
    })
}

/// Whether `definition` runs `program` itself: an alias whose command is the
/// program or a `script` that runs it, or a function named after the program
/// that runs it.
fn runs(definition: &Definition, script: Option<&str>, program: &str) -> bool {
    let is_program = |word: &str| word == program || word.ends_with(&format!("/{program}"));
    if definition.alias {
        return command_word(&definition.text).is_some_and(|word| is_program(&word))
            || script.is_some_and(|text| {
                text.lines()
                    .filter_map(command_word)
                    .any(|word| is_program(&word))
            });
    }
    definition.name.starts_with(program)
        && definition
            .text
            .split(|c: char| c.is_whitespace() || "\"';&|(){}".contains(c))
            .any(is_program)
}

/// The options that `definition` puts before the words after it, such as
/// `--yolo`: for an alias, the words after its command; for a function, the
/// words after the program up to the words that it hands on.
fn options(definition: &Definition, program: &str) -> Option<String> {
    let mut words = definition
        .text
        .split_whitespace()
        .map(|word| word.trim_matches(['"', '\'']));
    let command = if definition.alias {
        command_word(&definition.text)?
    } else {
        program.to_owned()
    };
    words.find(|word| *word == command || word.ends_with(&format!("/{command}")))?;
    let options = words
        .take_while(|word| {
            !word.is_empty()
                && !word.starts_with('$')
                && !word.contains([';', '|', '&', '}', '<', '>'])
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!options.is_empty() && options.len() <= MAX_OPTIONS).then_some(options)
}

/// Whether `definition` is an alias of a file that is not there.
fn gone(definition: &Definition, home: &Path) -> bool {
    definition.alias
        && command_word(&definition.text).is_some_and(|command| {
            command.contains('/') && !expand(&command, home).is_some_and(|path| path.is_file())
        })
}

/// The program that an alias runs: its first word after the variables that
/// it sets and after `env`, `command`, `exec` and `noglob`.
fn command_word(text: &str) -> Option<String> {
    let mut words = text
        .split_whitespace()
        .map(|word| word.trim_matches(['"', '\'']));
    let mut env = false;
    while let Some(word) = words.next() {
        let assignment = word.split_once('=').is_some_and(|(name, _)| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        });
        match word {
            _ if assignment => {}
            "env" | "command" | "exec" | "noglob" | "nocorrect" | "builtin" => env = word == "env",
            "-u" if env => drop(words.next()),
            _ if env && word.starts_with('-') => {}
            _ => return Some(word.to_owned()),
        }
    }
    None
}

/// The text of the script that an alias runs, when the alias runs a short
/// script of the person's own.
fn script(definition: &Definition, home: &Path) -> Option<String> {
    if !definition.alias {
        return None;
    }
    let path = expand(&command_word(&definition.text)?, home)?;
    if std::fs::metadata(&path).ok()?.len() > MAX_SCRIPT {
        return None;
    }
    read(&path).filter(|text| text.starts_with("#!"))
}

fn account(
    agent: Agent,
    home: &Path,
    name: &str,
    command: &str,
    folder: Option<&Path>,
    options: Option<String>,
) -> AgentAccount {
    AgentAccount {
        name: name.to_owned(),
        command: command.to_owned(),
        identity: folder.and_then(|folder| identity(agent, home, folder)),
        folder: folder.map(|folder| tilde(folder, home)),
        options,
    }
}

/// `path` with `~` for the home folder.
fn tilde(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
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
    Ok(account(agent, home, &name, &command, Some(&path), None))
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
