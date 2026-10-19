use std::{
    os::unix::fs::PermissionsExt as _,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::agents::{Agent, Runner};
use crate::dosi::{
    DosiAction, DosiAgent, DosiBody, DosiEntry, DosiItem, DosiLog, DosiRecord, DosiSetup, DosiTone,
    DosiToolStatus, SharedInstructions,
    engine::{Answer, Engine, Message, Output, Runners},
};

const STUB: &str = r#"#!/bin/sh
here=$(dirname "$0")
printf '%s\n' "$*" >> "$here/arguments"
printf '%s' "$KODOSI_DOSI_TOKEN" > "$here/token"
session=""
resumed=no
while [ $# -gt 0 ]; do
  case "$1" in
    --session-id) session="$2"; shift ;;
    --resume) session="$2"; resumed=yes; shift ;;
  esac
  shift
done
if [ "$resumed" = yes ] && [ -e "$here/missing" ]; then
  rm "$here/missing"
  echo "No conversation found with session ID: $session" >&2
  exit 1
fi
echo "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"$session\"}"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$here/input"
  case "$line" in
    *'"subtype":"interrupt"'*)
      echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"rejected","is_error":true}]}}'
      echo '{"type":"result","subtype":"success","is_error":false,"result":""}'
      ;;
    *'"content":"slow'*)
      echo '{"type":"assistant","message":{"id":"m2","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"kodosi terminal read api"}}]}}'
      ;;
    *'"content":"die'*)
      echo "Claude Code lost its way" >&2
      exit 3
      ;;
    *'"type":"user"'*)
      echo '{"type":"stream_event","event":{"type":"message_start","message":{"id":"m1"}}}'
      echo '{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}'
      echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Two "}}}'
      echo '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"need you."}}}'
      echo '{"type":"assistant","message":{"id":"m1","content":[{"type":"text","text":"Two need you."}]}}'
      echo '{"type":"result","subtype":"success","is_error":false,"result":"Two need you."}'
      ;;
  esac
done
"#;

struct Stub {
    program: PathBuf,
}

impl Runners for Stub {
    fn installed(&self) -> Answer<Option<Vec<DosiAgent>>> {
        Box::pin(async {
            Some(vec![
                DosiAgent {
                    agent: Agent::Claude,
                    installed: true,
                },
                DosiAgent {
                    agent: Agent::Codex,
                    installed: false,
                },
            ])
        })
    }

    fn runner(&self, _: Agent, _: Option<String>) -> Answer<Result<Runner, String>> {
        let program = self.program.clone();
        Box::pin(async move {
            Ok(Runner {
                program,
                path: std::env::var_os("PATH"),
                variables: Vec::new(),
            })
        })
    }
}

#[derive(Default)]
struct Server {
    records: Vec<(Uuid, DosiRecord)>,
    instructions: Option<SharedInstructions>,
}

impl Server {
    fn read(&self, device: Uuid, after: u64) -> DosiLog {
        let entries = (1..)
            .zip(&self.records)
            .filter(|(sequence, _)| *sequence > after)
            .map(|(sequence, (writer, record))| DosiEntry {
                sequence,
                mine: *writer == device,
                record: record.clone(),
            })
            .collect::<Vec<_>>();
        DosiLog {
            sequence: entries.last().map_or(after, |entry| entry.sequence),
            entries,
            instructions: self.instructions.clone(),
            reset: false,
        }
    }
}

struct Shared {
    server: Arc<Mutex<Server>>,
    user: Uuid,
    device: Uuid,
}

struct Rig {
    engine: Engine,
    messages: mpsc::Receiver<Message>,
    base: PathBuf,
    shared: Option<Shared>,
    _root: tempfile::TempDir,
}

impl Rig {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let base = kodosi_os::fs::canonicalize(root.path()).unwrap();
        let (engine, messages) = Self::engine(&base);
        Self {
            engine,
            messages,
            base,
            shared: None,
            _root: root,
        }
    }

    fn shared(server: &Arc<Mutex<Server>>, user: Uuid, computer: &str) -> Self {
        let mut rig = Self::new();
        rig.shared = Some(Shared {
            server: Arc::clone(server),
            user,
            device: Uuid::now_v7(),
        });
        rig.join(computer);
        rig
    }

    fn join(&mut self, computer: &str) {
        let user = self.shared.as_ref().map(|shared| shared.user);
        let mut out = Output::default();
        self.engine.share(user, || computer.to_owned(), &mut out);
        self.apply(out);
    }

    fn engine(base: &std::path::Path) -> (Engine, mpsc::Receiver<Message>) {
        let program = base.join("claude");
        if !program.exists() {
            std::fs::write(&program, STUB).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Engine::load(
            &base.join("data"),
            &base.join("home"),
            Arc::new(Stub { program }),
        )
    }

    fn restart(&mut self, computer: &str) {
        self.engine.shutdown();
        let (engine, messages) = Self::engine(&self.base);
        self.engine = engine;
        self.messages = messages;
        self.join(computer);
    }

    fn apply(&mut self, mut out: Output) {
        let Some(shared) = &self.shared else {
            return;
        };
        loop {
            let generation = self.engine.generation();
            let mut next = Output::default();
            if let Some(record) = out.write.take() {
                let mut server = shared.server.lock().unwrap();
                server.records.push((shared.device, record));
                let sequence = server.records.len() as u64;
                drop(server);
                self.engine.handle(
                    Message::Written {
                        generation,
                        result: Ok(sequence),
                    },
                    &mut next,
                );
            }
            if let Some(after) = out.sync.take() {
                let log = shared.server.lock().unwrap().read(shared.device, after);
                self.engine.handle(
                    Message::Synced {
                        generation,
                        result: Ok(log),
                    },
                    &mut next,
                );
            }
            if let Some(instructions) = out.save.take() {
                shared.server.lock().unwrap().instructions = Some(instructions.clone());
                self.engine.handle(
                    Message::Saved {
                        generation,
                        instructions,
                        result: Ok(()),
                    },
                    &mut next,
                );
            }
            if next.write.is_none() && next.sync.is_none() && next.save.is_none() {
                return;
            }
            out = next;
        }
    }

    fn file(&self, name: &str) -> String {
        std::fs::read_to_string(self.base.join(name)).unwrap_or_default()
    }

    fn snapshot(&self) -> Value {
        self.engine.snapshot()["dosi"].clone()
    }

    fn items(&self) -> Vec<DosiItem> {
        serde_json::from_value(self.snapshot()["items"].clone()).unwrap()
    }

    async fn until(&mut self, done: impl Fn(&Self) -> bool + Send + Sync) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !done(self) {
                let message = self.messages.recv().await.unwrap();
                let mut out = Output::default();
                self.engine.handle(message, &mut out);
                self.apply(out);
            }
        })
        .await
        .expect("the engine reaches the expected state");
    }

    async fn idle(&mut self) {
        self.until(|rig| rig.snapshot()["state"] == "idle").await;
    }

    fn send(&mut self, text: &str) {
        let mut out = Output::default();
        self.engine.send(text, None, &mut out).unwrap();
        self.apply(out);
    }

    fn stop(&mut self) {
        let mut out = Output::default();
        self.engine.stop(&mut out);
        self.apply(out);
    }

    fn nudge(&mut self) {
        let mut out = Output::default();
        self.engine.nudge(None, &mut out);
        self.apply(out);
    }
}

fn texts(items: &[DosiItem]) -> Vec<(String, bool)> {
    items
        .iter()
        .filter_map(|item| match &item.body {
            DosiBody::Text { text, live } => Some((text.clone(), *live)),
            _ => None,
        })
        .collect()
}

fn running(rig: &Rig) -> bool {
    rig.items().iter().any(|item| {
        matches!(
            item.body,
            DosiBody::Tool {
                status: DosiToolStatus::Running,
                ..
            }
        )
    })
}

#[tokio::test]
async fn a_reply_streams_into_one_item_and_survives_a_restart() {
    let mut rig = Rig::new();
    rig.send("What needs me?");
    assert_eq!(rig.snapshot()["state"], "working");
    rig.idle().await;
    let items = rig.items();
    assert!(matches!(&items[0].body, DosiBody::Person { text } if text == "What needs me?"));
    assert_eq!(texts(&items), vec![("Two need you.".to_owned(), false)]);
    assert!(rig.file("arguments").contains("--session-id"));
    let folder = rig.base.join("home/.kodosi/dosi");
    assert!(folder.join("AGENTS.md").exists() && folder.join("watching.md").exists());
    assert!(
        ["notes", "skills", "tools"]
            .iter()
            .all(|place| folder.join(place).is_dir())
    );
    rig.engine.shutdown();
    let (engine, _) = Rig::engine(&rig.base);
    let restored: Vec<DosiItem> =
        serde_json::from_value(engine.snapshot()["dosi"]["items"].clone()).unwrap();
    assert_eq!(restored, items);
}

#[tokio::test]
async fn stop_ends_the_turn_and_the_waiting_message_runs_next() {
    let mut rig = Rig::new();
    rig.send("slow please");
    rig.until(running).await;
    rig.send("What needs me?");
    assert_eq!(rig.snapshot()["queued"].as_array().unwrap().len(), 1);
    rig.stop();
    rig.until(|rig| texts(&rig.items()).len() == 1 && rig.snapshot()["state"] == "idle")
        .await;
    let items = rig.items();
    assert!(items.iter().any(|item| matches!(
        item.body,
        DosiBody::Tool {
            action: DosiAction::Read,
            status: DosiToolStatus::Failed,
            ..
        }
    )));
    assert!(items.iter().any(|item| matches!(
        item.body,
        DosiBody::Notice {
            tone: DosiTone::Stopped,
            ..
        }
    )));
    assert_eq!(rig.snapshot()["queued"], json!([]));
}

#[tokio::test]
async fn a_provider_that_dies_reports_what_it_said() {
    let mut rig = Rig::new();
    rig.send("die now");
    rig.idle().await;
    let last = rig.items().pop().unwrap();
    assert!(matches!(
        last.body,
        DosiBody::Notice { tone: DosiTone::Failed, text: Some(text) } if text.contains("lost its way")
    ));
}

#[tokio::test]
async fn a_lost_provider_conversation_starts_over_with_the_whole_history() {
    let mut rig = Rig::new();
    rig.send("hello");
    rig.idle().await;
    rig.engine
        .configure(
            DosiSetup {
                agent: Some(Agent::Claude),
                model: Some("sonnet".to_owned()),
                ..DosiSetup::default()
            },
            &mut Output::default(),
        )
        .unwrap();
    std::fs::write(rig.base.join("missing"), "").unwrap();
    rig.send("hello again");
    rig.until(|rig| texts(&rig.items()).len() == 2 && rig.snapshot()["state"] == "idle")
        .await;
    let arguments = rig.file("arguments");
    let launches = arguments.lines().collect::<Vec<_>>();
    assert_eq!(launches.len(), 3);
    assert!(launches[1].contains("--resume") && launches[1].contains("--model sonnet"));
    assert!(launches[2].contains("--session-id"));
    let input = rig.file("input");
    let last = input.lines().last().unwrap();
    assert!(last.contains("The person: hello") && last.contains("You: Two need you."));
    assert!(last.contains("hello again"));
}

#[tokio::test]
async fn only_dosi_asks_and_the_answer_continues_without_a_new_bubble() {
    let mut rig = Rig::new();
    rig.send("slow please");
    rig.until(|rig| !rig.file("token").is_empty()).await;
    let token = rig.file("token");
    let other = Uuid::now_v7().to_string();
    assert!(
        rig.engine
            .ask(
                &other,
                "Which?".to_owned(),
                vec!["a".to_owned()],
                false,
                None,
                &mut Output::default()
            )
            .is_err()
    );
    rig.engine
        .ask(
            &token,
            "Which folder?".to_owned(),
            vec!["api".to_owned(), "web".to_owned()],
            false,
            None,
            &mut Output::default(),
        )
        .unwrap();
    rig.stop();
    rig.idle().await;
    let question = rig
        .items()
        .into_iter()
        .find(|item| matches!(item.body, DosiBody::Question { .. }))
        .unwrap();
    assert!(
        rig.engine
            .answer(
                &question.id,
                vec!["mobile".to_owned()],
                &mut Output::default()
            )
            .is_err()
    );
    rig.engine
        .answer(&question.id, vec!["web".to_owned()], &mut Output::default())
        .unwrap();
    rig.until(|rig| texts(&rig.items()).len() == 1 && rig.snapshot()["state"] == "idle")
        .await;
    let items = rig.items();
    let people = items
        .iter()
        .filter(|item| matches!(item.body, DosiBody::Person { .. }))
        .count();
    assert_eq!(people, 1);
    assert!(items.iter().any(|item| matches!(
        &item.body,
        DosiBody::Question { answer: Some(answer), .. } if answer == &["web".to_owned()]
    )));
}

#[tokio::test]
async fn two_computers_share_one_conversation_and_tell_each_other_what_they_missed() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut laptop = Rig::shared(&server, user, "Laptop");
    studio.send("What needs me?");
    studio.idle().await;
    laptop.nudge();
    assert_eq!(laptop.items(), studio.items());
    assert_eq!(laptop.snapshot()["state"], "idle");

    laptop.send("And now?");
    laptop.idle().await;
    let first = laptop.file("input");
    assert!(first.contains("The person: What needs me?") && first.contains("You: Two need you."));

    studio.nudge();
    assert_eq!(studio.items(), laptop.items());
    studio.send("Third");
    studio.idle().await;
    let input = studio.file("input");
    let last = input.lines().last().unwrap();
    assert!(last.contains("Since your last turn") && last.contains("The person: And now?"));
    assert!(!last.contains("What needs me?"));

    let mut out = Output::default();
    studio
        .engine
        .write_instructions(Some("Be brief."), &mut out)
        .unwrap();
    studio.apply(out);
    laptop.nudge();
    assert_eq!(laptop.engine.read_instructions().text, "Be brief.");

    let items = studio.items();
    studio.restart("Studio");
    assert_eq!(studio.items(), items);
    studio.nudge();
    assert_eq!(studio.items(), items);

    let mut out = Output::default();
    laptop.engine.renew(&mut out);
    laptop.apply(out);
    studio.nudge();
    assert_eq!(studio.items(), Vec::new());
    assert_eq!(
        studio.snapshot()["conversation"],
        laptop.snapshot()["conversation"]
    );
}

#[tokio::test]
async fn a_turn_on_another_computer_holds_new_messages_and_stops_from_here() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut laptop = Rig::shared(&server, user, "Laptop");
    studio.send("slow please");
    studio.until(running).await;
    laptop.nudge();
    assert_eq!(laptop.snapshot()["state"], "working");
    assert_eq!(laptop.snapshot()["remote"]["computer"], "Studio");
    laptop.send("What needs me?");
    assert_eq!(laptop.snapshot()["queued"].as_array().unwrap().len(), 1);

    laptop.stop();
    assert_eq!(laptop.snapshot()["state"], "stopping");
    studio.nudge();
    studio.idle().await;
    assert!(studio.items().iter().any(|item| matches!(
        item.body,
        DosiBody::Notice {
            tone: DosiTone::Stopped,
            ..
        }
    )));

    laptop.nudge();
    laptop.idle().await;
    assert_eq!(laptop.snapshot()["queued"], json!([]));
    assert_eq!(
        texts(&laptop.items()),
        vec![("Two need you.".to_owned(), false)]
    );
    assert!(laptop.file("input").contains("What needs me?"));
}
