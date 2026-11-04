use std::{path::Path, process::Command};

use serde_json::Value;
use text::{Node, Schema, SchemaSpec, Step};

const CASES: &str = "150";

/// Runs random edits through the real `ProseMirror` under Bun, and checks that each step gives
/// the same tree here, or fails here too.
#[test]
fn each_step_gives_the_same_tree_as_prosemirror() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../views/text-parity");
    let output = Command::new("bun")
        .args(["run", "generate.ts", "1", CASES])
        .current_dir(folder)
        .output()
        .expect("bun runs at test time");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    let spec: SchemaSpec = serde_json::from_str(lines.next().unwrap()).unwrap();
    let schema = Schema::new(&spec).unwrap();
    let mut steps = 0;
    for (case, line) in lines.enumerate() {
        let case_json: Value = serde_json::from_str(line).unwrap();
        let mut doc = Node::from_json(&schema, &case_json["doc"]).unwrap();
        assert_eq!(doc.to_json(&schema), case_json["doc"], "case {case}");
        for (index, entry) in case_json["steps"].as_array().unwrap().iter().enumerate() {
            let step = Step::from_json(&schema, &entry["step"]).unwrap();
            assert_eq!(
                step.to_json(&schema),
                entry["step"],
                "case {case}, step {index}"
            );
            let result = step.apply(&schema, &doc);
            match (&result, &entry["after"]) {
                (Ok(node), Value::Null) => panic!(
                    "case {case}, step {index}: ProseMirror failed and this gave {}",
                    node.to_json(&schema)
                ),
                (Err(failed), after) if !after.is_null() => {
                    panic!("case {case}, step {index}: {failed} in {}", entry["step"])
                }
                (Ok(node), after) => {
                    assert_eq!(node.to_json(&schema), *after, "case {case}, step {index}");
                    doc = node.clone();
                }
                (Err(_), _) => {}
            }
            steps += 1;
        }
    }
    assert!(steps > 1000, "only {steps} steps ran");
}
