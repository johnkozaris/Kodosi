use std::cell::Cell;
use std::path::Path;

use jsonc_parser::cst::{CstNode, CstRootNode};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyHookRemoval {
    pub removed_entries: usize,
    pub remaining_reference: bool,
    pub hooks_empty: bool,
}

pub fn inspect_settings(path: &Path, hook_binary: &Path) -> Result<LegacyHookRemoval, String> {
    let raw_hook_path = hook_binary.to_string_lossy();
    let original = match std::fs::read_to_string(path) {
        Ok(original) => original,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LegacyHookRemoval::default());
        }
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    if !original.contains(raw_hook_path.as_ref()) {
        return Ok(LegacyHookRemoval::default());
    }

    let root = CstRootNode::parse(&original, &crate::ops::settings_mutation::parse_options())
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    let mut outcome = patch_root(&root, hook_binary)?;
    outcome.remaining_reference = root.to_string().contains(raw_hook_path.as_ref());
    Ok(outcome)
}

pub fn remove_from_settings(path: &Path, hook_binary: &Path) -> Result<LegacyHookRemoval, String> {
    let raw_hook_path = hook_binary.to_string_lossy();
    let original = match std::fs::read_to_string(path) {
        Ok(original) => original,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LegacyHookRemoval::default());
        }
        Err(error) => return Err(format!("read {}: {error}", path.display())),
    };
    if !original.contains(raw_hook_path.as_ref()) {
        return Ok(LegacyHookRemoval::default());
    }

    let inspected = inspect_settings(path, hook_binary)?;
    if inspected.removed_entries == 0 {
        return Ok(inspected);
    }

    let removed_entries = Cell::new(0_usize);
    let hooks_empty = Cell::new(false);
    crate::ops::settings_mutation::mutate_settings_file(path, |root| {
        let outcome = patch_root(root, hook_binary)?;
        removed_entries.set(outcome.removed_entries);
        hooks_empty.set(outcome.hooks_empty);
        Ok(())
    })?;

    let remaining_reference = std::fs::read_to_string(path)
        .map_err(|error| format!("read cleaned {}: {error}", path.display()))?
        .contains(raw_hook_path.as_ref());
    Ok(LegacyHookRemoval {
        removed_entries: removed_entries.get(),
        remaining_reference,
        hooks_empty: hooks_empty.get(),
    })
}

fn patch_root(root: &CstRootNode, hook_binary: &Path) -> Result<LegacyHookRemoval, String> {
    let root_object = root
        .object_value()
        .ok_or_else(|| "settings root must be an object".to_owned())?;
    let Some(hooks_property) = root_object.get("hooks") else {
        return Ok(LegacyHookRemoval::default());
    };
    let Some(hooks_object) = hooks_property.object_value() else {
        return Err("settings `hooks` must be an object".to_owned());
    };
    let hook_value = hooks_object
        .to_serde_value()
        .and_then(|value| value.as_object().cloned())
        .ok_or_else(|| "settings `hooks` must be an object".to_owned())?;

    let mut removed_entries = 0_usize;
    for event in hook_value.keys() {
        let Some(event_property) = hooks_object.get(event) else {
            continue;
        };
        let Some(event_entries) = event_property.array_value() else {
            continue;
        };
        for entry in event_entries.elements() {
            if is_exact_copilot_entry(&entry, event, hook_binary) {
                entry.remove();
                removed_entries = removed_entries.saturating_add(1);
                continue;
            }
            removed_entries =
                removed_entries.saturating_add(remove_nested_claude_commands(&entry, hook_binary));
        }
        if event_entries.elements().is_empty() {
            event_property.remove();
        }
    }

    let hooks_empty = hooks_object.properties().is_empty();
    if hooks_empty {
        hooks_property.remove();
    }
    Ok(LegacyHookRemoval {
        removed_entries,
        remaining_reference: false,
        hooks_empty,
    })
}

fn remove_nested_claude_commands(entry: &CstNode, hook_binary: &Path) -> usize {
    let Some(entry_object) = entry.as_object() else {
        return 0;
    };
    let Some(commands) = entry_object.array_value("hooks") else {
        return 0;
    };
    let mut removed = 0_usize;
    for command in commands.elements() {
        if is_exact_claude_command(&command, hook_binary) {
            command.remove();
            removed = removed.saturating_add(1);
        }
    }
    if commands.elements().is_empty() {
        entry.clone().remove();
    }
    removed
}

fn is_exact_claude_command(node: &CstNode, hook_binary: &Path) -> bool {
    let Some(value) = node.to_serde_value() else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.get("type").and_then(serde_json::Value::as_str) != Some("command") {
        return false;
    }
    let Some(command) = object.get("command").and_then(serde_json::Value::as_str) else {
        return false;
    };
    command == hook_binary.to_string_lossy() || command == claude_hook_command(hook_binary).as_str()
}

fn is_exact_copilot_entry(node: &CstNode, event: &str, hook_binary: &Path) -> bool {
    let Some(value) = node.to_serde_value() else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    if object.get("type").and_then(serde_json::Value::as_str) != Some("command") {
        return false;
    }
    object.get("bash").and_then(serde_json::Value::as_str)
        == Some(copilot_hook_bash_command(hook_binary, event).as_str())
        && object.get("powershell").and_then(serde_json::Value::as_str)
            == Some(copilot_hook_powershell_command(hook_binary, event).as_str())
}

fn claude_hook_command(hook_binary: &Path) -> String {
    let quoted = hook_binary.to_string_lossy().replace('\'', "'\\''");
    format!("'{quoted}'")
}

fn copilot_hook_bash_command(hook_binary: &Path, event: &str) -> String {
    let quoted = hook_binary.to_string_lossy().replace('\'', "'\\''");
    format!("'{quoted}' --dialect copilot --event {event}")
}

fn copilot_hook_powershell_command(hook_binary: &Path, event: &str) -> String {
    let quoted = hook_binary.to_string_lossy().replace('\'', "''");
    format!("& '{quoted}' --dialect copilot --event {event}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_only_exact_claude_commands_and_preserves_comments() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let binary = directory.path().join(".kodosi/bin/kodosi-hook");
        let ours = claude_hook_command(&binary);
        std::fs::write(
            &path,
            format!(
                r#"{{
  // keep this comment
  "model": "claude-sonnet-5",
  "hooks": {{
    "PreToolUse": [
      {{
        "matcher": "*",
        "hooks": [
          {{ "type": "command", "command": "{ours}" }},
          {{ "type": "command", "command": "/usr/local/bin/foreign" }},
        ],
      }},
    ],
    "Stop": [
      {{ "matcher": "*", "hooks": [{{ "type": "command", "command": "{ours}" }}] }},
    ],
  }},
}}
"#
            ),
        )
        .unwrap();

        let result = remove_from_settings(&path, &binary).unwrap();

        assert_eq!(result.removed_entries, 2);
        assert!(!result.remaining_reference);
        assert!(!result.hooks_empty);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("// keep this comment"));
        assert!(written.contains("/usr/local/bin/foreign"));
        assert!(!written.contains(binary.to_string_lossy().as_ref()));
    }

    #[test]
    fn removes_exact_copilot_entries_but_preserves_foreign_siblings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let binary = directory.path().join(".kodosi/bin/kodosi-hook");
        let bash = copilot_hook_bash_command(&binary, "preToolUse");
        let powershell = copilot_hook_powershell_command(&binary, "preToolUse");
        std::fs::write(
            &path,
            format!(
                r#"{{
  "hooks": {{
    "preToolUse": [
      {{ "type": "command", "bash": "{bash}", "powershell": "{powershell}", "timeoutSec": 600 }},
      {{ "type": "command", "bash": "foreign", "powershell": "foreign" }},
    ],
  }},
}}
"#
            ),
        )
        .unwrap();

        let result = remove_from_settings(&path, &binary).unwrap();

        assert_eq!(result.removed_entries, 1);
        assert!(!result.remaining_reference);
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("\"bash\": \"foreign\""));
        assert!(!written.contains(binary.to_string_lossy().as_ref()));
    }

    #[test]
    fn leaves_ambiguous_wrapper_reference_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let binary = directory.path().join(".kodosi/bin/kodosi-hook");
        std::fs::write(
            &path,
            format!(
                r#"{{"hooks":{{"PreToolUse":[{{"matcher":"*","hooks":[{{"type":"command","command":"sh -c '{}'"}}]}}]}}}}"#,
                binary.display()
            ),
        )
        .unwrap();

        let result = remove_from_settings(&path, &binary).unwrap();

        assert_eq!(result.removed_entries, 0);
        assert!(result.remaining_reference);
        assert!(std::fs::read_to_string(path).unwrap().contains("sh -c"));
    }
}
