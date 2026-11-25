#[cfg(unix)]
use std::fs;
use std::path::Path;

#[cfg(unix)]
use super::{accounts, shell::Facts};
use super::{
    accounts::referenced_home,
    models::{self, antigravity, codex, copilot, cursor, listed_values, opencode},
    *,
};

#[cfg(unix)]
fn definition(name: &str, text: &str) -> shell::Definition {
    shell::Definition {
        name: name.to_owned(),
        text: text.to_owned(),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_program_that_prints_without_end_lists_no_models() {
    let facts = Facts {
        path: None,
        variables: Vec::new(),
        definitions: Vec::new(),
    };
    let started = std::time::Instant::now();
    assert_eq!(
        models::run(&facts, Some(Path::new("/usr/bin/yes")), &[]).await,
        None
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

#[cfg(unix)]
fn run_script(shell: &str, arguments: &[&str], prelude: &str, family_script: &str) -> Facts {
    let output = std::process::Command::new(shell)
        .args(arguments)
        .arg(format!("{prelude}\n{family_script}"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("shell runs");
    shell::parse(&String::from_utf8_lossy(&output.stdout)).expect("both markers")
}

#[cfg(unix)]
#[test]
fn shell_scripts_report_path_sign_in_variables_and_agent_definitions() {
    let prelude = r#"export CLAUDE_CONFIG_DIR=/srv/claude-main
claude() { command claude "$@"; }
claudebeta() { CLAUDE_CONFIG_DIR="$HOME/.claude-beta" claude "$@"; }
alias codex-lab='CODEX_HOME=~/.codex-lab codex'
alias cc='claude --dangerously-skip-permissions'
alias gs='git status'
gitlog() { git log; }"#;
    for (shell, arguments) in [
        ("/bin/zsh", &["-f", "-c"][..]),
        ("/bin/bash", &["--norc", "--noprofile", "-c"][..]),
    ] {
        if !Path::new(shell).exists() {
            continue;
        }
        let family = shell::family(Path::new(shell)).expect("known shell");
        let facts = run_script(shell, arguments, prelude, &shell::script(family));
        assert_eq!(
            facts.path.as_deref(),
            Some("/usr/bin:/bin".as_ref()),
            "{shell}"
        );
        assert_eq!(
            facts.variable("CLAUDE_CONFIG_DIR"),
            Some("/srv/claude-main"),
            "{shell}"
        );
        assert_eq!(facts.variable("CODEX_HOME"), None, "{shell}");
        let mut names = facts
            .definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["claude", "claudebeta", "codex-lab"], "{shell}");
        let claudebeta = facts
            .definitions
            .iter()
            .find(|definition| definition.name == "claudebeta")
            .expect("function");
        assert!(
            claudebeta.text.contains(".claude-beta"),
            "{shell}: {claudebeta:?}"
        );
        assert!(!claudebeta.text.contains('\n'));
        let codex_lab = facts
            .definitions
            .iter()
            .find(|definition| definition.name == "codex-lab")
            .expect("alias");
        assert_eq!(codex_lab.text, "CODEX_HOME=~/.codex-lab codex", "{shell}");
    }
    if Path::new("/bin/sh").exists() {
        let family = shell::family(Path::new("/bin/sh")).expect("known shell");
        let facts = run_script(
            "/bin/sh",
            &["-c"],
            "export COPILOT_HOME=/srv/copilot; CODEX_HOME=/srv/codex",
            &shell::script(family),
        );
        assert_eq!(facts.variable("COPILOT_HOME"), Some("/srv/copilot"));
        assert_eq!(facts.variable("CODEX_HOME"), None);
        assert_eq!(facts.definitions, Vec::new());
    }
}

#[test]
fn definitions_name_the_sign_in_folder_they_give_the_agent() {
    let home = Path::new("/home/ada");
    let claude = |text| referenced_home(text, home, "CLAUDE_CONFIG_DIR");
    assert_eq!(
        claude("CLAUDE_CONFIG_DIR=~/.claude-work claude"),
        Some(home.join(".claude-work"))
    );
    assert_eq!(
        claude(r#"export CLAUDE_CONFIG_DIR="$HOME/.claude-beta"; claude "$@""#),
        Some(home.join(".claude-beta"))
    );
    assert_eq!(
        claude("env 'CLAUDE_CONFIG_DIR=/srv/claude two' claude"),
        Some("/srv/claude two".into())
    );
    assert_eq!(claude(r#"_claude_profile "$HOME/.claude-beta" "$@""#), None);
    assert_eq!(
        claude(r#"CLAUDE_CONFIG_DIR="$config_dir" "$HOME/.local/bin/claude""#),
        None
    );
    assert_eq!(claude("CLAUDE_CONFIG_DIR=$HOME/../x claude"), None);
    assert_eq!(claude("env -u CLAUDE_CONFIG_DIR claude"), None);
    assert_eq!(claude("MY_CLAUDE_CONFIG_DIR=~/notes claude"), None);
    assert_eq!(
        claude("OLD_CLAUDE_CONFIG_DIR=~/notes CLAUDE_CONFIG_DIR=~/.claude-work claude"),
        Some(home.join(".claude-work"))
    );
}

#[cfg(unix)]
fn jwt(email: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

    let claims = URL_SAFE_NO_PAD.encode(format!(r#"{{"email":"{email}"}}"#));
    format!("e30.{claims}.signature")
}

#[cfg(unix)]
fn put(home: &Path, path: &str, text: &str) {
    let path = home.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[cfg(unix)]
fn claude_sign_in(email: &str) -> String {
    format!(r#"{{"oauthAccount":{{"emailAddress":"{email}"}}}}"#)
}

#[cfg(unix)]
fn rows(catalog: &AgentCatalog, agent: Agent) -> Vec<String> {
    catalog
        .agents
        .iter()
        .find(|info| info.agent == agent)
        .expect("agent")
        .accounts
        .iter()
        .map(|account| {
            [
                Some(&account.name),
                account.identity.as_ref(),
                account.folder.as_ref(),
            ]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn accounts_are_the_sign_in_folders_of_each_agent() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    put(home, ".claude.json", &claude_sign_in("ada@example.com"));
    put(
        home,
        ".claude-beta/.claude.json",
        &claude_sign_in("beta@example.com"),
    );
    put(
        home,
        ".claude_personal/.claude.json",
        &claude_sign_in("me@example.com"),
    );
    put(home, ".claude-started/.claude.json", "{}");
    put(
        home,
        ".profiles/x/.claude.json",
        &claude_sign_in("x@example.com"),
    );
    put(
        home,
        "team/.claude/.claude.json",
        &claude_sign_in("team@example.com"),
    );
    fs::create_dir_all(home.join(".claude-work")).unwrap();
    fs::create_dir_all(home.join(".claude_work")).unwrap();
    fs::create_dir_all(home.join(".claude-default")).unwrap();
    fs::create_dir_all(home.join(".claude-squad/cache")).unwrap();
    put(
        home,
        ".codex-lab/auth.json",
        &format!(
            r#"{{"tokens":{{"id_token":"{}"}}}}"#,
            jwt("lab@example.com")
        ),
    );
    let facts = Facts {
        path: Some("/nowhere".into()),
        variables: Vec::new(),
        definitions: vec![
            definition("claude", r#"command claude "$@""#),
            definition("claude-dyn", r#"CLAUDE_CONFIG_DIR="$dir" claude "$@""#),
            definition("codex", "codex"),
            definition("claude-x", "CLAUDE_CONFIG_DIR=~/.profiles/x claude"),
            definition("claude-team", "CLAUDE_CONFIG_DIR=~/team/.claude claude"),
            definition("claude-beta", "CLAUDE_CONFIG_DIR=~/.claude-beta claude"),
            definition("claude-gone", "CLAUDE_CONFIG_DIR=~/.claude-gone claude"),
            definition("cc", "claude --dangerously-skip-permissions"),
        ],
    };
    let catalog = accounts::catalog(home, &facts);
    assert_eq!(
        rows(&catalog, Agent::Claude),
        [
            "default ada@example.com ~/.claude",
            "beta beta@example.com ~/.claude-beta",
            "default-2 ~/.claude-default",
            "personal me@example.com ~/.claude_personal",
            "started ~/.claude-started",
            "team team@example.com ~/team/.claude",
            "work ~/.claude-work",
            "work-2 ~/.claude_work",
            "x x@example.com ~/.profiles/x",
        ]
    );
    assert_eq!(
        rows(&catalog, Agent::Codex),
        ["default ~/.codex", "lab lab@example.com ~/.codex-lab"]
    );
    let claude = &catalog.agents[0].accounts;
    assert_eq!(claude[0].command, "claude");
    assert_eq!(
        claude[6].command,
        format!(
            "env CLAUDE_CONFIG_DIR={}/.claude-work claude",
            home.display()
        )
    );
    let copilot = catalog
        .agents
        .iter()
        .find(|info| info.agent == Agent::Copilot)
        .expect("copilot");
    assert!(!copilot.installed && copilot.accounts.is_empty());
    assert!(copilot.install.contains("copilot"));
}

#[cfg(unix)]
#[test]
fn a_new_account_gets_a_private_empty_folder() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let account = accounts::add(home, Agent::Codex, " Night Shift ", &[]).expect("added");
    assert_eq!(account.name, "night-shift");
    assert_eq!(
        account.command,
        format!("env CODEX_HOME={}/.codex-night-shift codex", home.display())
    );
    let mode = fs::metadata(home.join(".codex-night-shift"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    assert!(accounts::add(home, Agent::Codex, "night shift", &[]).is_err());
    assert!(accounts::add(home, Agent::Codex, "../x", &[]).is_err());
    assert!(accounts::add(home, Agent::Cursor, "work", &[]).is_err());
    assert_eq!(
        accounts::add(
            home,
            Agent::Codex,
            "Work",
            &["default".to_owned(), "work".to_owned()]
        )
        .unwrap_err()
        .to_string(),
        "There is already an account named work."
    );
}

#[test]
fn help_text_lists_the_values_of_a_flag() {
    let claude = "  --effort <level>                      Effort level for the current session\n                                        (low, medium, high, xhigh, max)\n  --environment <id>                    Create (a, b)";
    assert_eq!(
        listed_values(claude, "--effort"),
        ["low", "medium", "high", "xhigh", "max"]
    );
    let copilot = "      --reasoning-effort <level>\n          Set the reasoning effort level [possible values: none, low, high]\n      --context <tier>";
    assert_eq!(
        listed_values(copilot, "--reasoning-effort"),
        ["none", "low", "high"]
    );
    assert_eq!(listed_values(claude, "--model"), Vec::<String>::new());
    let agy = "  --effort                        Reasoning effort for the current CLI session (low|medium|high|xhigh|max)\n  -i                              Short alias for --prompt-interactive";
    assert_eq!(
        listed_values(agy, "--effort"),
        ["low", "medium", "high", "xhigh", "max"]
    );
}

#[test]
fn codex_lists_the_models_of_its_picker_in_order() {
    let (models, efforts) = codex(Some(
        r#"{"models":[
            {"slug":"b","display_name":"B","visibility":"list","priority":2,"supported_reasoning_levels":[{"effort":"low"},{"effort":"high"}]},
            {"slug":"hidden","visibility":"hide","priority":0},
            {"slug":"a","display_name":"A","visibility":"list","priority":1,"supported_reasoning_levels":[{"effort":"medium"},{"effort":"low"}]}
        ]}"#,
    ));
    assert_eq!(
        models
            .iter()
            .map(|model| (model.id.as_str(), model.argument.as_str()))
            .collect::<Vec<_>>(),
        [("a", "-m a"), ("b", "-m b")]
    );
    assert_eq!(models[1].efforts, ["low", "high"]);
    assert_eq!(
        efforts
            .iter()
            .map(|effort| effort.argument.as_str())
            .collect::<Vec<_>>(),
        [
            "-c model_reasoning_effort=medium",
            "-c model_reasoning_effort=low",
            "-c model_reasoning_effort=high"
        ]
    );
}

#[test]
fn copilot_cursor_opencode_and_antigravity_lists_become_models() {
    let config = "  `model`: AI model to use\n    - \"claude-sonnet-5.5\"\n    - \"gpt-6-sol\"\n\n  `contextTier`: tier\n    - \"default\"";
    assert_eq!(
        copilot(Some(config))
            .iter()
            .map(|model| model.argument.as_str())
            .collect::<Vec<_>>(),
        ["--model claude-sonnet-5.5", "--model gpt-6-sol"]
    );
    let list = "\u{1b}[1mAvailable models\u{1b}[0m\nauto - Auto\nopus-4.6-thinking - Claude 4.6 Opus (Thinking)  (default)\n";
    assert_eq!(
        cursor(Some(&models::without_escapes(list)))
            .iter()
            .map(|model| (model.id.as_str(), model.label.as_str()))
            .collect::<Vec<_>>(),
        [
            ("auto", "Auto"),
            ("opus-4.6-thinking", "Claude 4.6 Opus (Thinking)")
        ]
    );
    assert_eq!(
        opencode(Some(
            "anthropic/claude-opus-5\nnot a model\nopenrouter/x[1m]\n"
        ))
        .iter()
        .map(|model| model.argument.as_str())
        .collect::<Vec<_>>(),
        [
            "--model anthropic/claude-opus-5",
            "--model 'openrouter/x[1m]'"
        ]
    );
    let (models, efforts) = antigravity(Some(
        "Fetching available models...\ngemini-4-neon-high\tGemini 4 Neon (High)\ngemini-4-neon-xhigh\tGemini 4 Neon (XHigh)\ngemini-4-neon-low\tGemini 4 Neon (Low)\ngemini-3.1-pro-low\tGemini 3.1 Pro (Low)\ngemini-3.1-pro-high\tGemini 3.1 Pro (High)\nError: try again\n",
    ));
    assert_eq!(
        models
            .iter()
            .map(|model| (
                model.id.as_str(),
                model.label.as_str(),
                model.argument.as_str(),
                model.efforts.join(" ")
            ))
            .collect::<Vec<_>>(),
        [
            (
                "gemini-4-neon",
                "Gemini 4 Neon",
                "--model gemini-4-neon",
                "low high xhigh".to_owned()
            ),
            (
                "gemini-3.1-pro",
                "Gemini 3.1 Pro",
                "--model gemini-3.1-pro",
                "low high".to_owned()
            )
        ]
    );
    assert_eq!(
        efforts
            .iter()
            .map(|effort| effort.argument.as_str())
            .collect::<Vec<_>>(),
        ["--effort low", "--effort high", "--effort xhigh"]
    );
}

#[test]
fn quoted_prompts_reach_the_program_as_one_word() {
    let prompt = r#"fix "it's" $HOME `id` \ !! ‘ok’ ; exit"#;
    for (shell, arguments) in [
        ("/bin/zsh", &["-f", "-c"][..]),
        ("/bin/bash", &["--norc", "--noprofile", "-c"][..]),
        ("/bin/sh", &["-c"][..]),
    ] {
        if !Path::new(shell).exists() {
            continue;
        }
        let output = std::process::Command::new(shell)
            .args(arguments)
            .arg(format!("printf %s {}", quote(Path::new(shell), prompt)))
            .output()
            .expect("shell runs");
        assert_eq!(String::from_utf8_lossy(&output.stdout), prompt, "{shell}");
    }
    assert_eq!(quote(Path::new("/usr/bin/fish"), r"a'b\c"), r"'a\'b\\c'");
    assert_eq!(quote(Path::new("pwsh.exe"), "it's ‘x’"), "'it''s ‘‘x’’'");
    assert_eq!(
        quote(Path::new("cmd.exe"), r#"say "hi" \"#),
        r#""say \"hi\" \\""#
    );
    assert_eq!(quote(Path::new("nu"), "a'#b"), "r##'a'#b'##");
}

#[test]
fn a_prompt_with_lines_reaches_the_program_as_one_word_without_a_typed_line_break() {
    let prompt = "look at the changes:\n\t- code it's\n\n  cleanups";
    for (shell, arguments) in [
        ("/bin/zsh", &["-f", "-c"][..]),
        ("/bin/bash", &["--norc", "--noprofile", "-c"][..]),
    ] {
        if !Path::new(shell).exists() {
            continue;
        }
        let word = quote(Path::new(shell), &prompt.replace('\n', "\r\n"));
        assert!(!word.contains(['\n', '\r', '\t']), "{word}");
        let output = std::process::Command::new(shell)
            .args(arguments)
            .arg(format!("printf %s {word}"))
            .output()
            .expect("shell runs");
        assert_eq!(String::from_utf8_lossy(&output.stdout), prompt, "{shell}");
    }
    assert_eq!(quote(Path::new("/usr/bin/fish"), "a\n\tb"), r"'a'\n\t'b'");
    assert_eq!(quote(Path::new("pwsh.exe"), "a\nb"), "('a'+\"`n\"+'b')");
    assert_eq!(
        quote(Path::new("nu"), "a\nb"),
        r#"(r#'a'# ++ "\n" ++ r#'b'#)"#
    );
    assert_eq!(quote(Path::new("cmd.exe"), "a\n\tb"), r#""a  b""#);
}

#[test]
fn agents_get_kodosi_s_note_from_the_terminal_variable_in_every_shell() {
    let note = "You run in a Kodosi terminal; it's [Kodosi] & `kodosi skill`.";
    for (shell, arguments) in [
        ("/bin/zsh", &["-f", "-c"][..]),
        ("/bin/bash", &["--norc", "--noprofile", "-c"][..]),
        ("/bin/sh", &["-c"][..]),
    ] {
        if !Path::new(shell).exists() {
            continue;
        }
        let word = variable_word(
            Path::new(shell),
            "developer_instructions=",
            CONTEXT_VARIABLE,
        );
        let output = std::process::Command::new(shell)
            .args(arguments)
            .arg(format!("printf %s {word}"))
            .env(CONTEXT_VARIABLE, note)
            .output()
            .expect("shell runs");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("developer_instructions={note}"),
            "{shell}"
        );
    }
    assert_eq!(
        variable_word(Path::new("/usr/bin/fish"), "", CONTEXT_VARIABLE),
        "\"$KODOSI_AGENT_CONTEXT\""
    );
    assert_eq!(
        variable_word(Path::new("pwsh.exe"), "a=", CONTEXT_VARIABLE),
        "\"a=$env:KODOSI_AGENT_CONTEXT\""
    );
    assert_eq!(
        variable_word(Path::new("cmd.exe"), "", CONTEXT_VARIABLE),
        "\"%KODOSI_AGENT_CONTEXT%\""
    );
    assert_eq!(
        variable_word(Path::new("nu"), "a=", CONTEXT_VARIABLE),
        "$\"a=($env.KODOSI_AGENT_CONTEXT)\""
    );
    let notes = Notes {
        shell: "/bin/zsh".into(),
        ..Notes::default()
    };
    assert_eq!(
        Agent::Codex.with_context("codex", &notes),
        "codex --no-daemon -c \"developer_instructions=$KODOSI_AGENT_CONTEXT\""
    );
    let served = Notes {
        codex_servers: Some("unix:///tmp/kodosi-501-a/codex-".to_owned()),
        ..notes.clone()
    };
    assert_eq!(
        Agent::Codex.with_context("codex", &served),
        "codex --remote \"$KODOSI_CODEX_SERVER\""
    );
    for agent in [Agent::Claude, Agent::Copilot, Agent::Antigravity] {
        assert_eq!(agent.with_context(agent.program(), &notes), agent.program());
    }
    assert!(!CONTEXT.trim().contains(['"', '%', '\n']));
}

#[cfg(unix)]
#[test]
fn copilot_starts_with_kodosi_s_extension_and_kodosi_allows_it_for_the_folder() {
    let root = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(root.path()).unwrap();
    let folder = root.join("data root").join("agents");
    write_notes(&folder).unwrap();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    kodosi_os::process::write_program(&bin.join("copilot"), "#!/bin/sh\nprintf '%s\\0' \"$@\"\n")
        .unwrap();
    let notes = Notes {
        shell: "/bin/sh".into(),
        folder: folder.clone(),
        ..Notes::default()
    };
    let printed = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(Agent::Copilot.with_context("copilot", &notes))
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .output()
        .unwrap();
    let words = String::from_utf8(printed.stdout).unwrap();
    let plugin = folder.join("copilot");
    assert_eq!(
        words.split_terminator('\0').collect::<Vec<_>>(),
        ["--plugin-dir", plugin.to_str().unwrap(), "--experimental"]
    );
    assert!(
        plugin
            .join("extensions")
            .join("kodosi")
            .join("extension.mjs")
            .is_file()
    );

    let home = root.join("home").join(".copilot");
    let repository = root.join("shop");
    fs::create_dir_all(repository.join(".git")).unwrap();
    fs::create_dir_all(repository.join("src")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join("permissions-config.json"),
        r#"{"locations": {"/elsewhere": {"tool_approvals": [{"kind": "write"}]}}}"#,
    )
    .unwrap();
    allow_copilot_link(&home, &repository.join("src")).unwrap();
    allow_copilot_link(&home, &repository.join("src")).unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(home.join("permissions-config.json")).unwrap()).unwrap();
    assert_eq!(
        saved["locations"][repository.to_str().unwrap()]["tool_approvals"],
        serde_json::json!([{"kind": "extension-permission-access", "extensionName": "plugin:kodosi:kodosi"}]),
        "once, for the repository of the folder"
    );
    assert_eq!(
        saved["locations"]["/elsewhere"]["tool_approvals"],
        serde_json::json!([{"kind": "write"}])
    );
}

#[cfg(unix)]
#[test]
fn cursor_and_opencode_get_the_note_only_in_the_line_that_kodosi_types_where_they_take_it() {
    let root = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(root.path()).unwrap();
    let folder = root.join("data root").join("agents");
    write_notes(&folder).unwrap();
    let bin = root.join("bin");
    fs::create_dir(&bin).unwrap();
    for program in ["cursor-agent", "opencode"] {
        kodosi_os::process::write_program(
            &bin.join(program),
            "#!/bin/sh\nprintf '%s\\0' \"$OPENCODE_CONFIG_CONTENT\" \"$@\"\n",
        )
        .unwrap();
    }
    let run = |line: &str| {
        let printed = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(line)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        let printed = String::from_utf8(printed.stdout).unwrap();
        let mut words = printed.split_terminator('\0').map(str::to_owned);
        (words.next().unwrap_or_default(), words.collect::<Vec<_>>())
    };
    let notes = |cursor_plugins, opencode_version| Notes {
        shell: "/bin/sh".into(),
        folder: folder.clone(),
        installed: Installed {
            cursor_plugins,
            opencode_version,
        },
        codex_servers: None,
    };

    let cursor = Agent::Cursor.with_context("cursor-agent --force", &notes(true, None));
    let (_, arguments) = run(&cursor);
    let plugin = folder.join("cursor");
    assert_eq!(
        arguments,
        ["--force", "--plugin-dir", plugin.to_str().unwrap()]
    );
    let arguments = [vec!["cursor-agent".to_owned()], arguments].concat();
    assert!(!Agent::Cursor.started_without_note(&arguments, None, &notes(true, None)));
    assert!(Agent::Cursor.started_without_note(
        &["cursor-agent".to_owned(), "--force".to_owned()],
        None,
        &notes(true, None)
    ));
    let rule = fs::read_to_string(plugin.join("rules").join("kodosi.mdc")).unwrap();
    assert_eq!(
        rule,
        format!("---\nalwaysApply: true\n---\n\n{}\n", CONTEXT.trim())
    );
    let without_plugin = Notes {
        folder: root.join("nothing"),
        ..notes(true, None)
    };
    for notes in [notes(false, None), without_plugin] {
        assert_eq!(
            Agent::Cursor.with_context("cursor-agent", &notes),
            "cursor-agent"
        );
    }

    for (version, standalone) in [(0, false), (1, false), (2, true), (3, true)] {
        let line = Agent::Opencode.with_context("opencode --auto", &notes(false, Some(version)));
        let (config, arguments) = run(&line);
        let expected: &[&str] = if standalone {
            &["--auto", "--standalone"]
        } else {
            &["--auto"]
        };
        assert_eq!(arguments, expected, "{line}");
        let instructions: serde_json::Value = serde_json::from_str(&config).unwrap();
        let note = fs::read_to_string(instructions["instructions"][0].as_str().unwrap()).unwrap();
        assert_eq!(note.trim(), CONTEXT.trim());
        assert!(!Agent::Opencode.started_without_note(
            &["opencode".to_owned()],
            Some(&config),
            &notes(false, Some(version))
        ));
        assert!(Agent::Opencode.started_without_note(
            &["opencode".to_owned()],
            None,
            &notes(false, Some(version))
        ));
    }
    assert_eq!(
        Agent::Opencode.with_context("opencode", &notes(true, None)),
        "opencode"
    );
    assert_eq!(major_version("1.19.5\n"), Some(1));
    assert_eq!(major_version("opencode v2.3.0\n"), Some(2));
    assert_eq!(major_version("opencode\n"), None);
}

#[test]
fn kodosi_leaves_no_plugin_in_antigravity_s_folder_when_it_stops() {
    let root = tempfile::tempdir().unwrap();
    let root = kodosi_os::fs::canonicalize(root.path()).unwrap();
    let notes = root.join("data").join("agents");
    write_notes(&notes).unwrap();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    let target = home.join(".gemini/config/plugins/kodosi");

    mount_antigravity_plugin(&home, &notes, false).unwrap();
    assert!(
        !home.join(".gemini").exists(),
        "a host that starts makes no folder for an Antigravity CLI that is not there"
    );

    mount_antigravity_plugin(&home, &notes, true).unwrap();
    assert!(target.join("hooks.json").is_file());

    unmount_antigravity_plugin(&home, &notes).unwrap();
    assert!(!target.exists());
}

#[test]
fn a_running_agent_keeps_the_model_and_effort_of_its_arguments() {
    let chosen = |agent: Agent, arguments: &[&str]| {
        agent.chosen(
            &arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect::<Vec<_>>(),
            &AgentSettings::default(),
            true,
        )
    };
    assert_eq!(
        chosen(
            Agent::Copilot,
            &[
                "node",
                "/lib/node_modules/@github/copilot/index.js",
                "--yolo",
                "--model",
                "claude-opus-5.5",
                "--reasoning-effort",
                "max",
                "-i",
                "--model x"
            ]
        ),
        [
            "--model claude-opus-5.5",
            "--reasoning-effort max",
            "--yolo"
        ]
    );
    assert_eq!(
        chosen(
            Agent::Claude,
            &[
                "claude",
                "--model=sonnet",
                "--model",
                "opus",
                "--verbose",
                "--effort low",
                "--effort",
                "high"
            ]
        ),
        ["--model opus", "--effort high", "--verbose"]
    );
    assert_eq!(
        chosen(
            Agent::Codex,
            &[
                "codex",
                "-m",
                "gpt-5.6-sol",
                "-c",
                "model_reasoning_effort=xhigh",
                "-c",
                "developer_instructions=-m",
                "-a",
                "never",
                "--sandbox",
                "danger-full-access",
                "-c",
                "hide_agent_reasoning=true",
                "resume",
                "0198f0c4-5e6f-7a8b-9c0d-1e2f3a4b5c6d"
            ]
        ),
        [
            "-m gpt-5.6-sol",
            "-c model_reasoning_effort=xhigh",
            "-a never",
            "--sandbox danger-full-access",
            "-c hide_agent_reasoning=true"
        ]
    );
    assert_eq!(
        chosen(Agent::Copilot, &["copilot", "--model", "it's", "--effort"]),
        Vec::<String>::new()
    );
}

#[test]
fn kodosi_types_nothing_into_the_question_that_claude_code_asks_on_its_screen() {
    let asking = "❯ /model sonnet\n▔▔▔▔▔▔▔▔▔▔\n   Switch model?\n   Your next response will be slower and use more tokens\n   ❯ 1. Yes, switch to Sonnet 5.5\n     2. No, go back\n";
    assert!(asks(Agent::Claude.screen_questions(), asking));
    assert!(asks(
        Agent::Claude.screen_questions(),
        "   Change effort level?\n   ❯ 1. Yes\n"
    ));
    assert!(!asks(
        Agent::Claude.screen_questions(),
        "❯ /model sonnet\n  ⎿  Kept model as Haiku 5.5\n❯ \n"
    ));
    assert!(!asks(
        Agent::Claude.screen_questions(),
        "⏺ Should I ask: Switch model? Not now.\n"
    ));
    assert!(!asks(Agent::Codex.screen_questions(), asking));
}

#[test]
fn a_running_agent_changes_with_its_own_commands() {
    let settings = |model: Option<&str>, effort: Option<&str>, permissions| AgentSettings {
        model: model.map(str::to_owned),
        effort: effort.map(str::to_owned),
        permissions,
        ..AgentSettings::default()
    };
    let lines = |agent: Agent, change: AgentSettings| {
        agent
            .live_lines(&change)
            .map(|lines| lines.into_iter().map(|(line, _)| line).collect::<Vec<_>>())
    };
    assert_eq!(
        lines(Agent::Claude, settings(Some("sonnet"), Some("high"), None)).unwrap(),
        ["/model sonnet", "/effort high"]
    );
    assert_eq!(
        lines(
            Agent::Codex,
            settings(Some("gpt-6"), Some("high"), Some(Permissions::Auto))
        )
        .unwrap(),
        ["/model gpt-6 --reasoning=high", "/permissions :workspace"]
    );
    assert_eq!(
        lines(
            Agent::Codex,
            settings(None, Some("low"), Some(Permissions::Ask))
        )
        .unwrap(),
        ["/model --reasoning=low", "/permissions :read-only"]
    );
    assert_eq!(
        lines(
            Agent::Copilot,
            settings(Some("gpt-6"), None, Some(Permissions::Unrestricted))
        )
        .unwrap(),
        ["/model gpt-6", "/permissions allow-all"]
    );
    assert_eq!(
        lines(Agent::Cursor, settings(None, None, Some(Permissions::Ask))).unwrap(),
        ["/run-everything off"]
    );
    assert_eq!(
        lines(
            Agent::Antigravity,
            settings(Some("gemini-4-hydrogen"), Some("xhigh"), None)
        )
        .unwrap(),
        ["/model gemini-4-hydrogen", "/effort xhigh"]
    );
    for (agent, change, what) in [
        (
            Agent::Claude,
            settings(None, None, Some(Permissions::Ask)),
            "permissions",
        ),
        (
            Agent::Codex,
            settings(None, None, Some(Permissions::Unrestricted)),
            "permissions",
        ),
        (Agent::Copilot, settings(None, Some("high"), None), "effort"),
        (Agent::Opencode, settings(Some("big"), None, None), "model"),
        (
            Agent::Antigravity,
            settings(None, None, Some(Permissions::Ask)),
            "permissions",
        ),
    ] {
        let label = agent.label();
        assert_eq!(
            lines(agent, change).unwrap_err(),
            format!(
                "{label} can't change its {what} while it runs. Choose it when you start {label}."
            )
        );
    }
    assert!(
        lines(
            Agent::Claude,
            AgentSettings {
                output: Some(Output::Quiet),
                ..AgentSettings::default()
            }
        )
        .is_err()
    );
    assert!(
        lines(Agent::Claude, settings(Some("opus\r/exit"), None, None)).is_err(),
        "a value is one word, so it types no other command"
    );
}

#[test]
fn a_restart_keeps_what_kodosi_changed_while_the_agent_ran() {
    let arguments = [
        "claude",
        "--model",
        "opus",
        "--effort",
        "high",
        "--permission-mode",
        "manual",
        "--settings",
        r#"{"viewMode":"verbose"}"#,
    ]
    .map(str::to_owned);
    let changed = AgentSettings {
        model: Some("sonnet".to_owned()),
        output: Some(Output::Quiet),
        ..AgentSettings::default()
    };
    assert_eq!(
        Agent::Claude.chosen(&arguments, &changed, true).join(" "),
        format!("--model sonnet --permission-mode manual --settings '{CLAUDE_QUIET}'"),
        "a new model starts at its own effort"
    );
    let effort = AgentSettings {
        effort: Some("low".to_owned()),
        permissions: Some(Permissions::Unrestricted),
        ..AgentSettings::default()
    };
    assert_eq!(
        Agent::Claude.chosen(&arguments, &effort, true).join(" "),
        r#"--model opus --effort low --settings '{"viewMode":"verbose"}' --dangerously-skip-permissions"#
    );
    assert_eq!(
        Agent::Claude
            .chosen(&arguments, &AgentSettings::default(), true)
            .join(" "),
        r#"--model opus --effort high --permission-mode manual --settings '{"viewMode":"verbose"}'"#
    );
}
