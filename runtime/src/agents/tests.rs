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
claudesim() { CLAUDE_CONFIG_DIR="$HOME/.claude-alt" claude "$@"; }
alias codex-sw='CODEX_HOME=~/.codex-sw codex'
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
        assert_eq!(names, ["claude", "claudesim", "codex-sw"], "{shell}");
        let claudesim = facts
            .definitions
            .iter()
            .find(|definition| definition.name == "claudesim")
            .expect("function");
        assert!(
            claudesim.text.contains(".claude-alt"),
            "{shell}: {claudesim:?}"
        );
        assert!(!claudesim.text.contains('\n'));
        let codex_sw = facts
            .definitions
            .iter()
            .find(|definition| definition.name == "codex-sw")
            .expect("alias");
        assert_eq!(codex_sw.text, "CODEX_HOME=~/.codex-sw codex", "{shell}");
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
        claude(r#"export CLAUDE_CONFIG_DIR="$HOME/.claude-alt"; claude "$@""#),
        Some(home.join(".claude-alt"))
    );
    assert_eq!(
        claude("env 'CLAUDE_CONFIG_DIR=/srv/claude two' claude"),
        Some("/srv/claude two".into())
    );
    assert_eq!(claude(r#"_claude_profile "$HOME/.claude-alt" "$@""#), None);
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
        ".claude-alt/.claude.json",
        &claude_sign_in("sim@example.com"),
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
        ".codex-sw/auth.json",
        &format!(r#"{{"tokens":{{"id_token":"{}"}}}}"#, jwt("sw@example.com")),
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
            definition("claude-alt", "CLAUDE_CONFIG_DIR=~/.claude-alt claude"),
            definition("claude-gone", "CLAUDE_CONFIG_DIR=~/.claude-gone claude"),
            definition("cc", "claude --dangerously-skip-permissions"),
        ],
    };
    let catalog = accounts::catalog(home, &facts);
    assert_eq!(
        rows(&catalog, Agent::Claude),
        [
            "default ada@example.com ~/.claude",
            "alt sim@example.com ~/.claude-alt",
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
        ["default ~/.codex", "sw sw@example.com ~/.codex-sw"]
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
        .unwrap_err(),
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
    };
    let start = |account: Option<&str>, model: Option<&str>, effort: Option<&str>| AgentStart {
        agent: Agent::Claude,
        account: account.map(str::to_owned),
        model: model.map(str::to_owned),
        effort: effort.map(str::to_owned),
    };
    let (line, pick) = plan(&info, Some(&models), &saved, &start(None, None, None)).unwrap();
    assert_eq!(
        line,
        "CLAUDE_CONFIG_DIR=~/.claude-work claude --model opus --effort high"
    );
    assert_eq!(pick, saved);

    let (line, _) = plan(
        &info,
        Some(&models),
        &saved,
        &start(Some("default"), Some(""), None),
    )
    .unwrap();
    assert_eq!(line, "claude --effort high");

    let (line, pick) = plan(
        &info,
        Some(&models),
        &saved,
        &start(None, Some("haiku"), None),
    )
    .unwrap();
    assert_eq!(
        line,
        "CLAUDE_CONFIG_DIR=~/.claude-work claude --model haiku"
    );
    assert_eq!(pick.effort, None);

    let stale = AgentPick {
        account: Some("gone".to_owned()),
        model: Some("retired".to_owned()),
        ..saved.clone()
    };
    let (line, _) = plan(&info, Some(&models), &stale, &start(None, None, None)).unwrap();
    assert_eq!(line, "claude --effort high");

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

#[tokio::test]
async fn the_launch_choice_survives_a_restart() {
    let root = tempfile::tempdir().unwrap();
    let pick = AgentPick {
        agent: Agent::Codex,
        account: Some("default".to_owned()),
        model: Some("gpt-6".to_owned()),
        effort: None,
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
