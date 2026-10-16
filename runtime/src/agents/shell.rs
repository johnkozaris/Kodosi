use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

use super::Agent;

#[cfg(unix)]
const START: &str = "\u{1e}kodosi-agents";
#[cfg(unix)]
const END: &str = "\u{1e}end";
#[cfg(unix)]
const PROBE_TIME: Duration = Duration::from_secs(6);
const REUSE_TIME: Duration = Duration::from_secs(600);
#[cfg(unix)]
const MAX_OUTPUT: usize = 512 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Facts {
    pub path: Option<OsString>,
    pub variables: Vec<(String, String)>,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Definition {
    pub name: String,
    pub text: String,
}

impl Facts {
    pub(super) fn executable(&self, program: &str) -> Option<PathBuf> {
        self.path.as_ref().map_or_else(
            || kodosi_os::process::find_executable(program),
            |path| kodosi_os::process::find_executable_in(program, path),
        )
    }

    pub(super) fn defines(&self, name: &str) -> bool {
        self.definitions
            .iter()
            .any(|definition| definition.name == name)
    }

    pub(super) fn variable(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|(variable, _)| variable == name)
            .map(|(_, value)| value.as_str())
    }
}

struct Remembered {
    shell: PathBuf,
    at: Instant,
    facts: Facts,
}

static REMEMBERED: Mutex<Option<Remembered>> = Mutex::new(None);

pub(super) async fn facts(shell: &Path, fresh: bool) -> Facts {
    if !fresh
        && let Some(remembered) = REMEMBERED
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|remembered| remembered.shell == shell && remembered.at.elapsed() < REUSE_TIME)
    {
        return remembered.facts.clone();
    }
    let facts = probe(shell).await;
    *REMEMBERED.lock().unwrap_or_else(PoisonError::into_inner) = Some(Remembered {
        shell: shell.to_owned(),
        at: Instant::now(),
        facts: facts.clone(),
    });
    facts
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Family {
    Zsh,
    Bash,
    Fish,
    Posix,
}

#[cfg(unix)]
pub(super) fn family(shell: &Path) -> Option<Family> {
    match shell.file_stem()?.to_str()? {
        "zsh" => Some(Family::Zsh),
        "bash" => Some(Family::Bash),
        "fish" => Some(Family::Fish),
        "sh" | "dash" | "ash" | "ksh" | "mksh" => Some(Family::Posix),
        _ => None,
    }
}

#[cfg(unix)]
fn variables() -> impl Iterator<Item = &'static str> {
    Agent::ALL
        .into_iter()
        .filter_map(|agent| agent.home().map(|(_, variable)| variable))
}

#[cfg(unix)]
pub(super) fn script(family: Family) -> String {
    let names = Agent::ALL.map(Agent::program).join("|");
    let assignments = variables()
        .map(|variable| format!("*{variable}=*"))
        .collect::<Vec<_>>()
        .join("|");
    let report =
        |line: &dyn Fn(&str) -> String| variables().map(line).collect::<Vec<_>>().join("\n");
    match family {
        Family::Zsh => format!(
            r#"print -r -- $'\x1e'kodosi-agents
print -r -- "path"$'\t'"$PATH"
{variables}
for n in ${{(k)aliases}}; do if [[ $n == ({names}) || ${{aliases[$n]}} == ({assignments}) ]]; then print -r -- "alias"$'\t'"$n"$'\t'"${{aliases[$n]//$'\n'/ }}"; fi; done
for n in ${{(k)functions}}; do if [[ $n == ({names}) || ${{functions[$n]}} == ({assignments}) ]]; then print -r -- "function"$'\t'"$n"$'\t'"${{functions[$n]//$'\n'/ }}"; fi; done
print -r -- $'\x1e'end"#,
            variables = report(&|variable| format!(
                r#"v=$(printenv {variable}) && [[ -n $v ]] && print -r -- "variable"$'\t'"{variable}"$'\t'"$v""#
            )),
        ),
        Family::Bash => format!(
            r#"printf '\036kodosi-agents\n'
printf 'path\t%s\n' "$PATH"
{variables}
for n in $(compgen -a); do v=$(alias "$n"); case "$n" in {names}) ;; *) case "$v" in {assignments}) ;; *) continue;; esac;; esac; printf 'alias\t%s\t%s\n' "$n" "${{v//$'\n'/ }}"; done
for n in $(compgen -A function); do v=$(declare -f "$n"); case "$n" in {names}) ;; *) case "$v" in {assignments}) ;; *) continue;; esac;; esac; printf 'function\t%s\t%s\n' "$n" "${{v//$'\n'/ }}"; done
printf '\036end\n'"#,
            variables = report(&|variable| format!(
                r#"v=$(printenv {variable}) && [ -n "$v" ] && printf 'variable\t%s\t%s\n' {variable} "$v""#
            )),
        ),
        Family::Fish => {
            let names = Agent::ALL.map(Agent::program).join(" ");
            let assignments = variables()
                .map(|variable| format!("{variable}="))
                .collect::<Vec<_>>()
                .join("|");
            format!(
                r"printf '\036kodosi-agents\n'
printf 'path\t%s\n' (string join : $PATH)
{variables}
for n in (functions --names)
    set -l body (functions $n | string join ' ')
    if contains -- $n {names}; or string match -q -r -- '{assignments}' $body
        printf 'function\t%s\t%s\n' $n $body
    end
end
printf '\036end\n'",
                variables = report(&|variable| format!(
                    r#"set -l v (printenv {variable}); and test -n "$v"; and printf 'variable\t%s\t%s\n' {variable} "$v""#
                )),
            )
        }
        Family::Posix => format!(
            r#"printf '\036kodosi-agents\npath\t%s\n' "$PATH"
{variables}
printf '\036end\n'"#,
            variables = report(&|variable| format!(
                r#"v=$(printenv {variable}) && [ -n "$v" ] && printf 'variable\t%s\t%s\n' {variable} "$v""#
            )),
        ),
    }
}

/// Start files that need a terminal (ones that start tmux, for example) never
/// finish, so such a shell is asked again as a login shell only.
#[cfg(unix)]
async fn probe(shell: &Path) -> Facts {
    let Some(family) = family(shell) else {
        return Facts::default();
    };
    if family != Family::Posix
        && let Some(facts) = ask(shell, family, true).await
    {
        return facts;
    }
    ask(shell, family, false).await.unwrap_or_default()
}

#[cfg(unix)]
async fn ask(shell: &Path, family: Family, interactive: bool) -> Option<Facts> {
    use tokio::io::AsyncReadExt as _;

    let mut command = tokio::process::Command::new(shell);
    command.arg("-l");
    if interactive {
        command.arg("-i");
    }
    command
        .arg("-c")
        .arg(script(family))
        .env("TERM", "dumb")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    kodosi_os::process::without_terminal(command.as_std_mut());
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let mut output = Vec::new();
    let read = async {
        let mut chunk = [0_u8; 8192];
        while output.len() < MAX_OUTPUT && !contains(&output, END.as_bytes()) {
            match stdout.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(count) => output.extend_from_slice(&chunk[..count]),
            }
        }
    };
    if tokio::time::timeout(PROBE_TIME, read).await.is_err() {
        tracing::debug!(shell = %shell.display(), interactive, "the shell did not report its agents in time");
    }
    drop(child.start_kill());
    drop(tokio::time::timeout(Duration::from_secs(1), child.wait()).await);
    parse(&String::from_utf8_lossy(&output))
}

#[cfg(windows)]
#[expect(
    clippy::unused_async,
    reason = "Unix asks the shell, which Windows terminals do not need"
)]
async fn probe(_shell: &Path) -> Facts {
    Facts {
        variables: Agent::ALL
            .into_iter()
            .filter_map(|agent| agent.home().map(|(_, variable)| variable))
            .filter_map(|variable| Some((variable.to_owned(), std::env::var(variable).ok()?)))
            .filter(|(_, value)| !value.is_empty())
            .collect(),
        ..Facts::default()
    }
}

#[cfg(unix)]
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(unix)]
pub(super) fn parse(output: &str) -> Option<Facts> {
    let body = &output[output.find(START)? + START.len()..];
    let body = &body[..body.find(END)?];
    let mut facts = Facts::default();
    for line in body.lines() {
        let mut fields = line.splitn(3, '\t');
        match (fields.next(), fields.next(), fields.next()) {
            (Some("path"), Some(path), None) if !path.is_empty() => {
                facts.path = Some(path.into());
            }
            (Some("variable"), Some(name), Some(value))
                if variables().any(|variable| variable == name)
                    && !value.is_empty()
                    && value.len() <= 4096
                    && !value.chars().any(char::is_control) =>
            {
                facts.variables.push((name.to_owned(), value.to_owned()));
            }
            (Some(kind @ ("alias" | "function")), Some(name), Some(text))
                if !name.is_empty()
                    && name.len() <= 64
                    && name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
                    }) =>
            {
                let text = if kind == "alias" {
                    alias_value(name, text)
                } else {
                    text.to_owned()
                };
                facts.definitions.push(Definition {
                    name: name.to_owned(),
                    text: text.chars().take(16 * 1024).collect(),
                });
            }
            _ => {}
        }
    }
    Some(facts)
}

/// Bash prints `alias name='value'`, zsh the value alone.
#[cfg(unix)]
fn alias_value(name: &str, text: &str) -> String {
    text.strip_prefix("alias ")
        .and_then(|text| text.strip_prefix(name))
        .and_then(|text| text.strip_prefix('='))
        .map_or_else(
            || text.to_owned(),
            |value| {
                value
                    .strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
                    .map_or_else(|| value.to_owned(), |value| value.replace(r"'\''", "'"))
            },
        )
}
