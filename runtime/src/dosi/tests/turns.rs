use std::{
    collections::BTreeSet,
    os::unix::fs::PermissionsExt as _,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use serde_json::{Value, json};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::agents::{Agent, AgentChoiceChange, AgentPick, Runner};
use crate::dosi::{
    DosiAction, DosiAgent, DosiBody, DosiComputer, DosiEntry, DosiItem, DosiLog, DosiRecord,
    DosiTone, DosiToolStatus, Message, Output, SharedInstructions, Viewer,
    engine::{Answer, Engine, Runners},
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
    devices: BTreeSet<String>,
}

impl Server {
    fn device(server: &Arc<Mutex<Self>>) -> Uuid {
        let device = Uuid::now_v7();
        server.lock().unwrap().devices.insert(device.to_string());
        device
    }

    fn read(&self, device: Uuid, after: u64) -> DosiLog {
        if after > self.records.len() as u64 {
            return DosiLog {
                reset: true,
                ..DosiLog::default()
            };
        }
        let entries = (1..)
            .zip(&self.records)
            .filter(|(sequence, _)| *sequence > after)
            .map(|(sequence, (writer, record))| DosiEntry {
                sequence,
                mine: *writer == device,
                device: writer.to_string(),
                record: record.clone(),
            })
            .collect::<Vec<_>>();
        DosiLog {
            sequence: entries.last().map_or(after, |entry| entry.sequence),
            entries,
            instructions: self.instructions.clone(),
            reset: false,
            devices: self.devices.clone(),
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
            device: Server::device(server),
        });
        rig.join(computer);
        rig
    }

    fn join(&mut self, computer: &str) {
        let account = self
            .shared
            .as_ref()
            .map(|shared| (shared.user, shared.device.to_string()));
        let mut out = Output::default();
        self.engine.share(account, || computer.to_owned(), &mut out);
        self.apply(out);
    }

    fn computer(&self, name: &str) -> DosiComputer {
        DosiComputer {
            device: self.shared.as_ref().unwrap().device.to_string(),
            name: name.to_owned(),
        }
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
        self.engine.send(text, None, None, &mut out).unwrap();
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

    fn turn(&mut self, on: bool) {
        let mut out = Output::default();
        self.engine.turn(on, &mut out);
        self.apply(out);
    }

    /// Dosi here asks `question` in a turn that the person then stops.
    async fn question(&mut self, question: &str) -> DosiItem {
        self.send("slow please");
        self.until(|rig| !rig.file("token").is_empty()).await;
        let token = self.file("token");
        let mut out = Output::default();
        self.engine
            .ask(
                &token,
                question.to_owned(),
                vec!["api".to_owned(), "web".to_owned()],
                Vec::new(),
                false,
                None,
                &mut out,
            )
            .unwrap();
        self.apply(out);
        self.stop();
        self.idle().await;
        self.items()
            .into_iter()
            .find(|item| matches!(&item.body, DosiBody::Question { question: asked, .. } if asked == question))
            .unwrap()
    }
}

/// A phone of the person: it runs no agent, and asks Dosi on one of the person's computers.
struct Phone {
    viewer: Viewer,
    server: Arc<Mutex<Server>>,
    user: Uuid,
    device: Uuid,
    root: tempfile::TempDir,
}

impl Phone {
    fn new(server: &Arc<Mutex<Server>>, user: Uuid) -> Self {
        let root = tempfile::tempdir().unwrap();
        let (viewer, _) = Viewer::load(root.path());
        let mut phone = Self {
            viewer,
            server: Arc::clone(server),
            user,
            device: Server::device(server),
            root,
        };
        phone.join();
        phone
    }

    fn join(&mut self) {
        let mut out = Output::default();
        self.viewer
            .share(Some((self.user, self.device.to_string())), &mut out);
        self.apply(out);
    }

    fn restart(&mut self) {
        (self.viewer, _) = Viewer::load(self.root.path());
        self.join();
    }

    fn apply(&mut self, mut out: Output) {
        loop {
            let generation = self.viewer.generation();
            let mut next = Output::default();
            if let Some(record) = out.write.take() {
                let mut server = self.server.lock().unwrap();
                server.records.push((self.device, record));
                let sequence = server.records.len() as u64;
                drop(server);
                let written = Message::Written {
                    generation,
                    result: Ok(sequence),
                };
                self.viewer.handle(written, &mut next);
            }
            if let Some(after) = out.sync.take() {
                let log = self.server.lock().unwrap().read(self.device, after);
                let synced = Message::Synced {
                    generation,
                    result: Ok(log),
                };
                self.viewer.handle(synced, &mut next);
            }
            if next.write.is_none() && next.sync.is_none() {
                return;
            }
            out = next;
        }
    }

    fn act(&mut self, act: impl FnOnce(&mut Viewer, &mut Output)) {
        self.act_result(act);
    }

    fn act_result<T>(&mut self, act: impl FnOnce(&mut Viewer, &mut Output) -> T) -> T {
        let mut out = Output::default();
        let result = act(&mut self.viewer, &mut out);
        self.apply(out);
        result
    }

    fn snapshot(&self) -> Value {
        self.viewer.snapshot()["dosi"].clone()
    }

    fn items(&self) -> Vec<DosiItem> {
        serde_json::from_value(self.snapshot()["items"].clone()).unwrap()
    }

    fn ask(&mut self, text: &str, computer: DosiComputer) {
        self.act(|viewer, out| viewer.send(text, None, Some(computer), out).unwrap());
    }

    fn nudge(&mut self) {
        self.act(|viewer, out| viewer.nudge(None, out));
    }

    fn waiting(&self) -> Vec<String> {
        self.snapshot()["asks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ask| ask["computer"]["name"].as_str().unwrap().to_owned())
            .collect()
    }
}

fn people(items: &[DosiItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match &item.body {
            DosiBody::Person { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
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
    for change in [
        AgentChoiceChange::Pick(AgentPick {
            agent: Agent::Claude,
            account: None,
            model: Some("sonnet".to_owned()),
            effort: Some("high".to_owned()),
        }),
        AgentChoiceChange::Tab {
            agent: Some(Agent::Codex),
        },
        AgentChoiceChange::Tab {
            agent: Some(Agent::Claude),
        },
    ] {
        rig.engine
            .configure(&change, &mut Output::default())
            .unwrap();
    }
    let setup = &rig.snapshot()["setup"];
    assert_eq!(setup["agent"], "claude");
    assert_eq!(setup["picks"][0]["model"], "sonnet");
    std::fs::write(rig.base.join("missing"), "").unwrap();
    rig.send("hello again");
    rig.until(|rig| texts(&rig.items()).len() == 2 && rig.snapshot()["state"] == "idle")
        .await;
    let arguments = rig.file("arguments");
    let launches = arguments.lines().collect::<Vec<_>>();
    assert_eq!(launches.len(), 3);
    assert!(
        launches[1].contains("--resume") && launches[1].contains("--model sonnet --effort high")
    );
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
                Vec::new(),
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
            Vec::new(),
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
                vec!["api".to_owned(), "web".to_owned()],
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
async fn a_terminal_answer_types_its_keys_and_own_words_go_to_dosi() {
    let mut rig = Rig::new();
    rig.send("slow please");
    rig.until(|rig| !rig.file("token").is_empty()).await;
    let token = rig.file("token");
    for question in ["Overwrite the notes?", "Push it?"] {
        rig.engine
            .ask(
                &token,
                question.to_owned(),
                vec!["Yes".to_owned(), "No".to_owned()],
                vec!["y".to_owned(), "n".to_owned()],
                false,
                Some("notes".to_owned()),
                &mut Output::default(),
            )
            .unwrap();
    }
    rig.stop();
    rig.idle().await;
    let questions = rig
        .items()
        .into_iter()
        .filter(|item| matches!(item.body, DosiBody::Question { .. }))
        .collect::<Vec<_>>();
    let mut out = Output::default();
    rig.engine
        .answer(&questions[0].id, vec!["Yes".to_owned()], &mut out)
        .unwrap();
    let typed = out
        .typing
        .iter()
        .map(|typing| (typing.terminal.as_str(), typing.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(typed, [("notes", "y\r")]);
    assert_eq!(rig.snapshot()["state"], "idle");
    let mut out = Output::default();
    rig.engine
        .answer(
            &questions[1].id,
            vec!["Wait for the review".to_owned()],
            &mut out,
        )
        .unwrap();
    assert!(out.typing.is_empty());
    assert_eq!(rig.snapshot()["state"], "working");
    rig.idle().await;
    let input = rig.file("input");
    assert!(input.contains(r#"with Yes. Kodosi typed \"y\" and Enter in the terminal notes."#));
    assert!(input.contains(r#"in their own words: \"Wait for the review\". Kodosi typed nothing"#));
    let items = rig.items();
    rig.engine.shutdown();
    let (engine, _) = Rig::engine(&rig.base);
    let restored: Vec<DosiItem> =
        serde_json::from_value(engine.snapshot()["dosi"]["items"].clone()).unwrap();
    assert_eq!(restored, items);
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

#[tokio::test]
async fn a_phone_asks_a_computer_that_answers_in_the_turn_of_the_message() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut laptop = Rig::shared(&server, user, "Laptop");
    let mut phone = Phone::new(&server, user);
    phone.ask("What needs me?", studio.computer("Studio"));
    assert_eq!(phone.waiting(), ["Studio"]);
    assert_eq!(people(&phone.items()), ["What needs me?"]);

    laptop.nudge();
    assert_eq!(laptop.snapshot()["state"], "idle");
    assert_eq!(laptop.snapshot()["asks"][0]["computer"]["name"], "Studio");
    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "working");
    studio.idle().await;
    assert!(studio.file("input").contains("What needs me?"));
    assert_eq!(people(&studio.items()), ["What needs me?"]);
    assert_eq!(
        texts(&studio.items()),
        [("Two need you.".to_owned(), false)]
    );

    phone.nudge();
    assert_eq!(phone.items(), studio.items());
    assert_eq!(phone.waiting(), Vec::<String>::new());
    assert_eq!(phone.snapshot()["last"]["name"], "Studio");
    assert_eq!(phone.snapshot()["state"], "idle");

    studio.restart("Studio");
    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "idle");
    assert_eq!(studio.file("input").matches("What needs me?").count(), 1);
}

#[tokio::test]
async fn a_message_waits_for_its_computer_until_the_person_takes_it_back() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut phone = Phone::new(&server, user);
    let mut studio = Rig::shared(&server, user, "Studio");
    let away = studio.computer("Studio");
    phone.ask("Later", away.clone());
    phone.ask("Never mind", away);
    assert_eq!(phone.waiting(), ["Studio", "Studio"]);
    let taken = phone.snapshot()["asks"][1]["item"]
        .as_str()
        .unwrap()
        .to_owned();
    phone.act(|viewer, out| viewer.unqueue(&taken, out).unwrap());
    assert_eq!(phone.waiting(), ["Studio"]);
    assert_eq!(people(&phone.items()), ["Later"]);

    studio.nudge();
    studio.idle().await;
    let input = studio.file("input");
    assert!(input.contains("Later") && !input.contains("Never mind"));
    assert_eq!(people(&studio.items()), ["Later"]);
    phone.nudge();
    assert_eq!(phone.waiting(), Vec::<String>::new());
    assert!(
        phone
            .act_result(|viewer, out| viewer.unqueue(&taken, out))
            .is_err()
    );

    phone.restart();
    studio.restart("Studio");
    studio.nudge();
    assert_eq!(people(&phone.items()), ["Later"]);
    assert_eq!(people(&studio.items()), ["Later"]);
}

#[tokio::test]
async fn the_phone_answers_a_question_and_the_computer_that_asked_goes_on() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut laptop = Rig::shared(&server, user, "Laptop");
    let mut phone = Phone::new(&server, user);
    studio.send("slow please");
    studio.until(|rig| !rig.file("token").is_empty()).await;
    let token = studio.file("token");
    let mut out = Output::default();
    studio
        .engine
        .ask(
            &token,
            "Which folder?".to_owned(),
            vec!["api".to_owned(), "web".to_owned()],
            Vec::new(),
            false,
            None,
            &mut out,
        )
        .unwrap();
    studio.apply(out);
    studio.stop();
    studio.idle().await;

    phone.nudge();
    let question = phone
        .items()
        .into_iter()
        .find(|item| matches!(item.body, DosiBody::Question { .. }))
        .unwrap();
    phone.act(|viewer, out| {
        viewer
            .answer(&question.id, vec!["web".to_owned()], out)
            .unwrap();
    });
    laptop.nudge();
    let mut late = Output::default();
    assert!(
        laptop
            .engine
            .answer(&question.id, vec!["api".to_owned()], &mut late)
            .is_err()
    );
    assert_eq!(laptop.snapshot()["state"], "idle");

    studio.nudge();
    studio
        .until(|rig| texts(&rig.items()).len() == 1 && rig.snapshot()["state"] == "idle")
        .await;
    assert!(
        studio
            .file("input")
            .contains(r#"The person answered \"Which folder?\" with web"#)
    );
    phone.nudge();
    assert!(phone.items().iter().any(|item| matches!(
        &item.body,
        DosiBody::Question { answer: Some(answer), .. } if answer == &["web".to_owned()]
    )));
}

#[tokio::test]
async fn a_computer_with_dosi_off_tells_why_it_does_not_answer() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    let mut out = Output::default();
    studio.engine.turn(false, &mut out);
    studio.apply(out);
    assert_eq!(studio.snapshot()["state"], "off");
    phone.ask("What needs me?", studio.computer("Studio"));
    studio.nudge();
    phone.nudge();
    assert_eq!(phone.waiting(), Vec::<String>::new());
    assert!(phone.items().iter().any(|item| matches!(
        &item.body,
        DosiBody::Notice { tone: DosiTone::Failed, text: Some(text) } if text.contains("Studio")
    )));
    assert_eq!(studio.file("input"), "");
}

#[tokio::test]
async fn the_phone_stops_a_turn_and_starts_a_new_chat_for_every_device() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    studio.send("slow please");
    studio.until(running).await;
    phone.nudge();
    assert_eq!(phone.snapshot()["state"], "working");
    assert_eq!(phone.snapshot()["remote"]["computer"], "Studio");
    let items = phone.items();
    let earlier = phone.viewer.page(&items.last().unwrap().id);
    assert_eq!(earlier.items, items[..items.len() - 1]);

    phone.act(Viewer::stop);
    assert_eq!(phone.snapshot()["state"], "stopping");
    studio.nudge();
    studio.idle().await;
    phone.nudge();
    assert_eq!(phone.snapshot()["state"], "idle");
    assert!(phone.items().iter().any(|item| matches!(
        item.body,
        DosiBody::Notice {
            tone: DosiTone::Stopped,
            ..
        }
    )));

    phone.act(Viewer::renew);
    studio.nudge();
    assert_eq!(studio.items(), Vec::new());
    assert_eq!(
        studio.snapshot()["conversation"],
        phone.snapshot()["conversation"]
    );
}

#[tokio::test]
async fn an_answer_from_another_device_changes_only_the_answer() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    studio.send("slow please");
    studio.until(|rig| !rig.file("token").is_empty()).await;
    let token = studio.file("token");
    let mut out = Output::default();
    studio
        .engine
        .ask(
            &token,
            "Allow it?".to_owned(),
            vec!["Yes".to_owned(), "No".to_owned()],
            vec!["1".to_owned(), "2".to_owned()],
            false,
            Some("api".to_owned()),
            &mut out,
        )
        .unwrap();
    studio.apply(out);
    studio.stop();
    studio.idle().await;
    let mut question = studio
        .items()
        .into_iter()
        .find(|item| matches!(item.body, DosiBody::Question { .. }))
        .unwrap();
    question.body = DosiBody::Question {
        question: "Allow it?".to_owned(),
        options: vec!["Yes".to_owned(), "No".to_owned()],
        keys: vec!["rm -rf ~".to_owned(), "2".to_owned()],
        multiple: false,
        terminal: Some("web".to_owned()),
        answer: Some(vec!["Yes".to_owned()]),
    };
    let conversation = studio.snapshot()["conversation"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let phone = Server::device(&server);
    server.lock().unwrap().records.push((
        phone,
        DosiRecord::Item {
            conversation,
            item: question,
        },
    ));
    let mut out = Output::default();
    studio.engine.nudge(None, &mut out);
    let after = out.sync.take().unwrap();
    let log = server
        .lock()
        .unwrap()
        .read(studio.shared.as_ref().unwrap().device, after);
    let mut typed = Output::default();
    studio.engine.handle(
        Message::Synced {
            generation: studio.engine.generation(),
            result: Ok(log),
        },
        &mut typed,
    );
    let typing = typed
        .typing
        .iter()
        .map(|typing| (typing.terminal.as_str(), typing.text.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(typing, [("api", "1\r")]);
}

fn conversation(snapshot: &Value) -> String {
    snapshot["conversation"]["id"].as_str().unwrap().to_owned()
}

fn closed(items: &[DosiItem], id: &str) -> bool {
    items.iter().any(|item| {
        item.id == id
            && matches!(&item.body, DosiBody::Question { answer: Some(answer), .. } if answer.is_empty())
    })
}

#[tokio::test]
async fn a_question_closed_by_a_turn_on_another_computer_closes_here_without_a_turn() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut laptop = Rig::shared(&server, user, "Laptop");
    let question = studio.question("Which folder?").await;
    laptop.nudge();
    laptop.send("What needs me?");
    laptop.idle().await;

    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "idle");
    assert!(closed(&studio.items(), &question.id));
    assert!(!studio.file("input").contains("The person answered"));
}

#[tokio::test]
async fn records_that_the_server_loses_or_sends_again_do_not_run_again() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    phone.ask("What needs me?", studio.computer("Studio"));
    studio.nudge();
    studio.idle().await;
    let question = studio.question("Which folder?").await;
    phone.nudge();
    phone.act(|viewer, out| {
        viewer
            .answer(&question.id, vec!["web".to_owned()], out)
            .unwrap();
    });
    studio.nudge();
    studio
        .until(|rig| {
            rig.file("input").contains("The person answered") && rig.snapshot()["state"] == "idle"
        })
        .await;

    let records = server.lock().unwrap().records.clone();
    let asked = records
        .iter()
        .position(|(_, record)| matches!(record, DosiRecord::Ask { .. }))
        .unwrap();
    let answered = records
        .iter()
        .rposition(|(_, record)| {
            matches!(record, DosiRecord::Item { item, .. } if closed(std::slice::from_ref(item), &question.id) || matches!(&item.body, DosiBody::Question { answer: Some(answer), .. } if !answer.is_empty()))
        })
        .unwrap();
    server.lock().unwrap().records.truncate(answered + 1);
    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "idle");
    server
        .lock()
        .unwrap()
        .records
        .extend([records[asked].clone(), records[answered].clone()]);
    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "idle");
    studio.restart("Studio");
    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "idle");
    let input = studio.file("input");
    assert_eq!(input.matches("What needs me?").count(), 1);
    assert_eq!(
        input
            .matches(r#"The person answered \"Which folder?\""#)
            .count(),
        1
    );
}

#[tokio::test]
async fn every_device_keeps_the_newest_conversation() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    phone.act(Viewer::renew);
    let mut out = Output::default();
    studio.engine.renew(&mut out);
    studio.apply(out);
    let mut started = [
        conversation(&phone.snapshot()),
        conversation(&studio.snapshot()),
    ];
    started.sort();
    let [oldest, newest] = started;

    phone.nudge();
    studio.nudge();
    assert_eq!(conversation(&phone.snapshot()), newest);
    assert_eq!(conversation(&studio.snapshot()), newest);
    server.lock().unwrap().records.push((
        phone.device,
        DosiRecord::Begin {
            conversation: oldest,
        },
    ));
    phone.nudge();
    studio.nudge();
    assert_eq!(conversation(&phone.snapshot()), newest);
    assert_eq!(conversation(&studio.snapshot()), newest);
}

#[tokio::test]
async fn a_new_chat_follows_the_newest_conversation_when_the_clock_is_behind() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    let hour_ahead = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    let ahead =
        Uuid::new_v7(uuid::Timestamp::from_unix(uuid::NoContext, hour_ahead, 0)).to_string();
    let device = studio.shared.as_ref().unwrap().device;
    server.lock().unwrap().records.push((
        device,
        DosiRecord::Begin {
            conversation: ahead.clone(),
        },
    ));
    phone.nudge();
    assert_eq!(conversation(&phone.snapshot()), ahead);

    phone.act(Viewer::renew);
    let renewed = conversation(&phone.snapshot());
    assert!(renewed > ahead);
    studio.nudge();
    assert_eq!(conversation(&studio.snapshot()), renewed);
}

#[tokio::test]
async fn a_take_back_holds_after_the_person_removes_the_device_that_took_it_back() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    let mut pad = Phone::new(&server, user);
    phone.ask("What needs me?", studio.computer("Studio"));
    pad.nudge();
    let item = pad.snapshot()["asks"][0]["item"]
        .as_str()
        .unwrap()
        .to_owned();
    pad.act(|viewer, out| viewer.unqueue(&item, out).unwrap());
    let removed = pad.device.to_string();
    server.lock().unwrap().devices.remove(&removed);

    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "idle");
    assert_eq!(studio.file("input"), "");
}

#[tokio::test]
async fn a_message_that_waits_here_outlasts_many_turns_elsewhere_and_a_restart() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    studio.send("slow please");
    studio.until(running).await;
    phone.nudge();
    phone.ask("What needs me?", studio.computer("Studio"));
    let conversation = conversation(&studio.snapshot());
    let laptop = Server::device(&server);
    let long_ago = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        - 3_600_000;
    for _ in 0..1100 {
        server.lock().unwrap().records.push((
            laptop,
            DosiRecord::Turn {
                conversation: conversation.clone(),
                turn: Uuid::now_v7().to_string(),
                open: true,
                computer: "Laptop".to_owned(),
                at: long_ago,
            },
        ));
    }
    studio.nudge();
    studio.restart("Studio");
    studio.nudge();
    studio
        .until(|rig| {
            rig.file("input").contains("What needs me?") && rig.snapshot()["state"] == "idle"
        })
        .await;
}

#[tokio::test]
async fn the_phone_forgets_a_removed_computer_as_the_one_that_answered_last() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut laptop = Rig::shared(&server, user, "Laptop");
    let mut phone = Phone::new(&server, user);
    laptop.send("What needs me?");
    laptop.idle().await;
    phone.nudge();
    assert_eq!(phone.snapshot()["last"]["name"], "Laptop");

    let removed = laptop.shared.as_ref().unwrap().device.to_string();
    server.lock().unwrap().devices.remove(&removed);
    phone.nudge();
    assert_eq!(phone.snapshot()["last"], Value::Null);
    phone.restart();
    assert_eq!(phone.snapshot()["last"], Value::Null);
}

#[tokio::test]
async fn messages_from_a_device_that_the_person_removed_do_not_run() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    studio.send("slow please");
    studio.until(running).await;
    phone.nudge();
    phone.ask("What needs me?", studio.computer("Studio"));
    studio.nudge();
    assert_eq!(studio.snapshot()["asks"].as_array().unwrap().len(), 1);

    let removed = phone.device.to_string();
    server.lock().unwrap().devices.remove(&removed);
    phone.ask("And now?", studio.computer("Studio"));
    studio.nudge();
    studio.stop();
    studio.idle().await;
    assert_eq!(studio.snapshot()["asks"], json!([]));
    assert_eq!(people(&studio.items()), ["slow please"]);
    let input = studio.file("input");
    assert!(!input.contains("What needs me?") && !input.contains("And now?"));
}

#[tokio::test]
async fn dosi_off_closes_its_questions_and_leaves_other_computers_alone() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut laptop = Rig::shared(&server, user, "Laptop");
    let mut phone = Phone::new(&server, user);
    let question = studio.question("Which folder?").await;
    phone.nudge();
    studio.turn(false);
    assert!(closed(&studio.items(), &question.id));
    phone.act(|viewer, out| {
        viewer
            .answer(&question.id, vec!["web".to_owned()], out)
            .unwrap();
    });
    studio.nudge();
    studio.turn(true);
    assert_eq!(studio.snapshot()["state"], "idle");
    assert!(!studio.file("input").contains("The person answered"));

    laptop.nudge();
    let mut out = Output::default();
    laptop.engine.renew(&mut out);
    laptop.apply(out);
    laptop.send("slow please");
    laptop.until(running).await;
    studio.nudge();
    assert_eq!(studio.snapshot()["state"], "working");
    let stops = || {
        server
            .lock()
            .unwrap()
            .records
            .iter()
            .filter(|(_, record)| matches!(record, DosiRecord::Stop { .. }))
            .count()
    };
    let before = stops();
    studio.turn(false);
    assert_eq!(stops(), before);
    laptop.nudge();
    assert!(running(&laptop));
}

#[tokio::test]
async fn a_message_from_the_phone_reaches_a_resumed_agent_once() {
    let server = Arc::new(Mutex::new(Server::default()));
    let user = Uuid::now_v7();
    let mut studio = Rig::shared(&server, user, "Studio");
    let mut phone = Phone::new(&server, user);
    studio.send("What needs me?");
    studio.idle().await;
    phone.nudge();
    phone.ask("And the web?", studio.computer("Studio"));
    studio.nudge();
    studio.idle().await;
    assert_eq!(studio.file("input").matches("And the web?").count(), 1);
}

#[tokio::test]
async fn a_phone_sends_nothing_before_it_has_read_the_conversation() {
    let root = tempfile::tempdir().unwrap();
    let (mut viewer, _) = Viewer::load(root.path());
    let mut out = Output::default();
    viewer.share(Some((Uuid::now_v7(), Uuid::now_v7().to_string())), &mut out);
    let studio = DosiComputer {
        device: Uuid::now_v7().to_string(),
        name: "Studio".to_owned(),
    };
    let sent = viewer.send("What needs me?", None, Some(studio), &mut out);
    assert!(matches!(sent, Err(crate::Error::Busy)));
    assert!(out.write.is_none());
}
