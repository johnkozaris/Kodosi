use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::DegradationNotice;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CustomizationVendor {
    Claude,
    Copilot,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Scope {
    User,
    Workspace {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    Plugin {
        marketplace: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        plugin: Option<String>,
    },
    BuiltIn,
    Managed,
    Extension {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
}

impl Scope {
    #[must_use]
    pub fn from_source_str(source: &str, cwd: Option<&str>) -> Self {
        match source {
            "user" => Self::User,
            "project" | "workspace" => Self::Workspace {
                cwd: cwd.map(str::to_owned),
            },
            "builtin" | "built-in" | "built_in" => Self::BuiltIn,
            "managed" => Self::Managed,
            s if s.starts_with("plugin:") => {
                let marketplace = s.trim_start_matches("plugin:").to_owned();
                Self::Plugin {
                    marketplace,
                    plugin: None,
                }
            }
            s if s.starts_with("extension:") => Self::Extension {
                id: Some(s.trim_start_matches("extension:").to_owned()),
            },
            _ => Self::Extension { id: None },
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CustomizationStatus {
    #[default]
    Loaded,
    Degraded,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CustomizationKind {
    Skill,
    Hook,
    McpServer,
    Plugin,
    CustomAgent,
    Instructions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomizationEntry {
    pub kind: CustomizationKind,
    pub name: String,
    pub scope: Scope,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<PathBuf>,
    #[serde(default)]
    pub status: CustomizationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomizationError {
    pub source_path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActiveCustomizationsReport {
    pub vendor: CustomizationVendor,
    pub skills: Vec<CustomizationEntry>,
    pub hooks: Vec<CustomizationEntry>,
    pub mcp_servers: Vec<CustomizationEntry>,
    pub plugins: Vec<CustomizationEntry>,
    pub custom_agents: Vec<CustomizationEntry>,
    pub instructions: Vec<CustomizationEntry>,
    pub errors: Vec<CustomizationError>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notices: Vec<DegradationNotice>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_strings_round_trip_to_typed_scope() {
        assert_eq!(Scope::from_source_str("user", None), Scope::User);
        assert_eq!(
            Scope::from_source_str("project", Some("/repo")),
            Scope::Workspace {
                cwd: Some("/repo".to_owned()),
            }
        );
        assert_eq!(
            Scope::from_source_str("plugin:anthropic", None),
            Scope::Plugin {
                marketplace: "anthropic".to_owned(),
                plugin: None,
            }
        );
        assert_eq!(Scope::from_source_str("builtin", None), Scope::BuiltIn);
        assert_eq!(Scope::from_source_str("managed", None), Scope::Managed);
        assert_eq!(
            Scope::from_source_str("extension:github.copilot", None),
            Scope::Extension {
                id: Some("github.copilot".to_owned())
            }
        );
        assert_eq!(
            Scope::from_source_str("???", None),
            Scope::Extension { id: None }
        );
    }
}
