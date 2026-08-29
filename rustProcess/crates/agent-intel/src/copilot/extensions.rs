use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PluginSummary {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSummary {
    pub name: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<crate::mcp::McpHealth>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentDefSummary {
    pub name: String,
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub name: String,
    pub scope: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(flatten, default, skip_serializing_if = "BTreeMap::is_empty")]
    pub additional_fields: BTreeMap<String, Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_source_path_is_additive_and_camel_case() {
        let summary = SkillSummary {
            name: "review".to_owned(),
            scope: "user".to_owned(),
            source_path: Some("/home/user/.copilot/skills/review/SKILL.md".to_owned()),
            ..SkillSummary::default()
        };
        let value = serde_json::to_value(&summary).expect("serialize skill");
        assert_eq!(
            value["sourcePath"],
            "/home/user/.copilot/skills/review/SKILL.md"
        );

        let legacy: SkillSummary = serde_json::from_value(serde_json::json!({
            "name": "review",
            "scope": "user"
        }))
        .expect("decode legacy skill");
        assert_eq!(legacy.source_path, None);
    }
}
