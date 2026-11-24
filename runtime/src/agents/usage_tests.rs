use serde_json::json;

use super::*;

fn limit(hours: Option<u32>, scope: Option<&str>, used: u8, level: UsageLevel) -> UsageLimit {
    UsageLimit {
        hours,
        scope: scope.map(str::to_owned),
        model: None,
        used,
        level,
        resets_at_ms: None,
    }
}

#[test]
fn claude_rows_keep_their_severity_their_model_and_when_they_reset() {
    let report = json!({"rate_limits": {"limits": [
        {"kind": "session", "group": "session", "percent": 42.4, "resets_at": null, "scope": null, "severity": "normal", "is_active": false},
        {"kind": "weekly_all", "group": "weekly", "percent": 71, "resets_at": "2026-11-26T12:59:59.513299+00:00", "scope": null, "severity": "warning", "is_active": true},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 100, "resets_at": "2026-11-26T12:59:59.513427+00:00", "scope": {"model": {"display_name": "Fable"}, "surface": null}, "severity": "critical", "is_active": false},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 9, "resets_at": "soon", "scope": {"model": {"display_name": "Mythos"}, "surface": null}, "severity": "normal", "is_active": false},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 12, "scope": {"model": {"display_name": "Claude Opus 4.7"}, "surface": null}, "severity": "normal", "is_active": false},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 30, "scope": {"model": null, "surface": {"display_name": "Claude Code"}}, "severity": "normal", "is_active": false},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 50, "scope": {"model": {}}, "severity": "normal", "is_active": false},
        {"kind": "credits", "group": "spend", "percent": 10, "severity": "normal", "is_active": false}
    ], "extra_usage": null}});
    let resets = Some(1_795_697_999_513);
    assert_eq!(
        claude(&report),
        vec![
            limit(Some(5), None, 42, UsageLevel::Normal),
            UsageLimit {
                resets_at_ms: resets,
                ..limit(Some(168), None, 71, UsageLevel::Near)
            },
            UsageLimit {
                model: Some("fable".to_owned()),
                resets_at_ms: resets,
                ..limit(Some(168), Some("Fable"), 100, UsageLevel::Reached)
            },
            UsageLimit {
                model: Some("Mythos".to_owned()),
                ..limit(Some(168), Some("Mythos"), 9, UsageLevel::Normal)
            },
            UsageLimit {
                model: Some("opus".to_owned()),
                ..limit(Some(168), Some("Claude Opus 4.7"), 12, UsageLevel::Normal)
            },
            limit(Some(168), Some("Claude Code"), 30, UsageLevel::Normal),
        ]
    );
}

#[test]
fn codex_windows_are_named_by_their_length() {
    let month = json!({"rateLimits": {"primary": {"usedPercent": 8, "windowDurationMins": 43200, "resetsAt": 1_795_848_074}, "secondary": null, "planType": "free"}});
    assert_eq!(
        codex(&month),
        vec![UsageLimit {
            resets_at_ms: Some(1_795_848_074_000),
            ..limit(Some(720), None, 8, UsageLevel::Normal)
        }]
    );
    let both = json!({"rateLimits": {
        "primary": {"usedPercent": 42, "windowDurationMins": 300},
        "secondary": {"usedPercent": 85, "windowDurationMins": 10080, "resetsAt": null}
    }});
    assert_eq!(
        codex(&both),
        vec![
            limit(Some(5), None, 42, UsageLevel::Normal),
            limit(Some(168), None, 85, UsageLevel::Near),
        ]
    );
    let unnamed =
        json!({"rateLimits": {"primary": {"usedPercent": 5, "windowDurationMins": null}}});
    assert_eq!(codex(&unnamed), Vec::new());
}

#[test]
fn copilot_counts_the_premium_requests_of_the_month() {
    let quota = |remaining: f64, unlimited: bool, overage: bool| {
        json!({"quotaSnapshots": {
            "chat": {"isUnlimitedEntitlement": true, "remainingPercentage": 100},
            "premium_interactions": {
                "isUnlimitedEntitlement": unlimited,
                "remainingPercentage": remaining,
                "usageAllowedWithExhaustedQuota": overage,
                "resetDate": "2026-12-01T00:00:00.000Z"
            }
        }})
    };
    let month = |used: u8, level: UsageLevel| UsageLimit {
        resets_at_ms: Some(1_796_083_200_000),
        ..limit(None, None, used, level)
    };
    assert_eq!(
        copilot(&quota(41.4, false, false)),
        vec![month(59, UsageLevel::Normal)]
    );
    assert_eq!(
        copilot(&quota(0.0, false, true)),
        vec![month(100, UsageLevel::Near)]
    );
    assert_eq!(
        copilot(&quota(0.0, false, false)),
        vec![month(100, UsageLevel::Reached)]
    );
    assert_eq!(copilot(&quota(0.0, true, false)), Vec::new());
}

#[cfg(unix)]
fn program(folder: &Path, name: &str, script: &str) -> Runner {
    let path = folder.join(name);
    kodosi_os::process::write_program(&path, &format!("#!/bin/sh\n{script}")).unwrap();
    Runner {
        program: path,
        path: None,
        variables: Vec::new(),
    }
}

#[cfg(unix)]
const CLAUDE: &str = r#"
case "$*" in "--safe-mode -p /usage --output-format stream-json --verbose --no-session-persistence") ;; *) exit 3 ;; esac
printf '%s\n' '{"type":"system","subtype":"init"}' 'not json'
printf '{"type":"assistant","usage_report":{"rate_limits":{"limits":[{"kind":"weekly_scoped","group":"weekly","percent":7,"resets_at":"2099-01-01T00:00:00Z","scope":{"model":{"display_name":"%s"}},"severity":"normal","is_active":true}]}}}\n' "${CLAUDE_CONFIG_DIR:-unset}"
printf '%s\n' '{"type":"result","subtype":"success"}'
"#;

#[cfg(unix)]
#[tokio::test]
async fn claude_reports_the_usage_of_the_account_folder_it_is_given() {
    let root = tempfile::tempdir().unwrap();
    let mut runner = program(root.path(), "claude", CLAUDE);
    let folder = |name: &str| UsageLimit {
        model: Some(name.to_owned()),
        resets_at_ms: Some(4_070_908_800_000),
        ..limit(Some(168), Some(name), 7, UsageLevel::Normal)
    };
    assert_eq!(
        read(Agent::Claude, &runner, root.path()).await.unwrap(),
        vec![folder("unset")]
    );
    runner.variables = vec![("CLAUDE_CONFIG_DIR".to_owned(), "/w/.claude-work".to_owned())];
    assert_eq!(
        read(Agent::Claude, &runner, root.path()).await.unwrap(),
        vec![folder("/w/.claude-work")]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn codex_answers_after_the_handshake_of_its_app_server() {
    let root = tempfile::tempdir().unwrap();
    let runner = program(
        root.path(),
        "codex",
        r#"
[ "$*" = "app-server" ] || exit 3
while IFS= read -r line; do
  case "$line" in
    *'"method":"initialize"'*) printf '%s\n' '{"id":0,"result":{"userAgent":"stand-in"}}' ;;
    *'"method":"account/rateLimits/read"'*)
      printf '%s\n' '{"method":"account/updated","params":{}}' '{"id":1,"method":"item/tool/requestUserInput","params":{}}' \
        '{"id":1,"result":{"rateLimits":{"primary":{"usedPercent":42,"windowDurationMins":300},"secondary":{"usedPercent":18,"windowDurationMins":10080}}}}' ;;
  esac
done
"#,
    );
    assert_eq!(
        read(Agent::Codex, &runner, root.path()).await.unwrap(),
        vec![
            limit(Some(5), None, 42, UsageLevel::Normal),
            limit(Some(168), None, 18, UsageLevel::Normal),
        ]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_refused_request_is_an_error_and_an_early_end_too() {
    let root = tempfile::tempdir().unwrap();
    let refused = program(
        root.path(),
        "codex",
        r#"
while IFS= read -r line; do
  case "$line" in
    *'"id":0'*) printf '%s\n' '{"id":0,"result":{}}' ;;
    *'"id":1'*) printf '%s\n' '{"id":1,"error":{"message":"Sign in with ChatGPT first."}}' ;;
  esac
done
"#,
    );
    assert_eq!(
        read(Agent::Codex, &refused, root.path()).await,
        Err("Sign in with ChatGPT first.".to_owned())
    );
    let quiet = program(root.path(), "copilot", "read -r line");
    assert_eq!(
        read(Agent::Copilot, &quiet, root.path()).await,
        Err("Copilot CLI ended without its usage.".to_owned())
    );
}

#[cfg(unix)]
#[tokio::test]
async fn copilot_answers_in_framed_messages() {
    let root = tempfile::tempdir().unwrap();
    let runner = program(
        root.path(),
        "copilot",
        r#"
[ "$*" = "--server --stdio --no-auto-update" ] || exit 3
event='{"jsonrpc":"2.0","method":"session.event","params":{}}'
answer='{"jsonrpc":"2.0","id":1,"result":{"quotaSnapshots":{"premium_interactions":{"isUnlimitedEntitlement":false,"remainingPercentage":41.4,"usageAllowedWithExhaustedQuota":true,"resetDate":"2001-01-01T00:00:00.000Z"}}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#event}" "$event"
printf 'Content-Type: application/json\r\nContent-Length: %s\r\n\r\n%s' "${#answer}" "$answer"
cat >/dev/null
"#,
    );
    assert_eq!(
        read(Agent::Copilot, &runner, root.path()).await.unwrap(),
        vec![limit(None, None, 59, UsageLevel::Normal)]
    );
}
