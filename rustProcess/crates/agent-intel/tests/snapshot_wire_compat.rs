#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::too_many_lines,
    reason = "wire-compat tests panic with clear messages on shape drift, and the fixture builder is intentionally exhaustive"
)]

use agent_intel::domain::snapshot::AgentIntelSnapshot;
use std::{collections::BTreeSet, path::Path};

fn fixture() -> AgentIntelSnapshot {
    agent_intel::domain::snapshot::wire_authority_sample()
}

#[test]
fn generated_authority_owns_snapshot_top_level_keys() {
    let authority_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../protocol/desktop-runtime-authority.json");
    let authority: serde_json::Value = serde_json::from_slice(
        &std::fs::read(authority_path).expect("desktop runtime authority readable"),
    )
    .expect("desktop runtime authority parses");
    let schema = &authority["messageShapes"]["agent.intel.snapshot"]["object"]["payload"];
    let expected = schema["object"]
        .as_object()
        .expect("typed snapshot payload object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let value = serde_json::to_value(fixture()).expect("snapshot serialises");
    let actual = value
        .as_object()
        .expect("snapshot is object")
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "generated payload schema must match serialization"
    );
}

#[test]
fn nested_keys_use_camel_case() {
    let value = serde_json::to_value(fixture()).expect("snapshot serialises");
    let mut all_keys: BTreeSet<String> = BTreeSet::new();
    collect_keys(&value, &mut all_keys);
    for key in &all_keys {
        assert!(
            !key.contains('_'),
            "key `{key}` is not camelCase; check the corresponding domain module"
        );
    }
}

#[test]
fn snapshot_round_trips_through_json() {
    let original = fixture();
    let json = serde_json::to_string(&original).expect("serialise");
    let parsed: AgentIntelSnapshot = serde_json::from_str(&json).expect("deserialise");
    let reserialised = serde_json::to_string(&parsed).expect("re-serialise");
    assert_eq!(json, reserialised, "JSON shape changed across a round trip");
}

fn collect_keys(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                out.insert(k.clone());
                collect_keys(v, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_keys(item, out);
            }
        }
        _ => {}
    }
}
