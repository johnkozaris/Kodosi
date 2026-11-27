//! Replay tests driven by recorded JSONL sessions under `fixtures/`:
//! - `claude/`: events emitted by the Claude Code link module.
//! - `codex/`: relevant Codex app-server messages forwarded by the Codex link.
//! - `copilot/`: events emitted by the Copilot extension (Copilot CLI 1.0.115).
//! - `antigravity/`: `agy acp` 1.5.14 messages via `kodosi acp`, in sessions backed by a local
//!   Gemini API mock via `GOOGLE_GEMINI_BASE_URL`.
//!
//! Private paths in fixtures have been scrubbed. Synthetic harness directives in the JSONL
//! (`answer`, `prompt`, `stop`) represent expected prompt decisions, chat messages, and
//! user stops recorded during capture.

use serde_json::{Value, json};

use super::{
    agent::{Agent, Places, Stop},
    antigravity::Antigravity,
    claude::Claude,
    codex::Codex,
    copilot::Copilot,
    room::{Asked, ChatHandle, Job},
    state::Chat,
    *,
};

macro_rules! record {
    ($name:literal) => {
        include_str!(concat!("fixtures/claude/", $name, ".jsonl"))
    };
}

macro_rules! codex {
    ($name:literal) => {
        include_str!(concat!("fixtures/codex/", $name, ".jsonl"))
    };
}

macro_rules! copilot {
    ($name:literal) => {
        include_str!(concat!("fixtures/copilot/", $name, ".jsonl"))
    };
}

macro_rules! antigravity {
    ($name:literal) => {
        include_str!(concat!("fixtures/antigravity/", $name, ".jsonl"))
    };
}

struct Play {
    chat: Chat,
    agent: Box<dyn Agent>,
    view: ChatSnapshot,
    closed: Vec<String>,
    changes: Vec<ChatChange>,
    calls: Vec<Value>,
    answers_by: Option<ChatPerson>,
}

impl Default for Play {
    fn default() -> Self {
        let mut play = Self::new(Box::new(Claude::default()));
        play.take(&json!({"k": "cwd", "cwd": "/home/sam/shop"}));
        play
    }
}

impl Play {
    fn new(agent: Box<dyn Agent>) -> Self {
        Self::at_home(agent, "/home/sam")
    }

    fn at_home(agent: Box<dyn Agent>, home: &str) -> Self {
        Self {
            chat: Chat::at_home(Some(home.to_owned())),
            agent,
            view: ChatSnapshot::default(),
            closed: Vec::new(),
            changes: Vec::new(),
            calls: Vec::new(),
            answers_by: None,
        }
    }

    fn codex(records: &[&str], answers: &[ChatAnswer]) -> Self {
        let mut play = Self::new(Box::new(Codex::default()));
        play.answers_by = Some(sam());
        play.more(codex!("start"), &[]);
        let records = records.join("");
        play.more(&records, answers);
        play
    }

    fn antigravity(record: &str, answers: &[ChatAnswer]) -> Self {
        let mut play = Self::at_home(Box::new(Antigravity::default()), "/home/ana");
        play.answers_by = Some(sam());
        play.more(record, answers);
        play
    }

    fn copilot(records: &[&str], answers: &[ChatAnswer]) -> Self {
        let mut play = Self::new(Box::new(Copilot::default()));
        play.more(copilot!("start"), &[]);
        play.more(&records.join(""), answers);
        play
    }

    fn of(record: &str, answers: &[ChatAnswer]) -> Self {
        let mut play = Self::default();
        play.more(record, answers);
        play
    }

    /// Mirrors `Room::hears` by marking the link active before replaying `record`.
    fn more(&mut self, record: &str, answers: &[ChatAnswer]) {
        self.agent.back(&mut self.chat);
        let mut answers = answers.iter();
        for line in record.lines() {
            let line: Value = serde_json::from_str(line).expect("a line of the record");
            if let Some(text) = line.get("prompt").and_then(Value::as_str) {
                self.chat.queue(text.to_owned(), self.answers_by.clone());
            }
            if line.get("stop").is_some() {
                self.stop();
            }
            match line.get("answer") {
                Some(recorded) => {
                    let ask = recorded["ask"].as_str().expect("the question");
                    let answer = answers.next().expect("an answer for each question");
                    let decision = self
                        .agent
                        .decision(ask, answer, self.answers_by.as_ref())
                        .expect("a decision");
                    assert_eq!(decision, recorded["decision"]);
                    self.agent.answered(&mut self.chat, ask, &decision);
                }
                None if line.get("k").is_some() => self.take(&line),
                None => {}
            }
            if matches!(self.agent.name(), Codex::NAME | Antigravity::NAME) {
                let calls = self.agent.commands(&self.chat);
                self.calls.extend(calls);
            }
            self.follow();
        }
        assert!(answers.next().is_none(), "an answer had no question");
    }

    fn stop(&mut self) {
        match self.agent.stop(&self.chat) {
            Stop::Command(command) => self.calls.push(command),
            Stop::Decisions(decisions) => {
                for (ask, decision) in decisions {
                    self.agent.answered(&mut self.chat, &ask, &decision);
                }
                self.calls.extend(self.agent.commands(&self.chat));
            }
            Stop::Nothing => {}
        }
    }

    fn take(&mut self, event: &Value) {
        let closed = self.agent.apply(&mut self.chat, event);
        self.closed.extend(closed);
        self.follow();
    }

    fn follow(&mut self) {
        let Some(update) = self.chat.take() else {
            return;
        };
        assert_eq!(update.revision, self.view.revision + 1);
        self.view.revision = update.revision;
        for change in &update.changes {
            match change {
                ChatChange::Item { item } => {
                    match self.view.items.iter_mut().find(|known| known.id == item.id) {
                        Some(known) => *known = item.clone(),
                        None => self.view.items.push(item.clone()),
                    }
                }
                ChatChange::Text { id, text: more } => {
                    let item = self.view.items.iter_mut().find(|item| &item.id == id);
                    let Some(ChatBody::Text { text, .. }) = item.map(|item| &mut item.body) else {
                        panic!("more text for an item that is no answer");
                    };
                    text.push_str(more);
                }
                ChatChange::Remove { id } => self.view.items.retain(|item| &item.id != id),
                ChatChange::Asks { asks } => self.view.asks.clone_from(asks),
                ChatChange::Queued { queued } => self.view.queued.clone_from(queued),
                ChatChange::Tasks { tasks } => self.view.tasks.clone_from(tasks),
                ChatChange::Commands { commands } => self.view.commands.clone_from(commands),
                ChatChange::Settings { settings } => self.view.settings.clone_from(settings),
                ChatChange::Peers { peers } => self.view.peers.clone_from(peers),
                ChatChange::Facts { facts } => self.view.facts = (**facts).clone(),
                ChatChange::State { state, now } => {
                    self.view.state = *state;
                    self.view.now.clone_from(now);
                }
                ChatChange::Reset => {
                    self.view.items.clear();
                    self.view.asks.clear();
                    self.view.tasks.clear();
                }
            }
        }
        assert_eq!(
            self.view,
            self.chat.snapshot(),
            "the viewer has another chat"
        );
        self.changes.extend(update.changes);
    }

    fn tools(&self) -> Vec<&ChatTool> {
        self.view
            .items
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Tool { tool } => Some(&**tool),
                _ => None,
            })
            .collect()
    }

    fn people(&self) -> Vec<&str> {
        self.view
            .items
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Person { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn answers(&self) -> Vec<&str> {
        self.view
            .items
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    fn ends(&self) -> Vec<TurnEnd> {
        self.view
            .items
            .iter()
            .filter_map(|item| match &item.body {
                ChatBody::Turn { end, .. } => Some(*end),
                _ => None,
            })
            .collect()
    }
}

fn line(sign: ChangeSign, text: &str) -> ChangeLine {
    ChangeLine {
        sign,
        text: text.to_owned(),
    }
}

/// The places of a chat whose agent reported `folder`, on a computer whose home folder is `home`.
fn places(folder: Option<&str>, home: &str) -> Places {
    let mut chat = Chat::at_home(Some(home.to_owned()));
    chat.change_facts(|facts| facts.folder = folder.map(str::to_owned));
    Places::of(&chat)
}

#[test]
fn a_file_shows_its_name_in_the_terminals_folder_and_its_place_elsewhere() {
    let places = places(Some("/home/sam/shop"), "/home/sam");

    for (path, shown) in [
        ("/home/sam/shop/prices.json", "prices.json"),
        ("/home/sam/shop/data/prices.json", "prices.json"),
        ("data/prices.json", "prices.json"),
        (
            "/home/sam/Repos/Kodosi/prices.json",
            "~/Repos/Kodosi/prices.json",
        ),
        ("/home/sam/shop-old/prices.json", "~/shop-old/prices.json"),
        ("/home/sam/shop/../other/prices.json", "~/other/prices.json"),
        ("../other/prices.json", "~/other/prices.json"),
        ("/home/samuel/prices.json", "/home/samuel/prices.json"),
        ("/etc/hosts", "/etc/hosts"),
    ] {
        assert_eq!(places.file(path), shown, "{path}");
    }
}

/// A file in another folder with the name of a file in Sam's shop.
const OTHER_PRICES: &str = "/home/sam/Repos/Kodosi/prices.json";

#[test]
fn claude_code_asks_for_a_file_of_another_folder_with_its_place() {
    let record = [
        json!({"k": "row", "uuid": "r1", "door": "response", "content": [
            {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "/home/sam/shop/prices.json"}},
            {"type": "tool_use", "id": "t2", "name": "Read", "input": {"file_path": OTHER_PRICES}},
        ]}),
        json!({"k": "ask", "ask": "a1", "call": "t2", "tool": "Read", "input": {"file_path": OTHER_PRICES},
            "suggestions": [{"type": "addRules", "behavior": "allow", "destination": "session",
                "rules": [{"toolName": "Read", "ruleContent": "//home/sam/Repos/Kodosi/**"}]}]}),
        json!({"k": "ask", "ask": "a2", "tool": "Read", "input": {"file_path": OTHER_PRICES},
            "suggestions": [{"type": "addDirectories", "directories": ["/home/sam/Repos/Kodosi/"]}]}),
    ]
    .map(|line| line.to_string())
    .join("\n");
    let play = Play::of(&record, &[]);

    assert_eq!(
        play.tools()
            .iter()
            .map(|tool| tool.target.as_str())
            .collect::<Vec<_>>(),
        ["prices.json", "~/Repos/Kodosi/prices.json"]
    );
    let offers = play
        .view
        .asks
        .iter()
        .map(|ask| match &ask.body {
            AskBody::Permission { tool, always, .. } => (tool.target.as_str(), always.clone()),
            _ => panic!("the agent asks for a permission"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        offers,
        [
            (
                "~/Repos/Kodosi/prices.json",
                Some(AskAlways::Rule {
                    tool: "Read".to_owned(),
                    text: "~/Repos/Kodosi/**".to_owned()
                })
            ),
            (
                "~/Repos/Kodosi/prices.json",
                Some(AskAlways::Folder {
                    name: "~/Repos/Kodosi".to_owned()
                })
            ),
        ]
    );
}

#[test]
fn codex_asks_for_a_change_of_a_file_of_another_folder_with_its_place() {
    let turn = |item: Value| {
        json!({"k": "note", "m": "item/started", "p": {
            "threadId": "00000001-0000-7000-8000-000000000000", "turnId": "turn9", "item": item}})
        .to_string()
    };
    let record = [
        turn(json!({"type": "commandExecution", "id": "x1", "status": "inProgress",
            "command": "/bin/zsh -lc 'cat prices.json ../Repos/Kodosi/prices.json'",
            "commandActions": [
                {"type": "read", "command": "cat prices.json", "name": "prices.json", "path": "/home/sam/shop/prices.json"},
                {"type": "read", "command": "cat ../Repos/Kodosi/prices.json", "name": "prices.json", "path": OTHER_PRICES},
            ]})),
        turn(json!({"type": "fileChange", "id": "x2", "status": "inProgress",
            "changes": [{"path": OTHER_PRICES, "kind": {"type": "update"}, "diff": "@@ -1 +1 @@\n-1\n+2\n"}]})),
        json!({"k": "request", "id": 9, "m": "item/fileChange/requestApproval", "p": {
            "threadId": "00000001-0000-7000-8000-000000000000", "turnId": "turn9", "itemId": "x2"}})
        .to_string(),
    ]
    .join("\n");
    let play = Play::codex(&[codex!("first"), &record], &[]);

    let tools = play.tools();
    assert_eq!(
        tools[tools.len() - 2..]
            .iter()
            .map(|tool| tool.target.as_str())
            .collect::<Vec<_>>(),
        [
            "prices.json, ~/Repos/Kodosi/prices.json",
            "~/Repos/Kodosi/prices.json"
        ]
    );
    let [ask] = &play.view.asks[..] else {
        panic!("one question is open");
    };
    let AskBody::Permission { tool, always, .. } = &ask.body else {
        panic!("the question asks for a permission");
    };
    assert_eq!(tool.target, "~/Repos/Kodosi/prices.json");
    assert_eq!(
        always,
        &Some(AskAlways::Rule {
            tool: "apply_patch".to_owned(),
            text: "~/Repos/Kodosi/prices.json".to_owned()
        })
    );
}

#[test]
fn copilot_asks_for_a_change_of_a_file_of_another_folder_with_its_place() {
    let record = [
        json!({"k": "event", "type": "tool.execution_start", "data": {
            "toolCallId": "c1", "toolName": "view", "arguments": {"path": "prices.json"}}}),
        json!({"k": "event", "type": "tool.execution_start", "data": {
            "toolCallId": "c2", "toolName": "edit",
            "arguments": {"path": OTHER_PRICES, "old_str": "1", "new_str": "2"}}}),
        json!({"k": "ask", "ask": "a1", "request": {
            "kind": "write", "toolCallId": "c2", "intention": "Update file", "fileName": OTHER_PRICES}}),
    ]
    .map(|line| line.to_string())
    .join("\n");
    let play = Play::copilot(&[&record], &[]);

    assert_eq!(
        play.tools()
            .iter()
            .map(|tool| tool.target.as_str())
            .collect::<Vec<_>>(),
        ["prices.json", "~/Repos/Kodosi/prices.json"]
    );
    let [ask] = &play.view.asks[..] else {
        panic!("one question is open");
    };
    assert!(matches!(
        &ask.body,
        AskBody::Permission { tool, .. } if tool.target == "~/Repos/Kodosi/prices.json"
    ));
}

#[test]
fn antigravity_asks_for_a_file_of_another_folder_with_its_place() {
    let started = antigravity!("allow")
        .lines()
        .take_while(|line| !line.contains("session/request_permission"))
        .collect::<Vec<_>>()
        .join("\n");
    let session = "ab05ead0-ed08-43b7-b547-d2d6bc449422";
    let read = |id: &str, path: &str| {
        json!({"sessionUpdate": "tool_call", "toolCallId": id, "name": "read_file",
            "status": "pending", "kind": "read", "locations": [{"path": path}]})
    };
    let record = [
        json!({"k": "note", "m": "session/update", "p": {"sessionId": session,
            "update": read("g1", "/home/ana/work/prices.json")}}),
        json!({"k": "request", "id": "9", "m": "session/request_permission", "p": {
        "sessionId": session, "toolCall": read("g2", "/home/ana/Repos/Kodosi/prices.json"),
        "options": [
            {"optionId": "allow", "name": "Allow", "kind": "allow_once"},
            {"optionId": "always", "name": "Always", "kind": "allow_always"},
        ]}}),
    ]
    .map(|line| line.to_string())
    .join("\n");
    let play = Play::antigravity(&format!("{started}\n{record}"), &[]);

    let tools = play.tools();
    assert_eq!(
        tools[tools.len() - 2..]
            .iter()
            .map(|tool| tool.target.as_str())
            .collect::<Vec<_>>(),
        ["prices.json", "~/Repos/Kodosi/prices.json"]
    );
    let [ask] = &play.view.asks[..] else {
        panic!("one question is open");
    };
    let AskBody::Permission { tool, always, .. } = &ask.body else {
        panic!("the question asks for a permission");
    };
    assert_eq!(tool.target, "~/Repos/Kodosi/prices.json");
    assert_eq!(
        always,
        &Some(AskAlways::Rule {
            tool: "read_file".to_owned(),
            text: "~/Repos/Kodosi/prices.json".to_owned()
        })
    );
}

#[test]
fn always_gives_the_agent_its_own_offer_and_the_next_call_asks_nothing() {
    let play = Play::of(record!("always"), &[ChatAnswer::Always]);

    let asked = play
        .changes
        .iter()
        .filter(|change| matches!(change, ChatChange::Asks { asks } if !asks.is_empty()))
        .count();
    assert_eq!(asked, 1, "only the first call asked");
    assert_eq!(
        play.tools()
            .iter()
            .map(|tool| tool.state)
            .collect::<Vec<_>>(),
        [ToolState::Done, ToolState::Done]
    );
    assert_eq!(play.answers(), ["27.28", "27.28"]);
}

#[test]
fn a_plan_waits_for_approval_and_a_refusal_without_words_stops_the_turn() {
    let approval = ChatAnswer::Plan {
        choice: PlanChoice::Ask,
    };
    let play = Play::of(
        record!("plan"),
        &[approval, ChatAnswer::Deny { text: None }],
    );

    let planned = play.changes.iter().any(|change| {
        matches!(change, ChatChange::Asks { asks } if asks.iter().any(|ask| {
            matches!(&ask.body, AskBody::Plan { text, choices } if text.starts_with("# Add a discount field")
                && choices == &[PlanChoice::Auto, PlanChoice::Ask])
        }))
    });
    assert!(planned, "the plan showed for approval");
    let [proposal, write] = play.tools()[..] else {
        panic!(
            "the search for a tool, the file of the plan and the call that the agent did not finish are no items"
        );
    };
    assert_eq!(
        (proposal.kind, proposal.state),
        (ToolKind::Plan, ToolState::Done)
    );
    assert_eq!(
        (write.kind, write.state),
        (ToolKind::Write, ToolState::Denied)
    );
    assert_eq!(write.target, "items.json");
    assert_eq!(play.view.facts.mode, Some(ChatMode::Ask));
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_command_of_the_chat_runs_as_in_the_terminal_and_a_session_without_checks_can_stay_so() {
    let play = Play::of(record!("plan-command"), &[]);

    assert_eq!(
        play.people(),
        [
            "/plan Work only in this folder. Add an optional discount field to each item in items.json and take it off the price in the total. Keep the plan short."
        ]
    );
    assert_eq!(play.answers(), ["Turned on plan mode"]);
    let [ask] = &play.view.asks[..] else {
        panic!("the plan waits for approval");
    };
    assert!(matches!(&ask.body, AskBody::Plan { choices, .. }
        if choices == &[PlanChoice::Alone, PlanChoice::Ask]));
    let mut agent = play.agent;
    let alone = ChatAnswer::Plan {
        choice: PlanChoice::Alone,
    };
    assert_eq!(
        agent.decision(&ask.id, &alone, None).expect("a decision")["updatedPermissions"],
        json!([{"type": "setMode", "mode": "bypassPermissions", "destination": "session"}])
    );
}

#[test]
fn a_command_that_claude_code_shows_in_its_terminal_stays_until_it_ends_there() {
    let mut play = Play::of(record!("text"), &[]);
    play.chat.queue("/config".to_owned(), Some(sam()));
    assert_eq!(play.agent.commands(&play.chat).len(), 1);

    play.take(&json!({"k": "waits", "n": 1}));
    let queued = &play.view.queued[0];
    assert!(
        queued.terminal && queued.given,
        "the command is the terminal's"
    );
    play.take(&json!({"k": "did", "n": 1}));
    assert_eq!(play.view.queued, [] as [ChatQueued; 0]);
    assert_eq!(play.people().last(), Some(&"/config"));
}

#[test]
fn the_chat_changes_the_settings_of_claude_code_and_shows_why_it_refused_one() {
    let mut play = Play::default();
    play.take(&json!({
        "k": "hello",
        "agent": "claude",
        "session": "s1",
        "settings": true,
        "commands": [{"name": "config", "description": "Open settings"}, {"name": "rewind"}],
    }));
    let commands = play
        .view
        .commands
        .iter()
        .map(|command| (command.name.as_str(), command.opens, command.terminal))
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        [
            ("config", Some(ChatControl::Settings), false),
            ("rewind", None, true)
        ]
    );

    assert_eq!(
        play.agent.settings(&mut play.chat),
        Some(json!({"do": "settings"}))
    );
    let rows = |editor: &str| {
        json!({"k": "settings", "rows": [
            {"key": "verbose", "label": "Verbose output", "kind": "boolean", "value": false, "isLocked": false},
            {"key": "editor", "label": "Editor mode", "kind": "choice", "value": editor, "options": ["normal", "vim"]},
            {"key": "timeout", "label": "Timeout", "kind": "number", "value": 30},
            {"key": "remoteControl", "label": "Remote Control", "kind": "choice", "value": "default", "options": ["true", "false", "default"], "isLocked": true},
        ]})
    };
    play.take(&rows("normal"));
    assert_eq!(play.view.settings.len(), 4);
    assert_eq!(
        play.agent.set(&mut play.chat, "remoteControl", "true"),
        None
    );
    assert_eq!(play.agent.set(&mut play.chat, "editor", "emacs"), None);
    assert_eq!(
        play.agent.set(&mut play.chat, "timeout", "45"),
        Some(json!({"do": "set", "key": "timeout", "value": 45}))
    );

    assert_eq!(
        play.agent.set(&mut play.chat, "verbose", "true"),
        Some(json!({"do": "set", "key": "verbose", "value": true}))
    );
    play.follow();
    assert_eq!(
        play.view.settings[0].value, "true",
        "the change shows at once"
    );
    let refused = "Verbose output can only be changed from the /config panel.";
    play.take(&json!({"k": "set", "key": "verbose", "deny": refused}));
    assert_eq!(
        (
            play.view.settings[0].value.as_str(),
            play.view.settings[0].refusal.as_deref()
        ),
        ("false", Some(refused))
    );

    assert!(play.agent.set(&mut play.chat, "editor", "vim").is_some());
    play.take(&json!({"k": "set", "key": "editor", "value": "vim"}));
    play.take(&rows("vim"));
    assert_eq!(play.view.settings[1].value, "vim");
    assert_eq!(play.view.settings[0].refusal.as_deref(), Some(refused));
    play.agent.settings(&mut play.chat);
    play.follow();
    assert_eq!(
        play.view.settings[0].refusal, None,
        "opening the settings anew forgets why"
    );
}

#[test]
fn a_message_that_a_stop_in_the_terminal_dropped_waits_for_the_next_turn_of_claude_code() {
    let mut play = Play::of(record!("text"), &[]);
    play.chat
        .queue("Use the staging database.".to_owned(), Some(sam()));
    let sent = json!({"do": "prompt", "n": 1, "text": "Use the staging database."});
    assert_eq!(play.agent.commands(&play.chat), [sent]);

    let stopped = "the prompt was removed from the queue before it ran";
    play.take(&json!({"k": "did", "n": 1, "drop": stopped}));
    assert_eq!(play.view.queued[0].refused, None, "a stop refuses nothing");
    assert!(
        play.agent.commands(&play.chat).is_empty(),
        "Claude Code takes nothing until a person writes to it"
    );

    play.take(&json!({"k": "turn"}));
    play.take(&json!({"k": "turnEnd", "reason": "answer"}));
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 2, "text": "Use the staging database."})]
    );
    let blocked = "a hook answered without passing the prompt on";
    play.take(&json!({"k": "did", "n": 2, "drop": blocked}));
    assert_eq!(play.view.queued[0].refused.as_deref(), Some(blocked));
}

fn events(record: &str) -> Vec<Value> {
    record
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line of the record"))
        .collect()
}

async fn report(chat: &ChatHandle, link: &str, batch: u64, events: Vec<Value>) -> bool {
    report_from(chat, link, batch, events, None).await
}

/// Reports as a module whose process runs in `session`.
async fn report_from(
    chat: &ChatHandle,
    link: &str,
    batch: u64,
    events: Vec<Value>,
    session: Option<u32>,
) -> bool {
    let (reply, heard) = tokio::sync::oneshot::channel();
    let job = Job::Events {
        link: link.to_owned(),
        batch,
        events,
        session,
        reply,
    };
    chat.jobs()
        .upgrade()
        .expect("the chat")
        .send(job)
        .await
        .ok();
    heard.await.expect("the chat answers")
}

async fn frame(ends: &mut crate::canvas::PipeEnds) -> ChatFrame {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), ends.receiver.recv())
        .await
        .expect("a frame in time")
        .expect("the pipe is open");
    serde_json::from_slice(&frame).expect("a frame of the chat")
}

#[tokio::test]
async fn the_chat_tells_the_terminal_how_many_helpers_work() {
    let chat = ChatHandle::spawn(String::new(), None);
    let record = record!("helper");
    let ended = record
        .find("{\"k\":\"turnEnd\"")
        .expect("the first turn ends");
    let first = &record[..=record[ended..].find('\n').expect("a line") + ended];
    assert!(report(&chat, "one", 1, events(record!("text"))).await);
    assert!(report(&chat, "one", 2, events(first)).await);
    assert_eq!(
        chat.linked().borrow().map(|agent| agent.helpers),
        Some(1),
        "the helper works on after the turn that started it"
    );
}

#[tokio::test]
async fn an_answer_of_a_viewer_reaches_the_agent_before_or_after_the_agent_asks_for_it() {
    let (asking, _) = record!("command-allow")
        .split_once("{\"answer\"")
        .expect("the record has a question");
    for waits_first in [true, false] {
        let chat = ChatHandle::spawn(String::new(), None);
        assert!(report(&chat, "one", 1, events(record!("text"))).await);
        assert!(report(&chat, "one", 2, events(asking)).await);
        let mut viewer = chat
            .attach("7".to_owned(), "Sam".to_owned())
            .await
            .expect("the chat opens");
        let ChatFrame::Snapshot(snapshot) = frame(&mut viewer).await else {
            panic!("the chat as it is comes first");
        };
        let ask = snapshot.asks[0].id.clone();
        let jobs = chat.jobs().upgrade().expect("the chat");
        let wait = |ask: String| {
            let (reply, asked) = tokio::sync::oneshot::channel();
            let job = Job::Ask {
                link: "one".to_owned(),
                ask,
                reply,
            };
            jobs.try_send(job).ok();
            asked
        };
        let answer = serde_json::to_vec(&ChatRequest::Answer {
            ask: ask.clone(),
            answer: ChatAnswer::Allow,
        })
        .expect("a request");

        let asked = if waits_first {
            let asked = wait(ask.clone());
            viewer.sender.send(answer).await.expect("the pipe is open");
            asked
        } else {
            viewer.sender.send(answer).await.expect("the pipe is open");
            let ChatFrame::Update(update) = frame(&mut viewer).await else {
                panic!("a change");
            };
            assert!(
                update
                    .changes
                    .contains(&ChatChange::Asks { asks: Vec::new() })
            );
            wait(ask.clone())
        };

        let Ok(Asked::Decision(decision)) = asked.await else {
            panic!("the agent gets the decision");
        };
        assert_eq!(decision, json!({"behavior": "allow"}));
        assert!(matches!(wait(ask).await, Ok(Asked::Closed)));
    }
}

fn sam() -> ChatPerson {
    ChatPerson {
        id: "sam".to_owned(),
        name: "Sam".to_owned(),
    }
}

fn calls<'a>(play: &'a Play, method: &str) -> Vec<&'a Value> {
    play.calls
        .iter()
        .filter(|call| call["m"] == method)
        .collect()
}

#[test]
fn codex_shows_its_first_turn_from_the_moment_its_server_lets_the_chat_follow() {
    let play = Play::codex(&[codex!("first")], &[]);

    assert_eq!(play.people(), ["Say hello in five words."]);
    assert_eq!(play.answers(), ["Hello there, friend. Hope you're well."]);
    assert_eq!(play.ends(), [TurnEnd::Done]);
    assert_eq!(play.view.state, ChatState::Idle);
    let facts = &play.view.facts;
    assert_eq!(facts.agent.as_deref(), Some("codex"));
    assert_eq!(facts.version.as_deref(), Some("0.171.0"));
    assert_eq!(facts.model.as_deref(), Some("gpt-6-luna"));
    assert_eq!(facts.effort.as_deref(), Some("low"));
    assert_eq!(facts.mode, Some(ChatMode::Ask));
    assert_eq!(facts.folder.as_deref(), Some("/home/sam/shop"));
    assert_eq!(
        facts.context,
        Some(ChatContext {
            tokens: 17172,
            window: 760_000
        })
    );
    assert_eq!(
        facts
            .limits
            .iter()
            .map(|limit| (limit.hours, limit.used))
            .collect::<Vec<_>>(),
        [(Some(720), 12)]
    );
    // The first `thread/resume` fails before the thread has a turn, so the adapter retries;
    // Codex's internal title-generation thread is ignored.
    let followed = calls(&play, "thread/resume");
    assert_eq!(followed.len(), 2);
    assert!(
        followed
            .iter()
            .all(|call| call["p"]["threadId"] == "00000001-0000-7000-8000-000000000000")
    );
    assert_eq!(
        play.agent.conversation(),
        Some("00000001-0000-7000-8000-000000000000"),
        "the conversation that the terminal shows"
    );
}

#[test]
fn a_change_of_a_file_by_codex_shows_its_lines_and_is_allowed_once_or_for_the_session() {
    let play = Play::codex(&[codex!("first"), codex!("edit")], &[ChatAnswer::Allow]);

    let tools = play.tools();
    let tool = tools.last().expect("the change");
    assert_eq!((tool.kind, tool.state), (ToolKind::Edit, ToolState::Done));
    assert_eq!(tool.target, "notes.md");
    assert_eq!(
        tool.detail,
        Some(ToolDetail::Change {
            path: "/home/sam/shop/notes.md".to_owned(),
            added: 1,
            removed: 1,
            lines: vec![
                line(ChangeSign::Same, "- The tax is 24 percent."),
                line(ChangeSign::Removed, "- The totl must show two decimals."),
                line(ChangeSign::Added, "- The total must show two decimals."),
            ],
        })
    );

    // `Play::more` asserts that `Always` produces the recorded session-approval decision.
    Play::codex(&[codex!("first"), codex!("session")], &[ChatAnswer::Always]);
}

fn stopped_turn() -> (String, String) {
    let record = codex!("stop");
    let runs = record
        .lines()
        .position(|line| line.contains("item/commandExecution/outputDelta"))
        .expect("the command prints");
    (
        record.lines().take(runs + 1).collect::<Vec<_>>().join("\n"),
        record.lines().skip(runs + 1).collect::<Vec<_>>().join("\n"),
    )
}

#[test]
fn a_stop_of_the_chat_ends_the_turn_of_codex_and_the_command_that_codex_lets_run() {
    let (before, after) = stopped_turn();
    let mut play = Play::codex(&[codex!("first"), &before], &[]);
    assert_eq!(play.view.state, ChatState::Working);

    let Stop::Command(stop) = play.agent.stop(&play.chat) else {
        panic!("a turn runs");
    };
    assert_eq!(stop["m"], "turn/interrupt");
    assert_eq!(stop["p"]["turnId"], "turn10");

    play.more(&after, &[]);

    let ended = calls(&play, "thread/backgroundTerminals/terminate");
    assert_eq!(ended.len(), 1, "{:?}", play.calls);
    assert_eq!(ended[0]["p"]["processId"], "process6");
    let tools = play.tools();
    let tool = tools.last().expect("the command");
    assert_eq!(tool.state, ToolState::Stopped);
    assert_eq!(
        tool.detail,
        Some(ToolDetail::Command {
            command:
                "python3 -u -c \"import time; [print(i) or time.sleep(1) for i in range(25)]\""
                    .to_owned(),
            output: "0\n1\n2\n3\n".to_owned(),
            background: false,
        })
    );
    assert_eq!(play.ends().last(), Some(&TurnEnd::Stopped));
}

#[test]
fn a_stop_in_the_terminal_leaves_the_command_of_codex_to_the_person() {
    let (before, after) = stopped_turn();
    let ended = after
        .lines()
        .position(|line| line.contains("turn/completed"))
        .expect("the end of the turn");
    let until = after.lines().take(ended + 1).collect::<Vec<_>>().join("\n");
    let play = Play::codex(&[codex!("first"), &before, "\n", &until], &[]);

    assert_eq!(
        calls(&play, "thread/backgroundTerminals/list"),
        Vec::<&Value>::new()
    );
    let tools = play.tools();
    let tool = tools.last().expect("the command");
    assert_eq!(tool.state, ToolState::Running);
    assert!(matches!(
        tool.detail,
        Some(ToolDetail::Command {
            background: true,
            ..
        })
    ));
}

#[test]
fn a_message_goes_into_the_turn_of_codex_and_starts_the_next_turn_when_the_turn_ended_first() {
    let (before, after) = stopped_turn();
    let ended = after
        .lines()
        .position(|line| line.contains("turn/completed"))
        .expect("the end of the turn");
    let until = after.lines().take(ended + 1).collect::<Vec<_>>().join("\n");
    let mut play = Play::codex(&[codex!("first"), &before], &[]);
    play.chat
        .queue("Print only the even numbers.".to_owned(), Some(sam()));

    let steer = play.agent.commands(&play.chat);
    assert_eq!(steer.len(), 1, "{steer:?}");
    assert_eq!(steer[0]["m"], "turn/steer");
    assert_eq!(
        steer[0]["p"]["threadId"],
        "00000001-0000-7000-8000-000000000000"
    );
    assert_eq!(steer[0]["p"]["expectedTurnId"], "turn10");
    assert_eq!(
        steer[0]["p"]["input"][0]["text"],
        "Print only the even numbers."
    );

    play.more(&until, &[]);
    assert_eq!(
        play.chat.queued().len(),
        1,
        "the message waits for Codex's answer"
    );
    play.take(&json!({
        "k": "reply",
        "tag": steer[0]["tag"],
        "e": {"code": -32600, "message": "no active turn to steer"},
    }));

    let start = play.agent.commands(&play.chat);
    assert_eq!(start.len(), 1, "{start:?}");
    assert_eq!(start[0]["m"], "turn/start");
    assert_eq!(
        start[0]["p"]["input"][0]["text"],
        "Print only the even numbers."
    );
}

#[cfg(unix)]
#[tokio::test]
async fn the_next_host_ends_a_server_of_codex_that_the_host_before_left() {
    use std::{os::unix::net::UnixListener, time::Duration};

    const NAME: &str =
        "chat::tests::the_next_host_ends_a_server_of_codex_that_the_host_before_left";
    const LEFT: &str = "KODOSI_TEST_LEFT_SERVER";
    if let Some(address) = std::env::var_os(LEFT) {
        let _server = UnixListener::bind(address).expect("the address is free");
        std::thread::sleep(Duration::from_secs(60));
        return;
    }
    let folder = tempfile::tempdir().expect("a folder");
    let server = folder.path().join("codex-0123456789abcdef-s.sock");
    let closed = folder.path().join("codex-0123456789abcdef.sock");
    let other = folder.path().join("agents.sock");
    drop(UnixListener::bind(&closed).expect("the address is free"));
    std::fs::write(&other, "").expect("a file");
    let mut left = tokio::process::Command::new(std::env::current_exe().expect("this program"))
        .args(["--exact", NAME, "--test-threads=1"])
        .env(LEFT, &server)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .expect("the server starts");
    tokio::time::timeout(Duration::from_secs(30), async {
        while tokio::net::UnixStream::connect(&server).await.is_err() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the server takes programs");

    codex_link::end_left_servers(folder.path()).await;

    tokio::time::timeout(Duration::from_secs(10), left.wait())
        .await
        .expect("the server ended")
        .expect("the server was a child");
    assert!(!server.exists(), "the address of the server is free again");
    assert!(
        !closed.exists(),
        "the address that the earlier host served is gone"
    );
    assert!(other.exists(), "the other addresses of the folder stay");
}

async fn greet<S>(mut stream: S, terminal: uuid::Uuid) -> String
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let body = json!({
        "v": claude::VERSION,
        "terminal": terminal.to_string(),
        "link": "greeting",
        "batch": 1,
        "events": [{
            "k": "hello",
            "t": 1,
            "agent": "claude",
            "session": "11111111-1111-4111-8111-111111111111",
        }],
    })
    .to_string();
    let request = format!(
        "POST /v1/events HTTP/1.1\r\nhost: kodosi\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("the host reads");
    let mut answer = vec![0; 256];
    let read = tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut answer))
        .await
        .expect("the host answers in time")
        .expect("the host answers");
    String::from_utf8_lossy(&answer[..read])
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

async fn linked(chat: &TerminalChat) {
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        chat.handle.linked().wait_for(Option::is_some),
    )
    .await
    .expect("the chat follows the agent in time")
    .expect("the chat is there");
}

fn address<'a>(chat: &'a TerminalChat, variable: &str) -> Option<&'a str> {
    chat.variables
        .iter()
        .find(|(name, _)| *name == variable)
        .map(|(_, address)| address.as_str())
}

#[tokio::test]
async fn an_agent_reaches_the_chat_of_its_terminal_at_the_addresses_that_the_terminal_names() {
    let root = tempfile::tempdir().expect("a data root");
    let links = Links::serve(root.path()).await;

    let terminal = uuid::Uuid::now_v7();
    let chat = links.room(terminal);
    let file = address(&chat, LINK_VARIABLE).expect("the terminal names its socket file");
    let stream = kodosi_os::ipc::connect_file(std::path::Path::new(file))
        .await
        .expect("the host serves the socket file");
    assert_eq!(greet(stream, terminal).await, "HTTP/1.1 204 No Content");
    linked(&chat).await;

    let terminal = uuid::Uuid::now_v7();
    let chat = links.room(terminal);
    let pipe = address(&chat, PIPE_VARIABLE);
    assert_eq!(pipe.is_some(), cfg!(windows));
    #[cfg(windows)]
    {
        let stream = tokio::net::windows::named_pipe::ClientOptions::new()
            .open(pipe.expect("the terminal names its pipe"))
            .expect("the host serves the pipe");
        assert_eq!(greet(stream, terminal).await, "HTTP/1.1 204 No Content");
        linked(&chat).await;
    }
}

#[test]
fn always_allows_the_commands_of_copilot_for_the_session_and_the_next_one_asks_nothing() {
    let play = Play::copilot(&[copilot!("always")], &[ChatAnswer::Always]);

    let asked = play
        .changes
        .iter()
        .filter(|change| matches!(change, ChatChange::Asks { asks } if !asks.is_empty()))
        .count();
    assert_eq!(asked, 1, "only the first call asked");
    assert_eq!(
        play.tools()
            .iter()
            .map(|tool| tool.state)
            .collect::<Vec<_>>(),
        [ToolState::Done, ToolState::Done]
    );
}

#[test]
fn copilot_shows_its_effort_mode_context_and_plan_from_its_link_and_follows_their_changes() {
    let mut play = Play::new(Box::new(Copilot::default()));
    play.take(&json!({
        "k": "hello", "agent": "copilot", "version": "1.0.117", "cwd": "/home/sam/shop",
        "model": "claude-opus-5.5", "effort": "max", "mode": "interactive",
        "permissions": "allow-all", "context": {"currentTokens": 52_000, "tokenLimit": 100_000},
    }));
    play.take(&json!({"k": "history", "events": [
        {"type": "session.model_change", "data": {"newModel": "gpt-5.4", "reasoningEffort": "low"}, "at": 1},
        {"type": "session.mode_changed", "data": {"newMode": "plan"}, "at": 2},
        {"type": "session.permissions_changed", "data": {"mode": "manual"}, "at": 3},
    ]}));
    play.take(&json!({"k": "quota", "quota": {
        "remainingPercentage": 35.4, "isUnlimitedEntitlement": false,
        "usageAllowedWithExhaustedQuota": false, "resetDateEpochMs": 4_070_908_800_000_u64,
    }}));

    let facts = &play.view.facts;
    assert_eq!(
        (facts.model.as_deref(), facts.effort.as_deref(), facts.mode),
        (
            Some("claude-opus-5.5"),
            Some("max"),
            Some(ChatMode::Unrestricted)
        ),
        "the history's older changes leave the facts of the hello"
    );
    assert_eq!(
        facts.context,
        Some(ChatContext {
            tokens: 52_000,
            window: 100_000
        })
    );
    assert_eq!(
        facts
            .limits
            .iter()
            .map(|limit| (limit.hours, limit.used, limit.resets_at_ms))
            .collect::<Vec<_>>(),
        [(None, 65, Some(4_070_908_800_000))]
    );

    play.take(
        &json!({"k": "event", "type": "session.model_change", "data": {"newModel": "gpt-4.1"}}),
    );
    play.take(
        &json!({"k": "event", "type": "session.usage_info", "data": {
            "currentTokens": 61_000, "tokenLimit": 128_000, "messagesLength": 9,
        }}),
    );
    let facts = &play.view.facts;
    assert_eq!(
        (facts.model.as_deref(), facts.effort.as_deref()),
        (Some("gpt-4.1"), None),
        "a model without an effort shows none"
    );
    assert_eq!(
        facts.context,
        Some(ChatContext {
            tokens: 61_000,
            window: 128_000
        })
    );

    let modes = [
        ("session.mode_changed", json!({"newMode": "plan"})),
        ("session.permissions_changed", json!({"mode": "manual"})),
        ("session.mode_changed", json!({"newMode": "interactive"})),
    ]
    .map(|(kind, data)| {
        play.take(&json!({"k": "event", "type": kind, "data": data}));
        play.view.facts.mode
    });
    assert_eq!(
        modes,
        [
            Some(ChatMode::Plan),
            Some(ChatMode::Plan),
            Some(ChatMode::Ask)
        ],
        "plan mode shows over the permission mode, which shows again after it"
    );
}

#[test]
fn a_message_goes_into_the_turn_of_copilot_at_once_and_a_command_waits_for_its_end() {
    let (running, _) = split_at(copilot!("always"), "\"session.idle\"");
    let mut play = Play::copilot(&[&running], &[ChatAnswer::Always]);
    play.chat
        .queue("Also count the pens.".to_owned(), Some(sam()));
    play.chat.queue("/compact".to_owned(), Some(sam()));

    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 1, "text": "Also count the pens.", "steer": true})]
    );
    play.take(&json!({
        "k": "event",
        "type": "user.message",
        "data": {"content": "Also count the pens."},
        "at": 1_795_270_669_500_u64,
    }));
    let said = play
        .view
        .items
        .iter()
        .rev()
        .find_map(|item| match &item.body {
            ChatBody::Person { text, by, .. } => Some((text.as_str(), by.clone())),
            _ => None,
        });
    assert_eq!(said, Some(("Also count the pens.", Some(sam()))));
    assert!(
        play.agent.commands(&play.chat).is_empty(),
        "a command waits for the end of the turn"
    );

    play.take(&json!({
        "k": "event",
        "type": "session.idle",
        "data": {},
        "at": 1_795_270_669_615_u64,
    }));
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 2, "text": "/compact"})]
    );
}

#[test]
fn a_stop_takes_back_the_message_that_copilot_has_not_read_and_sends_it_after_the_turn() {
    let (running, _) = split_at(copilot!("always"), "\"session.idle\"");
    let mut play = Play::copilot(&[&running], &[ChatAnswer::Always]);
    play.chat
        .queue("Also count the pens.".to_owned(), Some(sam()));
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 1, "text": "Also count the pens.", "steer": true})]
    );
    play.take(&json!({"k": "did", "n": 1}));
    assert!(play.view.queued[0].given, "Copilot has the message");

    play.stop();
    assert_eq!(play.calls.last(), Some(&json!({"do": "stop", "back": [1]})));
    play.take(&json!({"k": "did", "n": 1, "back": true}));
    assert!(!play.view.queued[0].given, "Copilot gave the message back");
    assert!(
        play.agent.commands(&play.chat).is_empty(),
        "the message waits for the end of the stopped turn"
    );

    for kind in ["abort", "session.idle"] {
        play.take(&json!({"k": "event", "type": kind, "data": {}, "at": 1_795_270_669_615_u64}));
    }
    assert_eq!(play.ends().last(), Some(&TurnEnd::Stopped));
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 2, "text": "Also count the pens."})]
    );
}

#[test]
fn codex_compacts_for_the_chat_and_leaves_a_command_of_its_terminal_to_the_person() {
    const THREAD: &str = "00000001-0000-7000-8000-000000000000";
    let mut play = Play::codex(&[codex!("first")], &[]);
    for text in ["/status", "/compact", "Count the pens."] {
        let id = play.chat.queue(text.to_owned(), Some(sam()));
        if !play.agent.takes(text) {
            play.chat.in_terminal(&id.expect("room in the queue"));
        }
    }

    let compact = play.agent.commands(&play.chat);
    assert_eq!(compact.len(), 1, "{compact:?}");
    assert_eq!(compact[0]["m"], "thread/compact/start");
    assert_eq!(compact[0]["p"], json!({"threadId": THREAD}));
    play.take(&json!({"k": "reply", "tag": compact[0]["tag"], "r": {}}));
    let turn = json!({"id": "turn20", "status": "completed"});
    let item = json!({"type": "contextCompaction", "id": "item20"});
    play.take(&json!({"k": "note", "m": "turn/started", "p": {"threadId": THREAD, "turn": turn}}));
    play.take(&json!({
        "k": "note",
        "m": "item/started",
        "p": {"threadId": THREAD, "turnId": "turn20", "item": item},
    }));
    assert_eq!(play.view.now, Some(ChatNow::Compacting));
    play.take(&json!({
        "k": "note",
        "m": "item/completed",
        "p": {"threadId": THREAD, "turnId": "turn20", "item": item},
    }));
    play.take(
        &json!({"k": "note", "m": "turn/completed", "p": {"threadId": THREAD, "turn": turn}}),
    );

    assert_eq!(play.people().last(), Some(&"/compact"));
    assert!(play.view.items.iter().any(|item| matches!(
        item.body,
        ChatBody::Note {
            note: ChatNote::Compacted
        }
    )));
    let start = play.agent.commands(&play.chat);
    assert_eq!(start.len(), 1, "{start:?}");
    assert_eq!(start[0]["m"], "turn/start");
    assert_eq!(start[0]["p"]["input"][0]["text"], "Count the pens.");
    assert_eq!(
        (
            play.view.queued[0].text.as_str(),
            play.view.queued[0].terminal
        ),
        ("/status", true)
    );
}

#[test]
fn a_message_that_codex_took_into_its_turn_is_given() {
    let (before, _) = stopped_turn();
    let mut play = Play::codex(&[codex!("first"), &before], &[]);
    play.chat
        .queue("Print only the even numbers.".to_owned(), Some(sam()));
    let steer = play.agent.commands(&play.chat);

    play.take(&json!({"k": "reply", "tag": steer[0]["tag"], "r": {"turnId": "turn10"}}));
    assert!(play.view.queued[0].given);
}

/// The lines of a record before its first line with `mark`, and the lines from there.
fn split_at(record: &str, mark: &str) -> (String, String) {
    let at = record
        .lines()
        .position(|line| line.contains(mark))
        .expect("the mark");
    let lines = record.lines().collect::<Vec<_>>();
    (lines[..at].join("\n"), lines[at..].join("\n"))
}

#[test]
fn a_person_allows_a_command_and_a_file_of_antigravity_in_the_chat() {
    let (before, after) = split_at(antigravity!("allow"), "\"answer\"");
    let mut play = Play::antigravity(&before, &[]);

    assert_eq!(play.view.state, ChatState::Asking);
    let [ask] = play.view.asks.as_slice() else {
        panic!("one question");
    };
    let AskBody::Permission { tool, always, .. } = &ask.body else {
        panic!("a permission");
    };
    assert_eq!(
        (tool.name.as_str(), tool.target.as_str(), tool.state),
        ("run_command", "ls", ToolState::Asking)
    );
    assert_eq!(*always, None, "the agent offers to allow only this call");

    play.more(&after, &[ChatAnswer::Allow, ChatAnswer::Allow]);

    let [command, write] = play.tools()[..] else {
        panic!("two calls");
    };
    assert_eq!(command.state, ToolState::Done);
    assert!(
        matches!(&command.detail, Some(ToolDetail::Command { output, .. }) if output == "a.txt"),
        "the output without the agent's times and exit code: {:?}",
        command.detail
    );
    assert_eq!(
        (write.kind, write.target.as_str(), write.state),
        (ToolKind::Write, "notes.txt", ToolState::Done)
    );
    assert!(matches!(
        &write.detail,
        Some(ToolDetail::Change { path, added: 1, removed: 0, .. }) if path == "/home/ana/work/notes.txt"
    ));
    assert_eq!(play.answers(), ["I listed the files and wrote notes.txt."]);
    assert_eq!(play.ends(), [TurnEnd::Done]);
    assert_eq!(play.view.state, ChatState::Idle);
    let facts = &play.view.facts;
    assert_eq!(
        (facts.model.as_deref(), facts.effort.as_deref(), facts.mode),
        (Some("gemini-3.1-pro"), Some("low"), Some(ChatMode::Ask))
    );
}

#[test]
fn a_restored_antigravity_loads_its_conversation_with_its_history() {
    let play = Play::antigravity(antigravity!("load"), &[]);

    let load = calls(&play, "session/load");
    assert_eq!(load.len(), 1);
    let session = load[0]["p"]["sessionId"]
        .as_str()
        .expect("the conversation");
    assert_eq!(play.agent.conversation(), Some(session));
    assert_eq!(
        play.people(),
        ["Change a.txt to two, then read missing.txt."]
    );
    assert_eq!(
        play.tools()
            .iter()
            .map(|tool| (tool.name.as_str(), tool.state))
            .collect::<Vec<_>>(),
        [
            ("read_file", ToolState::Done),
            ("edit_file", ToolState::Done),
            ("run_command", ToolState::Failed)
        ]
    );
    assert_eq!(play.answers(), ["a.txt now says two."]);
}

fn timber_elk() -> Peer {
    Peer {
        shown: ChatPeer {
            terminal: "b".to_owned(),
            name: "Timber Elk".to_owned(),
            agent: "copilot".to_owned(),
            place: "site".to_owned(),
        },
        name: "Timber Elk".to_owned(),
        agent: crate::agents::Agent::Copilot,
    }
}

/// A chat of the terminal `a` with Claude Code linked and idle, which reaches the agent of
/// Timber Elk, and a viewer of Sam that saw the chat as it is.
async fn chat_that_reaches_timber_elk() -> (
    ChatHandle,
    tokio::sync::watch::Sender<mention::Peers>,
    crate::canvas::PipeEnds,
) {
    let itself = Peer {
        shown: ChatPeer {
            terminal: "a".to_owned(),
            name: "Sandy Condor".to_owned(),
            ..timber_elk().shown
        },
        ..timber_elk()
    };
    let (peers, known) =
        tokio::sync::watch::channel(std::sync::Arc::new(vec![timber_elk(), itself]));
    let chat = ChatHandle::spawn("a".to_owned(), Some(known));
    assert!(report(&chat, "one", 1, events(record!("text"))).await);
    let mut viewer = chat
        .attach("sam".to_owned(), "Sam".to_owned())
        .await
        .expect("the chat opens");
    let ChatFrame::Snapshot(snapshot) = frame(&mut viewer).await else {
        panic!("the chat as it is comes first");
    };
    assert_eq!(
        snapshot.peers,
        [timber_elk().shown],
        "the chat lists the agents of the other terminals, not its own"
    );
    (chat, peers, viewer)
}

async fn send(viewer: &crate::canvas::PipeEnds, request: &ChatRequest) {
    let request = serde_json::to_vec(request).expect("a request");
    viewer.sender.send(request).await.expect("the pipe is open");
}

/// The text of the prompt that the link takes next.
async fn prompted(chat: &ChatHandle) -> String {
    let (reply, commands) = tokio::sync::oneshot::channel();
    let next = Job::Next {
        link: "one".to_owned(),
        reply,
    };
    chat.jobs()
        .upgrade()
        .expect("the chat")
        .send(next)
        .await
        .ok();
    let commands = commands.await.expect("the chat answers").expect("commands");
    let [command] = commands.as_slice() else {
        panic!("one command: {commands:?}");
    };
    assert_eq!(command["do"], "prompt");
    command["text"].as_str().expect("the text").to_owned()
}

/// The person's message that the viewer sees once the agent recorded `text` as its prompt.
async fn recorded(chat: &ChatHandle, viewer: &mut crate::canvas::PipeEnds, text: &str) -> ChatBody {
    let row = json!({
        "k": "row",
        "t": 1_795_226_800_000_u64,
        "door": "prompt",
        "origin": {"kind": "plugin", "name": "kodosi", "asUser": true},
        "uuid": "5f1e2d3c-4b5a-4968-8776-655443322110",
        "content": [{"type": "text", "text": text}],
    });
    assert!(report(chat, "one", 2, vec![row]).await);
    loop {
        if let ChatFrame::Update(update) = frame(viewer).await {
            for change in update.changes {
                if let ChatChange::Item { item } = change
                    && matches!(item.body, ChatBody::Person { .. })
                {
                    return item.body;
                }
            }
        }
    }
}

#[tokio::test]
async fn an_agent_learns_who_a_message_names_and_the_chat_shows_the_words_alone() {
    let (chat, _peers, mut viewer) = chat_that_reaches_timber_elk().await;
    let words = "Ask @timber elk for the schema.";
    send(
        &viewer,
        &ChatRequest::Send {
            text: words.to_owned(),
        },
    )
    .await;

    let said = prompted(&chat).await;
    assert_eq!(
        said,
        format!(
            "{words}\n\n[Kodosi] @Timber Elk is Copilot CLI in a Kodosi terminal. Details: kodosi terminal info \"Timber Elk\""
        )
    );
    assert_eq!(
        recorded(&chat, &mut viewer, &said).await,
        ChatBody::Person {
            text: words.to_owned(),
            by: Some(sam()),
            mentions: vec![ChatMention {
                name: "Timber Elk".to_owned(),
                agent: "copilot".to_owned(),
            }],
            handoff: None,
        }
    );
}

#[tokio::test]
async fn a_hand_off_gives_the_agent_the_work_of_another_and_the_chat_shows_whose() {
    let (chat, _peers, mut viewer) = chat_that_reaches_timber_elk().await;
    send(
        &viewer,
        &ChatRequest::HandOff {
            terminal: "a".to_owned(),
        },
    )
    .await;
    send(
        &viewer,
        &ChatRequest::HandOff {
            terminal: "b".to_owned(),
        },
    )
    .await;

    let said = prompted(&chat).await;
    assert_eq!(
        said,
        "[Kodosi] Hand-off: continue the work of @Timber Elk, Copilot CLI in a Kodosi terminal. Read its conversation: kodosi terminal read \"Timber Elk\" --conversation",
        "the work of the agent itself is no hand-off"
    );
    assert_eq!(
        recorded(&chat, &mut viewer, &said).await,
        ChatBody::Person {
            text: String::new(),
            by: Some(sam()),
            mentions: Vec::new(),
            handoff: Some(ChatMention {
                name: "Timber Elk".to_owned(),
                agent: "copilot".to_owned(),
            }),
        }
    );
}

#[tokio::test]
async fn the_chat_follows_the_agents_that_its_agent_can_reach() {
    let (_chat, peers, mut viewer) = chat_that_reaches_timber_elk().await;
    peers.send_replace(std::sync::Arc::new(Vec::new()));
    let ChatFrame::Update(update) = frame(&mut viewer).await else {
        panic!("a change");
    };
    assert_eq!(update.changes, [ChatChange::Peers { peers: Vec::new() }]);
}

#[test]
fn a_command_keeps_its_words_and_a_message_that_names_nobody_reaches_the_agent_as_written() {
    for words in [
        "/review @Timber Elk",
        "Email @timberelk about it",
        "Ask @Timber Elkhound",
    ] {
        assert_eq!(
            mention::Message::naming(words.to_owned(), &[timber_elk()]),
            words.to_owned().into(),
            "{words}"
        );
    }
}

#[test]
fn another_agent_reads_the_conversation_as_its_chat_shows_it() {
    let hello = record!("text").lines().next().expect("the link says hello");
    let mut play = Play::of(hello, &[]);
    play.more(record!("command-allow"), &[ChatAnswer::Allow]);

    assert_eq!(
        transcript(&play.view).as_deref(),
        Some(
            "User: Run python3 app.py and tell me the total in one short sentence.\n  ran python3 app.py\n\nClaude Code: The total is 33.83, which is 22.00 with the 1.24 tax applied twice (the correct single-tax total would be 27.28)."
        )
    );
    assert_eq!(transcript(&ChatSnapshot::default()), None);
}
