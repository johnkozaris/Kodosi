use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::extensions::{AgentDefSummary, McpServerSummary, PluginSummary, SkillSummary};

#[must_use]
pub fn copilot_home() -> PathBuf {
    crate::runtime::paths::home_dir().join(".copilot")
}

#[must_use]
pub fn events_jsonl_path(session_id: &str) -> PathBuf {
    copilot_home()
        .join("session-state")
        .join(session_id)
        .join("events.jsonl")
}

#[must_use]
pub fn session_store_db_path() -> PathBuf {
    copilot_home().join("session-store.db")
}

#[derive(Debug, Clone, Deserialize)]
struct InstalledPluginEntry {
    name: String,
    marketplace: String,
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    cache_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    #[serde(default)]
    installed_plugins: Vec<InstalledPluginEntry>,
    #[serde(default)]
    enabled_plugins: std::collections::BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct PluginManifest {
    #[serde(default)]
    version: Option<String>,
}

fn enabled_plugin_entries(home: &Path, settings: SettingsView) -> Vec<InstalledPluginEntry> {
    if !settings.installed_plugins.is_empty() {
        return settings
            .installed_plugins
            .into_iter()
            .filter(|entry| entry.enabled)
            .collect();
    }

    settings
        .enabled_plugins
        .into_iter()
        .filter(|(_, enabled)| *enabled)
        .filter_map(|(id, _)| {
            let (name, marketplace) = id.split_once('@')?;
            let cache_path = home.join("installed-plugins").join(marketplace).join(name);
            cache_path.is_dir().then(|| InstalledPluginEntry {
                name: name.to_owned(),
                marketplace: marketplace.to_owned(),
                enabled: true,
                cache_path: Some(cache_path.to_string_lossy().into_owned()),
            })
        })
        .collect()
}

#[must_use]
pub fn read_installed_plugins(copilot_home: &Path) -> Vec<PluginSummary> {
    let path = copilot_home.join("settings.json");
    let Some(settings) = read_json_typed::<SettingsView>(&path) else {
        return Vec::new();
    };

    let mut plugins: Vec<PluginSummary> = enabled_plugin_entries(copilot_home, settings)
        .into_iter()
        .map(|entry| {
            let version = entry
                .cache_path
                .as_ref()
                .and_then(|p| read_plugin_manifest_version(Path::new(p)));
            PluginSummary {
                name: entry.name,
                version,
                source: Some(entry.marketplace),
                ..PluginSummary::default()
            }
        })
        .collect();
    plugins.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.name.cmp(&b.name)));
    plugins
}

fn read_plugin_manifest_version(cache_path: &Path) -> Option<String> {
    for manifest_path in [
        cache_path.join(".claude-plugin").join("plugin.json"),
        cache_path.join("plugin.json"),
        cache_path
            .join(".github")
            .join("plugin")
            .join("plugin.json"),
    ] {
        if let Some(manifest) = read_json_typed::<PluginManifest>(&manifest_path) {
            return manifest.version;
        }
    }
    None
}

#[must_use]
pub fn read_mcp_servers(copilot_home: &Path, cwd: Option<&Path>) -> Vec<McpServerSummary> {
    crate::mcp::enumerate_copilot(copilot_home, cwd)
        .into_iter()
        .map(|entry| McpServerSummary {
            scope: entry.scope.label().to_owned(),
            source_path: Some(entry.source_path.to_string_lossy().into_owned()),
            command: sanitised_mcp_display(entry.command.as_deref(), entry.url.as_deref()),
            name: entry.name,
            health: None,
            ..McpServerSummary::default()
        })
        .collect()
}

fn sanitised_mcp_display(command: Option<&str>, url: Option<&str>) -> Option<String> {
    if let Some(url) = url {
        return crate::mcp::url::redact_url_for_display(url);
    }
    if let Some(cmd) = command {
        let first_token = cmd.split_whitespace().next().unwrap_or(cmd);
        let basename = Path::new(first_token).file_name().map_or_else(
            || first_token.to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        return Some(basename);
    }
    None
}

#[must_use]
pub fn scan_skills_with_cwd(copilot_home: &Path, cwd: Option<&Path>) -> Vec<SkillSummary> {
    let mut out = Vec::new();
    collect_skills_in(&copilot_home.join("skills"), "user", &mut out);
    if let Some(cwd) = cwd {
        collect_skills_in(&cwd.join(".github").join("skills"), "workspace", &mut out);
    }

    let settings_path = copilot_home.join("settings.json");
    if let Some(view) = read_json_typed::<SettingsView>(&settings_path) {
        for entry in enabled_plugin_entries(copilot_home, view) {
            if let Some(cache_path) = entry.cache_path.as_deref() {
                let scope = format!("plugin:{}", entry.name);
                collect_skills_in(&Path::new(cache_path).join("skills"), &scope, &mut out);
            }
        }
    }

    out.sort_by(|a, b| a.scope.cmp(&b.scope).then_with(|| a.name.cmp(&b.name)));
    out
}

fn collect_skills_in(skills_dir: &Path, scope: &str, out: &mut Vec<SkillSummary>) {
    let Ok(entries) = std::fs::read_dir(skills_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let description = parse_description_frontmatter(&skill_md);
        out.push(SkillSummary {
            name,
            scope: scope.to_owned(),
            source_path: Some(skill_md.to_string_lossy().into_owned()),
            description,
            ..SkillSummary::default()
        });
    }
}

#[must_use]
pub fn scan_custom_agents(copilot_home: &Path, cwd: Option<&Path>) -> Vec<AgentDefSummary> {
    let mut out = Vec::new();
    collect_agents_in(&copilot_home.join("agents"), "user", &mut out);
    if let Some(cwd) = cwd {
        collect_agents_in(&cwd.join(".github").join("agents"), "workspace", &mut out);
    }
    let settings_path = copilot_home.join("settings.json");
    if let Some(view) = read_json_typed::<SettingsView>(&settings_path) {
        for entry in enabled_plugin_entries(copilot_home, view) {
            if let Some(cache_path) = entry.cache_path.as_deref() {
                let scope = format!("plugin:{}", entry.name);
                collect_agents_in(&Path::new(cache_path).join("agents"), &scope, &mut out);
            }
        }
    }
    out.sort_by(|a, b| a.scope.cmp(&b.scope).then_with(|| a.name.cmp(&b.name)));
    out
}

fn collect_agents_in(agents_dir: &Path, scope: &str, out: &mut Vec<AgentDefSummary>) {
    for path in crate::ops::custom_agents::find_custom_agent_files(
        agents_dir,
        crate::ops::custom_agents::AgentFileConvention::CopilotAgentMarkdown,
    ) {
        let Ok(parsed) = crate::ops::custom_agents::CustomAgentFile::parse(&path) else {
            continue;
        };
        let fallback_name = path
            .file_name()
            .map(|name| {
                name.to_string_lossy()
                    .trim_end_matches(".agent.md")
                    .trim_end_matches(".md")
                    .to_owned()
            })
            .unwrap_or_default();
        out.push(AgentDefSummary {
            name: if parsed.name.is_empty() {
                fallback_name
            } else {
                parsed.name
            },
            scope: scope.to_owned(),
            description: (!parsed.description.is_empty()).then_some(parsed.description),
            tools: parsed.tools,
            ..AgentDefSummary::default()
        });
    }
}

fn parse_description_frontmatter(path: &Path) -> Option<String> {
    let content = read_bounded(path, MAX_FRONTMATTER_BYTES)?;
    let mut in_frontmatter = false;
    for line in content.lines().take(80) {
        if line.trim() == "---" {
            if in_frontmatter {
                return None;
            }
            in_frontmatter = true;
            continue;
        }
        if !in_frontmatter {
            continue;
        }
        if let Some(rest) = line.strip_prefix("description:") {
            return Some(strip_quotes(rest.trim()).to_owned());
        }
    }
    None
}

fn strip_quotes(s: &str) -> &str {
    s.trim_matches('"').trim_matches('\'')
}

const MAX_CONFIG_BYTES: u64 = 1_048_576;
const MAX_FRONTMATTER_BYTES: u64 = 65_536;

fn read_bounded(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn read_json_typed<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let content = read_bounded(path, MAX_CONFIG_BYTES)?;
    serde_json::from_str(&content).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "agent-intel-copilot-fs-{}-{}",
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
        let mut f = fs::File::create(path).expect("create");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn installed_plugins_pulls_from_settings_and_enriches_version() {
        let home = scratch_dir("plugins-happy");
        let cache = home.join("cache").join("rust");
        write(
            &home.join("settings.json"),
            &format!(
                r#"{{
                  "copilotTokens": {{ "hidden": true }},
                  "installedPlugins": [
                    {{
                      "name": "rust",
                      "marketplace": "jko-claude-plugins",
                      "enabled": true,
                      "cache_path": "{}"
                    }},
                    {{
                      "name": "disabled-plugin",
                      "marketplace": "x",
                      "enabled": false,
                      "cache_path": "/nope"
                    }}
                  ]
                }}"#,
                cache.to_string_lossy()
            ),
        );
        write(
            &cache.join(".claude-plugin").join("plugin.json"),
            r#"{"name":"rust","version":"1.0.0"}"#,
        );

        let plugins = read_installed_plugins(&home);
        assert_eq!(plugins.len(), 1, "disabled plugins filtered");
        assert_eq!(plugins[0].name, "rust");
        assert_eq!(plugins[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(plugins[0].source.as_deref(), Some("jko-claude-plugins"));
    }

    #[test]
    fn enabled_plugins_resolve_current_installed_cache_tree() {
        let home = scratch_dir("plugins-current");
        let cache = home
            .join("installed-plugins")
            .join("jko-claude-plugins")
            .join("backend-architecture");
        write(
            &home.join("settings.json"),
            r#"{"enabledPlugins":{"backend-architecture@jko-claude-plugins":true,"disabled@x":false}}"#,
        );
        write(
            &cache.join(".claude-plugin").join("plugin.json"),
            r#"{"version":"2.4.0"}"#,
        );

        let plugins = read_installed_plugins(&home);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "backend-architecture");
        assert_eq!(plugins[0].source.as_deref(), Some("jko-claude-plugins"));
        assert_eq!(plugins[0].version.as_deref(), Some("2.4.0"));
    }

    #[test]
    fn installed_plugins_missing_file_returns_empty() {
        let home = scratch_dir("plugins-missing");
        assert!(read_installed_plugins(&home).is_empty());
    }

    #[test]
    fn installed_plugins_malformed_settings_returns_empty_no_panic() {
        let home = scratch_dir("plugins-malformed");
        write(&home.join("settings.json"), "{not valid json");
        assert!(read_installed_plugins(&home).is_empty());
    }

    #[test]
    fn installed_plugins_manifest_absent_still_surfaces_row_without_version() {
        let home = scratch_dir("plugins-no-manifest");
        let cache = home.join("cache").join("foo");
        fs::create_dir_all(&cache).unwrap();
        write(
            &home.join("settings.json"),
            &format!(
                r#"{{"installedPlugins":[{{"name":"foo","marketplace":"m","enabled":true,"cache_path":"{}"}}]}}"#,
                cache.to_string_lossy()
            ),
        );
        let plugins = read_installed_plugins(&home);
        assert_eq!(plugins.len(), 1);
        assert!(plugins[0].version.is_none());
    }

    #[test]
    fn mcp_sanitises_url_and_preserves_user_then_project_order() {
        let home = scratch_dir("mcp-happy");
        let cwd = home.join("project");
        write(
            &home.join("mcp-config.json"),
            r#"{"mcpServers":{"cloudflare":{"type":"http","url":"https://mcp.cf.com/mcp?token=s3cret"}}}"#,
        );
        write(
            &cwd.join(".mcp.json"),
            r#"{"mcpServers":{"local":{"command":"/usr/local/bin/node","args":["--token","abcd"]}}}"#,
        );

        let servers = read_mcp_servers(&home, Some(&cwd));
        assert_eq!(servers.len(), 2);
        assert_eq!(servers[0].scope, "user");
        assert_eq!(
            servers[0].command.as_deref(),
            Some("https://mcp.cf.com"),
            "display must redact userinfo, path, query, and fragment"
        );
        assert_eq!(
            servers[1].scope, "project",
            "per-cwd scope is unified to `project` across Claude + Copilot"
        );
        assert_eq!(
            servers[1].command.as_deref(),
            Some("node"),
            "stdio command should be reduced to basename only"
        );
    }

    #[test]
    fn mcp_fragment_stripped_too() {
        let home = scratch_dir("mcp-fragment");
        write(
            &home.join("mcp-config.json"),
            r#"{"mcpServers":{"s":{"url":"https://x.y/z#secret"}}}"#,
        );
        let servers = read_mcp_servers(&home, None);
        assert_eq!(servers[0].command.as_deref(), Some("https://x.y"));
    }

    #[test]
    fn mcp_url_userinfo_is_stripped() {
        let home = scratch_dir("mcp-userinfo");
        write(
            &home.join("mcp-config.json"),
            r#"{"mcpServers":{
                "gw":{"url":"https://oauth2:gho_REDACTED@mcp.example.com/mcp?token=x#frag"},
                "noscheme":{"url":"not-a-url"}
            }}"#,
        );
        let servers = read_mcp_servers(&home, None);
        assert_eq!(servers.len(), 2);
        assert_eq!(
            servers[0].command.as_deref(),
            Some("https://mcp.example.com"),
            "userinfo + query + fragment must all be stripped"
        );
        assert_eq!(
            servers[1].command, None,
            "opaque non-URL strings return None rather than echoing back"
        );
    }

    #[test]
    fn mcp_stdio_command_with_inline_args_drops_args() {
        let home = scratch_dir("mcp-inline-args");
        write(
            &home.join("mcp-config.json"),
            r#"{"mcpServers":{"bad":{"command":"npx --token s3cret run"}}}"#,
        );
        let servers = read_mcp_servers(&home, None);
        assert_eq!(
            servers[0].command.as_deref(),
            Some("npx"),
            "first whitespace token only — drop inline args"
        );
    }

    #[test]
    fn oversized_config_is_skipped_not_read() {
        let home = scratch_dir("oversized");
        let settings = home.join("settings.json");
        let mut body = String::from(r#"{"installedPlugins":["#);
        while body.len() < usize::try_from(MAX_CONFIG_BYTES).unwrap() + 1024 {
            body.push_str(r#"{"name":"x","marketplace":"m","enabled":true},"#);
        }
        body.push_str("]}");
        fs::write(&settings, body).unwrap();
        assert!(
            read_installed_plugins(&home).is_empty(),
            "oversized settings.json must be skipped, not loaded"
        );
    }

    #[test]
    fn skills_scan_walks_user_and_plugin_caches() {
        let home = scratch_dir("skills-happy");
        let plugin = home.join("cache").join("rust");
        write(
            &home.join("skills").join("user-skill").join("SKILL.md"),
            "---\ndescription: a user skill\n---\nbody",
        );
        write(
            &plugin.join("skills").join("plugin-skill").join("SKILL.md"),
            "---\ndescription: a plugin skill\n---\nbody",
        );
        write(
            &home.join("settings.json"),
            &format!(
                r#"{{"installedPlugins":[{{"name":"rust","marketplace":"m","enabled":true,"cache_path":"{}"}}]}}"#,
                plugin.to_string_lossy()
            ),
        );

        let skills = scan_skills_with_cwd(&home, None);
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0].scope, "plugin:rust");
        assert_eq!(skills[0].name, "plugin-skill");
        assert_eq!(
            skills[0].source_path.as_deref(),
            Some(
                plugin
                    .join("skills/plugin-skill/SKILL.md")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert_eq!(skills[1].scope, "user");
        assert_eq!(skills[1].name, "user-skill");
        assert_eq!(
            skills[1].source_path.as_deref(),
            Some(
                home.join("skills/user-skill/SKILL.md")
                    .to_string_lossy()
                    .as_ref()
            )
        );
    }

    #[test]
    fn skills_scan_preserves_exact_workspace_path() {
        let home = scratch_dir("skills-workspace");
        let cwd = home.join("project");
        let skill = cwd.join(".github/skills/review/SKILL.md");
        write(&skill, "---\ndescription: workspace review\n---\nbody");

        let skills = scan_skills_with_cwd(&home, Some(&cwd));

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].scope, "workspace");
        assert_eq!(
            skills[0].source_path.as_deref(),
            Some(skill.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn skills_scan_skips_dirs_without_skill_md() {
        let home = scratch_dir("skills-no-md");
        fs::create_dir_all(home.join("skills").join("empty")).unwrap();
        assert!(scan_skills_with_cwd(&home, None).is_empty());
    }

    #[test]
    fn custom_agents_scan_parses_tools_list() {
        let home = scratch_dir("agents");
        write(
            &home.join("agents").join("researcher.agent.md"),
            "---\nname: researcher\ndescription: does research\ntools:\n  - read\n  - web\n---\nbody",
        );
        let agents = scan_custom_agents(&home, None);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].scope, "user");
        assert_eq!(agents[0].name, "researcher");
        assert_eq!(agents[0].description.as_deref(), Some("does research"));
        assert_eq!(agents[0].tools, vec!["read".to_owned(), "web".to_owned()]);
    }

    #[test]
    fn settings_view_drops_secret_siblings() {
        let home = scratch_dir("settings-secret");
        write(
            &home.join("settings.json"),
            r#"{
                "copilotTokens": {"token": "gho_XXXXXXXXXXXXXXX"},
                "lastLoggedInUser": {"token": "gho_YYYYYYYYYYY"},
                "installedPlugins": []
            }"#,
        );
        let plugins = read_installed_plugins(&home);
        assert!(plugins.is_empty());
        let raw = fs::read_to_string(home.join("settings.json")).unwrap();
        assert!(raw.contains("gho_"));
    }
}
