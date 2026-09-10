use serde::{Deserialize, Serialize};
use specta::Type;

use super::dto::AgentSettingsBundle;

pub const MAX_SETTINGS_TREE_DEPTH: usize = 12;
pub const MAX_SETTINGS_TREE_NODES: usize = 2_048;
pub const MAX_SETTINGS_STRING_BYTES: usize = 64 * 1024;
pub const MAX_SETTINGS_TREE_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum SettingsNodeKind {
    Object,
    Array,
    String,
    Integer,
    Number,
    Boolean,
    Null,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsTreeNode {
    pub node_id: u32,
    pub parent_id: Option<u32>,
    pub depth: u16,
    pub key: String,
    pub kind: SettingsNodeKind,
    pub child_count: u32,
    pub string_value: Option<String>,
    pub integer_value: Option<i64>,
    pub number_value: Option<f64>,
    pub boolean_value: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsTree {
    pub nodes: Vec<SettingsTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsTreeBundle {
    pub managed: Option<SettingsTree>,
    pub user: Option<SettingsTree>,
    pub project: Option<SettingsTree>,
    pub local: Option<SettingsTree>,
}

impl AgentSettingsTreeBundle {
    pub fn project(bundle: &AgentSettingsBundle) -> Result<Self, String> {
        Ok(Self {
            managed: bundle.managed.as_ref().map(project_tree).transpose()?,
            user: bundle.user.as_ref().map(project_tree).transpose()?,
            project: bundle.project.as_ref().map(project_tree).transpose()?,
            local: bundle.local.as_ref().map(project_tree).transpose()?,
        })
    }
}

fn project_tree(value: &serde_json::Value) -> Result<SettingsTree, String> {
    let mut nodes = Vec::new();
    append_node(value, None, "Value", 0, &mut nodes)?;
    let tree = SettingsTree { nodes };
    let bytes = serde_json::to_vec(&tree)
        .map_err(|error| format!("encode settings tree: {error}"))?
        .len();
    if bytes > MAX_SETTINGS_TREE_BYTES {
        return Err(format!(
            "settings tree exceeds {MAX_SETTINGS_TREE_BYTES} byte limit"
        ));
    }
    Ok(tree)
}

fn append_node(
    value: &serde_json::Value,
    parent_id: Option<u32>,
    key: &str,
    depth: usize,
    nodes: &mut Vec<SettingsTreeNode>,
) -> Result<(), String> {
    if depth > MAX_SETTINGS_TREE_DEPTH {
        return Err(format!(
            "settings tree exceeds depth limit {MAX_SETTINGS_TREE_DEPTH}"
        ));
    }
    if nodes.len() >= MAX_SETTINGS_TREE_NODES {
        return Err(format!(
            "settings tree exceeds node limit {MAX_SETTINGS_TREE_NODES}"
        ));
    }
    require_bounded_string("settings key", key, 512)?;
    let node_id = u32::try_from(nodes.len()).map_err(|_| "settings tree is too large")?;
    let depth = u16::try_from(depth).map_err(|_| "settings tree depth overflow")?;
    let (kind, child_count, string_value, integer_value, number_value, boolean_value) = match value
    {
        serde_json::Value::Object(values) => (
            SettingsNodeKind::Object,
            u32::try_from(values.len()).unwrap_or(u32::MAX),
            None,
            None,
            None,
            None,
        ),
        serde_json::Value::Array(values) => (
            SettingsNodeKind::Array,
            u32::try_from(values.len()).unwrap_or(u32::MAX),
            None,
            None,
            None,
            None,
        ),
        serde_json::Value::String(value) => {
            require_bounded_string("settings string", value, MAX_SETTINGS_STRING_BYTES)?;
            (
                SettingsNodeKind::String,
                0,
                Some(value.clone()),
                None,
                None,
                None,
            )
        }
        serde_json::Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                (
                    SettingsNodeKind::Integer,
                    0,
                    None,
                    Some(integer),
                    None,
                    None,
                )
            } else {
                let number = value
                    .as_f64()
                    .filter(|number| number.is_finite())
                    .ok_or_else(|| "settings number is outside supported bounds".to_owned())?;
                (SettingsNodeKind::Number, 0, None, None, Some(number), None)
            }
        }
        serde_json::Value::Bool(value) => {
            (SettingsNodeKind::Boolean, 0, None, None, None, Some(*value))
        }
        serde_json::Value::Null => (SettingsNodeKind::Null, 0, None, None, None, None),
    };
    nodes.push(SettingsTreeNode {
        node_id,
        parent_id,
        depth,
        key: key.to_owned(),
        kind,
        child_count,
        string_value,
        integer_value,
        number_value,
        boolean_value,
    });
    match value {
        serde_json::Value::Object(values) => {
            let mut entries = values.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            for (key, child) in entries {
                append_node(child, Some(node_id), key, usize::from(depth) + 1, nodes)?;
            }
        }
        serde_json::Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                append_node(
                    child,
                    Some(node_id),
                    &(index + 1).to_string(),
                    usize::from(depth) + 1,
                    nodes,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn require_bounded_string(label: &str, value: &str, maximum: usize) -> Result<(), String> {
    if value.len() > maximum {
        return Err(format!("{label} exceeds {maximum} byte limit"));
    }
    if value.chars().any(|character| character == '\0') {
        return Err(format!("{label} contains NUL"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_flat_sorted_and_typed() {
        let tree = project_tree(&serde_json::json!({
            "z": true,
            "a": [1, "two", null]
        }))
        .unwrap();
        assert_eq!(tree.nodes[0].kind, SettingsNodeKind::Object);
        assert_eq!(tree.nodes[1].key, "a");
        assert_eq!(tree.nodes[2].integer_value, Some(1));
        assert_eq!(tree.nodes[3].string_value.as_deref(), Some("two"));
        assert_eq!(tree.nodes.last().unwrap().key, "z");
    }

    #[test]
    fn oversized_string_is_rejected() {
        let error = project_tree(&serde_json::Value::String(
            "x".repeat(MAX_SETTINGS_STRING_BYTES + 1),
        ))
        .unwrap_err();
        assert!(error.contains("settings string"));
    }

    #[test]
    fn excessive_depth_is_rejected() {
        let mut value = serde_json::Value::Null;
        for _ in 0..=MAX_SETTINGS_TREE_DEPTH {
            value = serde_json::json!([value]);
        }
        assert!(project_tree(&value).unwrap_err().contains("depth"));
    }
}
