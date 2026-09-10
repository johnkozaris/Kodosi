use std::path::Path;

use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use serde::{Deserialize, Serialize};

const MAX_AUTO_MODE_RULE_BYTES: usize = 10 * 1024;
const MAX_AUTO_MODE_RULE_COUNT: usize = 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub struct AutoModeRules {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub soft_deny: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hard_deny: Vec<String>,
}

impl AutoModeRules {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (field, rules) in [
            ("environment", &self.environment),
            ("allow", &self.allow),
            ("soft_deny", &self.soft_deny),
            ("hard_deny", &self.hard_deny),
        ] {
            if rules.len() > MAX_AUTO_MODE_RULE_COUNT {
                return Err(format!(
                    "autoMode.{field}: too many rules ({} > cap {MAX_AUTO_MODE_RULE_COUNT})",
                    rules.len()
                ));
            }
            for (idx, rule) in rules.iter().enumerate() {
                if rule.len() > MAX_AUTO_MODE_RULE_BYTES {
                    return Err(format!(
                        "autoMode.{field}[{idx}] exceeds byte cap ({} > {MAX_AUTO_MODE_RULE_BYTES})",
                        rule.len()
                    ));
                }
            }
        }
        Ok(())
    }
}

pub async fn read_auto_mode_rules(home: &Path) -> Result<AutoModeRules, String> {
    let claude_home = home.join(".claude");
    tokio::task::spawn_blocking(move || read_blocking(&claude_home))
        .await
        .map_err(|error| format!("auto-mode rules read task join error: {error}"))?
}

fn read_blocking(claude_home: &Path) -> Result<AutoModeRules, String> {
    let settings_path = claude_home.join("settings.json");
    if !settings_path.exists() {
        return Ok(AutoModeRules::default());
    }
    let contents = std::fs::read_to_string(&settings_path)
        .map_err(|error| format!("read settings.json: {error}"))?;
    parse_rules_content(&contents)
}

pub(crate) fn parse_rules_content(contents: &str) -> Result<AutoModeRules, String> {
    let root = CstRootNode::parse(contents, &crate::ops::settings_mutation::parse_options())
        .map_err(|error| format!("parse settings.json: {error}"))?;
    let object = root
        .object_value()
        .ok_or_else(|| "settings.json root must be an object".to_owned())?;
    let Some(auto_mode) = object.get("autoMode") else {
        return Ok(AutoModeRules::default());
    };
    let value = auto_mode
        .to_serde_value()
        .ok_or_else(|| "autoMode has no value".to_owned())?;
    serde_json::from_value(value).map_err(|error| format!("parse autoMode block: {error}"))
}

pub async fn write_auto_mode_rules(home: &Path, rules: AutoModeRules) -> Result<(), String> {
    rules.validate()?;
    let home = home.to_owned();
    tokio::task::spawn_blocking(move || write_auto_mode_rules_blocking(&home, &rules))
        .await
        .map_err(|error| format!("auto-mode rules write task join error: {error}"))?
}

pub(crate) fn write_auto_mode_rules_blocking(
    home: &Path,
    rules: &AutoModeRules,
) -> Result<(), String> {
    rules.validate()?;
    let settings_path = home.join(".claude/settings.json");
    crate::ops::settings_mutation::mutate_settings_file(&settings_path, |root| {
        patch_auto_mode(root, rules)
    })
}

fn patch_auto_mode(root: &CstRootNode, rules: &AutoModeRules) -> Result<(), String> {
    let settings = root
        .object_value()
        .ok_or_else(|| "settings.json root must be an object".to_owned())?;
    let auto_mode = settings
        .object_value_or_create("autoMode")
        .ok_or_else(|| "autoMode must be an object".to_owned())?;
    set_field(&auto_mode, "environment", &rules.environment);
    set_field(&auto_mode, "allow", &rules.allow);
    set_field(&auto_mode, "soft_deny", &rules.soft_deny);
    set_field(&auto_mode, "hard_deny", &rules.hard_deny);
    Ok(())
}

pub(crate) fn render_auto_mode_rules(
    contents: Option<&[u8]>,
    rules: &AutoModeRules,
) -> Result<Vec<u8>, String> {
    let text = match contents {
        None => "{}",
        Some(bytes) => std::str::from_utf8(bytes)
            .map_err(|error| format!("settings.json is not UTF-8: {error}"))?,
    };
    let root = CstRootNode::parse(text, &crate::ops::settings_mutation::parse_options())
        .map_err(|error| format!("parse settings.json: {error}"))?;
    if root.object_value().is_none() {
        return Err("settings.json root must be an object".to_owned());
    }
    patch_auto_mode(&root, rules)?;
    Ok(root.to_string().into_bytes())
}

fn set_field(object: &CstObject, key: &str, values: &[String]) {
    if values.is_empty() {
        if let Some(property) = object.get(key) {
            property.remove();
        }
        return;
    }
    let value = CstInputValue::Array(values.iter().cloned().map(Into::into).collect());
    match object.get(key) {
        Some(property) => property.set_value(value),
        None => {
            object.append(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_full_persists_all_four_fields() {
        let tmp = tempfile::tempdir().unwrap();
        write_auto_mode_rules(
            tmp.path(),
            AutoModeRules {
                environment: vec!["$defaults".into(), "extra".into()],
                allow: vec!["allow git push".into()],
                soft_deny: vec!["never deploy".into()],
                hard_deny: vec!["never `rm -rf /`".into()],
            },
        )
        .await
        .expect("write");
        let body = std::fs::read(tmp.path().join(".claude/settings.json")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let auto = &value["autoMode"];
        assert_eq!(auto["environment"][0], "$defaults");
        assert_eq!(auto["allow"][0], "allow git push");
        assert_eq!(auto["soft_deny"][0], "never deploy");
        assert_eq!(auto["hard_deny"][0], "never `rm -rf /`");
    }

    #[tokio::test]
    async fn write_preserves_comments_and_unowned_properties() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
  // root comment
  "other": true,
  "autoMode": {
    // owned values may change
    "allow": ["old"],
    "unowned": "keep"
  }
}
"#,
        )
        .unwrap();

        write_auto_mode_rules(
            tmp.path(),
            AutoModeRules {
                allow: vec!["new".into()],
                ..AutoModeRules::default()
            },
        )
        .await
        .unwrap();

        let written = std::fs::read_to_string(path).unwrap();
        assert!(written.contains("// root comment"));
        assert!(written.contains("// owned values may change"));
        assert!(written.contains("\"other\": true"));
        assert!(written.contains("\"unowned\": \"keep\""));
        assert!(written.contains("\"allow\": [\"new\"]"));
    }

    #[tokio::test]
    async fn empty_lists_remove_only_owned_properties() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"autoMode":{"environment":["x"],"allow":["x"],"soft_deny":["x"],"hard_deny":["x"],"other":1}}"#,
        )
        .unwrap();

        write_auto_mode_rules(tmp.path(), AutoModeRules::default())
            .await
            .unwrap();

        let root = CstRootNode::parse(
            &std::fs::read_to_string(path).unwrap(),
            &crate::ops::settings_mutation::parse_options(),
        )
        .unwrap();
        let value = root.to_serde_value().unwrap();
        assert_eq!(value["autoMode"], serde_json::json!({"other": 1}));
    }

    #[tokio::test]
    async fn read_returns_default_when_settings_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let rules = read_auto_mode_rules(tmp.path()).await.expect("read");
        assert_eq!(rules, AutoModeRules::default());
    }

    #[tokio::test]
    async fn read_accepts_comments_and_trailing_commas() {
        let tmp = tempfile::tempdir().unwrap();
        let claude_home = tmp.path().join(".claude");
        std::fs::create_dir_all(&claude_home).unwrap();
        std::fs::write(
            claude_home.join("settings.json"),
            r#"{
  // Claude settings are JSONC.
  "autoMode": {
    "environment": ["$defaults",],
    "allow": ["x"],
    "soft_deny": ["y"],
    "hard_deny": ["z"],
  },
}
"#,
        )
        .unwrap();
        let rules = read_auto_mode_rules(tmp.path()).await.expect("read");
        assert_eq!(rules.environment, vec!["$defaults".to_string()]);
        assert_eq!(rules.allow, vec!["x".to_string()]);
        assert_eq!(rules.soft_deny, vec!["y".to_string()]);
        assert_eq!(rules.hard_deny, vec!["z".to_string()]);
    }

    #[tokio::test]
    async fn non_object_auto_mode_is_non_destructive() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(".claude/settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{\"autoMode\":42}\n").unwrap();

        let error = write_auto_mode_rules(tmp.path(), AutoModeRules::default())
            .await
            .expect_err("scalar autoMode must fail");

        assert!(error.contains("autoMode must be an object"));
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "{\"autoMode\":42}\n"
        );
    }

    #[tokio::test]
    async fn validate_rejects_overlong_rules() {
        let rules = AutoModeRules {
            environment: vec!["x".repeat(MAX_AUTO_MODE_RULE_BYTES + 1)],
            ..Default::default()
        };
        let tmp = tempfile::tempdir().unwrap();
        let result = write_auto_mode_rules(tmp.path(), rules).await;
        assert!(result.is_err());
    }
}
