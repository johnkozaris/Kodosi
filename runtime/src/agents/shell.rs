use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
    time::{Duration, Instant},
};

#[cfg(any(unix, test))]
use super::Agent;

#[cfg(any(unix, test))]
const START: &str = "\u{1e}kodosi-agents";
#[cfg(any(unix, test))]
const END: &str = "\u{1e}end";
#[cfg(unix)]
const PROBE_TIME: Duration = Duration::from_secs(6);
const REUSE_TIME: Duration = Duration::from_secs(600);
#[cfg(unix)]
const MAX_OUTPUT: usize = 512 * 1024;

/// What a new terminal's shell knows: its `PATH`, the functions whose names
/// begin with the name of an agent, and the aliases whose names or commands
/// name one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Facts {
    pub path: Option<OsString>,
    pub definitions: Vec<Definition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Definition {
    pub name: String,
    pub text: String,
    /// An alias, rather than a function: the shell adds the words after it.
    pub alias: bool,
}

impl Facts {
    /// Finds `program` on the shell's `PATH`, or on the runtime's own when the
    /// shell did not report one.
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
}

struct Remembered {
    shell: PathBuf,
    at: Instant,
    facts: Facts,
}

static REMEMBERED: Mutex<Option<Remembered>> = Mutex::new(None);

/// The facts of `shell`. Without `fresh`, facts from the last ten minutes do.
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

#[cfg(any(unix, test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Family {
    Zsh,
    Bash,
    Fish,
    Posix,
}

#[cfg(any(unix, test))]
pub(super) fn family(shell: &Path) -> Option<Family> {
    match shell.file_stem()?.to_str()? {
        "zsh" => Some(Family::Zsh),
        "bash" => Some(Family::Bash),
        "fish" => Some(Family::Fish),
        "sh" | "dash" | "ash" | "ksh" | "mksh" => Some(Family::Posix),
        _ => None,
    }
}

#[cfg(any(unix, test))]
pub(super) fn script(family: Family) -> String {
    let programs = Agent::ALL.map(Agent::program);
    let pattern = programs.map(|program| format!("{program}*")).join("|");
    let mentions = programs.map(|program| format!("*{program}*")).join("|");
    match family {
        Family::Zsh => format!(
            r#"print -r -- $'\x1e'kodosi-agents
print -r -- "path"$'\t'"$PATH"
for n in ${{(k)aliases}}; do if [[ $n == ({pattern}) || ${{aliases[$n]}} == ({mentions}) ]]; then print -r -- "alias"$'\t'"$n"$'\t'"${{aliases[$n]//$'\n'/ }}"; fi; done
for n in ${{(k)functions}}; do case $n in ({pattern}) print -r -- "function"$'\t'"$n"$'\t'"${{functions[$n]//$'\n'/ }}";; esac; done
print -r -- $'\x1e'end"#
        ),
        Family::Bash => format!(
            r#"printf '\036kodosi-agents\n'
printf 'path\t%s\n' "$PATH"
for n in $(compgen -a); do v=$(alias "$n"); case "$n" in {pattern}) ;; *) case "$v" in {mentions}) ;; *) continue;; esac;; esac; printf 'alias\t%s\t%s\n' "$n" "${{v//$'\n'/ }}"; done
for n in $(compgen -A function); do case $n in {pattern}) v=$(declare -f "$n"); printf 'function\t%s\t%s\n' "$n" "${{v//$'\n'/ }}";; esac; done
printf '\036end\n'"#
        ),
        Family::Fish => {
            let patterns = programs.map(|program| format!("'{program}*'")).join(" ");
            format!(
                r"printf '\036kodosi-agents\n'
printf 'path\t%s\n' (string join : $PATH)
for n in (functions --names)
    switch $n
        case {patterns}
            printf 'function\t%s\t%s\n' $n (functions $n | string join ' ')
    end
end
printf '\036end\n'"
            )
        }
        Family::Posix => r#"printf '\036kodosi-agents\npath\t%s\n\036end\n' "$PATH""#.to_owned(),
    }
}

/// Asks the shell as an interactive login shell, as a terminal starts it. A
/// shell whose start files never finish without a terminal (one that starts
/// tmux, for example) is asked again as a login shell only, for its `PATH`.
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
fn probe(_shell: &Path) -> std::future::Ready<Facts> {
    std::future::ready(Facts::default())
}

#[cfg(unix)]
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(any(unix, test))]
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
                    alias: kind == "alias",
                });
            }
            _ => {}
        }
    }
    Some(facts)
}

/// The value of an alias. Bash prints it as `alias name='value'`, zsh as the
/// value alone.
#[cfg(any(unix, test))]
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
