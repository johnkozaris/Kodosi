use super::*;
use serde_json::{Value, json};
use uuid::Uuid;

fn accepts(value: Value) -> bool {
    serde_json::from_value::<Action>(value).is_ok_and(|action| action.validate().is_ok())
}

#[test]
fn agent_commands_accept_normal_work_and_reject_unusable_payloads() {
    let id = Uuid::now_v7().to_string();
    for action in [
        json!({"type":"read"}),
        json!({"type":"post","text":"Ready for review","agent":"Claude","terminalId":id}),
        json!({"type":"createTask","title":"Review renderer","repositoryIds":[id],"terminalId":id}),
        json!({"type":"updateTask","taskId":id,"change":"close","note":"Fixed in PR #2","terminalId":id}),
        json!({"type":"addRepository","url":"git@github.com:team/project.git","provider":"github"}),
        json!({"type":"issues","repositoryId":id}),
        json!({"type":"importIssue","repositoryId":id,"number":23}),
    ] {
        assert!(accepts(action.clone()), "{action}");
    }
    for action in [
        json!({"type":"post","text":" \n "}),
        json!({"type":"post","text":"text\0with nul"}),
        json!({"type":"post","text":"x".repeat(16_385)}),
        json!({"type":"post","text":"hello","agent":""}),
        json!({"type":"post","text":"hello","terminalId":"not a terminal"}),
        json!({"type":"createTask","title":"x".repeat(257)}),
        json!({"type":"createTask","title":"Review","description":"x".repeat(16_385)}),
        json!({"type":"createTask","title":"Review","repositoryIds":["wrong"]}),
        json!({"type":"createTask","title":"Review","repositoryIds":vec![id.clone();65]}),
        json!({"type":"updateTask","taskId":id,"change":"close","note":""}),
        json!({"type":"updateTask","taskId":"wrong","change":"claim"}),
        json!({"type":"addRepository","url":"https://forge.example/team/repo","provider":"unsupported"}),
        json!({"type":"issues","repositoryId":"wrong"}),
        json!({"type":"post","text":"hello","silentUnknownFlag":true}),
    ] {
        assert!(!accepts(action.clone()), "{action}");
    }
}

#[test]
fn a_claim_takes_only_a_task_that_nobody_else_has() {
    let mut task: Task = serde_json::from_value(json!({
        "id":Uuid::now_v7(),"version":1,"title":"Fix the login test","description":"","closed":false,
        "assignedTo":null,"assignedName":null,"terminalId":null,"repositoryIds":[],"note":null,"issue":null
    }))
    .unwrap();
    let live = |terminal: &str| terminal == "one";
    task.claim("ana", "Ana".into(), Some("one".into()), live)
        .unwrap();
    assert_eq!(
        task.claim("ben", "Ben".into(), None, live),
        Err("Ana has this task. Take another task, or ask them to release it.".into())
    );
    assert_eq!(
        task.claim("ana", "Ana".into(), Some("two".into()), live),
        Err("You have this task in another terminal. Release it there first.".into())
    );
    task.claim("ana", "Ana".into(), Some("one".into()), live)
        .unwrap();
    assert_eq!(
        (task.assigned_to.as_deref(), task.terminal_id.as_deref()),
        (Some("ana"), Some("one"))
    );
    task.claim("ana", "Ana".into(), Some("two".into()), |_| false)
        .unwrap();
    assert_eq!(task.terminal_id.as_deref(), Some("two"));
    task.closed = true;
    assert!(
        task.claim("ana", "Ana".into(), Some("two".into()), live)
            .is_err()
    );
}
