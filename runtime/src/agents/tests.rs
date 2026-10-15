use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use super::{
    accounts::{self, referenced_home},
    learned,
    models::{self, codex, copilot, cursor, listed_values, opencode},
    shell::{Definition, Facts},
    *,
};

fn definition(name: &str, text: &str) -> Definition {
    Definition {
        name: name.to_owned(),
        text: text.to_owned(),
        alias: false,
    }
}

fn alias(name: &str, text: &str) -> Definition {
    Definition {
        alias: true,
        ..definition(name, text)
    }
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
fn shell_scripts_report_path_and_agent_definitions() {
    let prelude = r#"claudesim() { _claude_profile "$HOME/.claude-alt" "$@"; }
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
        let names = facts
            .definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"claudesim"), "{shell}: {names:?}");
        assert!(names.contains(&"codex-sw"), "{shell}: {names:?}");
        assert!(names.contains(&"cc"), "{shell}: {names:?}");
        assert!(
            !names.contains(&"gitlog") && !names.contains(&"gs"),
            "{shell}: {names:?}"
        );
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
}

#[cfg(unix)]
#[test]
fn probe_output_is_read_between_the_markers_only() {
    let facts = shell::parse(
        "motd line\n\u{1e}kodosi-agents\npath\t/opt/bin:/usr/bin\nfunction\tclaude-x\tbody\nfunction\tbad name\tbody\n\u{1e}end\nfunction\tlate\tbody\n",
    )
    .expect("markers");
    assert_eq!(facts.path.as_deref(), Some("/opt/bin:/usr/bin".as_ref()));
    assert_eq!(facts.definitions, vec![definition("claude-x", "body")]);
    assert!(shell::parse("no markers").is_none());
}

#[test]
fn definitions_name_the_sign_in_folder_they_use() {
    let home = Path::new("/home/ada");
    let claude = |text| referenced_home(text, home, ".claude", "CLAUDE_CONFIG_DIR");
    assert_eq!(
        claude(r#"_claude_profile "$HOME/.claude-alt" "$@""#),
        Some(home.join(".claude-alt"))
    );
    assert_eq!(
        claude("CLAUDE_CONFIG_DIR=~/.claude-work claude"),
        Some(home.join(".claude-work"))
    );
    assert_eq!(
        claude("env 'CLAUDE_CONFIG_DIR=/srv/claude two' claude"),
        Some("/srv/claude two".into())
    );
    assert_eq!(
        claude(r#"CLAUDE_CONFIG_DIR="$config_dir" "$HOME/.local/bin/claude""#),
        None
    );
    assert_eq!(
        claude("printf 'claude-sw -> %s' \"$(_cc_acct claude-sw)\""),
        None
    );
    assert_eq!(claude("/opt/other/.claude-x/bin"), None);
    assert_eq!(claude("CLAUDE_CONFIG_DIR=$HOME/../x claude"), None);
}

fn jwt(email: &str) -> String {
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
                account.options.as_ref(),
            ]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ")
        })
        .collect()
}

#[test]
fn accounts_come_from_definitions_and_sign_in_folders() {
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
    put(
        home,
        ".claude-script/.claude.json",
        &claude_sign_in("script@example.com"),
    );
    put(home, ".claude-started/.claude.json", "{}");
    fs::create_dir_all(home.join(".claude-work")).unwrap();
    fs::create_dir_all(home.join(".claude-tool/cache")).unwrap();
    put(
        home,
        ".codex-sw/auth.json",
        &format!(r#"{{"tokens":{{"id_token":"{}"}}}}"#, jwt("sw@example.com")),
    );
    put(
        home,
        "bin/claude-script",
        "#!/bin/sh\nexport CLAUDE_CONFIG_DIR=\"$HOME/.claude-script\"\nexec claude \"$@\"\n",
    );
    let facts = Facts {
        path: Some("/nowhere".into()),
        definitions: vec![
            definition("claude", "env -u CLAUDE_CONFIG_DIR claude"),
            definition("claudesim", r#"_claude_profile "$HOME/.claude-alt" "$@""#),
            definition(
                "claudesim-yolo",
                r#"_claude_profile "$HOME/.claude-alt" --yolo"#,
            ),
            definition("claude-acct", "_cc_acct claude-sw"),
            definition("claude_usage", r#"ccusage "$@""#),
            definition("claude-dyn", r#"CLAUDE_CONFIG_DIR="$dir" claude "$@""#),
            definition("codex", "codex"),
            alias("cc", "claude --dangerously-skip-permissions"),
            alias("cw", "claudesim --verbose"),
            alias("claude-config", "vim ~/.claude/settings.json"),
            alias("claudeGone", r#""$HOME/.local/bin/claudeGone""#),
            alias("claudeScript", r#""$HOME/bin/claude-script""#),
            alias("gs", "git status"),
        ],
    };
    let catalog = accounts::catalog(home, &facts, &learned::Learned::default());
    assert_eq!(
        rows(&catalog, Agent::Claude),
        [
            "claude ada@example.com ~/.claude",
            "cc ada@example.com ~/.claude --dangerously-skip-permissions",
            "claudesim sim@example.com ~/.claude-alt",
            "claude-dyn",
            "claudeScript script@example.com ~/.claude-script",
            "cw sim@example.com ~/.claude-alt --verbose",
            "personal me@example.com ~/.claude_personal",
            "started ~/.claude-started",
            "work ~/.claude-work",
        ]
    );
    assert_eq!(
        rows(&catalog, Agent::Codex),
        ["codex ~/.codex", "sw sw@example.com ~/.codex-sw"]
    );
    let work = &catalog.agents[0].accounts.last().expect("work").command;
    assert_eq!(
        *work,
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

#[test]
fn a_command_keeps_the_sign_in_folder_that_its_agent_got() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    put(
        home,
        ".profiles/x/.claude.json",
        &claude_sign_in("x@example.com"),
    );
    let facts = Facts {
        path: Some("/nowhere".into()),
        definitions: vec![
            definition("claude", r#"command claude "$@""#),
            definition("claude-x", r#"_profile x "$@""#),
        ],
    };
    let names = || {
        accounts::catalog(home, &facts, &learned::load(home)).agents[0]
            .accounts
            .iter()
            .map(|account| (account.name.clone(), account.identity.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(names().len(), 1);
    let folder = home.join(".profiles/x").display().to_string();
    remember(home, home, "claude", "env", Some(folder.clone()));
    remember(home, home, "claude", "claude-x", Some(folder));
    assert_eq!(
        names()[1],
        ("claude-x".to_owned(), Some("x@example.com".to_owned()))
    );
}

#[test]
fn a_new_account_gets_a_private_empty_folder() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    let account = accounts::add(home, Agent::Codex, " Night Shift ").expect("added");
    assert_eq!(account.name, "night-shift");
    assert_eq!(
        account.command,
        format!("env CODEX_HOME={}/.codex-night-shift codex", home.display())
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(home.join(".codex-night-shift"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }
    assert!(accounts::add(home, Agent::Codex, "night shift").is_err());
    assert!(accounts::add(home, Agent::Codex, "../x").is_err());
    assert!(accounts::add(home, Agent::Gemini, "work").is_err());
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
