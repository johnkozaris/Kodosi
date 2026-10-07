use super::{
    Context, Duration, Error, PathBuf, Result, Subcommand, Uuid, Value, json, local_host,
    mission_id, missions, only, print_json, print_text, render, request,
};
use crate::rooms::{Action, Snapshot, TaskChange};

#[derive(clap::Args)]
pub(super) struct Arguments {
    #[arg(
        long,
        global = true,
        help = "Room name or identifier; defaults to this terminal's room."
    )]
    room: Option<String>,
    #[command(subcommand)]
    action: RoomAction,
}
#[derive(Subcommand)]
enum RoomAction {
    List,
    Skill,
    Context,
    Read {
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        before: Option<u64>,
    },
    Post {
        text: String,
        #[arg(long)]
        agent: Option<String>,
    },
    Wait {
        #[arg(long, default_value_t = 60)]
        timeout: u64,
        #[arg(long)]
        after: Option<u64>,
    },
    Share {
        session: Option<String>,
    },
    #[command(subcommand)]
    Task(TaskAction),
    #[command(subcommand)]
    Repo(RepoAction),
}
#[derive(Subcommand)]
enum TaskAction {
    List {
        #[arg(long)]
        available: bool,
    },
    Create {
        title: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long = "repo")]
        repositories: Vec<String>,
    },
    Claim {
        task: String,
    },
    Release {
        task: String,
    },
    Close {
        task: String,
        #[arg(long)]
        note: Option<String>,
    },
    Reopen {
        task: String,
    },
    Import {
        repository: String,
        number: u64,
    },
}
#[derive(Subcommand)]
enum RepoAction {
    List,
    Add {
        url: Option<String>,
        #[arg(long)]
        directory: Option<PathBuf>,
        #[arg(long)]
        provider: Option<String>,
    },
    Issues {
        repository: String,
    },
}

fn terminal() -> Option<String> {
    std::env::var("KODOSI_SESSION_ID")
        .ok()
        .filter(|id| Uuid::parse_str(id).is_ok())
}

pub(super) async fn run(
    args: Arguments,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    if matches!(args.action, RoomAction::Skill) {
        return print_text(include_str!("../../skills/kodosi-room/SKILL.md"));
    }
    let catalog = missions(client, context).await?;
    if matches!(args.action, RoomAction::List) {
        return output(&catalog, json_output, &render::result(&catalog, context));
    }
    let room = resolve_room(args.room, &catalog, context)?;
    match args.action {
        RoomAction::Context => return show_context(client, context, &room).await,
        RoomAction::Share { session } => {
            return share_terminal(client, context, &room, session, json_output).await;
        }
        _ => {}
    }
    let before = match &args.action {
        RoomAction::Read { before, .. } => *before,
        _ => None,
    };
    let initial = call(client, context, &room, Action::Read { before }).await?;
    let mut snapshot: Snapshot = serde_json::from_value(initial["room"].clone())?;
    match args.action {
        RoomAction::Read { since, .. } => {
            if let Some(after) = since {
                snapshot.messages.retain(|message| message.sequence > after);
            }
            output(
                &serde_json::to_value(&snapshot)?,
                json_output,
                &conversation(&snapshot),
            )
        }
        RoomAction::Wait { timeout, after } => {
            wait(client, context, &room, &snapshot, after, timeout).await
        }
        RoomAction::Task(TaskAction::List { available }) => {
            show_tasks(&snapshot, available, json_output)
        }
        RoomAction::Repo(RepoAction::List) => output(
            &json!({"repositories":snapshot.repositories}),
            json_output,
            &snapshot
                .repositories
                .iter()
                .map(|repo| format!("{}  {}", repo.name, repo.url))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        command => {
            let action = match command {
                RoomAction::Post { text, agent } => Action::Post {
                    text,
                    agent,
                    terminal_id: terminal(),
                },
                RoomAction::Task(action) => task_action(action, &snapshot)?,
                RoomAction::Repo(action) => repository_action(action, &snapshot)?,
                _ => return Err(Error::Invalid("Unsupported room action.".into())),
            };
            let result = call(client, context, &room, action).await?;
            if !result["issues"].is_null() {
                return print_json(&result["issues"]);
            }
            let suffix = result["result"]["itemId"]
                .as_str()
                .map_or_else(String::new, |id| {
                    format!(" {}", &id[id.len().saturating_sub(6)..])
                });
            output(&result, json_output, &format!("Done.{suffix}"))
        }
    }
}

fn resolve_room(reference: Option<String>, catalog: &Value, context: &Context) -> Result<String> {
    if let Some(reference) = reference {
        return mission_id(catalog, &reference);
    }
    if let Some(id) = terminal().and_then(|id| {
        context
            .sessions
            .iter()
            .find(|session| session["id"].as_str() == Some(&id))
            .and_then(|session| session["missionId"].as_str())
            .map(str::to_owned)
    }) {
        return Ok(id);
    }
    let list = catalog["missions"]
        .as_array()
        .ok_or_else(|| Error::Invalid("Create or join a room first.".into()))?;
    if list.len() != 1 {
        return Err(Error::Invalid(
            "Choose a room with --room, or run this command inside one of its terminals.".into(),
        ));
    }
    list[0]["id"]
        .as_str()
        .map(str::to_owned)
        .ok_or(Error::Stale)
}

async fn show_context(
    client: &mut local_host::Client,
    context: &mut Context,
    room: &str,
) -> Result<()> {
    let event = request(
        client,
        context,
        json!({"type":"mission.open","requestId":Uuid::now_v7(),"missionId":room}),
        false,
    )
    .await?;
    let terminals = context
        .sessions
        .iter()
        .filter(|session| session["missionId"].as_str() == Some(room))
        .collect::<Vec<_>>();
    print_json(
        &json!({"room":event["mission"],"members":event["members"],"terminals":terminals,"currentTerminalId":terminal()}),
    )
}

async fn share_terminal(
    client: &mut local_host::Client,
    context: &mut Context,
    room: &str,
    session: Option<String>,
    json_output: bool,
) -> Result<()> {
    let reference = session
        .or_else(terminal)
        .ok_or_else(|| Error::Invalid("Choose the terminal to share.".into()))?;
    let id = context.session_id(&reference)?;
    let event = request(
        client,
        context,
        json!({"type":"session.attachMission","requestId":Uuid::now_v7(),"sessionId":id,
        "expectedRuntimeIncarnationId":context.incarnation(id)?,"missionId":room}),
        json_output,
    )
    .await?;
    output(&event, json_output, "The terminal is shared with the room.")
}

async fn wait(
    client: &mut local_host::Client,
    context: &mut Context,
    room: &str,
    snapshot: &Snapshot,
    after: Option<u64>,
    timeout: u64,
) -> Result<()> {
    if timeout == 0 || timeout > 3600 {
        return Err(Error::Invalid(
            "Use a wait timeout from 1 to 3600 seconds.".into(),
        ));
    }
    let after = after.unwrap_or(snapshot.sequence);
    if snapshot.sequence > after {
        return print_json(&serde_json::to_value(snapshot)?);
    }
    let next = tokio::time::timeout(Duration::from_secs(timeout), async {
        loop {
            let frame = client.next().await?;
            if let Some(event) = frame.get("event")
                && context.observe(event)
                && event["type"] == "room.snapshot"
                && event["room"]["roomId"] == room
                && event["room"]["sequence"]
                    .as_u64()
                    .is_some_and(|sequence| sequence > after)
            {
                return Ok::<_, Error>(event["room"].clone());
            }
        }
    })
    .await;
    match next {
        Ok(value) => print_json(&value?),
        Err(_) => print_json(&json!({"roomId":room,"sequence":after,"timedOut":true})),
    }
}

fn show_tasks(snapshot: &Snapshot, available: bool, json_output: bool) -> Result<()> {
    let tasks = snapshot
        .tasks
        .iter()
        .filter(|task| !available || (!task.closed && task.assigned_to.is_none()))
        .collect::<Vec<_>>();
    let text = tasks
        .iter()
        .map(|task| {
            format!(
                "{}  {}  {}",
                &task.id[task.id.len().saturating_sub(6)..],
                if task.closed {
                    "done"
                } else if task.assigned_to.is_some() {
                    "in progress"
                } else {
                    "up for grabs"
                },
                task.title
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    output(
        &json!({"tasks":tasks,"hasMore":snapshot.more_tasks}),
        json_output,
        &text,
    )
}

fn task_action(action: TaskAction, snapshot: &Snapshot) -> Result<Action> {
    let (reference, change, note) = match action {
        TaskAction::Create {
            title,
            description,
            repositories,
        } => {
            return Ok(Action::CreateTask {
                title,
                description,
                repository_ids: repositories
                    .iter()
                    .map(|reference| repo_id(snapshot, reference))
                    .collect::<Result<_>>()?,
                terminal_id: terminal(),
            });
        }
        TaskAction::Import { repository, number } => {
            return Ok(Action::ImportIssue {
                repository_id: repo_id(snapshot, &repository)?,
                number,
            });
        }
        TaskAction::Claim { task } => (task, TaskChange::Claim, None),
        TaskAction::Release { task } => (task, TaskChange::Release, None),
        TaskAction::Close { task, note } => (task, TaskChange::Close, note),
        TaskAction::Reopen { task } => (task, TaskChange::Reopen, None),
        TaskAction::List { .. } => return Err(Error::Invalid("Expected a task change.".into())),
    };
    let matching = snapshot
        .tasks
        .iter()
        .filter(|task| {
            task.id == reference || task.id.ends_with(&reference) || task.title == reference
        })
        .map(|task| task.id.as_str())
        .collect::<Vec<_>>();
    Ok(Action::UpdateTask {
        task_id: only(&matching, "task", &reference, "room task list")?.to_owned(),
        change,
        note,
        terminal_id: terminal(),
    })
}

fn repository_action(action: RepoAction, snapshot: &Snapshot) -> Result<Action> {
    match action {
        RepoAction::Issues { repository } => Ok(Action::Issues {
            repository_id: repo_id(snapshot, &repository)?,
        }),
        RepoAction::Add {
            url,
            directory,
            provider,
        } => {
            let url = if let Some(url) = url {
                url
            } else {
                let mut command = std::process::Command::new("git");
                if let Some(directory) = directory {
                    command.arg("-C").arg(directory);
                }
                let result = command.args(["remote", "get-url", "origin"]).output()?;
                if !result.status.success() {
                    return Err(Error::Invalid(
                        "Choose a repository URL or a working folder with an origin remote.".into(),
                    ));
                }
                String::from_utf8(result.stdout)
                    .map_err(|_| Error::Invalid("Repository address is not UTF-8.".into()))?
                    .trim()
                    .to_owned()
            };
            Ok(Action::AddRepository { url, provider })
        }
        RepoAction::List => Err(Error::Invalid("Expected a repository action.".into())),
    }
}

fn repo_id(snapshot: &Snapshot, reference: &str) -> Result<String> {
    let matching = snapshot
        .repositories
        .iter()
        .filter(|repo| {
            repo.id == reference
                || repo.id.ends_with(reference)
                || repo.name == reference
                || repo.url == reference
        })
        .map(|repo| repo.id.as_str())
        .collect::<Vec<_>>();
    only(&matching, "repository", reference, "room repo list").map(str::to_owned)
}
fn output(value: &Value, json_output: bool, text: &str) -> Result<()> {
    if json_output {
        print_json(value)
    } else {
        print_text(text)
    }
}
fn conversation(snapshot: &Snapshot) -> String {
    snapshot
        .messages
        .iter()
        .map(|message| {
            format!(
                "{}{}: {}",
                message.author_name,
                message
                    .agent
                    .as_ref()
                    .map_or_else(String::new, |agent| format!(" · {agent}")),
                message.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
async fn call(
    client: &mut local_host::Client,
    context: &mut Context,
    room: &str,
    action: Action,
) -> Result<Value> {
    let request = Uuid::now_v7().to_string();
    client
        .command(context.envelope(
            json!({"type":"room.command","requestId":request,"roomId":room,"action":action}),
        )?)
        .await?;
    tokio::time::timeout(Duration::from_mins(1), async {
        let mut snapshot = Value::Null;
        let mut issues = Value::Null;
        loop {
            let frame = client.next().await?;
            if frame["kind"] == "rejected" {
                return Err(Error::Other(
                    frame["message"]
                        .as_str()
                        .unwrap_or("Room request was rejected.")
                        .into(),
                ));
            }
            let Some(event) = frame.get("event") else {
                continue;
            };
            if !context.observe(event) {
                continue;
            }
            match event["type"].as_str() {
                Some("room.snapshot") if event["room"]["roomId"] == room => {
                    snapshot = event["room"].clone();
                }
                Some("room.issues") if event["requestId"] == request => {
                    issues = event["issues"].clone();
                }
                Some("room.error") if event["requestId"] == request => {
                    return Err(Error::Other(
                        event["message"]
                            .as_str()
                            .unwrap_or("Room request failed.")
                            .into(),
                    ));
                }
                Some("room.result") if event["requestId"] == request => {
                    return Ok(json!({"room":snapshot,"issues":issues,"result":event}));
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| {
        Error::Other("The room did not respond in time. The action was not repeated.".into())
    })?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Snapshot {
        serde_json::from_value(json!({
            "roomId":Uuid::now_v7(),"messages":[],"sequence":0,"hasOlder":false,"moreTasks":false,
            "repositories":[
                {"id":"019f0000-0000-7000-8000-000000000001","name":"team/api","url":"https://github.com/team/api","host":"github.com","owner":"team","repository":"api","provider":"github"},
                {"id":"019f0000-0000-7000-8000-000000000002","name":"team/ui","url":"https://forge.example/team/ui","host":"forge.example","owner":"team","repository":"ui","provider":"gitea"}
            ],
            "tasks":[
                {"id":"019f0000-0000-7000-8000-000000000003","version":1,"title":"Review","description":"","closed":false,"assignedTo":null,"assignedName":null,"terminalId":null,"repositoryIds":[],"note":null,"issue":null}
            ]
        })).unwrap()
    }

    #[test]
    fn commands_resolve_work_across_repositories_without_a_shared_checkout() {
        let mut room = snapshot();
        let create = task_action(
            TaskAction::Create {
                title: "Join API and UI".into(),
                description: "Shared context".into(),
                repositories: vec!["team/api".into(), "000002".into()],
            },
            &room,
        )
        .unwrap();
        let Action::CreateTask { repository_ids, .. } = create else {
            panic!()
        };
        assert_eq!(
            repository_ids,
            room.repositories
                .iter()
                .map(|r| r.id.clone())
                .collect::<Vec<_>>()
        );
        for command in [
            TaskAction::Claim {
                task: "Review".into(),
            },
            TaskAction::Release {
                task: "000003".into(),
            },
            TaskAction::Close {
                task: room.tasks[0].id.clone(),
                note: Some("PR #27".into()),
            },
            TaskAction::Reopen {
                task: "Review".into(),
            },
        ] {
            let Action::UpdateTask { task_id, .. } = task_action(command, &room).unwrap() else {
                panic!()
            };
            assert_eq!(task_id, room.tasks[0].id);
        }
        let imported = task_action(
            TaskAction::Import {
                repository: room.repositories[1].url.clone(),
                number: 4,
            },
            &room,
        )
        .unwrap();
        assert!(matches!(imported, Action::ImportIssue { number: 4, .. }));
        let issues = repository_action(
            RepoAction::Issues {
                repository: "team/api".into(),
            },
            &room,
        )
        .unwrap();
        assert!(
            matches!(issues,Action::Issues { repository_id } if repository_id == room.repositories[0].id)
        );
        let add = repository_action(
            RepoAction::Add {
                url: Some("https://github.com/another/repo".into()),
                directory: None,
                provider: None,
            },
            &room,
        )
        .unwrap();
        assert!(matches!(add, Action::AddRepository { .. }));
        assert!(repo_id(&room, "unknown").is_err());
        assert!(repo_id(&room, "00000").is_err());
        let mut duplicate = room.tasks[0].clone();
        duplicate.id = Uuid::now_v7().to_string();
        room.tasks.push(duplicate);
        assert!(
            task_action(
                TaskAction::Claim {
                    task: "Review".into()
                },
                &room
            )
            .is_err()
        );
        assert!(
            task_action(
                TaskAction::Claim {
                    task: "unknown".into()
                },
                &room
            )
            .is_err()
        );
    }

    #[test]
    fn room_names_must_be_unambiguous_and_agent_authorship_is_visible() {
        let context = Context::from_events(&[]);
        let id = Uuid::now_v7().to_string();
        let rooms = json!({"missions":[{"id":id,"name":"Workspace"}]});
        assert_eq!(
            resolve_room(Some("Workspace".into()), &rooms, &context).unwrap(),
            id
        );
        assert_eq!(resolve_room(None, &rooms, &context).unwrap(), id);
        assert!(resolve_room(None, &json!({"missions":[]}), &context).is_err());
        assert!(
            resolve_room(
                None,
                &json!({"missions":[{"id":id},{"id":"second"}]}),
                &context
            )
            .is_err()
        );
        assert!(resolve_room(None, &json!({}), &context).is_err());
        let mut room = snapshot();
        room.messages = serde_json::from_value(json!([
            {"id":"1","sequence":1,"authorId":id,"authorName":"Alice","agent":null,"terminalId":null,"text":"Can someone review?","createdAt":"now"},
            {"id":"2","sequence":2,"authorId":id,"authorName":"Alice","agent":"Claude","terminalId":null,"text":"I can pick it up.","createdAt":"now"}
        ])).unwrap();
        assert_eq!(
            conversation(&room),
            "Alice: Can someone review?\nAlice · Claude: I can pick it up."
        );
    }
}
