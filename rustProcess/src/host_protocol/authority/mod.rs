mod catalog;
mod permissions;
mod shapes;
mod version;

pub use version::{
    AddedRequiredField, CompatibilityReport, PROTOCOL_VERSION, RemovedEntry, compatibility_report,
    generated_frozen_snapshot,
};

use serde::Serialize;
use std::collections::BTreeMap;

use crate::Result;

const PROTOCOL_VERSION_FIELD: u32 = PROTOCOL_VERSION;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopRuntimeAuthority {
    protocol_version: u32,
    terminal_checkpoint_schema_version: u32,
    terminal_checkpoint_required_fields: Vec<&'static str>,
    terminal_checkpoint_byte_fields: Vec<&'static str>,
    terminal_checkpoint_max_raw_bytes: usize,
    terminal_presentation_schema_version: u32,
    terminal_presentation_required_fields: Vec<&'static str>,
    terminal_presentation_byte_fields: Vec<&'static str>,
    terminal_active_screens: Vec<&'static str>,
    terminal_presentation_max_serialized_bytes: usize,
    terminal_control_max_bytes: usize,
    terminal_close_required_fields: Vec<&'static str>,
    terminal_close_sequence_semantics: &'static str,
    terminal_command_types: Vec<String>,
    terminal_event_types: Vec<String>,
    system_command_types: Vec<String>,
    system_event_types: Vec<String>,
    session_command_types: Vec<String>,
    session_event_types: Vec<String>,
    account_context_event_lanes: Vec<&'static str>,
    account_context_event_authorities: Vec<&'static str>,
    account_context_event_required_fields: Vec<&'static str>,
    account_context_event_optional_fields: Vec<&'static str>,
    account_context_event_nullable_fields: Vec<&'static str>,
    agent_intel_command_types: Vec<String>,
    agent_intel_event_types: Vec<String>,
    agent_global_event_types: Vec<String>,
    auth_command_types: Vec<String>,
    auth_event_types: Vec<String>,
    auth_ready_keys: Vec<&'static str>,
    auth_requirement_reasons: Vec<String>,
    friends_command_types: Vec<String>,
    friends_event_types: Vec<String>,
    device_command_types: Vec<String>,
    device_event_types: Vec<String>,
    device_link_outcomes: Vec<String>,
    self_device_link_outcomes: Vec<String>,
    trust_command_types: Vec<String>,
    trust_event_types: Vec<String>,
    room_command_types: Vec<String>,
    room_event_types: Vec<String>,
    room_event_envelope_required_fields: Vec<&'static str>,
    room_action_statuses: Vec<String>,
    room_chat_entry_keys: Vec<&'static str>,
    runtime_session_statuses: Vec<String>,
    session_scopes: Vec<String>,
    session_access_levels: Vec<String>,
    relay_action_statuses: Vec<String>,
    semantic_send_modes: Vec<String>,
    steer_transitions: Vec<String>,
    steer_delivery_states: Vec<String>,
    desktop_permission_bit_positions: BTreeMap<String, u32>,
    desktop_permission_role_masks: BTreeMap<String, u32>,
    relay_permission_bit_positions: BTreeMap<String, u32>,
    relay_permission_role_masks: BTreeMap<String, u32>,
    runtime_session_modes: Vec<String>,
    remote_session_connection_states: Vec<String>,
    remote_session_access_states: Vec<String>,
    remote_session_access_issues: Vec<String>,
    session_list_entry_meta_fields: Vec<String>,
    local_session_entry_fields: Vec<String>,
    remote_session_entry_fields: Vec<String>,
    hidden_session_entry_fields: Vec<String>,
    room_list_entry_fields: Vec<String>,
    room_entry_fields: Vec<String>,
    trust_pin_entry_fields: Vec<String>,
    friend_entry_fields: Vec<String>,
    friend_request_entry_fields: Vec<String>,
    my_device_entry_fields: Vec<String>,

    message_shapes: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
pub(in crate::host_protocol) use shapes::sample_runtime_local_session_info;

pub fn render_desktop_runtime_authority_json() -> Result<String> {
    let authority = build_authority()?;
    let mut json = serde_json::to_string_pretty(&authority)?;
    json.push('\n');
    Ok(json)
}

#[expect(
    clippy::too_many_lines,
    reason = "one literal authority assembly keeps every generated contract field visibly complete"
)]
fn build_authority() -> Result<DesktopRuntimeAuthority> {
    shapes::validate_authority_shapes()?;
    catalog::reset_message_shapes();
    Ok(DesktopRuntimeAuthority {
        protocol_version: PROTOCOL_VERSION_FIELD,
        terminal_checkpoint_schema_version:
            kodosi_domain::terminal::TERMINAL_CHECKPOINT_SCHEMA_VERSION,
        terminal_checkpoint_required_fields: vec![
            "schemaVersion",
            "rows",
            "cols",
            "activeScreen",
            "semanticCheckpoint",
            "cursorX",
            "cursorY",
            "cursorHidden",
        ],
        terminal_checkpoint_byte_fields: vec!["semanticCheckpoint"],
        terminal_checkpoint_max_raw_bytes:
            kodosi_domain::terminal::TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
        terminal_presentation_schema_version:
            kodosi_domain::terminal::TERMINAL_PRESENTATION_SCHEMA_VERSION,
        terminal_presentation_required_fields: vec![
            "schemaVersion",
            "rows",
            "cols",
            "activeScreen",
            "plainLines",
            "cursorX",
            "cursorY",
            "cursorHidden",
        ],
        terminal_presentation_byte_fields: Vec::new(),
        terminal_active_screens: vec!["primary", "alternate"],
        terminal_presentation_max_serialized_bytes:
            kodosi_domain::terminal::TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES,
        terminal_control_max_bytes:
            kodosi_domain::terminal::TERMINAL_PRESENTATION_CONTROL_MAX_BYTES,
        terminal_close_required_fields: vec!["type", "reason", "finalSequence"],
        terminal_close_sequence_semantics: "exclusive boundary; every lower data sequence is delivered before close",
        terminal_command_types: catalog::terminal_command_types()?,
        terminal_event_types: catalog::terminal_event_types()?,
        system_command_types: catalog::system_command_types()?,
        system_event_types: catalog::system_event_types()?,
        session_command_types: catalog::session_command_types()?,
        session_event_types: catalog::session_event_types()?,
        account_context_event_lanes: vec![
            "session",
            "agentIntel",
            "friends",
            "devices",
            "trust",
            "room",
        ],
        account_context_event_authorities: vec!["accountContext"],
        account_context_event_required_fields: vec!["authority", "accountEpoch", "type"],
        account_context_event_optional_fields: vec!["accountUserId"],
        account_context_event_nullable_fields: vec!["accountUserId"],
        agent_intel_command_types: catalog::agent_intel_command_types()?,
        agent_intel_event_types: catalog::agent_intel_event_types()?,
        agent_global_event_types: catalog::agent_global_event_types()?,
        auth_command_types: catalog::auth_command_types()?,
        auth_event_types: catalog::auth_event_types()?,
        auth_ready_keys: catalog::auth_ready_keys(),
        auth_requirement_reasons: catalog::auth_required_reasons()?,
        friends_command_types: catalog::friends_command_types()?,
        friends_event_types: catalog::friends_event_types()?,
        device_command_types: catalog::device_command_types()?,
        device_event_types: catalog::device_event_types()?,
        device_link_outcomes: catalog::device_link_outcomes()?,
        self_device_link_outcomes: catalog::self_device_link_outcomes()?,
        trust_command_types: catalog::trust_command_types()?,
        trust_event_types: catalog::trust_event_types()?,
        room_command_types: catalog::room_command_types()?,
        room_event_types: catalog::room_event_types()?,
        room_event_envelope_required_fields: vec!["accountUserId", "accountEpoch", "type"],
        room_action_statuses: catalog::room_action_statuses()?,
        room_chat_entry_keys: catalog::room_chat_entry_keys(),
        runtime_session_statuses: catalog::runtime_session_statuses()?,
        session_scopes: catalog::session_scopes()?,
        session_access_levels: catalog::session_access_levels()?,
        relay_action_statuses: catalog::relay_action_statuses()?,
        semantic_send_modes: catalog::semantic_send_modes()?,
        steer_transitions: catalog::steer_transitions()?,
        steer_delivery_states: catalog::steer_delivery_states()?,
        desktop_permission_bit_positions: permissions::desktop_bit_positions(),
        desktop_permission_role_masks: permissions::desktop_role_masks(),
        relay_permission_bit_positions: permissions::relay_bit_positions(),
        relay_permission_role_masks: permissions::relay_role_masks(),
        runtime_session_modes: catalog::runtime_session_modes()?,
        remote_session_connection_states: catalog::remote_session_connection_states()?,
        remote_session_access_states: catalog::remote_session_access_states()?,
        remote_session_access_issues: catalog::remote_session_access_issues()?,
        session_list_entry_meta_fields: shapes::to_strings(shapes::SESSION_LIST_ENTRY_META_FIELDS),
        local_session_entry_fields: shapes::to_strings(shapes::LOCAL_SESSION_ENTRY_FIELDS),
        remote_session_entry_fields: shapes::to_strings(shapes::REMOTE_SESSION_ENTRY_FIELDS),
        hidden_session_entry_fields: shapes::to_strings(shapes::HIDDEN_SESSION_ENTRY_FIELDS),
        room_list_entry_fields: shapes::to_strings(shapes::ROOM_LIST_ENTRY_FIELDS),
        room_entry_fields: shapes::to_strings(shapes::ROOM_ENTRY_FIELDS),
        trust_pin_entry_fields: shapes::to_strings(shapes::TRUST_PIN_ENTRY_FIELDS),
        friend_entry_fields: shapes::to_strings(shapes::FRIEND_ENTRY_FIELDS),
        friend_request_entry_fields: shapes::to_strings(shapes::FRIEND_REQUEST_ENTRY_FIELDS),
        my_device_entry_fields: shapes::to_strings(shapes::MY_DEVICE_ENTRY_FIELDS),
        message_shapes: catalog::take_message_shapes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn account_context_envelope_contract_is_explicit() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        assert_eq!(
            authority["accountContextEventLanes"],
            serde_json::json!([
                "session",
                "agentIntel",
                "friends",
                "devices",
                "trust",
                "room"
            ])
        );
        assert_eq!(
            authority["accountContextEventAuthorities"],
            serde_json::json!(["accountContext"])
        );
        assert_eq!(
            authority["accountContextEventRequiredFields"],
            serde_json::json!(["authority", "accountEpoch", "type"])
        );
        assert_eq!(
            authority["accountContextEventOptionalFields"],
            serde_json::json!(["accountUserId"])
        );
        assert_eq!(
            authority["accountContextEventNullableFields"],
            serde_json::json!(["accountUserId"])
        );
    }

    #[test]
    fn every_typed_array_sample_captures_an_element_shape() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        let shapes = authority["messageShapes"]
            .as_object()
            .expect("messageShapes object");
        let incomplete = shapes
            .iter()
            .filter_map(|(message_type, shape)| {
                contains_empty_array_shape(shape).then_some(message_type.as_str())
            })
            .collect::<Vec<_>>();
        assert!(
            incomplete.is_empty(),
            "protocol samples with empty typed arrays cannot freeze nested element fields: {incomplete:?}"
        );
    }

    #[test]
    fn every_object_shape_declares_field_presence() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        assert_field_presence_metadata(&authority["messageShapes"], "messageShapes");
    }

    fn assert_field_presence_metadata(value: &serde_json::Value, path: &str) {
        let Some(shape) = value.as_object() else {
            if let Some(items) = value.as_array() {
                for (index, item) in items.iter().enumerate() {
                    assert_field_presence_metadata(item, &format!("{path}[{index}]"));
                }
            }
            return;
        };

        if let Some(fields) = shape.get("object").and_then(serde_json::Value::as_object) {
            let required = shape["requiredFields"]
                .as_array()
                .unwrap_or_else(|| panic!("{path} missing requiredFields"));
            let optional = shape["optionalFields"]
                .as_array()
                .unwrap_or_else(|| panic!("{path} missing optionalFields"));
            let nullable = shape["nullableFields"]
                .as_array()
                .unwrap_or_else(|| panic!("{path} missing nullableFields"));

            for key in fields.keys() {
                let key = serde_json::Value::String(key.clone());
                assert_ne!(
                    required.contains(&key),
                    optional.contains(&key),
                    "{path} must classify each object field exactly once"
                );
            }
            assert_eq!(
                fields.len(),
                required.len() + optional.len(),
                "{path} presence metadata contains unknown or duplicate fields"
            );
            assert!(
                nullable.iter().all(|key| fields.contains_key(
                    key.as_str()
                        .unwrap_or_else(|| panic!("{path} nullableFields must contain strings"))
                )),
                "{path} nullableFields contains an unknown field"
            );

            assert!(
                nullable.iter().all(|key| optional.contains(key)),
                "{path} declares a nullable field as required"
            );
        }

        for (key, child) in shape {
            assert_field_presence_metadata(child, &format!("{path}.{key}"));
        }
    }

    #[test]
    fn steering_vocabularies_are_explicit() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        assert_eq!(
            authority["steerTransitions"],
            serde_json::json!([
                "queued",
                "sending",
                "injected",
                "failed",
                "cancelled",
                "deliveryUnknown"
            ])
        );
    }

    #[test]
    fn runtime_health_cleanup_shape_is_additive_and_identifier_free() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        assert!(
            authority["systemEventTypes"]
                .as_array()
                .expect("system event types")
                .contains(&serde_json::json!("runtime.health"))
        );
        let shape = &authority["messageShapes"]["runtime.health"];
        assert_eq!(
            required_fields(shape),
            BTreeSet::from(["collaborationCleanup".to_owned(), "type".to_owned()])
        );
        let cleanup = &shape["object"]["collaborationCleanup"];
        assert_eq!(cleanup["object"]["state"], serde_json::json!("string"));
        assert_eq!(
            cleanup["object"]["pendingCount"],
            serde_json::json!("number")
        );
        assert_eq!(
            cleanup["object"]["quarantinedCount"],
            serde_json::json!("number")
        );
        assert!(optional_fields(cleanup).contains("message"));
        assert_eq!(
            required_fields(cleanup),
            BTreeSet::from([
                "pendingCount".to_owned(),
                "quarantinedCount".to_owned(),
                "state".to_owned(),
            ])
        );
    }

    #[test]
    fn mcp_health_is_global_and_vendor_tagged() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        let shape = &authority["messageShapes"]["agent.global.mcp.health"];
        assert_eq!(shape["object"]["vendor"], serde_json::json!("string"));
        assert_eq!(shape["object"]["serverName"], serde_json::json!("string"));
        assert!(
            !authority["agentIntelEventTypes"]
                .as_array()
                .expect("agent intel event types")
                .iter()
                .any(|event| event == "agent.intel.mcpHealth")
        );
    }

    #[test]
    fn health_and_degradation_enum_unions_are_exhaustive() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");

        let health = &authority["messageShapes"]["agent.global.mcp.health"]["object"]["health"];
        let health_variants = health["union"].as_array().expect("health union");
        let health_kinds = health_variants
            .iter()
            .filter_map(|variant| variant["discriminator"]["value"].as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            health_kinds,
            BTreeSet::from(["healthy", "misconfigured", "unknown", "unreachable"])
        );
        for kind in ["misconfigured", "unreachable"] {
            let variant = health_variants
                .iter()
                .find(|variant| variant["discriminator"]["value"] == kind)
                .unwrap_or_else(|| panic!("missing {kind} health variant"));
            assert_eq!(variant["object"]["reason"], serde_json::json!("string"));
            assert!(optional_fields(variant).contains("reason"));
        }

        let notices = authority["messageShapes"]["agent.global.claude.status"]["object"]["status"]
            ["object"]["notices"]["array"]
            .as_array()
            .expect("degradation notice union");
        let notice_kinds = notices
            .iter()
            .filter_map(|variant| variant["discriminator"]["value"].as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            notice_kinds,
            BTreeSet::from([
                "agentParseFailed",
                "copilotSessionStoreFailure",
                "malformedCopilotEvent",
                "malformedSettings",
                "terminalFallback",
                "unknownClaudeSubtype",
                "unsupportedCopilotSessionStore",
            ])
        );
        let unsupported = notices
            .iter()
            .find(|variant| variant["discriminator"]["value"] == "unsupportedCopilotSessionStore")
            .expect("unsupported store variant");
        assert_eq!(
            unsupported["object"]["supportedVersions"]["array"][0],
            "number"
        );
        assert_eq!(unsupported["object"]["schemaVersion"], "number");

        let store_failure = notices
            .iter()
            .find(|variant| variant["discriminator"]["value"] == "copilotSessionStoreFailure")
            .expect("store failure variant");
        assert_eq!(store_failure["object"]["failure"], "string");
        assert_eq!(store_failure["object"]["message"], "string");

        let terminal = notices
            .iter()
            .find(|variant| variant["discriminator"]["value"] == "terminalFallback")
            .expect("terminal fallback variant");
        assert_eq!(terminal["object"]["fields"]["array"][0], "string");
    }

    #[test]
    fn session_list_entries_declare_per_variant_presence() {
        let authority: serde_json::Value =
            serde_json::from_str(&render_desktop_runtime_authority_json().expect("authority"))
                .expect("authority JSON");
        let entries = authority["messageShapes"]["session.list"]["object"]["sessions"]["array"]
            .as_array()
            .expect("session list element shapes");
        assert_eq!(entries.len(), 2, "both list variants must be sampled");

        let local = entries
            .iter()
            .find(|entry| entry["object"].get("backendSessionId").is_some())
            .expect("local entry shape");
        let remote = entries
            .iter()
            .find(|entry| entry["object"].get("permissions").is_some())
            .expect("remote entry shape");

        assert!(
            required_fields(local).contains("kind"),
            "the tag is required"
        );
        assert!(
            optional_fields(local).is_superset(&BTreeSet::from([
                "backendSessionId".to_owned(),
                "createRequestId".to_owned(),
                "meta".to_owned(),
                "roomId".to_owned(),
                "roomName".to_owned(),
            ])),
            "local entry optionals: {:?}",
            optional_fields(local)
        );
        assert_eq!(nullable_fields(local), optional_fields(local));

        assert!(required_fields(remote).contains("kind"));
        assert!(
            optional_fields(remote).is_superset(&BTreeSet::from([
                "accessIssue".to_owned(),
                "accessReason".to_owned(),
                "accessState".to_owned(),
                "connectionReason".to_owned(),
                "connectionState".to_owned(),
                "owner".to_owned(),
            ])),
            "remote entry optionals: {:?}",
            optional_fields(remote)
        );
        assert!(
            required_fields(remote).contains("permissions"),
            "permissions is not optional on a remote entry"
        );

        let meta = &local["object"]["meta"];
        assert!(
            optional_fields(meta).contains("gitRepo"),
            "nested meta optionals: {:?}",
            optional_fields(meta)
        );
        assert!(required_fields(meta).contains("workingDir"));
    }

    fn required_fields(shape: &serde_json::Value) -> BTreeSet<String> {
        string_set(&shape["requiredFields"])
    }

    fn optional_fields(shape: &serde_json::Value) -> BTreeSet<String> {
        string_set(&shape["optionalFields"])
    }

    fn nullable_fields(shape: &serde_json::Value) -> BTreeSet<String> {
        string_set(&shape["nullableFields"])
    }

    fn string_set(value: &serde_json::Value) -> BTreeSet<String> {
        value
            .as_array()
            .expect("field list")
            .iter()
            .map(|entry| entry.as_str().expect("field name").to_owned())
            .collect()
    }

    fn contains_empty_array_shape(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(fields) => {
                fields
                    .get("array")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(Vec::is_empty)
                    || fields.values().any(contains_empty_array_shape)
            }
            serde_json::Value::Array(values) => values.iter().any(contains_empty_array_shape),
            _ => false,
        }
    }
}
