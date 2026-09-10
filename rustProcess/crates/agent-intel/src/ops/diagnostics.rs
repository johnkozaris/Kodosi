use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::claude::extensions::InstructionSource;
use crate::domain::diagnostics::{
    ActiveCustomizationsReport, CustomizationEntry, CustomizationError, CustomizationKind,
    CustomizationStatus, CustomizationVendor, Scope,
};
use crate::ops::instructions::{self, InstructionAgentType};

pub async fn list_active_customizations(
    home: &Path,
    cwd: &Path,
    vendor: CustomizationVendor,
) -> Result<ActiveCustomizationsReport, String> {
    let agent_type = match vendor {
        CustomizationVendor::Claude => InstructionAgentType::Claude,
        CustomizationVendor::Copilot => InstructionAgentType::Copilot,
        CustomizationVendor::Unknown => {
            return Err("active customizations require vendor `claude` or `copilot`".to_owned());
        }
    };
    let instructions_list =
        instructions::scan_instructions(home, cwd.to_string_lossy().as_ref(), agent_type).await?;
    let home_owned = home.to_path_buf();
    let cwd_owned = cwd.to_path_buf();
    tokio::task::spawn_blocking(move || match vendor {
        CustomizationVendor::Claude => {
            build_claude_blocking(&home_owned, &cwd_owned, instructions_list)
        }
        CustomizationVendor::Copilot => {
            build_copilot_blocking(&home_owned, &cwd_owned, instructions_list)
        }
        CustomizationVendor::Unknown => unreachable!("validated before spawn"),
    })
    .await
    .map_err(|error| format!("diagnostics task join error: {error}"))
}

pub async fn list_active_customizations_from_root(
    home: &Path,
    root: &std::fs::File,
    root_path: &Path,
    vendor: CustomizationVendor,
) -> Result<ActiveCustomizationsReport, String> {
    let agent_type = match vendor {
        CustomizationVendor::Claude => InstructionAgentType::Claude,
        CustomizationVendor::Copilot => InstructionAgentType::Copilot,
        CustomizationVendor::Unknown => {
            return Err("active customizations require vendor `claude` or `copilot`".to_owned());
        }
    };
    let instructions_list =
        instructions::scan_instructions_from_root(home, root, root_path, agent_type).await?;
    let home_owned = home.to_path_buf();
    let root_path = root_path.to_path_buf();
    tokio::task::spawn_blocking(move || match vendor {
        CustomizationVendor::Claude => {
            build_claude_blocking(&home_owned, &root_path, instructions_list)
        }
        CustomizationVendor::Copilot => {
            build_copilot_blocking(&home_owned, &root_path, instructions_list)
        }
        CustomizationVendor::Unknown => unreachable!("validated before spawn"),
    })
    .await
    .map_err(|error| format!("diagnostics task join error: {error}"))
}

fn build_claude_blocking(
    home: &Path,
    cwd: &Path,
    instructions_list: Vec<InstructionSource>,
) -> ActiveCustomizationsReport {
    let claude_home = home.join(".claude");
    let mut report = ActiveCustomizationsReport {
        vendor: CustomizationVendor::Claude,
        ..ActiveCustomizationsReport::default()
    };

    for summary in crate::claude::filesystem::scan_skills_with_cwd(&claude_home, Some(cwd)) {
        report.skills.push(CustomizationEntry {
            kind: CustomizationKind::Skill,
            name: summary.name.clone(),
            scope: Scope::from_source_str(&summary.source, Some(cwd.to_string_lossy().as_ref())),
            enabled: true,
            source_path: summary
                .source_path
                .map(PathBuf::from)
                .or_else(|| skill_source_path(&claude_home, cwd, &summary.source, &summary.name)),
            description: summary.description,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    for summary in crate::claude::filesystem::read_mcp_servers(&claude_home, Some(cwd)) {
        let scope = Scope::from_source_str(&summary.scope, Some(cwd.to_string_lossy().as_ref()));
        let source_path = match summary.scope.as_str() {
            "user" => Some(claude_home.join("mcp.json")),
            "project" | "workspace" => Some(cwd.join(".mcp.json")),
            _ => None,
        };
        report.mcp_servers.push(CustomizationEntry {
            kind: CustomizationKind::McpServer,
            name: summary.name,
            scope,
            enabled: summary.enabled,
            source_path,
            description: summary.transport,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    for summary in crate::claude::filesystem::read_installed_plugins(&claude_home) {
        report.plugins.push(CustomizationEntry {
            kind: CustomizationKind::Plugin,
            name: summary.id.clone(),
            scope: Scope::Plugin {
                marketplace: summary.marketplace.clone(),
                plugin: Some(summary.id.clone()),
            },
            enabled: true,
            source_path: Some(
                claude_home
                    .join("plugins")
                    .join("cache")
                    .join(&summary.marketplace)
                    .join(&summary.id),
            ),
            description: summary.version.clone(),
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: summary.installed_at,
        });
    }

    collect_custom_agents(&mut report, &claude_home.join("agents"), &Scope::User);
    collect_custom_agents(
        &mut report,
        &cwd.join(".claude").join("agents"),
        &Scope::Workspace {
            cwd: Some(cwd.to_string_lossy().into_owned()),
        },
    );

    for entry in instructions_list {
        let path = PathBuf::from(&entry.path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        report.instructions.push(CustomizationEntry {
            kind: CustomizationKind::Instructions,
            name,
            scope: instruction_scope(&path, home, cwd),
            enabled: true,
            source_path: Some(path),
            description: None,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    collect_hooks(
        &mut report,
        &claude_home.join("settings.json"),
        &Scope::User,
    );
    collect_hooks(
        &mut report,
        &cwd.join(".claude").join("settings.json"),
        &Scope::Workspace {
            cwd: Some(cwd.to_string_lossy().into_owned()),
        },
    );
    collect_hooks(
        &mut report,
        &cwd.join(".claude").join("settings.local.json"),
        &Scope::Workspace {
            cwd: Some(cwd.to_string_lossy().into_owned()),
        },
    );

    decorate_overrides(&mut report.skills);
    decorate_overrides(&mut report.custom_agents);
    decorate_overrides(&mut report.mcp_servers);
    decorate_hook_overrides(&mut report.hooks);

    report
}

fn build_copilot_blocking(
    home: &Path,
    cwd: &Path,
    instructions_list: Vec<InstructionSource>,
) -> ActiveCustomizationsReport {
    let copilot_home = home.join(".copilot");
    let mut report = ActiveCustomizationsReport {
        vendor: CustomizationVendor::Copilot,
        ..ActiveCustomizationsReport::default()
    };

    for summary in crate::copilot::filesystem::scan_skills_with_cwd(&copilot_home, Some(cwd)) {
        report.skills.push(CustomizationEntry {
            kind: CustomizationKind::Skill,
            name: summary.name,
            scope: Scope::from_source_str(&summary.scope, Some(cwd.to_string_lossy().as_ref())),
            enabled: true,
            source_path: summary.source_path.map(PathBuf::from),
            description: summary.description,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    for summary in crate::copilot::filesystem::read_mcp_servers(&copilot_home, Some(cwd)) {
        report.mcp_servers.push(CustomizationEntry {
            kind: CustomizationKind::McpServer,
            name: summary.name,
            scope: Scope::from_source_str(&summary.scope, Some(cwd.to_string_lossy().as_ref())),
            enabled: true,
            source_path: summary.source_path.map(PathBuf::from),
            description: summary.command,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    for summary in crate::copilot::filesystem::read_installed_plugins(&copilot_home) {
        report.plugins.push(CustomizationEntry {
            kind: CustomizationKind::Plugin,
            name: summary.name,
            scope: Scope::Plugin {
                marketplace: summary.source.clone().unwrap_or_default(),
                plugin: None,
            },
            enabled: true,
            source_path: Some(copilot_home.join("settings.json")),
            description: summary.version,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    collect_custom_agents(&mut report, &copilot_home.join("agents"), &Scope::User);
    collect_custom_agents(
        &mut report,
        &cwd.join(".github").join("agents"),
        &Scope::Workspace {
            cwd: Some(cwd.to_string_lossy().into_owned()),
        },
    );

    for entry in instructions_list {
        let path = PathBuf::from(&entry.path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        report.instructions.push(CustomizationEntry {
            kind: CustomizationKind::Instructions,
            name,
            scope: instruction_scope(&path, home, cwd),
            enabled: true,
            source_path: Some(path),
            description: None,
            overrides: Vec::new(),
            status: CustomizationStatus::Loaded,
            status_message: None,
            nonce: None,
        });
    }

    let user_settings_path = copilot_home.join("settings.json");
    let workspace_settings_path = cwd.join(".github/copilot/settings.json");
    let local_settings_path = cwd.join(".github/copilot/settings.local.json");
    let user_settings = read_diagnostic_json(&mut report, &user_settings_path);
    let workspace_settings = read_diagnostic_json(&mut report, &workspace_settings_path);
    let local_settings = read_diagnostic_json(&mut report, &local_settings_path);
    let disable_all = [&user_settings, &workspace_settings, &local_settings]
        .into_iter()
        .flatten()
        .any(|settings| {
            settings
                .get("disableAllHooks")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        });
    collect_copilot_hook_files(
        &mut report,
        &copilot_home.join("hooks"),
        &Scope::User,
        !disable_all,
    );
    collect_copilot_hook_files(
        &mut report,
        &cwd.join(".github/hooks"),
        &Scope::Workspace {
            cwd: Some(cwd.to_string_lossy().into_owned()),
        },
        !disable_all,
    );
    for (path, scope, settings) in [
        (user_settings_path, Scope::User, user_settings),
        (
            workspace_settings_path,
            Scope::Workspace {
                cwd: Some(cwd.to_string_lossy().into_owned()),
            },
            workspace_settings,
        ),
        (
            local_settings_path,
            Scope::Workspace {
                cwd: Some(cwd.to_string_lossy().into_owned()),
            },
            local_settings,
        ),
    ] {
        if let Some(settings) = settings {
            collect_copilot_hook_value(
                &mut report,
                &path,
                &scope,
                &settings,
                !disable_all && copilot_hook_source_enabled(&settings),
            );
        }
    }

    decorate_overrides(&mut report.skills);
    decorate_overrides(&mut report.custom_agents);
    decorate_overrides(&mut report.mcp_servers);
    report
}

fn collect_custom_agents(
    report: &mut ActiveCustomizationsReport,
    agents_dir: &Path,
    scope: &Scope,
) {
    let convention = match report.vendor {
        CustomizationVendor::Claude => {
            crate::ops::custom_agents::AgentFileConvention::ClaudeMarkdown
        }
        CustomizationVendor::Copilot | CustomizationVendor::Unknown => {
            crate::ops::custom_agents::AgentFileConvention::CopilotAgentMarkdown
        }
    };
    for path in crate::ops::custom_agents::find_custom_agent_files(agents_dir, convention) {
        match crate::ops::custom_agents::CustomAgentFile::parse(&path) {
            Ok(parsed) => {
                let fallback = path
                    .file_name()
                    .map(|name| {
                        name.to_string_lossy()
                            .trim_end_matches(".agent.md")
                            .trim_end_matches(".md")
                            .to_owned()
                    })
                    .unwrap_or_default();
                let status = if parsed.errors.is_empty() {
                    CustomizationStatus::Loaded
                } else {
                    CustomizationStatus::Degraded
                };
                if !parsed.errors.is_empty() {
                    report
                        .notices
                        .push(crate::domain::DegradationNotice::AgentParseFailed {
                            vendor: vendor_label(report.vendor).to_owned(),
                            path: path.to_string_lossy().into_owned(),
                            message: parsed.errors.join("; "),
                        });
                }
                report.custom_agents.push(CustomizationEntry {
                    kind: CustomizationKind::CustomAgent,
                    name: if parsed.name.is_empty() {
                        fallback
                    } else {
                        parsed.name
                    },
                    scope: scope.clone(),
                    enabled: true,
                    source_path: Some(path),
                    description: (!parsed.description.is_empty()).then_some(parsed.description),
                    overrides: Vec::new(),
                    status,
                    status_message: (!parsed.errors.is_empty()).then(|| parsed.errors.join("; ")),
                    nonce: None,
                });
            }
            Err(reason) => {
                report
                    .notices
                    .push(crate::domain::DegradationNotice::AgentParseFailed {
                        vendor: vendor_label(report.vendor).to_owned(),
                        path: path.to_string_lossy().into_owned(),
                        message: reason.clone(),
                    });
                report.errors.push(CustomizationError {
                    source_path: path,
                    reason,
                });
            }
        }
    }
}

const fn vendor_label(vendor: CustomizationVendor) -> &'static str {
    match vendor {
        CustomizationVendor::Claude => "claude",
        CustomizationVendor::Copilot => "copilot",
        CustomizationVendor::Unknown => "unknown",
    }
}

pub(crate) fn catalog_notices(
    home: &Path,
    cwd: Option<&Path>,
    vendor: CustomizationVendor,
) -> Vec<crate::domain::DegradationNotice> {
    let mut report = ActiveCustomizationsReport {
        vendor,
        ..ActiveCustomizationsReport::default()
    };
    match vendor {
        CustomizationVendor::Claude => {
            let claude_home = home.join(".claude");
            collect_settings_notice(&mut report, &claude_home.join("settings.json"));
            collect_agent_notices(&mut report, &claude_home.join("agents"));
            if let Some(cwd) = cwd {
                collect_settings_notice(&mut report, &cwd.join(".claude/settings.json"));
                collect_settings_notice(&mut report, &cwd.join(".claude/settings.local.json"));
                collect_agent_notices(&mut report, &cwd.join(".claude/agents"));
            }
        }
        CustomizationVendor::Copilot => {
            let copilot_home = home.join(".copilot");
            collect_settings_notice(&mut report, &copilot_home.join("settings.json"));
            collect_agent_notices(&mut report, &copilot_home.join("agents"));
            if let Some(cwd) = cwd {
                collect_agent_notices(&mut report, &cwd.join(".github/agents"));
            }
        }
        CustomizationVendor::Unknown => {}
    }
    report.notices
}

fn collect_settings_notice(report: &mut ActiveCustomizationsReport, path: &Path) {
    drop(read_diagnostic_json(report, path));
}

fn collect_agent_notices(report: &mut ActiveCustomizationsReport, agents_dir: &Path) {
    let convention = match report.vendor {
        CustomizationVendor::Claude => {
            crate::ops::custom_agents::AgentFileConvention::ClaudeMarkdown
        }
        CustomizationVendor::Copilot | CustomizationVendor::Unknown => {
            crate::ops::custom_agents::AgentFileConvention::CopilotAgentMarkdown
        }
    };
    for path in crate::ops::custom_agents::find_custom_agent_files(agents_dir, convention) {
        match crate::ops::custom_agents::CustomAgentFile::parse(&path) {
            Ok(parsed) if !parsed.errors.is_empty() => {
                report
                    .notices
                    .push(crate::domain::DegradationNotice::AgentParseFailed {
                        vendor: vendor_label(report.vendor).to_owned(),
                        path: path.to_string_lossy().into_owned(),
                        message: parsed.errors.join("; "),
                    });
            }
            Err(message) => {
                report
                    .notices
                    .push(crate::domain::DegradationNotice::AgentParseFailed {
                        vendor: vendor_label(report.vendor).to_owned(),
                        path: path.to_string_lossy().into_owned(),
                        message,
                    });
            }
            Ok(_) => {}
        }
    }
}

fn collect_copilot_hook_files(
    report: &mut ActiveCustomizationsReport,
    hooks_dir: &Path,
    scope: &Scope,
    globally_enabled: bool,
) {
    let Ok(entries) = std::fs::read_dir(hooks_dir) else {
        return;
    };
    let mut paths = Vec::new();
    let mut scan_truncated = false;
    for entry in entries.take(MAX_DIAGNOSTIC_DIRECTORY_ENTRIES.saturating_add(1)) {
        let Ok(entry) = entry else {
            continue;
        };
        if paths.len() == MAX_DIAGNOSTIC_DIRECTORY_ENTRIES {
            scan_truncated = true;
            break;
        }
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            paths.push(path);
        }
    }
    if scan_truncated {
        push_settings_notice(
            report,
            hooks_dir,
            format!("hook directory scan exceeds {MAX_DIAGNOSTIC_DIRECTORY_ENTRIES} entry limit"),
        );
    }
    paths.sort();
    for path in paths {
        let Some(value) = read_diagnostic_json(report, &path) else {
            continue;
        };
        collect_copilot_hook_value(
            report,
            &path,
            scope,
            &value,
            globally_enabled && copilot_hook_source_enabled(&value),
        );
    }
}

fn copilot_hook_source_enabled(value: &serde_json::Value) -> bool {
    if value
        .get("disableAllHooks")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return false;
    }
    value
        .get("enabled")
        .or_else(|| value.get("hooksEnabled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

fn collect_copilot_hook_value(
    report: &mut ActiveCustomizationsReport,
    source_path: &Path,
    scope: &Scope,
    value: &serde_json::Value,
    source_enabled: bool,
) {
    let Some(hooks) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return;
    };
    for (event_name, configs) in hooks {
        let Some(configs) = configs.as_array() else {
            continue;
        };
        for (index, config) in configs.iter().enumerate() {
            let entry_enabled = config
                .get("enabled")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            let nested_enabled = config
                .get("hooks")
                .and_then(serde_json::Value::as_array)
                .is_none_or(|entries| {
                    entries.iter().any(|entry| {
                        entry
                            .get("enabled")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true)
                    })
                });
            let enabled = source_enabled && entry_enabled && nested_enabled;
            report.hooks.push(CustomizationEntry {
                kind: CustomizationKind::Hook,
                name: format!("{event_name}[{index}]"),
                scope: scope.clone(),
                enabled,
                source_path: Some(source_path.to_path_buf()),
                description: config
                    .get("matcher")
                    .and_then(serde_json::Value::as_str)
                    .map_or_else(
                        || Some(event_name.clone()),
                        |matcher| Some(matcher.to_owned()),
                    ),
                overrides: Vec::new(),
                status: CustomizationStatus::Loaded,
                status_message: (!enabled).then(|| {
                    if source_enabled {
                        "disabled by hook source".to_owned()
                    } else {
                        "disabled by Copilot settings".to_owned()
                    }
                }),
                nonce: None,
            });
        }
    }
}

fn skill_source_path(claude_home: &Path, cwd: &Path, source: &str, name: &str) -> Option<PathBuf> {
    match source {
        "user" => Some(claude_home.join("skills").join(name).join("SKILL.md")),
        "project" | "workspace" => Some(
            cwd.join(".claude")
                .join("skills")
                .join(name)
                .join("SKILL.md"),
        ),
        _ => None,
    }
}

fn instruction_scope(path: &Path, home: &Path, cwd: &Path) -> Scope {
    if path.starts_with(cwd) {
        Scope::Workspace {
            cwd: Some(cwd.to_string_lossy().into_owned()),
        }
    } else if path.starts_with(home) {
        Scope::User
    } else {
        Scope::BuiltIn
    }
}

fn collect_hooks(report: &mut ActiveCustomizationsReport, settings_path: &Path, scope: &Scope) {
    let Some(value) = read_diagnostic_json(report, settings_path) else {
        return;
    };
    let Some(hooks) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return;
    };
    for (event_name, configs) in hooks {
        if let Some(matchers) = configs.as_array() {
            for (idx, matcher) in matchers.iter().enumerate() {
                let matcher_pattern = matcher
                    .get("matcher")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                let command_identity = matcher
                    .get("hooks")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|hook| {
                        hook.get("command")
                            .or_else(|| hook.get("bash"))
                            .and_then(serde_json::Value::as_str)
                    })
                    .collect::<Vec<_>>()
                    .join("\u{1f}");
                report.hooks.push(CustomizationEntry {
                    kind: CustomizationKind::Hook,
                    name: format!(
                        "{event_name}\u{1f}{}\u{1f}{command_identity}",
                        matcher_pattern.as_deref().unwrap_or_default()
                    ),
                    scope: scope.clone(),
                    enabled: true,
                    source_path: Some(settings_path.to_path_buf()),
                    description: matcher_pattern.or_else(|| Some(event_name.clone())),
                    overrides: Vec::new(),
                    status: CustomizationStatus::Loaded,
                    status_message: None,
                    nonce: Some(idx.to_string()),
                });
            }
        }
    }
}

const MAX_DIAGNOSTIC_SETTINGS_BYTES: u64 = 1024 * 1024;
const MAX_DIAGNOSTIC_DIRECTORY_ENTRIES: usize = 4096;

fn read_diagnostic_json(
    report: &mut ActiveCustomizationsReport,
    path: &Path,
) -> Option<serde_json::Value> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            push_settings_notice(report, path, error.to_string());
            return None;
        }
    };
    if !metadata.is_file() {
        push_settings_notice(
            report,
            path,
            "settings path is not a regular file".to_owned(),
        );
        return None;
    }
    if metadata.len() > MAX_DIAGNOSTIC_SETTINGS_BYTES {
        push_settings_notice(
            report,
            path,
            format!("settings file exceeds {MAX_DIAGNOSTIC_SETTINGS_BYTES} byte limit"),
        );
        return None;
    }
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            push_settings_notice(report, path, error.to_string());
            return None;
        }
    };
    let mut contents = String::new();
    if let Err(error) = file
        .take(MAX_DIAGNOSTIC_SETTINGS_BYTES.saturating_add(1))
        .read_to_string(&mut contents)
    {
        push_settings_notice(report, path, error.to_string());
        return None;
    }
    if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_DIAGNOSTIC_SETTINGS_BYTES {
        push_settings_notice(
            report,
            path,
            format!("settings file exceeds {MAX_DIAGNOSTIC_SETTINGS_BYTES} byte limit"),
        );
        return None;
    }
    match serde_json::from_str(&contents) {
        Ok(value) => Some(value),
        Err(error) => {
            push_settings_notice(report, path, error.to_string());
            None
        }
    }
}

fn push_settings_notice(report: &mut ActiveCustomizationsReport, path: &Path, message: String) {
    report
        .notices
        .push(crate::domain::DegradationNotice::MalformedSettings {
            vendor: vendor_label(report.vendor).to_owned(),
            path: path.to_string_lossy().into_owned(),
            message,
        });
}

fn hook_match_key(entry: &CustomizationEntry) -> &str {
    entry.name.as_str()
}

fn hook_display_name(entry: &CustomizationEntry) -> String {
    let event = entry
        .name
        .split('\u{1f}')
        .next()
        .unwrap_or(entry.name.as_str());
    let index = entry.nonce.as_deref().unwrap_or("0");
    format!("{event}[{index}]")
}

fn decorate_hook_overrides(entries: &mut [CustomizationEntry]) {
    decorate_overrides(entries);
    for entry in entries {
        entry.name = hook_display_name(entry);
    }
}

fn decorate_overrides(entries: &mut [CustomizationEntry]) {
    let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        by_name
            .entry(hook_match_key(entry).to_owned())
            .or_default()
            .push(i);
    }
    let mut shadowed: HashMap<usize, Vec<PathBuf>> = HashMap::new();
    for indices in by_name.values() {
        if indices.len() < 2 {
            continue;
        }
        let workspace_indices: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|i| matches!(entries[*i].scope, Scope::Workspace { .. }))
            .collect();
        let other_paths: Vec<PathBuf> = indices
            .iter()
            .copied()
            .filter(|i| !matches!(entries[*i].scope, Scope::Workspace { .. }))
            .filter_map(|i| entries[i].source_path.clone())
            .collect();
        for ws_idx in workspace_indices {
            shadowed.insert(ws_idx, other_paths.clone());
        }
    }
    for (idx, paths) in shadowed {
        entries[idx].overrides = paths;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn override_detection_marks_workspace_shadowing_user() {
        let mut entries = vec![
            CustomizationEntry {
                kind: CustomizationKind::Skill,
                name: "share".into(),
                scope: Scope::User,
                enabled: true,
                source_path: Some(PathBuf::from("/home/u/.claude/skills/share/SKILL.md")),
                description: None,
                overrides: vec![],
                status: CustomizationStatus::Loaded,
                status_message: None,
                nonce: None,
            },
            CustomizationEntry {
                kind: CustomizationKind::Skill,
                name: "share".into(),
                scope: Scope::Workspace {
                    cwd: Some("/repo".into()),
                },
                enabled: true,
                source_path: Some(PathBuf::from("/repo/.claude/skills/share/SKILL.md")),
                description: None,
                overrides: vec![],
                status: CustomizationStatus::Loaded,
                status_message: None,
                nonce: None,
            },
        ];
        decorate_overrides(&mut entries);
        assert!(
            entries[0].overrides.is_empty(),
            "user-scope entry should not record overrides"
        );
        assert_eq!(
            entries[1].overrides,
            vec![PathBuf::from("/home/u/.claude/skills/share/SKILL.md")],
            "workspace-scope entry should record what it shadowed"
        );
    }

    #[tokio::test]
    async fn list_active_customizations_returns_empty_on_fresh_home() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert!(report.skills.is_empty());
        assert!(report.hooks.is_empty());
        assert!(report.mcp_servers.is_empty());
        assert!(report.plugins.is_empty());
        assert!(report.custom_agents.is_empty());
        assert!(report.instructions.is_empty());
        assert!(report.errors.is_empty());
        assert_eq!(report.vendor, CustomizationVendor::Claude);
    }

    #[tokio::test]
    async fn list_active_customizations_picks_up_user_scope_skill() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let skill_dir = temp.path().join(".claude").join("skills").join("hello");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: a hello skill\nuser-invocable: true\n---\nbody",
        )
        .unwrap();
        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert_eq!(report.skills.len(), 1);
        assert_eq!(report.skills[0].name, "hello");
        assert_eq!(report.skills[0].scope, Scope::User);
        assert_eq!(
            report.skills[0].description.as_deref(),
            Some("a hello skill")
        );
    }

    #[tokio::test]
    async fn plugin_skill_uses_exact_versioned_cache_path() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let skill = temp
            .path()
            .join(".claude/plugins/cache/market/plugin/1.2.3/skills/review/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "---\ndescription: review\n---\nbody").unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");

        assert_eq!(report.skills.len(), 1);
        assert_eq!(report.skills[0].source_path.as_ref(), Some(&skill));
    }

    #[tokio::test]
    async fn list_active_customizations_marks_workspace_shadowing_user_skill() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let user_skill = temp.path().join(".claude").join("skills").join("share");
        fs::create_dir_all(&user_skill).unwrap();
        fs::write(
            user_skill.join("SKILL.md"),
            "---\ndescription: from user\n---\nbody",
        )
        .unwrap();
        let ws_skill = cwd.path().join(".claude").join("skills").join("share");
        fs::create_dir_all(&ws_skill).unwrap();
        fs::write(
            ws_skill.join("SKILL.md"),
            "---\ndescription: from workspace\n---\nbody",
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert_eq!(report.skills.len(), 2, "user + workspace must both surface");
        let workspace_entry = report
            .skills
            .iter()
            .find(|e| matches!(e.scope, Scope::Workspace { .. }))
            .expect("workspace skill must be present");
        assert_eq!(
            workspace_entry.overrides.len(),
            1,
            "workspace must record what it shadowed"
        );
        let user_entry = report
            .skills
            .iter()
            .find(|e| e.scope == Scope::User)
            .expect("user skill must be present");
        assert!(
            user_entry.overrides.is_empty(),
            "user-scope must not claim overrides"
        );
    }

    #[tokio::test]
    async fn list_active_customizations_picks_up_project_hook() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let claude_in_cwd = cwd.path().join(".claude");
        fs::create_dir_all(&claude_in_cwd).unwrap();
        fs::write(
            claude_in_cwd.join("settings.json"),
            r#"{
                "hooks": {
                    "PostToolUse": [
                        { "matcher": "Edit|Write|MultiEdit", "hooks": [{ "type": "command", "command": "swiftformat $FILE" }] }
                    ]
                }
            }"#,
        )
        .unwrap();
        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert_eq!(report.hooks.len(), 1, "project hook must surface");
        let hook = &report.hooks[0];
        assert_eq!(hook.name, "PostToolUse[0]");
        std::assert_matches!(hook.scope, Scope::Workspace { .. });
        assert_eq!(
            hook.description.as_deref(),
            Some("Edit|Write|MultiEdit"),
            "matcher pattern should be the description"
        );
    }

    #[tokio::test]
    async fn list_active_customizations_does_not_correlate_unrelated_hook_groups_by_index() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let claude_in_home = temp.path().join(".claude");
        fs::create_dir_all(&claude_in_home).unwrap();
        fs::write(
            claude_in_home.join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"*","hooks":[{"type":"command","command":"echo user"}]}]}}"#,
        )
        .unwrap();
        let claude_in_cwd = cwd.path().join(".claude");
        fs::create_dir_all(&claude_in_cwd).unwrap();
        fs::write(
            claude_in_cwd.join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo project"}]}]}}"#,
        )
        .unwrap();
        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert_eq!(
            report.hooks.len(),
            2,
            "user + workspace hook must both surface"
        );
        let workspace_hook = report
            .hooks
            .iter()
            .find(|h| matches!(h.scope, Scope::Workspace { .. }))
            .expect("workspace hook must be present");
        assert!(
            workspace_hook.overrides.is_empty(),
            "different matcher and command identities must not override by array index"
        );
        let user_hook = report
            .hooks
            .iter()
            .find(|h| h.scope == Scope::User)
            .expect("user hook must be present");
        assert!(user_hook.overrides.is_empty());
    }

    #[tokio::test]
    async fn copilot_catalog_is_vendor_tagged_and_uses_dot_agent_md_files() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let agent = temp.path().join(".copilot/agents/reviewer.agent.md");
        fs::create_dir_all(agent.parent().unwrap()).unwrap();
        fs::write(
            &agent,
            "---\nname: reviewer\ndescription: Reviews code\ntools: [view, rg]\n---\n",
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Copilot)
                .await
                .expect("diagnostics");
        assert_eq!(report.vendor, CustomizationVendor::Copilot);
        assert_eq!(report.custom_agents.len(), 1);
        assert_eq!(report.custom_agents[0].name, "reviewer");
        assert_eq!(report.custom_agents[0].source_path.as_ref(), Some(&agent));
    }

    #[tokio::test]
    async fn copilot_plugin_skill_uses_exact_nonstandard_cache_path() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("outside-default-layout/plugin-v9");
        let skill = plugin.join("skills/review/SKILL.md");
        fs::create_dir_all(skill.parent().unwrap()).unwrap();
        fs::write(&skill, "---\ndescription: review\n---\nbody").unwrap();
        fs::create_dir_all(temp.path().join(".copilot")).unwrap();
        fs::write(
            temp.path().join(".copilot/settings.json"),
            format!(
                r#"{{"installedPlugins":[{{"name":"tools","marketplace":"custom","enabled":true,"cache_path":"{}"}}]}}"#,
                plugin.to_string_lossy()
            ),
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Copilot)
                .await
                .expect("diagnostics");

        assert_eq!(report.skills.len(), 1);
        assert_eq!(report.skills[0].source_path.as_ref(), Some(&skill));
    }

    #[tokio::test]
    async fn copilot_mcp_diagnostics_preserve_each_configuration_source_path() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        fs::write(
            cwd.path().join(".mcp.json"),
            r#"{"mcpServers":{"root":{"command":"root"}}}"#,
        )
        .unwrap();
        fs::create_dir_all(cwd.path().join(".github")).unwrap();
        fs::write(
            cwd.path().join(".github/mcp.json"),
            r#"{"mcpServers":{"github":{"command":"github"}}}"#,
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Copilot)
                .await
                .expect("diagnostics");
        let paths = report
            .mcp_servers
            .iter()
            .map(|entry| (entry.name.as_str(), entry.source_path.as_deref()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let root_path = cwd.path().join(".mcp.json");
        let github_path = cwd.path().join(".github/mcp.json");

        assert_eq!(paths["root"], Some(root_path.as_path()));
        assert_eq!(paths["github"], Some(github_path.as_path()));
    }

    #[tokio::test]
    async fn malformed_settings_and_agent_parse_failures_return_typed_notices() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".claude/agents")).unwrap();
        fs::write(temp.path().join(".claude/settings.json"), "{not json").unwrap();
        fs::write(
            temp.path().join(".claude/agents/broken.md"),
            "---\nname: broken\n---\n",
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert!(report.notices.iter().any(|notice| matches!(
            notice,
            crate::domain::DegradationNotice::MalformedSettings { vendor, .. }
                if vendor == "claude"
        )));
        assert!(report.notices.iter().any(|notice| matches!(
            notice,
            crate::domain::DegradationNotice::AgentParseFailed { vendor, .. }
                if vendor == "claude"
        )));
    }

    #[tokio::test]
    async fn oversized_settings_return_degradation_without_loading_hooks() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".claude")).unwrap();
        let path = temp.path().join(".claude/settings.json");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_DIAGNOSTIC_SETTINGS_BYTES + 1).unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Claude)
                .await
                .expect("diagnostics");
        assert!(report.hooks.is_empty());
        assert!(report.notices.iter().any(|notice| matches!(
            notice,
            crate::domain::DegradationNotice::MalformedSettings { path: notice_path, message, .. }
                if notice_path == &path.to_string_lossy() && message.contains("exceeds")
        )));
    }

    #[tokio::test]
    async fn copilot_catalog_scans_user_workspace_and_inline_hooks_with_activation() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let user_hooks = temp.path().join(".copilot/hooks");
        let workspace_hooks = cwd.path().join(".github/hooks");
        fs::create_dir_all(&user_hooks).unwrap();
        fs::create_dir_all(&workspace_hooks).unwrap();
        fs::write(
            user_hooks.join("user.json"),
            r#"{"version":1,"hooks":{"sessionStart":[{"type":"command","bash":"echo user"}]}}"#,
        )
        .unwrap();
        fs::write(
            workspace_hooks.join("workspace.json"),
            r#"{"version":1,"disableAllHooks":true,"hooks":{"agentStop":[{"type":"command","bash":"echo workspace"}]}}"#,
        )
        .unwrap();
        fs::create_dir_all(temp.path().join(".copilot")).unwrap();
        fs::write(
            temp.path().join(".copilot/settings.json"),
            r#"{"hooks":{"preToolUse":[{"type":"command","bash":"echo inline"}]}}"#,
        )
        .unwrap();
        fs::create_dir_all(cwd.path().join(".github/copilot")).unwrap();
        fs::write(
            cwd.path().join(".github/copilot/settings.json"),
            r#"{"hooks":{"postToolUse":[{"type":"command","bash":"echo project","enabled":false}]}}"#,
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Copilot)
                .await
                .expect("diagnostics");
        assert_eq!(report.hooks.len(), 4);
        assert!(report.hooks.iter().any(|hook| {
            hook.name == "sessionStart[0]" && hook.enabled && hook.scope == Scope::User
        }));
        assert!(report.hooks.iter().any(|hook| {
            hook.name == "agentStop[0]"
                && !hook.enabled
                && matches!(hook.scope, Scope::Workspace { .. })
        }));
        assert!(report.hooks.iter().any(|hook| {
            hook.name == "preToolUse[0]" && hook.enabled && hook.scope == Scope::User
        }));
        assert!(report.hooks.iter().any(|hook| {
            hook.name == "postToolUse[0]"
                && !hook.enabled
                && matches!(hook.scope, Scope::Workspace { .. })
        }));
    }

    #[tokio::test]
    async fn copilot_disable_all_hooks_marks_every_source_inactive() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join(".copilot/hooks")).unwrap();
        fs::write(
            temp.path().join(".copilot/hooks/user.json"),
            r#"{"version":1,"hooks":{"sessionStart":[{"type":"command","bash":"echo user"}]}}"#,
        )
        .unwrap();
        fs::write(
            temp.path().join(".copilot/settings.json"),
            r#"{"disableAllHooks":true,"hooks":{"preToolUse":[{"type":"command","bash":"echo inline"}]}}"#,
        )
        .unwrap();

        let report =
            list_active_customizations(temp.path(), cwd.path(), CustomizationVendor::Copilot)
                .await
                .expect("diagnostics");
        assert!(!report.hooks.is_empty());
        assert!(report.hooks.iter().all(|hook| !hook.enabled));
    }
}
