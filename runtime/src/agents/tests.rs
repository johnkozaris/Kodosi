use std::{fs, path::Path, sync::Arc};

use super::{
    accounts::{self, referenced_home},
    launch::plan,
    models::{self, codex, copilot, cursor, listed_values, opencode},
    shell::Facts,
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

#[cfg(unix)]
#[test]
fn probe_output_is_read_between_the_markers_only() {
    let facts = shell::parse(
        "motd line\n\u{1e}kodosi-agents\npath\t/opt/bin:/usr/bin\nvariable\tCODEX_HOME\t/srv/codex\nvariable\tPATH\t/x\nvariable\tCLAUDE_CONFIG_DIR\t\nfunction\tclaude-x\tbody\nfunction\tbad name\tbody\n\u{1e}end\nfunction\tlate\tbody\n",
    )
    .expect("markers");
    assert_eq!(facts.path.as_deref(), Some("/opt/bin:/usr/bin".as_ref()));
    assert_eq!(
        facts.variables,
        [("CODEX_HOME".to_owned(), "/srv/codex".to_owned())]
    );
    assert_eq!(facts.definitions, vec![definition("claude-x", "body")]);
    assert!(shell::parse("no markers").is_none());
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

fn put(home: &Path, path: &str, text: &str) {
    let path = home.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn claude_sign_in(email: &str) -> String {
    format!(r#"{{"oauthAccount":{{"emailAddress":"{email}"}}}}"#)
}

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
fn the_folder_that_the_shell_chooses_is_the_first_account() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    put(home, ".claude.json", &claude_sign_in("old@example.com"));
    put(
        home,
        ".claude-main/.claude.json",
        &claude_sign_in("main@example.com"),
    );
    let facts = Facts {
        path: Some("/nowhere".into()),
        variables: vec![("CLAUDE_CONFIG_DIR".to_owned(), "~/.claude-main".to_owned())],
        definitions: vec![definition("claude", r#"command claude "$@""#)],
    };
    let catalog = accounts::catalog(home, &facts);
    assert_eq!(
        rows(&catalog, Agent::Claude),
        ["default main@example.com ~/.claude-main"]
    );
    assert_eq!(catalog.agents[0].accounts[0].command, "claude");
}

#[cfg(unix)]
#[test]
fn the_catalog_says_where_the_shell_finds_the_kodosi_command() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().unwrap();
    let home = &kodosi_os::fs::canonicalize(root.path()).unwrap();
    let facts = Facts {
        path: Some(home.join(".local/bin").into_os_string()),
        ..Facts::default()
    };
    assert_eq!(accounts::catalog(home, &facts).kodosi, None);
    put(home, ".local/bin/kodosi", "#!/bin/sh\n");
    fs::set_permissions(
        home.join(".local/bin/kodosi"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert_eq!(
        accounts::catalog(home, &facts).kodosi.as_deref(),
        Some("~/.local/bin/kodosi")
    );
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
    assert!(accounts::add(home, Agent::Gemini, "work", &[]).is_err());
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

#[cfg(windows)]
#[test]
fn windows_lists_the_usual_account_and_adds_none() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    put(home, ".claude.json", &claude_sign_in("ada@example.com"));
    put(
        home,
        ".claude-work/.claude.json",
        &claude_sign_in("work@example.com"),
    );
    put(home, "bin/claude.exe", "");
    let facts = Facts {
        path: Some(home.join("bin").into_os_string()),
        ..Facts::default()
    };
    let catalog = accounts::catalog(home, &facts);
    assert_eq!(
        rows(&catalog, Agent::Claude),
        ["default ada@example.com ~/.claude"]
    );
    assert!(!catalog.agents[0].adds_accounts);
    assert!(accounts::add(home, Agent::Claude, "night", &[]).is_err());
    assert!(!home.join(".claude-night").exists());
}

#[test]
fn identities_need_the_agent_record() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let copilot = home.join(".copilot");
    fs::create_dir_all(&copilot).unwrap();
    fs::write(
        copilot.join("config.json"),
        "// managed\n{\"lastLoggedInUser\":{\"host\":\"https://github.com\",\"login\":\"ada\"}}",
    )
    .unwrap();
    assert_eq!(
        accounts::identity(Agent::Copilot, home, &copilot).as_deref(),
        Some("ada")
    );
    assert_eq!(accounts::identity(Agent::Claude, home, &copilot), None);
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
fn copilot_cursor_and_opencode_lists_become_models() {
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
}

#[test]
fn shell_words_quote_what_a_shell_would_change() {
    assert_eq!(shell_word("opus").as_deref(), Some("opus"));
    assert_eq!(
        shell_word("claude-fable-5-1[1m]").as_deref(),
        Some("'claude-fable-5-1[1m]'")
    );
    assert_eq!(shell_word("it's"), None);
    assert_eq!(shell_word(""), None);
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
    for agent in [Agent::Claude, Agent::Copilot, Agent::Gemini] {
        assert_eq!(agent.with_context(agent.program(), &notes), agent.program());
    }
    assert!(!CONTEXT.trim().contains(['"', '%', '\n']));
}

#[test]
fn only_a_codex_that_a_person_started_by_hand_misses_kodosi_s_note() {
    let notes = Notes::default();
    let words = |line: &str| line.split(' ').map(str::to_owned).collect::<Vec<_>>();
    assert!(Agent::Codex.started_without_note(&words("codex -m gpt-5.6-sol"), None, &notes));
    assert!(!Agent::Codex.started_without_note(
        &[
            "codex".to_owned(),
            "-c".to_owned(),
            format!("developer_instructions={}", CONTEXT.trim()),
        ],
        None,
        &notes
    ));
    assert!(!Agent::Codex.started_without_note(&[], None, &notes));
    assert!(!Agent::Claude.started_without_note(&words("claude"), None, &notes));
    assert!(!Agent::Copilot.started_without_note(&words("copilot"), None, &notes));
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
        let stub = bin.join(program);
        fs::write(
            &stub,
            "#!/bin/sh\nprintf '%s\\0' \"$OPENCODE_CONFIG_CONTENT\" \"$@\"\n",
        )
        .unwrap();
        fs::set_permissions(&stub, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
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
fn agents_find_the_note_and_the_plugin_in_the_folder_that_kodosi_writes() {
    let folder = tempfile::tempdir().unwrap();
    let notes = kodosi_os::fs::canonicalize(folder.path())
        .unwrap()
        .join("agents");
    assert_eq!(plugin_dir(&notes), None);
    write_notes(&notes).unwrap();
    write_notes(&notes).unwrap();
    let text = fs::read_to_string(
        notes
            .join(".github")
            .join("instructions")
            .join("kodosi.instructions.md"),
    )
    .unwrap();
    assert_eq!(
        text,
        format!("---\napplyTo: \"**\"\n---\n\n{}\n", CONTEXT.trim())
    );
    let plugin = notes.join("plugin");
    for (root, manifest) in [
        (plugin.clone(), ".claude-plugin"),
        (notes.join("cursor"), ".cursor-plugin"),
    ] {
        let manifest: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(root.join(manifest).join("plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "kodosi");
        let skills = root.join("skills");
        for (skill, text) in [
            ("kodosi", SKILL),
            ("kodosi-canvas", CANVAS_SKILL),
            ("kodosi-documents", DOCUMENTS_SKILL),
        ] {
            let file = skills.join(skill).join("SKILL.md");
            assert_eq!(fs::read_to_string(file).unwrap(), text);
        }
    }
    let hooks: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(plugin.join("hooks").join("hooks.json")).unwrap())
            .unwrap();
    assert_eq!(
        hooks["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "kodosi skill context"
    );
    assert_eq!(plugin_dir(&notes).as_deref(), plugin.to_str());
    assert!(
        skill_dirs(&notes)
            .unwrap()
            .ends_with(plugin.join("skills").to_str().unwrap())
    );
}

fn claude_with_accounts() -> AgentInfo {
    AgentInfo {
        agent: Agent::Claude,
        program: "claude".to_owned(),
        installed: true,
        accounts: ["default", "work"]
            .into_iter()
            .map(|name| AgentAccount {
                name: name.to_owned(),
                command: if name == "default" {
                    "claude".to_owned()
                } else {
                    format!("CLAUDE_CONFIG_DIR=~/.claude-{name} claude")
                },
                identity: None,
                folder: None,
            })
            .collect(),
        install: String::new(),
        adds_accounts: true,
        reports_usage: true,
        permissions: Agent::Claude.permissions(),
        outputs: Agent::Claude.outputs(),
        default_permissions: Agent::Claude.default_permissions(),
        default_output: Agent::Claude.default_output(),
        fast: true,
    }
}

fn claude_models() -> AgentModels {
    let option = |id: &str| AgentOption {
        id: id.to_owned(),
        argument: format!("--effort {id}"),
    };
    AgentModels {
        models: vec![
            AgentModel {
                id: "opus".to_owned(),
                label: "Opus".to_owned(),
                argument: "--model opus".to_owned(),
                efforts: Vec::new(),
            },
            AgentModel {
                id: "haiku".to_owned(),
                label: "Haiku".to_owned(),
                argument: "--model haiku".to_owned(),
                efforts: vec!["low".to_owned()],
            },
        ],
        efforts: vec![option("low"), option("high")],
    }
}

#[test]
fn an_agent_start_takes_the_last_choice_and_explicit_values_win() {
    let info = claude_with_accounts();
    let models = claude_models();
    let saved = AgentPick {
        agent: Agent::Claude,
        account: Some("work".to_owned()),
        model: Some("opus".to_owned()),
        effort: Some("high".to_owned()),
        permissions: Some(Permissions::Unrestricted),
        output: Some(Output::Standard),
        fast: false,
    };
    let start = |account: Option<&str>, model: Option<&str>, effort: Option<&str>| AgentStart {
        agent: Agent::Claude,
        account: account.map(str::to_owned),
        model: model.map(str::to_owned),
        effort: effort.map(str::to_owned),
        permissions: None,
        output: None,
        fast: None,
    };
    let plan =
        |info: &AgentInfo, models: Option<&AgentModels>, saved: &AgentPick, start: &AgentStart| {
            plan(info, models, saved, start, Path::new("/bin/zsh"))
        };
    let (line, pick) = plan(&info, Some(&models), &saved, &start(None, None, None)).unwrap();
    assert_eq!(
        line,
        "CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus --effort high --dangerously-skip-permissions"
    );
    assert_eq!(pick, saved);

    let (line, _) = plan(
        &info,
        Some(&models),
        &saved,
        &start(Some("default"), Some(""), None),
    )
    .unwrap();
    assert_eq!(line, "claude --effort high --dangerously-skip-permissions");

    let (line, pick) = plan(
        &info,
        Some(&models),
        &saved,
        &start(None, Some("haiku"), None),
    )
    .unwrap();
    assert_eq!(
        line,
        "CLAUDE_CONFIG_DIR=~/.claude-work claude --model haiku --dangerously-skip-permissions"
    );
    assert_eq!(pick.effort, None);

    let stale = AgentPick {
        account: Some("gone".to_owned()),
        model: Some("retired".to_owned()),
        ..saved.clone()
    };
    let (line, _) = plan(&info, Some(&models), &stale, &start(None, None, None)).unwrap();
    assert_eq!(line, "claude --effort high --dangerously-skip-permissions");

    assert_eq!(
        plan(
            &info,
            Some(&models),
            &saved,
            &start(None, Some("fable"), None)
        )
        .unwrap_err(),
        "Claude Code has no model fable. Models: opus, haiku."
    );
    assert_eq!(
        plan(
            &info,
            Some(&models),
            &saved,
            &start(None, Some("haiku"), Some("high"))
        )
        .unwrap_err(),
        "Claude Code has no effort high here. Efforts: low."
    );
    assert_eq!(
        plan(&info, None, &saved, &start(Some("home"), None, None)).unwrap_err(),
        "Claude Code has no account named home. Accounts: default, work."
    );
}

#[test]
fn a_terminal_like_a_running_agent_takes_what_its_host_knows_of_it() {
    let home = Path::new("/home/ana");
    let mut accounts = claude_with_accounts().accounts;
    for account in &mut accounts {
        account.folder = Some(match account.name.as_str() {
            "default" => "~/.claude".to_owned(),
            name => format!("~/.claude-{name}"),
        });
    }
    let settings = |model: Option<&str>, effort: Option<&str>, permissions| AgentSettings {
        model: model.map(str::to_owned),
        effort: effort.map(str::to_owned),
        permissions,
        ..AgentSettings::default()
    };
    let like_in = |accounts: &[AgentAccount], folder: Option<&str>, settings, pending| AgentStart {
        fast: None,
        ..RunningAgent {
            agent: Agent::Claude,
            account_folder: folder.map(|folder| home.join(folder)),
            settings,
            pending,
        }
        .start(accounts, home)
    };
    let like = |folder, settings, pending| like_in(&accounts, folder, settings, pending);
    let start = |account: Option<&str>, model: Option<&str>, effort: Option<&str>, permissions| {
        AgentStart {
            agent: Agent::Claude,
            account: account.map(str::to_owned),
            model: model.map(str::to_owned),
            effort: effort.map(str::to_owned),
            permissions,
            output: None,
            fast: None,
        }
    };
    let running = Some(settings(Some("opus"), Some("low"), Some(Permissions::Auto)));

    assert_eq!(
        like(
            Some(".claude-work"),
            running.clone(),
            Some(settings(None, Some("high"), Some(Permissions::Ask)))
        ),
        start(
            Some("work"),
            Some("opus"),
            Some("high"),
            Some(Permissions::Ask)
        ),
        "a change that waits for the end of the turn wins"
    );
    assert_eq!(
        like(
            Some(".claude"),
            running,
            Some(settings(Some("haiku"), None, None))
        ),
        start(
            Some("default"),
            Some("haiku"),
            Some(""),
            Some(Permissions::Auto)
        ),
        "a new model runs at its own effort"
    );
    assert_eq!(
        like(
            Some(".claude"),
            Some(settings(None, None, Some(Permissions::Unrestricted))),
            None
        ),
        start(
            Some("default"),
            Some(""),
            Some(""),
            Some(Permissions::Unrestricted)
        ),
        "a model and an effort that the agent started without are its own"
    );
    assert_eq!(
        like(Some(".claude-gone"), None, None),
        start(None, None, None, None),
        "an account outside the list and unknown choices stay the last choice"
    );
    let mut added = accounts.clone();
    added.insert(
        0,
        AgentAccount {
            name: "alpha".to_owned(),
            command: "CLAUDE_CONFIG_DIR=~/.claude-alpha claude".to_owned(),
            identity: None,
            folder: Some("~/.claude-alpha".to_owned()),
        },
    );
    assert_eq!(
        like_in(&added, Some(".claude-work"), None, None).account,
        Some("work".to_owned()),
        "an account that came before it in the list since it started changes nothing"
    );
}

#[test]
fn a_terminal_like_a_running_agent_takes_the_fast_mode_that_its_host_knows() {
    let fast = |fast| {
        Some(AgentSettings {
            fast,
            ..AgentSettings::default()
        })
    };
    let like = |settings, pending| {
        RunningAgent {
            agent: Agent::Codex,
            account_folder: None,
            settings,
            pending,
        }
        .start(&[], Path::new("/home/ana"))
        .fast
    };
    assert_eq!(like(fast(Some(true)), None), Some(true));
    assert_eq!(
        like(fast(Some(true)), fast(Some(false))),
        Some(false),
        "a fast mode that waits for the end of the turn wins"
    );
    assert_eq!(
        like(fast(None), None),
        Some(false),
        "an agent that started without a fast mode keeps its own setting"
    );
    assert_eq!(
        like(None, None),
        None,
        "an agent that Kodosi knows nothing of starts with the last choice"
    );
}

fn installed(agent: Agent) -> AgentInfo {
    AgentInfo {
        agent,
        program: agent.program().to_owned(),
        installed: true,
        accounts: vec![AgentAccount {
            name: agent.program().to_owned(),
            command: agent.program().to_owned(),
            identity: None,
            folder: None,
        }],
        install: String::new(),
        adds_accounts: false,
        reports_usage: false,
        permissions: agent.permissions(),
        outputs: agent.outputs(),
        default_permissions: agent.default_permissions(),
        default_output: agent.default_output(),
        fast: agent.has_fast(),
    }
}

#[test]
fn an_agent_starts_with_its_last_permissions_and_output_else_unrestricted_and_quiet() {
    let line = |agent: Agent,
                saved: Option<Permissions>,
                permissions: Option<Permissions>,
                output: Option<Output>| {
        let saved = AgentPick {
            agent,
            account: None,
            model: None,
            effort: None,
            permissions: saved,
            output: None,
            fast: false,
        };
        let start = AgentStart {
            agent,
            account: None,
            model: None,
            effort: None,
            permissions,
            output,
            fast: None,
        };
        plan(
            &installed(agent),
            None,
            &saved,
            &start,
            Path::new("/bin/zsh"),
        )
        .map(|(line, pick)| (line, pick.permissions))
    };
    let (ask, auto, unrestricted) = (
        Permissions::Ask,
        Permissions::Auto,
        Permissions::Unrestricted,
    );

    assert_eq!(
        line(Agent::Claude, None, None, None).unwrap(),
        (
            format!("claude --dangerously-skip-permissions --settings '{CLAUDE_QUIET}'"),
            Some(unrestricted)
        )
    );
    assert_eq!(
        line(Agent::Claude, Some(ask), None, Some(Output::Standard)).unwrap(),
        ("claude --permission-mode manual".to_owned(), Some(ask))
    );
    assert_eq!(
        line(Agent::Claude, Some(unrestricted), Some(auto), None)
            .unwrap()
            .0,
        format!("claude --permission-mode auto --settings '{CLAUDE_QUIET}'")
    );
    assert_eq!(
        line(Agent::Codex, None, None, None).unwrap().0,
        "codex --dangerously-bypass-approvals-and-sandbox -c hide_agent_reasoning=true \
         -c show_raw_agent_reasoning=false -c model_reasoning_summary=none -c tui.auto_recap=false \
         -c tui.show_tooltips=false -c tui.animations=false -c 'tui.status_line=[]'"
    );
    assert_eq!(
        line(Agent::Copilot, None, None, None).unwrap(),
        ("copilot --yolo".to_owned(), Some(unrestricted))
    );
    assert_eq!(
        line(Agent::Opencode, None, None, None).unwrap(),
        ("opencode".to_owned(), Some(ask))
    );
    assert_eq!(
        line(Agent::Cursor, Some(auto), None, None).unwrap().0,
        "cursor-agent --force"
    );
    assert_eq!(
        line(Agent::Cursor, None, Some(auto), None).unwrap_err(),
        "Cursor has no permissions auto. Permissions: ask, unrestricted."
    );
    assert_eq!(
        line(Agent::Copilot, None, None, Some(Output::Detailed)).unwrap_err(),
        "Copilot CLI has no output detailed. Outputs: quiet, standard."
    );
}

#[test]
fn an_agent_starts_fast_when_chosen_and_else_with_its_own_setting() {
    let line = |agent: Agent, saved: bool, fast: Option<bool>| {
        let saved = AgentPick {
            agent,
            account: None,
            model: None,
            effort: None,
            permissions: Some(Permissions::Ask),
            output: None,
            fast: saved,
        };
        let start = AgentStart {
            agent,
            account: None,
            model: None,
            effort: None,
            permissions: None,
            output: Some(Output::Standard),
            fast,
        };
        plan(
            &installed(agent),
            None,
            &saved,
            &start,
            Path::new("/bin/zsh"),
        )
        .map(|(line, pick)| (line, pick.fast))
    };
    assert_eq!(
        line(Agent::Claude, true, None).unwrap(),
        (
            "claude --permission-mode manual --fast-mode on".to_owned(),
            true
        )
    );
    assert_eq!(
        line(Agent::Claude, true, Some(false)).unwrap(),
        ("claude --permission-mode manual".to_owned(), false),
        "off leaves the agent's own setting"
    );
    assert_eq!(
        line(Agent::Codex, false, None).unwrap(),
        ("codex -a on-request -s read-only".to_owned(), false)
    );
    assert_eq!(
        line(Agent::Copilot, true, None).unwrap(),
        ("copilot".to_owned(), false),
        "an agent without fast mode starts as it is"
    );
    assert_eq!(
        line(Agent::Copilot, false, Some(true)).unwrap_err(),
        "Copilot CLI has no fast mode."
    );
}

#[cfg(unix)]
#[test]
fn a_running_agent_shows_each_permissions_output_and_fast_mode_that_kodosi_starts_it_with() {
    let shell = Path::new("/bin/sh");
    let arguments = |line: &str| {
        let printed = std::process::Command::new(shell)
            .arg("-c")
            .arg(format!(r"printf '%s\0' {line}"))
            .env_clear()
            .output()
            .unwrap();
        String::from_utf8(printed.stdout)
            .unwrap()
            .split_terminator('\0')
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    for agent in [Agent::Claude, Agent::Codex, Agent::Copilot] {
        let mut outputs = agent.outputs();
        if outputs.is_empty() {
            outputs.push(agent.default_output());
        }
        let speeds: &[Option<bool>] = if agent.has_fast() {
            &[None, Some(true)]
        } else {
            &[None]
        };
        for permissions in agent.permissions() {
            for &output in &outputs {
                for &fast in speeds {
                    let start = AgentStart {
                        agent,
                        account: None,
                        model: None,
                        effort: None,
                        permissions: Some(permissions),
                        output: Some(output),
                        fast,
                    };
                    let saved = AgentPick {
                        agent,
                        account: None,
                        model: None,
                        effort: None,
                        permissions: None,
                        output: None,
                        fast: false,
                    };
                    let (line, _) = plan(&installed(agent), None, &saved, &start, shell).unwrap();
                    let notes = Notes {
                        shell: shell.to_owned(),
                        ..Notes::default()
                    };
                    let line = agent.with_context(&line, &notes);
                    let started = agent.started_with(&arguments(&line));
                    assert_eq!(started.permissions, Some(permissions), "{line}");
                    if agent != Agent::Copilot {
                        assert_eq!(started.output, Some(output), "{line}");
                    }
                    assert_eq!(started.fast, fast, "{line}");
                }
            }
        }
    }
}

#[tokio::test]
async fn the_launch_choice_survives_a_restart() {
    let root = tempfile::tempdir().unwrap();
    let pick = AgentPick {
        agent: Agent::Codex,
        account: Some("default".to_owned()),
        model: Some("gpt-6".to_owned()),
        effort: None,
        permissions: Some(Permissions::Auto),
        output: Some(Output::Detailed),
        fast: true,
    };
    let launcher = Arc::new(Launcher::load(
        root.path(),
        root.path(),
        Path::new("/bin/sh"),
    ));
    launcher.choose(&AgentChoiceChange::Pick(pick.clone()));
    launcher.choose(&AgentChoiceChange::Tab {
        agent: Some(Agent::Codex),
    });
    launcher.save().await;
    let choice = Launcher::load(root.path(), root.path(), Path::new("/bin/sh")).choice();
    assert_eq!(choice.agent, Some(Agent::Codex));
    assert_eq!(choice.pick(Agent::Codex), pick);
}

#[cfg(unix)]
fn shell_finding(folder: &Path, programs: &[&str]) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    let bin = folder.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = |path: std::path::PathBuf, text: String| {
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    };
    for program in programs {
        executable(bin.join(program), "#!/bin/sh\n".to_owned());
    }
    executable(
        folder.join("sh"),
        format!(
            "#!/bin/sh\n[ \"$1\" = -l ] && shift\nPATH='{}' exec /bin/sh \"$@\"\n",
            bin.display()
        ),
    )
}

#[cfg(unix)]
#[tokio::test]
async fn a_person_who_never_chose_starts_on_the_first_installed_agent() {
    let root = tempfile::tempdir().unwrap();
    let nothing = shell_finding(&root.path().join("nothing"), &[]);
    let agents = shell_finding(&root.path().join("agents"), &["codex", "copilot"]);
    let load = |data: &str, shell: &Path| {
        let data = root.path().join(data);
        fs::create_dir_all(&data).unwrap();
        Arc::new(Launcher::load(&data, root.path(), shell))
    };
    let first = async |launcher: &Launcher| launcher.catalog().await.unwrap().choice.agent;

    let new = load("new", &agents);
    assert_eq!(new.choice().agent, None, "before Kodosi knows the agents");
    assert_eq!(first(&new).await, Some(Agent::Codex));
    assert_eq!(first(&load("bare", &nothing)).await, None);

    let chosen = load("chosen", &agents);
    chosen.choose(&AgentChoiceChange::Tab {
        agent: Some(Agent::Copilot),
    });
    assert_eq!(first(&chosen).await, Some(Agent::Copilot));

    let plain = load("plain", &agents);
    plain.choose(&AgentChoiceChange::Tab { agent: None });
    assert_eq!(first(&plain).await, None);
    Arc::clone(&plain).save().await;
    assert_eq!(
        first(&load("plain", &agents)).await,
        None,
        "the plain terminal stays the choice after a restart"
    );
}

#[cfg(unix)]
#[test]
fn a_running_agent_takes_the_place_of_its_account_folder() {
    let home = Path::new("/home/ana");
    let account = |name: &str, folder: &str| AgentAccount {
        name: name.to_owned(),
        command: "claude".to_owned(),
        identity: None,
        folder: Some(folder.to_owned()),
    };
    let accounts = [
        account("default", "~/.claude"),
        account("alt", "~/.claude-alt"),
        account("team", "/srv/claude-team"),
    ];
    let place = |folder: &str| accounts::place(&accounts, home, Path::new(folder));
    assert_eq!(place("/home/ana/.claude"), 0);
    assert_eq!(place("/home/ana/.claude-alt"), 1);
    assert_eq!(place("/srv/claude-team"), 2);
    assert_eq!(place("/home/ana/.claude-new"), 3);
}

#[cfg(unix)]
#[test]
fn an_agent_folder_comes_from_its_variable_or_the_usual_folder() {
    let home = Path::new("/home/ana");
    let folder = |agent: Agent, variable: Option<&str>| {
        agent.account_folder(home, variable.map(str::to_owned))
    };
    assert_eq!(folder(Agent::Claude, None), Some(home.join(".claude")));
    assert_eq!(
        folder(Agent::Claude, Some("/home/ana/.claude-alt/")),
        Some(home.join(".claude-alt"))
    );
    assert_eq!(
        folder(Agent::Claude, Some(".claude-alt")),
        Some(home.join(".claude"))
    );
    assert_eq!(
        folder(Agent::Codex, Some("/srv/codex")),
        Some(Path::new("/srv/codex").to_path_buf())
    );
    assert_eq!(folder(Agent::Gemini, None), None);
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
        lines(Agent::Gemini, settings(Some("pro"), None, None)).unwrap(),
        ["/model set pro"]
    );
    assert_eq!(
        lines(Agent::Cursor, settings(None, None, Some(Permissions::Ask))).unwrap(),
        ["/run-everything off"]
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
fn a_running_agent_changes_its_fast_mode_with_its_own_commands() {
    let change = |model: Option<&str>, effort: Option<&str>, fast| AgentSettings {
        model: model.map(str::to_owned),
        effort: effort.map(str::to_owned),
        fast: Some(fast),
        ..AgentSettings::default()
    };
    let lines = |agent: Agent, change: AgentSettings| {
        agent
            .live_lines(&change)
            .map(|lines| lines.into_iter().map(|(line, _)| line).collect::<Vec<_>>())
    };
    assert_eq!(
        lines(Agent::Claude, change(Some("opus"), None, true)).unwrap(),
        ["/model opus", "/fast on"]
    );
    assert_eq!(
        lines(Agent::Claude, change(None, None, false)).unwrap(),
        ["/fast off"]
    );
    assert_eq!(
        lines(Agent::Codex, change(None, None, true)).unwrap(),
        ["/model --speed=fast"]
    );
    assert_eq!(
        lines(Agent::Codex, change(Some("gpt-6"), Some("high"), false)).unwrap(),
        ["/model gpt-6 --reasoning=high", "/model --speed=standard"],
        "a later change of one of them replaces only its own command"
    );
    assert_eq!(
        lines(Agent::Copilot, change(None, None, true)).unwrap_err(),
        "Copilot CLI has no fast mode."
    );
}

#[test]
fn a_new_model_runs_at_its_own_effort() {
    let now = AgentSettings {
        model: Some("opus".to_owned()),
        effort: Some("high".to_owned()),
        ..AgentSettings::default()
    };
    let model = AgentSettings {
        model: Some("sonnet".to_owned()),
        ..AgentSettings::default()
    };
    assert_eq!(now.changed(&model).effort, None);
    let effort = AgentSettings {
        effort: Some("low".to_owned()),
        ..AgentSettings::default()
    };
    assert_eq!(now.changed(&effort).model.as_deref(), Some("opus"));
    assert_eq!(now.changed(&effort).effort.as_deref(), Some("low"));
}

#[test]
fn a_running_agent_shows_what_it_started_with() {
    let started = |agent: Agent, arguments: &[&str]| {
        agent.started_with(
            &arguments
                .iter()
                .map(|argument| (*argument).to_owned())
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        started(
            Agent::Claude,
            &[
                "claude",
                "--model",
                "opus",
                "--effort",
                "high",
                "--dangerously-skip-permissions",
                "--settings",
                r#"{"viewMode":"focus"}"#
            ]
        ),
        AgentSettings {
            model: Some("opus".to_owned()),
            effort: Some("high".to_owned()),
            permissions: Some(Permissions::Unrestricted),
            output: Some(Output::Quiet),
            fast: None,
        }
    );
    assert_eq!(
        started(
            Agent::Codex,
            &[
                "codex",
                "-m",
                "gpt-5.1-codex",
                "-c",
                "model_reasoning_effort=high",
                "-a",
                "on-request",
                "-s",
                "workspace-write"
            ]
        ),
        AgentSettings {
            model: Some("gpt-5.1-codex".to_owned()),
            effort: Some("high".to_owned()),
            permissions: Some(Permissions::Auto),
            output: Some(Output::Standard),
            fast: None,
        }
    );
    assert_eq!(
        started(Agent::Claude, &["claude", "--fast-mode", "off"]).fast,
        Some(false)
    );
    assert_eq!(
        started(Agent::Codex, &["codex", "-c", r#"service_tier="priority""#]).fast,
        Some(true),
        "Codex also calls its fast tier priority"
    );
    assert_eq!(
        started(Agent::Codex, &["codex", "-c", "service_tier=flex"]).fast,
        None
    );
    assert_eq!(
        started(Agent::Copilot, &["copilot", "--allow-all"]).permissions,
        Some(Permissions::Unrestricted)
    );
    assert_eq!(
        started(Agent::Copilot, &["copilot"]).permissions,
        Some(Permissions::Ask)
    );
    assert_eq!(
        started(Agent::Claude, &[]),
        AgentSettings::default(),
        "Windows shows no arguments, so nothing is known"
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
        Agent::Claude.chosen(&arguments, &changed).join(" "),
        format!("--model sonnet --permission-mode manual --settings '{CLAUDE_QUIET}'"),
        "a new model starts at its own effort"
    );
    let effort = AgentSettings {
        effort: Some("low".to_owned()),
        permissions: Some(Permissions::Unrestricted),
        ..AgentSettings::default()
    };
    assert_eq!(
        Agent::Claude.chosen(&arguments, &effort).join(" "),
        r#"--model opus --effort low --settings '{"viewMode":"verbose"}' --dangerously-skip-permissions"#
    );
    assert_eq!(
        Agent::Claude
            .chosen(&arguments, &AgentSettings::default())
            .join(" "),
        r#"--model opus --effort high --permission-mode manual --settings '{"viewMode":"verbose"}'"#
    );
}

#[test]
fn a_restart_keeps_the_fast_mode_that_the_agent_started_with_or_that_kodosi_changed() {
    let fast = |fast| AgentSettings {
        fast: Some(fast),
        ..AgentSettings::default()
    };
    let claude = ["claude", "--settings", CLAUDE_QUIET, "--fast-mode", "on"].map(str::to_owned);
    assert_eq!(
        Agent::Claude
            .chosen(&claude, &AgentSettings::default())
            .join(" "),
        format!("--settings '{CLAUDE_QUIET}' --fast-mode on")
    );
    assert_eq!(
        Agent::Claude.chosen(&claude, &fast(false)).join(" "),
        format!("--settings '{CLAUDE_QUIET}' --fast-mode off")
    );
    assert_eq!(
        Agent::Claude
            .chosen(&["claude".to_owned()], &fast(true))
            .join(" "),
        "--fast-mode on",
        "a fast mode that Kodosi turned on while the agent ran is added"
    );
    let codex = [
        "codex",
        "-c",
        "hide_agent_reasoning=true",
        "-c",
        "service_tier=fast",
    ]
    .map(str::to_owned);
    assert_eq!(
        Agent::Codex
            .chosen(&codex, &AgentSettings::default())
            .join(" "),
        "-c hide_agent_reasoning=true -c service_tier=fast"
    );
    assert_eq!(
        Agent::Codex.chosen(&codex, &fast(false)).join(" "),
        "-c hide_agent_reasoning=true -c service_tier=default"
    );
}
