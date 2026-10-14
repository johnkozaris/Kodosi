use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

use super::{
    accounts::{self, referenced_home},
    models::{self, codex, copilot, cursor, listed_values, opencode},
    shell::{self, Definition, Facts},
    *,
};

fn definition(name: &str, text: &str) -> Definition {
    Definition {
        name: name.to_owned(),
        text: text.to_owned(),
    }
}

fn run_script(shell: &str, arguments: &[&str], prelude: &str, family_script: &str) -> Facts {
    let output = std::process::Command::new(shell)
        .args(arguments)
        .arg(format!("{prelude}\n{family_script}"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("shell runs");
    shell::parse(&String::from_utf8_lossy(&output.stdout)).expect("both markers")
}

#[test]
fn shell_scripts_report_path_and_agent_definitions() {
    let prelude = r#"claudesim() { _claude_profile "$HOME/.claude-alt" "$@"; }
alias codex-sw='CODEX_HOME=~/.codex-sw codex'
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
        assert!(!names.contains(&"gitlog"), "{shell}: {names:?}");
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

#[test]
fn accounts_come_from_definitions_and_sign_in_folders() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path();
    fs::write(
        home.join(".claude.json"),
        r#"{"oauthAccount":{"emailAddress":"ada@example.com"}}"#,
    )
    .unwrap();
    fs::create_dir_all(home.join(".claude-alt")).unwrap();
    fs::write(
        home.join(".claude-alt/.claude.json"),
        r#"{"oauthAccount":{"emailAddress":"sim@example.com"}}"#,
    )
    .unwrap();
    fs::create_dir_all(home.join(".claude_personal")).unwrap();
    fs::write(
        home.join(".claude_personal/.claude.json"),
        r#"{"oauthAccount":{"emailAddress":"me@example.com"}}"#,
    )
    .unwrap();
    fs::create_dir_all(home.join(".claude-work")).unwrap();
    fs::create_dir_all(home.join(".claude-started")).unwrap();
    fs::write(home.join(".claude-started/.claude.json"), "{}").unwrap();
    fs::create_dir_all(home.join(".claude-tool/cache")).unwrap();
    fs::create_dir_all(home.join(".codex-sw")).unwrap();
    fs::write(
        home.join(".codex-sw/auth.json"),
        format!(r#"{{"tokens":{{"id_token":"{}"}}}}"#, jwt("sw@example.com")),
    )
    .unwrap();
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
            definition("codex", "codex"),
        ],
    };
    let catalog = accounts::catalog(home, &facts);
    let accounts_of = |agent| {
        catalog
            .agents
            .iter()
            .find(|info| info.agent == agent)
            .expect("agent")
            .accounts
            .iter()
            .map(|account| {
                (
                    account.name.as_str(),
                    account.command.as_str(),
                    account.identity.as_deref(),
                )
            })
            .collect::<Vec<_>>()
    };
    let work = format!(
        "env CLAUDE_CONFIG_DIR={}/.claude-work claude",
        home.display()
    );
    let personal = format!(
        "env CLAUDE_CONFIG_DIR={}/.claude_personal claude",
        home.display()
    );
    let started = format!(
        "env CLAUDE_CONFIG_DIR={}/.claude-started claude",
        home.display()
    );
    assert_eq!(
        accounts_of(Agent::Claude),
        vec![
            ("claude", "claude", Some("ada@example.com")),
            ("claudesim", "claudesim", Some("sim@example.com")),
            ("personal", personal.as_str(), Some("me@example.com")),
            ("started", started.as_str(), None),
            ("work", work.as_str(), None),
        ]
    );
    let sw = format!("env CODEX_HOME={}/.codex-sw codex", home.display());
    assert_eq!(
        accounts_of(Agent::Codex),
        vec![
            ("codex", "codex", None),
            ("sw", sw.as_str(), Some("sw@example.com")),
        ]
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
