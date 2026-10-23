use serde_json::json;

use super::*;

fn limit(
    hours: Option<u32>,
    scope: Option<&str>,
    used: u8,
    level: UsageLevel,
    headline: bool,
) -> UsageLimit {
    UsageLimit {
        hours,
        scope: scope.map(str::to_owned),
        used,
        level,
        headline,
    }
}

#[test]
fn claude_rows_keep_their_severity_and_the_worst_one_leads() {
    let report = json!({"rate_limits": {"limits": [
        {"kind": "session", "group": "session", "percent": 42.4, "resets_at": null, "scope": null, "severity": "normal", "is_active": false},
        {"kind": "weekly_all", "group": "weekly", "percent": 71, "resets_at": "2026-10-29T13:00:00+00:00", "scope": null, "severity": "warning", "is_active": true},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 100, "scope": {"model": {"display_name": "Fable"}, "surface": null}, "severity": "critical", "is_active": false},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 50, "scope": {"model": {}}, "severity": "normal", "is_active": false},
        {"kind": "credits", "group": "spend", "percent": 10, "severity": "normal", "is_active": false}
    ], "extra_usage": null}});
    assert_eq!(
        headlined(claude(&report)),
        vec![
            limit(Some(5), None, 42, UsageLevel::Normal, false),
            limit(Some(168), None, 71, UsageLevel::Near, false),
            limit(Some(168), Some("Fable"), 100, UsageLevel::Reached, true),
        ]
    );
}

#[test]
fn a_claude_account_without_plan_limits_reports_none() {
    assert_eq!(claude(&json!({"rate_limits": null})), Vec::new());
    assert_eq!(
        claude(&json!({"rate_limits": {"limits": null}})),
        Vec::new()
    );
}

#[test]
fn codex_windows_are_named_by_their_length() {
    let week = json!({"rateLimits": {"primary": {"usedPercent": 12, "windowDurationMins": 10080, "resetsAt": 1_793_321_722}, "secondary": null, "planType": "prolite"}});
    assert_eq!(
        headlined(codex(&week)),
        vec![limit(Some(168), None, 12, UsageLevel::Normal, true)]
    );
    let both = json!({"rateLimits": {
        "primary": {"usedPercent": 42, "windowDurationMins": 300},
        "secondary": {"usedPercent": 85, "windowDurationMins": 10080}
    }});
    assert_eq!(
        headlined(codex(&both)),
        vec![
            limit(Some(5), None, 42, UsageLevel::Normal, false),
            limit(Some(168), None, 85, UsageLevel::Near, true),
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
                "usageAllowedWithExhaustedQuota": overage
            }
        }})
    };
    assert_eq!(
        copilot(&quota(41.4, false, false)),
        vec![limit(None, None, 59, UsageLevel::Normal, false)]
    );
    assert_eq!(
        copilot(&quota(0.0, false, true)),
        vec![limit(None, None, 100, UsageLevel::Near, false)]
    );
    assert_eq!(
        copilot(&quota(0.0, false, false)),
        vec![limit(None, None, 100, UsageLevel::Reached, false)]
    );
    assert_eq!(copilot(&quota(0.0, true, false)), Vec::new());
}

#[test]
fn the_headline_is_the_worst_level_then_the_most_used_then_the_first() {
    use UsageLevel::{Near, Normal, Reached};

    let headlines = |limits: &[(u8, UsageLevel)]| {
        headlined(
            limits
                .iter()
                .map(|(used, level)| limit(Some(5), None, *used, *level, false))
                .collect(),
        )
        .iter()
        .map(|limit| limit.headline)
        .collect::<Vec<_>>()
    };
    assert_eq!(
        headlines(&[(30, Normal), (30, Normal), (10, Normal)]),
        [true, false, false]
    );
    assert_eq!(headlines(&[(10, Normal), (90, Near)]), [false, true]);
    assert_eq!(headlines(&[(78, Normal), (70, Near)]), [false, true]);
    assert_eq!(headlines(&[(100, Reached), (100, Near)]), [true, false]);
    assert_eq!(headlines(&[]), Vec::<bool>::new());
}

#[test]
fn scope_names_are_short_plain_text() {
    assert_eq!(scope_name(" Opus "), Some("Opus".to_owned()));
    assert_eq!(scope_name(""), None);
    assert_eq!(scope_name("a\u{7}b"), None);
    assert_eq!(scope_name(&"x".repeat(33)), None);
}

#[cfg(unix)]
fn program(folder: &Path, name: &str, script: &str) -> Runner {
    use std::os::unix::fs::PermissionsExt as _;

    let path = folder.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{script}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
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
printf '{"type":"assistant","usage_report":{"rate_limits":{"limits":[{"kind":"weekly_scoped","group":"weekly","percent":7,"scope":{"model":{"display_name":"%s"}},"severity":"normal","is_active":true}]}}}\n' "${CLAUDE_CONFIG_DIR:-unset}"
printf '%s\n' '{"type":"result","subtype":"success"}'
"#;

#[cfg(unix)]
#[tokio::test]
async fn claude_reports_the_usage_of_the_account_folder_it_is_given() {
    let root = tempfile::tempdir().unwrap();
    let mut runner = program(root.path(), "claude", CLAUDE);
    assert_eq!(
        read(Agent::Claude, &runner, root.path()).await.unwrap(),
        vec![limit(Some(168), Some("unset"), 7, UsageLevel::Normal, true)]
    );
    runner.variables = vec![("CLAUDE_CONFIG_DIR".to_owned(), "/w/.claude-work".to_owned())];
    assert_eq!(
        read(Agent::Claude, &runner, root.path()).await.unwrap(),
        vec![limit(
            Some(168),
            Some("/w/.claude-work"),
            7,
            UsageLevel::Normal,
            true
        )]
    );
}

#[cfg(unix)]
#[tokio::test]
async fn claude_without_a_report_has_no_plan_limits() {
    let root = tempfile::tempdir().unwrap();
    let runner = program(
        root.path(),
        "claude",
        r#"printf '%s\n' '{"type":"assistant","message":{"content":[]}}' '{"type":"result","subtype":"success"}'"#,
    );
    assert_eq!(
        read(Agent::Claude, &runner, root.path()).await,
        Ok(Vec::new())
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
            limit(Some(5), None, 42, UsageLevel::Normal, true),
            limit(Some(168), None, 18, UsageLevel::Normal, false),
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
answer='{"jsonrpc":"2.0","id":1,"result":{"quotaSnapshots":{"premium_interactions":{"isUnlimitedEntitlement":false,"remainingPercentage":41.4,"usageAllowedWithExhaustedQuota":true}}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#event}" "$event"
printf 'Content-Type: application/json\r\nContent-Length: %s\r\n\r\n%s' "${#answer}" "$answer"
cat >/dev/null
"#,
    );
    assert_eq!(
        read(Agent::Copilot, &runner, root.path()).await.unwrap(),
        vec![limit(None, None, 59, UsageLevel::Normal, true)]
    );
}
