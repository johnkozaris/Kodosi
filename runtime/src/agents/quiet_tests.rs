use serde_json::json;

use super::*;

const SETTINGS_OF_COPILOT: &str = r#"{
  "showReasoning": true,
  "model": "claude-opus-5.5",
  "footer": {
    "showQuota": false,
    "showBranch": true
  },
  "banner": "always",
  "allowedUrls": [
    "github.com"
  ]
}
"#;

fn account() -> (tempfile::TempDir, PathBuf) {
    let folder = tempfile::tempdir().unwrap();
    let settings = folder.path().join("settings.json");
    std::fs::write(&settings, SETTINGS_OF_COPILOT).unwrap();
    (folder, settings)
}

fn text(settings: &Path) -> String {
    std::fs::read_to_string(settings).unwrap()
}

fn value(settings: &Path) -> Value {
    serde_json::from_str(&text(settings)).unwrap()
}

#[test]
fn a_quiet_copilot_leaves_its_settings_as_they_were() {
    let root = tempfile::tempdir().unwrap();
    let (_folder, settings) = account();
    let quiet = QuietSettings::load(root.path());
    let terminal = Uuid::now_v7();

    quiet.hold(terminal, &settings);
    assert_eq!(
        value(&settings),
        json!({
            "showReasoning": false,
            "model": "claude-opus-5.5",
            "footer": {"showQuota": false, "showBranch": true},
            "banner": "never",
            "allowedUrls": ["github.com"],
            "showTimestamps": false,
            "showToolDurations": false,
        })
    );
    quiet.release(terminal);
    assert_eq!(text(&settings), SETTINGS_OF_COPILOT);

    let new_account = tempfile::tempdir().unwrap();
    let created = new_account.path().join("settings.json");
    quiet.hold(terminal, &created);
    assert_eq!(
        value(&created),
        json!({
            "showReasoning": false,
            "showTimestamps": false,
            "showToolDurations": false,
            "banner": "never",
        })
    );
    quiet.release(terminal);
    assert!(!created.exists());
}

#[test]
fn a_setting_that_the_person_changes_while_copilot_runs_stays() {
    let root = tempfile::tempdir().unwrap();
    let (_folder, settings) = account();
    let quiet = QuietSettings::load(root.path());
    let terminal = Uuid::now_v7();

    quiet.hold(terminal, &settings);
    let mut changed = value(&settings);
    changed["banner"] = json!("once");
    std::fs::write(&settings, serde_json::to_string_pretty(&changed).unwrap()).unwrap();
    quiet.release(terminal);

    let kept = value(&settings);
    assert_eq!(kept["banner"], json!("once"));
    assert_eq!(kept["showReasoning"], json!(true));
    assert_eq!(kept.get("showTimestamps"), None);
}

#[test]
fn copilots_of_one_account_keep_it_quiet_until_the_last_one_ends() {
    let root = tempfile::tempdir().unwrap();
    let (_folder, settings) = account();
    let quiet = QuietSettings::load(root.path());
    let (first, second) = (Uuid::now_v7(), Uuid::now_v7());

    quiet.hold(first, &settings);
    quiet.hold(second, &settings);
    quiet.release(first);
    assert_eq!(value(&settings)["showReasoning"], json!(false));
    quiet.release(second);
    assert_eq!(text(&settings), SETTINGS_OF_COPILOT);
}

#[test]
fn settings_that_are_not_json_stay_as_they_are() {
    let root = tempfile::tempdir().unwrap();
    let (_folder, settings) = account();
    let commented = "{\n  // Ctrl+T shows it\n  \"showReasoning\": true\n}\n";
    std::fs::write(&settings, commented).unwrap();
    let quiet = QuietSettings::load(root.path());
    let terminal = Uuid::now_v7();

    quiet.hold(terminal, &settings);
    assert_eq!(text(&settings), commented);
    quiet.release(terminal);
    assert_eq!(text(&settings), commented);
}

#[test]
fn the_settings_that_an_ended_host_held_go_back_when_kodosi_starts() {
    let root = tempfile::tempdir().unwrap();
    let (_folder, settings) = account();

    QuietSettings::load(root.path()).hold(Uuid::now_v7(), &settings);
    assert_ne!(text(&settings), SETTINGS_OF_COPILOT);
    QuietSettings::load(root.path());
    assert_eq!(text(&settings), SETTINGS_OF_COPILOT);
}
