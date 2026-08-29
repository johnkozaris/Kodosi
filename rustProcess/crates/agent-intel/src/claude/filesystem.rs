use std::{io::Read, path::Path};

use serde_json::Value;

use super::global::{AgentDefSummary, McpServerSummary, PluginSummary, SkillSummary};

#[must_use]
pub fn read_installed_plugins(claude_home: &Path) -> Vec<PluginSummary> {
    let path = claude_home.join("plugins").join("installed_plugins.json");
    let Some(value): Option<Value> = read_json(&path) else {
        return Vec::new();
    };
    let Some(plugins_map) = value.get("plugins").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (id_with_market, entries) in plugins_map {
        let (plugin_id, marketplace) = match id_with_market.split_once('@') {
            Some((id, market)) => (id.to_owned(), market.to_owned()),
            None => (id_with_market.clone(), String::new()),
        };
        if let Some(arr) = entries.as_array() {
            for entry in arr {
                let scope = entry
                    .get("scope")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_owned();
                let version = entry
                    .get("version")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let installed_at = entry
                    .get("installedAt")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                out.push(PluginSummary {
                    id: plugin_id.clone(),
                    marketplace: marketplace.clone(),
                    scope,
                    version,
                    installed_at,
                });
            }
        }
    }
    out
}

#[must_use]
pub fn scan_skills_with_cwd(claude_home: &Path, cwd: Option<&Path>) -> Vec<SkillSummary> {
    let mut out = Vec::new();
    collect_skills_in(&claude_home.join("skills"), "user", &mut out);
    if let Some(cwd) = cwd {
        collect_skills_in(&cwd.join(".claude").join("skills"), "project", &mut out);
    }
    let plugin_cache = claude_home.join("plugins").join("cache");
    if let Ok(entries) = std::fs::read_dir(&plugin_cache) {
        for entry in entries.flatten() {
            let marketplace = entry.file_name().to_string_lossy().to_string();
            collect_plugin_skills(&entry.path(), &marketplace, &mut out);
        }
    }
    out
}

fn collect_plugin_skills(marketplace_dir: &Path, marketplace: &str, out: &mut Vec<SkillSummary>) {
    let Ok(plugins) = std::fs::read_dir(marketplace_dir) else {
        return;
    };
    for plugin in plugins.flatten() {
        let plugin_path = plugin.path();
        let Ok(versions) = std::fs::read_dir(&plugin_path) else {
            continue;
        };
        for version in versions.flatten() {
            let skills_dir = version.path().join("skills");
            collect_skills_in(&skills_dir, &format!("plugin:{marketplace}"), out);
        }
    }
}

fn collect_skills_in(skills_dir: &Path, source: &str, out: &mut Vec<SkillSummary>) {
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let skill_md = entry_path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = entry_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let (description, user_invocable) = parse_skill_frontmatter(&skill_md);
        out.push(SkillSummary {
            name,
            source: source.to_owned(),
            source_path: Some(skill_md.to_string_lossy().into_owned()),
            description,
            user_invocable,
        });
    }
}

const MAX_SKILL_FRONTMATTER_BYTES: u64 = 64 * 1024;

fn parse_skill_frontmatter(skill_md: &Path) -> (Option<String>, bool) {
    let Ok(file) = std::fs::File::open(skill_md) else {
        return (None, false);
    };
    let mut content = String::new();
    if file
        .take(MAX_SKILL_FRONTMATTER_BYTES.saturating_add(1))
        .read_to_string(&mut content)
        .is_err()
        || u64::try_from(content.len()).unwrap_or(u64::MAX) > MAX_SKILL_FRONTMATTER_BYTES
    {
        return (None, false);
    }
    let mut in_frontmatter = false;
    let mut description = None;
    let mut user_invocable = false;
    for line in content.lines().take(80) {
        if line.trim() == "---" {
            if in_frontmatter {
                break;
            }
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            continue;
        }
        if let Some(rest) = line.strip_prefix("description:") {
            description = Some(rest.trim().trim_matches('"').trim_matches('\'').to_owned());
        } else if let Some(rest) = line.strip_prefix("user-invocable:") {
            user_invocable = rest.trim().eq_ignore_ascii_case("true");
        }
    }
    (description, user_invocable)
}

#[must_use]
pub fn scan_custom_agents(claude_home: &Path, cwd: Option<&Path>) -> Vec<AgentDefSummary> {
    let mut out = Vec::new();
    collect_agents_in(&claude_home.join("agents"), "user", &mut out);
    if let Some(cwd) = cwd {
        collect_agents_in(&cwd.join(".claude").join("agents"), "project", &mut out);
    }
    out
}

fn collect_agents_in(agents_dir: &Path, source: &str, out: &mut Vec<AgentDefSummary>) {
    for path in crate::ops::custom_agents::find_custom_agent_files(
        agents_dir,
        crate::ops::custom_agents::AgentFileConvention::ClaudeMarkdown,
    ) {
        let Ok(parsed) = crate::ops::custom_agents::CustomAgentFile::parse(&path) else {
            continue;
        };
        let fallback_name = path
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push(AgentDefSummary {
            name: if parsed.name.is_empty() {
                fallback_name
            } else {
                parsed.name
            },
            source: source.to_owned(),
            description: (!parsed.description.is_empty()).then_some(parsed.description),
            disallowed_tools: parsed.disallowed_tools,
        });
    }
}

#[must_use]
pub fn read_mcp_servers(claude_home: &Path, cwd: Option<&Path>) -> Vec<McpServerSummary> {
    crate::mcp::enumerate_claude(claude_home, cwd)
        .into_iter()
        .map(|entry| McpServerSummary {
            transport: entry.transport().map(ToOwned::to_owned),
            scope: entry.scope.label().to_owned(),
            name: entry.name,
            enabled: true,
            health: None,
        })
        .collect()
}

const MAX_CONFIG_JSON_BYTES: u64 = 10 * 1024 * 1024;

fn read_json(path: &Path) -> Option<Value> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_CONFIG_JSON_BYTES {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agent-intel-claude-fs-{}-{}",
            std::process::id(),
            name
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        let mut file = fs::File::create(path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn installed_plugins_splits_id_and_market() {
        let dir = scratch_dir("plugins");
        write(
            &dir.join("plugins").join("installed_plugins.json"),
            r#"{
                "plugins": {
                    "rust@jko-claude-plugins": [
                        { "scope": "user", "version": "1.0.0", "installedAt": "2026-03-01" }
                    ]
                }
            }"#,
        );
        let plugins = read_installed_plugins(&dir);
        assert_eq!(plugins.len(), 1);
        let plugin = plugins.first().expect("first");
        assert_eq!(plugin.id, "rust");
        assert_eq!(plugin.marketplace, "jko-claude-plugins");
        assert_eq!(plugin.scope, "user");
        assert_eq!(plugin.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn skill_frontmatter_extracts_description() {
        let dir = scratch_dir("skills");
        let skill_dir = dir.join("skills").join("example-skill");
        write(
            &skill_dir.join("SKILL.md"),
            "---\nname: example-skill\ndescription: A fine example\nuser-invocable: true\n---\nbody",
        );
        let skills = scan_skills_with_cwd(&dir, None);
        assert_eq!(skills.len(), 1);
        let skill = skills.first().expect("first");
        assert_eq!(skill.name, "example-skill");
        assert_eq!(skill.description.as_deref(), Some("A fine example"));
        assert!(skill.user_invocable);
    }

    #[test]
    fn oversized_skill_frontmatter_is_ignored() {
        let dir = scratch_dir("skills-oversized");
        let skill_md = dir.join("skills").join("large").join("SKILL.md");
        if let Some(parent) = skill_md.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let file = fs::File::create(&skill_md).unwrap();
        file.set_len(MAX_SKILL_FRONTMATTER_BYTES + 1).unwrap();

        let skills = scan_skills_with_cwd(&dir, None);
        let skill = skills.iter().find(|skill| skill.name == "large").unwrap();
        assert_eq!(skill.description, None);
        assert!(!skill.user_invocable);
    }

    #[test]
    fn plugin_skills_found_under_version_slash_skills() {
        let dir = scratch_dir("plugin_skills");
        let skill_md = dir
            .join("plugins")
            .join("cache")
            .join("acme-market")
            .join("widget")
            .join("1.0.0")
            .join("skills")
            .join("widget-helper")
            .join("SKILL.md");
        write(
            &skill_md,
            "---\ndescription: helps with widgets\nuser-invocable: false\n---\nbody",
        );
        let skills = scan_skills_with_cwd(&dir, None);
        let widget = skills
            .iter()
            .find(|s| s.name == "widget-helper")
            .expect("plugin-cached skill must surface");
        assert_eq!(widget.source, "plugin:acme-market");
        assert_eq!(
            widget.source_path.as_deref(),
            Some(skill_md.to_string_lossy().as_ref()),
            "plugin skill summaries must retain the exact cache path"
        );
        assert_eq!(
            widget.description.as_deref(),
            Some("helps with widgets"),
            "frontmatter parse must still work for plugin skills"
        );
    }
}
