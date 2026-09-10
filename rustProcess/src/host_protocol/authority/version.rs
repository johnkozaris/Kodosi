use serde_json::Value;

pub const PROTOCOL_VERSION: u32 = 38;

const FROZEN_SNAPSHOT: &str = include_str!("../../../../protocol/versions/v38.json");

pub fn generated_frozen_snapshot() -> Result<Value, serde_json::Error> {
    serde_json::from_str(FROZEN_SNAPSHOT)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityReport {
    pub removed_fields: Vec<String>,
    pub removed_entries: Vec<RemovedEntry>,
    pub added_required_fields: Vec<AddedRequiredField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedEntry {
    pub field: String,
    pub entry: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddedRequiredField {
    pub object: String,
    pub field: String,
}

impl CompatibilityReport {
    #[must_use]
    pub fn is_compatible(&self) -> bool {
        self.removed_fields.is_empty()
            && self.removed_entries.is_empty()
            && self.added_required_fields.is_empty()
    }
}

#[must_use]
pub fn compatibility_report(frozen: &Value, current: &Value) -> CompatibilityReport {
    let mut report = CompatibilityReport {
        removed_fields: Vec::new(),
        removed_entries: Vec::new(),
        added_required_fields: Vec::new(),
    };

    let (Some(frozen_obj), Some(current_obj)) = (frozen.as_object(), current.as_object()) else {
        return report;
    };

    for (field, frozen_val) in frozen_obj {
        let Some(current_val) = current_obj.get(field) else {
            report.removed_fields.push(field.clone());
            continue;
        };
        if field == "protocolVersion" {
            if current_val.as_u64().unwrap_or(0) < frozen_val.as_u64().unwrap_or(u64::MAX) {
                report.removed_entries.push(RemovedEntry {
                    field: field.clone(),
                    entry: stringify(frozen_val),
                });
            }
        } else {
            compare_nested(field, frozen_val, current_val, &mut report);
        }
    }

    report
}

fn compare_nested(path: &str, frozen: &Value, current: &Value, report: &mut CompatibilityReport) {
    match (frozen, current) {
        (Value::Object(frozen), Value::Object(current)) => {
            if let (Some(frozen_required), Some(current_required)) = (
                frozen.get("requiredFields").and_then(Value::as_array),
                current.get("requiredFields").and_then(Value::as_array),
            ) {
                for field in current_required {
                    if !frozen_required.contains(field) {
                        report.added_required_fields.push(AddedRequiredField {
                            object: path.to_owned(),
                            field: stringify(field),
                        });
                    }
                }
            }

            for (key, frozen_value) in frozen {
                let child_path = format!("{path}.{key}");
                let Some(current_value) = current.get(key) else {
                    report.removed_fields.push(child_path);
                    continue;
                };
                compare_nested(&child_path, frozen_value, current_value, report);
            }
        }
        (Value::Array(frozen), Value::Array(current)) => {
            for entry in frozen {
                if let Some(current_entry) = current
                    .iter()
                    .find(|current_entry| value_is_compatible(entry, current_entry))
                {
                    compare_nested(path, entry, current_entry, report);
                } else {
                    report.removed_entries.push(RemovedEntry {
                        field: path.to_owned(),
                        entry: stringify(entry),
                    });
                }
            }
        }
        _ if frozen != current => report.removed_entries.push(RemovedEntry {
            field: path.to_owned(),
            entry: stringify(frozen),
        }),
        _ => {}
    }
}

fn value_is_compatible(frozen: &Value, current: &Value) -> bool {
    match (frozen, current) {
        (Value::Object(frozen), Value::Object(current)) => {
            frozen.iter().all(|(key, frozen_value)| {
                current
                    .get(key)
                    .is_some_and(|current_value| value_is_compatible(frozen_value, current_value))
            })
        }
        (Value::Array(frozen), Value::Array(current)) => frozen.iter().all(|frozen_entry| {
            current
                .iter()
                .any(|current_entry| value_is_compatible(frozen_entry, current_entry))
        }),
        _ => frozen == current,
    }
}

fn stringify(value: &Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn report_is_empty_when_current_is_identical() {
        let v = json!({"protocolVersion": 1, "commands": ["a", "b"]});
        let report = compatibility_report(&v, &v);
        assert!(report.is_compatible());
    }

    #[test]
    fn report_is_empty_when_current_adds_entries() {
        let frozen = json!({"protocolVersion": 1, "commands": ["a"]});
        let current = json!({"protocolVersion": 1, "commands": ["a", "b", "c"]});
        let report = compatibility_report(&frozen, &current);
        assert!(report.is_compatible());
    }

    #[test]
    fn report_is_empty_when_current_adds_fields() {
        let frozen = json!({"protocolVersion": 1, "commands": ["a"]});
        let current = json!({"protocolVersion": 1, "commands": ["a"], "events": ["x"]});
        let report = compatibility_report(&frozen, &current);
        assert!(report.is_compatible());
    }

    #[test]
    fn report_is_empty_when_nested_array_object_adds_fields() {
        let frozen = json!({
            "protocolVersion": 3,
            "variants": [{"name": "ready", "fields": [{"name": "id"}]}]
        });
        let current = json!({
            "protocolVersion": 3,
            "variants": [{
                "name": "ready",
                "fields": [{"name": "id", "type": "string"}],
                "documentation": "additive"
            }]
        });

        assert!(compatibility_report(&frozen, &current).is_compatible());
    }

    #[test]
    fn report_is_empty_when_existing_command_adds_optional_field() {
        let frozen = json!({
            "messageShapes": {
                "session.create": {
                    "object": {"type": "string"},
                    "optionalFields": [],
                    "requiredFields": ["type"]
                }
            }
        });
        let current = json!({
            "messageShapes": {
                "session.create": {
                    "object": {"requestId": "string", "type": "string"},
                    "optionalFields": ["requestId"],
                    "requiredFields": ["type"]
                }
            }
        });

        assert!(compatibility_report(&frozen, &current).is_compatible());
    }

    #[test]
    fn report_is_empty_when_new_command_has_required_fields() {
        let frozen = json!({"messageShapes": {}});
        let current = json!({
            "messageShapes": {
                "session.create": {
                    "object": {"type": "string"},
                    "optionalFields": [],
                    "requiredFields": ["type"]
                }
            }
        });

        assert!(compatibility_report(&frozen, &current).is_compatible());
    }

    #[test]
    fn report_flags_removed_field() {
        let frozen = json!({"protocolVersion": 1, "events": ["x"]});
        let current = json!({"protocolVersion": 1});
        let report = compatibility_report(&frozen, &current);
        assert!(!report.is_compatible());
        assert_eq!(report.removed_fields, vec!["events".to_owned()]);
    }

    #[test]
    fn report_flags_removed_entry() {
        let frozen = json!({"protocolVersion": 1, "commands": ["a", "b"]});
        let current = json!({"protocolVersion": 1, "commands": ["a"]});
        let report = compatibility_report(&frozen, &current);
        assert!(!report.is_compatible());
        assert_eq!(report.removed_entries.len(), 1);
        assert_eq!(report.removed_entries[0].field, "commands");
        assert_eq!(report.removed_entries[0].entry, "b");
    }

    #[test]
    fn report_flags_removed_nested_message_field() {
        let frozen = json!({
            "protocolVersion": 3,
            "messageShapes": {
                "session.create": {
                    "object": {
                        "type": "string",
                        "requestId": "string"
                    }
                }
            }
        });
        let current = json!({
            "protocolVersion": 3,
            "messageShapes": {
                "session.create": {
                    "object": {
                        "type": "string"
                    }
                }
            }
        });
        let report = compatibility_report(&frozen, &current);
        assert!(
            report
                .removed_fields
                .contains(&"messageShapes.session.create.object.requestId".to_owned())
        );
    }

    #[test]
    fn report_flags_optional_field_tightened_to_required() {
        let frozen = json!({
            "messageShapes": {
                "room.chat.post": {
                    "requiredFields": ["body", "room_id", "type"],
                    "optionalFields": ["recipient_session_ids"]
                }
            }
        });
        let current = json!({
            "messageShapes": {
                "room.chat.post": {
                    "requiredFields": [
                        "body",
                        "recipient_session_ids",
                        "room_id",
                        "type"
                    ],
                    "optionalFields": []
                }
            }
        });

        let report = compatibility_report(&frozen, &current);
        assert!(!report.is_compatible());
        assert!(report.removed_entries.iter().any(|entry| {
            entry.field == "messageShapes.room.chat.post.optionalFields"
                && entry.entry == "recipient_session_ids"
        }));
        assert_eq!(
            report.added_required_fields,
            vec![AddedRequiredField {
                object: "messageShapes.room.chat.post".to_owned(),
                field: "recipient_session_ids".to_owned(),
            }]
        );
    }

    #[test]
    fn report_flags_new_required_field_on_existing_command() {
        let frozen = json!({
            "messageShapes": {
                "session.create": {
                    "object": {"type": "string"},
                    "optionalFields": [],
                    "requiredFields": ["type"]
                }
            }
        });
        let current = json!({
            "messageShapes": {
                "session.create": {
                    "object": {"requestId": "string", "type": "string"},
                    "optionalFields": [],
                    "requiredFields": ["requestId", "type"]
                }
            }
        });

        let report = compatibility_report(&frozen, &current);
        assert_eq!(
            report.added_required_fields,
            vec![AddedRequiredField {
                object: "messageShapes.session.create".to_owned(),
                field: "requestId".to_owned(),
            }]
        );
        assert!(!report.is_compatible());
    }

    #[test]
    fn report_flags_new_required_field_in_existing_array_element() {
        let frozen = json!({
            "messageShapes": {
                "session.list": {
                    "object": {
                        "sessions": {
                            "array": [{
                                "object": {"id": "string"},
                                "optionalFields": [],
                                "requiredFields": ["id"]
                            }]
                        }
                    }
                }
            }
        });
        let current = json!({
            "messageShapes": {
                "session.list": {
                    "object": {
                        "sessions": {
                            "array": [{
                                "object": {"id": "string", "status": "string"},
                                "optionalFields": [],
                                "requiredFields": ["id", "status"]
                            }]
                        }
                    }
                }
            }
        });

        let report = compatibility_report(&frozen, &current);
        assert_eq!(
            report.added_required_fields,
            vec![AddedRequiredField {
                object: "messageShapes.session.list.object.sessions.array".to_owned(),
                field: "status".to_owned(),
            }]
        );
        assert!(!report.is_compatible());
    }

    #[test]
    fn frozen_snapshot_parses() {
        generated_frozen_snapshot().expect("frozen snapshot must parse");
    }

    #[test]
    fn live_build_is_forward_compatible_with_frozen_snapshot() {
        let frozen = generated_frozen_snapshot().expect("frozen snapshot parses");
        let live_json = crate::host_protocol::authority::render_desktop_runtime_authority_json()
            .expect("live render");
        let live: Value = serde_json::from_str(&live_json).expect("live parses");

        let report = compatibility_report(&frozen, &live);
        assert!(
            report.is_compatible(),
            "live protocol regressed against the frozen snapshot: removed_fields={:?}, removed_entries={:?}, added_required_fields={:?}",
            report.removed_fields,
            report.removed_entries,
            report.added_required_fields
        );
    }

    #[test]
    fn regression_detection_round_trip() {
        let frozen = generated_frozen_snapshot().expect("frozen snapshot parses");
        let mut shrunk = frozen.clone();
        if let Some(arr) = shrunk
            .as_object_mut()
            .and_then(|m| m.get_mut("terminalCommandTypes"))
            .and_then(Value::as_array_mut)
        {
            arr.pop();
        }
        let report = compatibility_report(&frozen, &shrunk);
        assert!(
            !report.is_compatible(),
            "report must flag entry removal as incompatible"
        );
        assert!(
            report
                .removed_entries
                .iter()
                .any(|e| e.field == "terminalCommandTypes"),
            "report must name the affected field"
        );
    }
}
