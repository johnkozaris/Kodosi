use serde_json::{Value, json};

use super::{
    DosiAction, DosiBody, DosiItem, DosiTarget, DosiTargetKind,
    adapters::{Adapter, Event},
    context,
    receipt::{self, Shown},
};
use crate::agents::Agent;

#[cfg(unix)]
mod turns;

fn parse(agent: Agent, lines: &[Value]) -> Vec<Event> {
    let mut adapter = Adapter::new(agent).unwrap();
    lines
        .iter()
        .flat_map(|line| adapter.parse(&line.to_string()))
        .collect()
}

#[test]
fn claude_streams_one_text_block_and_ends_the_turn() {
    let events = parse(
        Agent::Claude,
        &[
            json!({"type":"system","subtype":"init","session_id":"s1"}),
            json!({"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Look first."}}}),
            json!({"type":"assistant","message":{"id":"m1","content":[{"type":"thinking","thinking":"Look first."}]}}),
            json!({"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}}),
            json!({"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Two "}}}),
            json!({"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Two need you."}]}}),
            json!({"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"kodosi terminal list"}}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"api"}],"is_error":false}]}}),
            json!({"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}}),
            json!({"type":"result","subtype":"success","is_error":true,"result":"Please run /login"}),
        ],
    );
    assert_eq!(events[0], Event::Session("s1".to_owned()));
    assert_eq!(
        events[1],
        Event::ReasoningDelta {
            block: "m1:0".to_owned(),
            text: "Look first.".to_owned()
        }
    );
    assert_eq!(
        events[2],
        Event::ReasoningDone {
            block: "m1:0".to_owned(),
            text: "Look first.".to_owned()
        }
    );
    assert_eq!(
        events[4],
        Event::TextDone {
            block: "m1:1".to_owned(),
            text: "Two need you.".to_owned()
        }
    );
    assert!(
        matches!(&events[5], Event::ToolStart { id, name, .. } if id == "t1" && name == "Bash")
    );
    assert_eq!(
        events[6],
        Event::ToolEnd {
            id: "t1".to_owned(),
            failed: false,
            output: "api".to_owned()
        }
    );
    let Event::Reply(reply) = &events[7] else {
        panic!("a permission request gets an answer");
    };
    let reply: Value = serde_json::from_str(reply).unwrap();
    assert_eq!(reply["response"]["request_id"], "r1");
    assert_eq!(reply["response"]["response"]["behavior"], "allow");
    assert_eq!(
        events[8],
        Event::TurnEnd {
            failed: true,
            message: Some("Please run /login".to_owned())
        }
    );
}

#[test]
fn codex_items_become_tools_and_text_and_an_unfinished_turn_fails() {
    let mut adapter = Adapter::new(Agent::Codex).unwrap();
    let events = [
        json!({"type":"thread.started","thread_id":"th1"}),
        json!({"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'kodosi terminal read api'","status":"in_progress"}}),
        json!({"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'kodosi terminal read api'","aggregated_output":"$ ","exit_code":0,"status":"completed"}}),
        json!({"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"api waits."}}),
        json!({"type":"error","message":"stream disconnected"}),
    ]
    .iter()
    .flat_map(|line| adapter.parse(&line.to_string()))
    .collect::<Vec<_>>();
    assert_eq!(events[0], Event::Session("th1".to_owned()));
    assert!(
        matches!(&events[1], Event::ToolStart { input, .. } if input["command"] == "kodosi terminal read api")
    );
    assert!(matches!(&events[2], Event::ToolEnd { failed: false, .. }));
    assert!(matches!(&events[3], Event::TextDone { text, .. } if text == "api waits."));
    assert_eq!(
        adapter.ended(),
        Some(Event::TurnEnd {
            failed: true,
            message: Some("stream disconnected".to_owned())
        })
    );
}

fn shown(command: &str) -> Option<(DosiAction, Option<DosiTarget>, Option<String>)> {
    match receipt::command_receipt(command, None) {
        Shown::Receipt(receipt) => Some((receipt.action, receipt.target, receipt.text)),
        Shown::Hidden => None,
    }
}

fn target(kind: DosiTargetKind, value: &str) -> DosiTarget {
    DosiTarget {
        kind,
        value: value.to_owned(),
    }
}

#[test]
fn kodosi_commands_become_receipts_and_other_commands_stay_commands() {
    assert_eq!(
        shown(r#"kodosi terminal input "Release notes" --enter y"#),
        Some((
            DosiAction::Type,
            Some(target(DosiTargetKind::Terminal, "Release notes")),
            Some("y".to_owned())
        ))
    );
    assert_eq!(
        shown(r#"kodosi --json room --room 'Launch week' post "Docs are live." --agent Dosi"#),
        Some((
            DosiAction::Post,
            Some(target(DosiTargetKind::Room, "Launch week")),
            Some("Docs are live.".to_owned())
        ))
    );
    assert_eq!(
        shown("NO_COLOR=1 kodosi terminal keys api down enter"),
        Some((
            DosiAction::Press,
            Some(target(DosiTargetKind::Terminal, "api")),
            Some("down enter".to_owned())
        ))
    );
    assert_eq!(
        shown(r#"kodosi terminal start --agent codex --prompt "Add tests""#),
        Some((
            DosiAction::Start,
            Some(target(DosiTargetKind::Agent, "codex")),
            Some("Add tests".to_owned())
        ))
    );
    assert_eq!(
        shown(r#"kodosi dosi ask "Which?" --option a --option b"#),
        None
    );
    assert!(matches!(
        shown("npm test && kodosi terminal read api"),
        Some((
            DosiAction::Run,
            Some(DosiTarget {
                kind: DosiTargetKind::Command,
                ..
            }),
            None
        ))
    ));
    assert_eq!(
        receipt::unwrap_shell("/bin/zsh -lc 'kodosi terminal list'"),
        "kodosi terminal list"
    );
}

#[test]
fn a_catch_up_keeps_the_newest_lines_within_its_bounds() {
    let items = (0..100)
        .map(|index| DosiItem {
            id: index.to_string(),
            turn: "turn".to_owned(),
            at: 0,
            body: DosiBody::Person {
                text: format!("message {index} {}", "x".repeat(300)),
            },
        })
        .collect::<Vec<_>>();
    let shown = context::catch_up(&items.iter().collect::<Vec<_>>(), true).unwrap();
    assert!(shown.len() <= 13 * 1024);
    assert!(shown.contains("Older messages are left out."));
    assert!(shown.contains("message 99") && !shown.contains("message 0 "));
    assert_eq!(context::catch_up(&[], false), None);
}
