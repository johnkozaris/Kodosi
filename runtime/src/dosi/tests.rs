use serde_json::{Value, json};

use uuid::{NoContext, Timestamp, Uuid};

use super::{
    adapters::{Adapter, Event},
    store::Marks,
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

#[test]
fn marks_keep_old_turns_settled_after_they_let_their_ids_go() {
    let mut marks = Marks::default();
    let hour_ahead = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let ahead = Uuid::new_v7(Timestamp::from_unix(NoContext, hour_ahead, 0)).to_string();
    assert!(marks.settle(&ahead));
    let waiting = Uuid::now_v7().to_string();
    let turns = (0..1100)
        .map(|_| Uuid::now_v7().to_string())
        .collect::<Vec<_>>();
    for turn in &turns {
        assert!(marks.settle(turn));
        marks.trim(Some(&waiting));
    }
    assert!(!marks.settled(&waiting));
    marks.trim(None);
    assert!(turns.iter().all(|turn| marks.settled(turn)));
    assert!(!marks.settled(&Uuid::now_v7().to_string()));
}
