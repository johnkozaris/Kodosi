#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Claude,
    Copilot,
}

impl AgentKind {
    #[must_use]
    pub fn from_argv(argv: &[String]) -> Option<Self> {
        let executable = stem(argv.first()?);
        if let Some(kind) = match_stem(executable) {
            return Some(kind);
        }
        match executable {
            "node" => node_argv(argv),
            "deno" => deno_argv(argv),
            "bun" => bun_argv(argv),
            "bash" | "sh" | "zsh" | "fish" => unix_shell_argv(argv),
            "env" => env_argv(argv),
            "gh" => gh_argv(argv),
            _ => None,
        }
    }

    #[must_use]
    pub fn from_banner(banner: &str) -> Option<Self> {
        match banner {
            "Claude" => Some(Self::Claude),
            "GitHub Copilot" => Some(Self::Copilot),
            _ => None,
        }
    }

    #[must_use]
    pub const fn banner(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Copilot => "GitHub Copilot",
        }
    }

    #[must_use]
    pub const fn canonical(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Copilot => "copilot",
        }
    }
}

fn stem(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn match_stem(value: &str) -> Option<AgentKind> {
    if value.eq_ignore_ascii_case("claude") || value.eq_ignore_ascii_case("claude-code") {
        Some(AgentKind::Claude)
    } else if value.eq_ignore_ascii_case("copilot") || value.eq_ignore_ascii_case("gh-copilot") {
        Some(AgentKind::Copilot)
    } else {
        None
    }
}

fn match_script_path(path: &str) -> Option<AgentKind> {
    if let Some(kind) = match_stem(stem(path)) {
        return Some(kind);
    }
    let components: Vec<_> = path
        .split('/')
        .filter(|component| !component.is_empty())
        .collect();
    for pair in components.windows(2) {
        if pair[0].eq_ignore_ascii_case("@anthropic-ai")
            && pair[1].eq_ignore_ascii_case("claude-code")
        {
            return Some(AgentKind::Claude);
        }
        if pair[0].eq_ignore_ascii_case("@github") && pair[1].eq_ignore_ascii_case("copilot") {
            return Some(AgentKind::Copilot);
        }
    }
    None
}

fn node_argv(argv: &[String]) -> Option<AgentKind> {
    let mut index = 1;
    while index < argv.len() {
        let value = argv[index].as_str();
        if matches!(value, "-e" | "-p" | "--eval" | "--print")
            || value.starts_with("--eval=")
            || value.starts_with("--print=")
        {
            return None;
        }
        if matches!(
            value,
            "-r" | "--require"
                | "--loader"
                | "--import"
                | "--conditions"
                | "--title"
                | "--inspect-port"
                | "--icu-data-dir"
                | "--openssl-config"
                | "--redirect-warnings"
                | "--snapshot-blob"
                | "--security-revert"
                | "--diagnostic-dir"
        ) {
            index += 2;
            continue;
        }
        if value == "--" || (value.starts_with("--") && value.contains('=')) {
            index += 1;
            continue;
        }
        if matches!(
            value,
            "-v" | "--version"
                | "-h"
                | "--help"
                | "--check"
                | "--inspect"
                | "--inspect-brk"
                | "--inspect-wait"
                | "--no-warnings"
                | "--trace-warnings"
                | "--use-bundled-ca"
                | "--use-openssl-ca"
                | "--use-system-ca"
                | "--watch"
                | "--watch-preserve-output"
        ) {
            index += 1;
            continue;
        }
        if value.starts_with('-') {
            return None;
        }
        return match_script_path(value);
    }
    None
}

fn deno_argv(argv: &[String]) -> Option<AgentKind> {
    let mut index = 1;
    while index < argv.len() {
        let value = argv[index].as_str();
        match value {
            "fmt" | "lint" | "test" | "doc" | "bundle" | "compile" | "info" | "task"
            | "install" | "uninstall" | "cache" | "check" | "eval" | "repl" => return None,
            "run" => index += 1,
            "--config" | "-c" | "--import-map" | "--lock" | "--location" | "--cert" | "--seed"
            | "--inspect-brk" | "--v8-flags" => index += 2,
            _ if value.starts_with('-') => index += 1,
            _ => return match_script_path(value),
        }
    }
    None
}

fn bun_argv(argv: &[String]) -> Option<AgentKind> {
    let mut index = 1;
    let mut saw_run = false;
    while index < argv.len() {
        let value = argv[index].as_str();
        if matches!(value, "-e" | "--eval") || value.starts_with("--eval=") {
            return None;
        }
        if value == "run" && !saw_run {
            saw_run = true;
            index += 1;
            continue;
        }
        if value.starts_with('-') {
            index += 1;
            continue;
        }
        if saw_run && !value.contains('/') && !value.starts_with('.') {
            return None;
        }
        return match_script_path(value);
    }
    None
}

fn unix_shell_argv(argv: &[String]) -> Option<AgentKind> {
    for value in &argv[1..] {
        if value == "--command"
            || value.starts_with("--command=")
            || value
                .strip_prefix('-')
                .is_some_and(|flags| !flags.starts_with('-') && flags.contains('c'))
        {
            return None;
        }
        if value.starts_with('-') {
            continue;
        }
        return match_script_path(value);
    }
    None
}

fn env_argv(argv: &[String]) -> Option<AgentKind> {
    let mut index = 1;
    while index < argv.len() {
        let value = argv[index].as_str();
        if matches!(value, "-i" | "--ignore-environment" | "-") {
            index += 1;
        } else if matches!(value, "-u" | "-C" | "-S" | "--unset" | "--chdir") {
            index += 2;
        } else if value.starts_with("--unset=")
            || value.starts_with("--chdir=")
            || value.starts_with("-S")
            || value
                .find('=')
                .is_some_and(|equals| value.find('/').is_none_or(|slash| equals < slash))
        {
            index += 1;
        } else {
            return AgentKind::from_argv(&argv[index..]);
        }
    }
    None
}

fn gh_argv(argv: &[String]) -> Option<AgentKind> {
    let mut index = 1;
    while index < argv.len() {
        let value = argv[index].as_str();
        if matches!(value, "--repo" | "-R" | "--hostname") {
            index += 2;
        } else if value.starts_with("--repo=")
            || value.starts_with("--hostname=")
            || value.starts_with('-')
        {
            index += 1;
        } else {
            return (value == "copilot").then_some(AgentKind::Copilot);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn detects_supported_agents() {
        assert_eq!(
            AgentKind::from_argv(&argv(&["/opt/homebrew/bin/claude"])),
            Some(AgentKind::Claude)
        );
        assert_eq!(
            AgentKind::from_argv(&argv(&["node", "/opt/homebrew/bin/copilot"])),
            Some(AgentKind::Copilot)
        );
    }

    #[test]
    fn detects_supported_wrappers_without_inspecting_commands() {
        assert_eq!(
            AgentKind::from_argv(&argv(&["env", "node", "/usr/local/bin/claude"])),
            Some(AgentKind::Claude)
        );
        assert_eq!(
            AgentKind::from_argv(&argv(&["gh", "copilot", "suggest"])),
            Some(AgentKind::Copilot)
        );
        assert_eq!(AgentKind::from_argv(&argv(&["bash", "-c", "claude"])), None);
        assert_eq!(
            AgentKind::from_argv(&argv(&["node", "-e", "copilot"])),
            None
        );
    }

    #[test]
    fn banners_and_canonical_names_are_stable() {
        assert_eq!(AgentKind::Claude.banner(), "Claude");
        assert_eq!(AgentKind::Copilot.banner(), "GitHub Copilot");
        assert_eq!(AgentKind::Claude.canonical(), "claude");
        assert_eq!(AgentKind::Copilot.canonical(), "copilot");
    }
}
