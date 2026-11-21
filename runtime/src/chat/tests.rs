//! The records in `fixtures/claude` are what the link module sent from real sessions of Claude Code.
//! Those in `fixtures/codex` are the messages of Codex's app-server in real sessions, as the link
//! gives them to the chat, without the messages that the chat does not read.
//!
//! The names of private places are replaced. A line with `answer` holds the decision that the
//! recording gave the agent for a question, which the agent then took.

use serde_json::{Value, json};

use super::{
    agent::{Agent, Stop},
    claude::Claude,
    codex::Codex,
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

/// A chat that takes a record, and a viewer that follows the chat's changes as an app does.
struct Play {
    chat: Chat,
    agent: Box<dyn Agent>,
    view: ChatSnapshot,
    closed: Vec<String>,
    changes: Vec<ChatChange>,
    /// What Codex's adapter gave its link to do.
    calls: Vec<Value>,
    /// The person who gives the answers of the play.
    answers_by: Option<ChatPerson>,
}

impl Default for Play {
    fn default() -> Self {
        Self::new(Box::new(Claude::default()))
    }
}

impl Play {
    fn new(agent: Box<dyn Agent>) -> Self {
        Self {
            chat: Chat::default(),
            agent,
            view: ChatSnapshot::default(),
            closed: Vec::new(),
            changes: Vec::new(),
            calls: Vec::new(),
            answers_by: None,
        }
    }

    /// Plays records of Codex after the start of its link. As the chat's room does after each
    /// message, the play takes what the adapter gives the link to do.
    fn codex(records: &[&str], answers: &[ChatAnswer]) -> Self {
        let mut play = Self::new(Box::new(Codex::default()));
        play.answers_by = Some(sam());
        play.more(codex!("start"), &[]);
        let records = records.join("");
        play.more(&records, answers);
        play
    }

    /// Plays a record of Claude Code. Each question gets the next of `answers`, and the decision
    /// that the answer makes must be the one that the agent took in the recording.
    fn of(record: &str, answers: &[ChatAnswer]) -> Self {
        let mut play = Self::default();
        play.more(record, answers);
        play
    }

    /// As the chat's room does for each request of the module that it follows, the play first
    /// counts the module as present.
    fn more(&mut self, record: &str, answers: &[ChatAnswer]) {
        self.agent.back(&mut self.chat);
        let mut answers = answers.iter();
        for line in record.lines() {
            let line: Value = serde_json::from_str(line).expect("a line of the record");
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
                None => self.take(&line),
            }
            if self.agent.name() == Codex::NAME {
                let calls = self.agent.commands(&self.chat);
                self.calls.extend(calls);
            }
            self.follow();
        }
        assert!(answers.next().is_none(), "an answer had no question");
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
                ChatChange::Facts { facts } => self.view.facts = (**facts).clone(),
                ChatChange::State { state, now } => {
                    self.view.state = *state;
                    self.view.now.clone_from(now);
                }
                ChatChange::Reset => {
                    self.view.items.clear();
                    self.view.asks.clear();
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

#[test]
fn an_answer_grows_in_one_item_and_ends_as_the_agent_stored_it() {
    let play = Play::of(record!("text"), &[]);

    assert_eq!(play.people(), ["Say hello in five words."]);
    assert_eq!(play.answers(), ["Hello, nice to meet you!"]);
    assert_eq!(play.ends(), [TurnEnd::Done]);
    assert_eq!(play.view.state, ChatState::Idle);
    let grew = play.changes.iter().any(|change| {
        matches!(change, ChatChange::Item { item } if matches!(item.body, ChatBody::Text { live: true, .. }))
    });
    assert!(grew, "the answer showed while the agent wrote it");
    let facts = &play.view.facts;
    assert_eq!(facts.agent.as_deref(), Some("claude"));
    assert_eq!(facts.model.as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(facts.effort.as_deref(), Some("medium"));
    assert_eq!(facts.mode, Some(ChatMode::Ask));
    assert_eq!(facts.folder.as_deref(), Some("/home/sam/shop"));
    assert_eq!(
        facts.context,
        Some(ChatContext {
            tokens: 30740,
            window: 1_000_000
        })
    );
    assert_eq!(
        facts
            .limits
            .iter()
            .map(|limit| (limit.hours, limit.used))
            .collect::<Vec<_>>(),
        [(Some(168), 18)]
    );
    assert!(facts.models.iter().any(|model| model == "opus"));
}

#[test]
fn a_command_waits_for_its_answer_then_runs_and_shows_its_output() {
    let mut play = Play::default();
    let (before, after) = record!("command-allow")
        .split_once("{\"answer\"")
        .expect("the record has a question");
    play.more(before, &[]);

    assert_eq!(play.view.state, ChatState::Asking);
    let [ask] = &play.view.asks[..] else {
        panic!("one question is open");
    };
    let AskBody::Permission { tool, always, .. } = &ask.body else {
        panic!("the question asks for a permission");
    };
    assert_eq!(
        (tool.kind, tool.target.as_str()),
        (ToolKind::Command, "python3 app.py")
    );
    assert_eq!(
        always,
        &Some(AskAlways::Rule {
            tool: "Bash".to_owned(),
            text: "python3 app.py".to_owned()
        })
    );
    assert_eq!(play.tools()[0].state, ToolState::Asking);
    assert_eq!(
        ask.item.as_ref(),
        play.view.items.last().map(|item| &item.id)
    );

    play.more(&format!("{{\"answer\"{after}"), &[ChatAnswer::Allow]);

    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
    assert_eq!(
        play.closed,
        [] as [String; 0],
        "the chat's own decision closed the question"
    );
    let [command] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(command.state, ToolState::Done);
    assert_eq!(command.ms, Some(46));
    assert_eq!(
        command.detail,
        Some(ToolDetail::Command {
            command: "python3 app.py".to_owned(),
            output: "Mug         12.00\nNotebook     7.50\nPen          2.50\nTotal       33.83"
                .to_owned(),
            background: false,
        })
    );
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn the_words_of_a_refusal_go_to_the_agent_as_the_persons_and_the_change_shows_as_refused() {
    let refusal = ChatAnswer::Deny {
        text: Some(
            "Do not change the file. Say in one short sentence what you would change.".to_owned(),
        ),
    };
    let play = Play::of(record!("edit-deny"), &[refusal]);

    let [edit] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!((edit.kind, edit.state), (ToolKind::Edit, ToolState::Denied));
    assert_eq!(edit.target, "app.py");
    assert_eq!(
        edit.detail,
        Some(ToolDetail::Change {
            path: "/home/sam/shop/app.py".to_owned(),
            added: 1,
            removed: 1,
            lines: vec![
                line(
                    ChangeSign::Removed,
                    "    subtotal = sum(item[\"price\"] for item in items) * TAX"
                ),
                line(
                    ChangeSign::Added,
                    "    subtotal = sum(item[\"price\"] for item in items)"
                ),
                line(ChangeSign::Same, "    return subtotal * TAX"),
            ],
        })
    );
    assert_eq!(play.ends(), [TurnEnd::Done]);
    assert!(
        play.answers()[0].starts_with("I would remove the `* TAX`"),
        "the agent followed the person's words"
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
fn an_answer_in_the_terminal_closes_the_question_of_the_chat() {
    let mut play = Play::default();
    let (asking, decided) = record!("terminal-allow")
        .split_once("{\"k\":\"decided\"")
        .expect("the record has a decision");
    play.more(asking, &[]);
    let ask = play.view.asks[0].id.clone();
    assert_eq!(play.closed, [] as [String; 0]);

    let (decision, running) = format!("{{\"k\":\"decided\"{decided}")
        .split_once('\n')
        .map(|(decision, rest)| (decision.to_owned(), rest.to_owned()))
        .expect("more after the decision");
    play.more(&decision, &[]);

    assert_eq!(play.closed, [ask]);
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
    assert_eq!(play.tools()[0].state, ToolState::Running);
    assert_eq!(play.view.state, ChatState::Working);
    assert!(matches!(play.view.now, Some(ChatNow::Tool { .. })));

    play.more(&running, &[]);
    assert_eq!(play.tools()[0].state, ToolState::Done);
    assert_eq!(play.answers(), ["It printed `finished`."]);
}

#[test]
fn a_refusal_in_the_terminal_shows_the_call_as_refused() {
    let play = Play::of(record!("terminal-refuse"), &[]);

    assert_eq!(play.closed.len(), 1);
    assert_eq!(play.tools()[0].state, ToolState::Denied);
    assert_eq!(play.view.state, ChatState::Idle);
    assert_eq!(play.answers(), [] as [&str; 0]);
}

#[test]
fn a_skill_a_read_and_a_change_show_as_their_kinds() {
    let play = Play::of(record!("skill"), &[ChatAnswer::Allow]);

    let [skill, read, edit] = play.tools()[..] else {
        panic!("three calls");
    };
    assert_eq!(
        (skill.kind, skill.target.as_str()),
        (ToolKind::Skill, "tidy-notes")
    );
    assert_eq!(
        (read.kind, read.target.as_str()),
        (ToolKind::Read, "notes.md")
    );
    assert_eq!(
        read.detail, None,
        "the chat does not repeat a file that the agent read"
    );
    assert_eq!((edit.kind, edit.state), (ToolKind::Edit, ToolState::Done));
    assert!(matches!(
        &edit.detail,
        Some(ToolDetail::Change {
            added: 1,
            removed: 1,
            ..
        })
    ));
    assert_eq!(play.people(), ["Tidy the notes."]);
}

#[test]
fn a_helper_shows_in_its_call_and_its_end_starts_no_message_of_a_person() {
    let mut play = Play::default();
    let record = record!("helper");
    let ended = record
        .find("{\"k\":\"turnEnd\"")
        .expect("the first turn ends");
    let (first, rest) = record.split_at(record[ended..].find('\n').expect("a line") + ended + 1);
    play.more(first, &[]);

    let helper = play.tools()[0];
    assert_eq!(
        (helper.kind, helper.state),
        (ToolKind::Agent, ToolState::Running)
    );
    assert_eq!(helper.target, "Find tax rate file");
    assert_eq!(play.view.facts.helpers, 1);
    assert_eq!(play.view.state, ChatState::Idle, "the main turn ended");
    assert_eq!(play.view.facts.background, 1);

    play.more(rest, &[]);

    let [helper] = play.tools()[..] else {
        panic!("the helper's own calls are no items");
    };
    assert_eq!(helper.state, ToolState::Done);
    let Some(ToolDetail::Agent {
        name,
        steps,
        doing,
        said,
    }) = &helper.detail
    else {
        panic!("the call of a helper");
    };
    assert_eq!((name.as_str(), *steps), ("Explore", 1));
    assert_eq!(
        doing.as_ref().map(|step| step.kind),
        Some(ToolKind::Command)
    );
    assert!(said.starts_with("The tax rate is defined in `/home/sam/shop/app.py`"));
    assert_eq!(play.view.facts.helpers, 0);
    assert_eq!(
        play.people(),
        [
            "Use the Explore subagent to find which file holds the tax rate, then tell me in one short sentence."
        ]
    );
    assert_eq!(
        play.answers().last(),
        Some(&"The tax rate is in `app.py`, where line 3 sets `TAX = 1.24`.")
    );
}

#[test]
fn a_question_takes_the_chosen_label_and_the_call_shows_the_answer() {
    let mut play = Play::default();
    let (before, after) = record!("question")
        .split_once("{\"answer\"")
        .expect("the record has a question");
    play.more(before, &[]);
    let AskBody::Question { questions } = &play.view.asks[0].body else {
        panic!("a question with choices");
    };
    assert_eq!(questions[0].question, "Which currency should be used?");
    assert_eq!(
        questions[0]
            .options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>(),
        ["EUR", "USD"]
    );

    let chosen = ChatAnswer::Answers {
        answers: vec![vec!["EUR".to_owned()]],
    };
    play.more(&format!("{{\"answer\"{after}"), &[chosen]);

    let [question] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(question.state, ToolState::Done);
    let Some(ToolDetail::Question { answers, .. }) = &question.detail else {
        panic!("the call of a question");
    };
    assert_eq!(answers, &["EUR"]);
    assert_eq!(play.answers(), ["You chose EUR."]);
}

#[test]
fn a_plan_waits_for_approval_and_a_refusal_without_words_stops_the_turn() {
    let approval = ChatAnswer::Approve { asks: true };
    let play = Play::of(
        record!("plan"),
        &[approval, ChatAnswer::Deny { text: None }],
    );

    let planned = play.changes.iter().any(|change| {
        matches!(change, ChatChange::Asks { asks } if asks.iter().any(|ask| {
            matches!(&ask.body, AskBody::Plan { text } if text.starts_with("# Add a discount field"))
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
fn calls_of_one_step_keep_their_order_and_a_failed_command_shows_its_error() {
    let play = Play::of(record!("parallel-fail"), &[ChatAnswer::Allow]);

    let [items, notes, command] = play.tools()[..] else {
        panic!("three calls");
    };
    assert_eq!(
        (items.target.as_str(), items.state),
        ("items.json", ToolState::Done)
    );
    assert_eq!(
        (notes.target.as_str(), notes.state),
        ("notes.md", ToolState::Done)
    );
    assert_eq!(command.state, ToolState::Failed);
    let Some(ToolDetail::Command { output, .. }) = &command.detail else {
        panic!("a command");
    };
    assert!(output.starts_with("Exit code 2"));
}

#[test]
fn a_stop_in_the_terminal_ends_the_command_and_the_turn_as_stopped() {
    let play = Play::of(record!("stop-terminal"), &[ChatAnswer::Allow]);

    assert_eq!(play.tools()[0].state, ToolState::Stopped);
    assert_eq!(play.ends(), [TurnEnd::Stopped]);
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_stop_of_the_chat_ends_the_turn_and_the_command_that_the_end_moved_to_the_background() {
    let mut play = Play::default();
    let (ending, stopped) = record!("stop-chat")
        .split_once("{\"k\":\"halted\"")
        .expect("the module stopped the command");
    play.more(ending, &[ChatAnswer::Allow]);

    assert_eq!(play.ends(), [TurnEnd::Stopped]);
    let command = play.tools()[0];
    assert_eq!(command.state, ToolState::Done);
    assert!(
        matches!(
            &command.detail,
            Some(ToolDetail::Command {
                background: true,
                ..
            })
        ),
        "Claude Code lets the command run on"
    );

    play.more(&format!("{{\"k\":\"halted\"{stopped}"), &[]);

    let command = play.tools()[0];
    assert_eq!(command.state, ToolState::Stopped);
    assert!(matches!(
        &command.detail,
        Some(ToolDetail::Command {
            background: false,
            ..
        })
    ));
}

#[test]
fn a_message_that_the_agent_refuses_stays_in_the_queue_until_the_agent_takes_messages_again() {
    let mut play = Play::of(record!("text"), &[]);
    play.chat.queue("Say the word third.".to_owned(), None);
    let first = play.agent.commands(&play.chat);
    assert_eq!(first[0]["n"], 1, "the agent is idle");
    let (refusal, typed) = record!("refused")
        .split_once('\n')
        .expect("a refusal, then a message in the terminal");

    play.more(refusal, &[]);

    assert_eq!(
        play.view.queued[0].refused.as_deref(),
        Some("the session was stopped and nobody has sent input since")
    );
    assert_eq!(
        play.agent.commands(&play.chat),
        Vec::<Value>::new(),
        "the agent takes the next message only in its terminal"
    );

    play.more(typed, &[]);

    assert_eq!(play.people().last(), Some(&"Say ok."));
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 2, "text": "Say the word third."})]
    );
}

#[test]
fn a_cleared_conversation_empties_the_chat() {
    let mut play = Play::of(record!("text"), &[]);
    assert_ne!(play.view.items, [] as [ChatItem; 0]);
    play.changes.clear();

    play.more(record!("clear"), &[]);

    assert_eq!(
        play.view.items,
        [] as [ChatItem; 0],
        "the record of the command is no message"
    );
    assert!(matches!(play.changes.first(), Some(ChatChange::Reset)));
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn claude_code_that_starts_again_in_the_terminal_starts_the_chat_again() {
    let mut play = Play::of(record!("text"), &[]);
    assert_eq!(play.people(), ["Say hello in five words."]);
    let again = record!("text")
        .lines()
        .next()
        .expect("the greeting")
        .replace("11111111-1111-4111-8111-111111111111", "another");

    play.more(&again, &[]);

    assert_eq!(play.view.items, [] as [ChatItem; 0]);
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_module_that_loads_again_adds_no_row_twice() {
    let play = Play::of(record!("reload"), &[]);

    assert_eq!(play.people(), ["Say hello in five words.", "Say ok."]);
    assert_eq!(play.answers(), ["Hello, nice to meet you!", "ok"]);
}

#[test]
fn a_session_that_resumes_shows_its_earlier_rows() {
    let record = record!("reload");
    let earlier = record
        .lines()
        .filter(|line| line.contains("\"k\":\"hello\"") || line.contains("\"k\":\"history\""))
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");

    let play = Play::of(&earlier, &[]);

    assert_eq!(play.people(), ["Say hello in five words."]);
    assert_eq!(play.answers(), ["Hello, nice to meet you!"]);
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_message_of_the_chat_waits_for_the_end_of_the_turn_and_shows_who_sent_it() {
    let mut play = Play::default();
    let record = record!("text");
    let (running, ending) = record
        .split_once("{\"k\":\"turnEnd\"")
        .expect("the turn ends");
    play.more(running, &[]);
    let sam = ChatPerson {
        id: "7".to_owned(),
        name: "Sam".to_owned(),
    };
    play.chat
        .queue("And in French?".to_owned(), Some(sam.clone()));

    assert_eq!(
        play.agent.commands(&play.chat),
        Vec::<Value>::new(),
        "a turn runs"
    );

    play.more(&format!("{{\"k\":\"turnEnd\"{ending}"), &[]);
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 1, "text": "And in French?"})]
    );
    assert_eq!(
        play.agent.commands(&play.chat),
        Vec::<Value>::new(),
        "Claude Code has the message"
    );
    assert_eq!(play.view.queued.len(), 1);

    let row = record
        .lines()
        .find(|line| line.contains("\"door\":\"prompt\""))
        .expect("the row of a message")
        .replace("Say hello in five words.", "And in French?")
        .replace("\"uuid\":\"", "\"uuid\":\"next-");
    play.more(&row, &[]);

    assert_eq!(play.view.queued, [] as [ChatQueued; 0]);
    assert_eq!(
        play.view.items.last().map(|item| &item.body),
        Some(&ChatBody::Person {
            text: "And in French?".to_owned(),
            by: Some(sam),
        })
    );
}

#[test]
fn a_stop_refuses_the_open_question_and_else_ends_the_turn_through_the_module() {
    let mut play = Play::default();
    let (asking, _) = record!("command-allow")
        .split_once("{\"answer\"")
        .expect("the record has a question");
    play.more(asking, &[]);
    let Stop::Decisions(decisions) = play.agent.stop(&play.chat) else {
        panic!("the question takes the stop");
    };
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].0, play.view.asks[0].id);
    assert_eq!(decisions[0].1["behavior"], "deny");
    assert_eq!(decisions[0].1["interrupt"], true);

    let mut play = Play::default();
    let (running, _) = record!("text")
        .split_once("{\"k\":\"turnEnd\"")
        .expect("the turn ends");
    play.more(running, &[]);
    assert!(matches!(
        play.agent.stop(&play.chat),
        Stop::Command(command) if command == json!({"do": "stop"})
    ));

    let mut play = Play::of(record!("text"), &[]);
    assert!(
        matches!(play.agent.stop(&play.chat), Stop::Nothing),
        "no turn runs"
    );
}

fn events(record: &str) -> Vec<Value> {
    record
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line of the record"))
        .collect()
}

async fn report(chat: &ChatHandle, link: &str, batch: u64, events: Vec<Value>) -> bool {
    let (reply, heard) = tokio::sync::oneshot::channel();
    let job = Job::Events {
        link: link.to_owned(),
        batch,
        events,
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
async fn an_answer_of_a_viewer_reaches_the_agent_before_or_after_the_agent_asks_for_it() {
    let (asking, _) = record!("command-allow")
        .split_once("{\"answer\"")
        .expect("the record has a question");
    for waits_first in [true, false] {
        let chat = ChatHandle::spawn();
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

#[tokio::test]
async fn a_message_and_a_stop_of_a_viewer_reach_the_module_that_asks_for_commands() {
    let chat = ChatHandle::spawn();
    assert!(report(&chat, "one", 1, events(record!("text"))).await);
    let viewer = chat
        .attach("7".to_owned(), "Sam".to_owned())
        .await
        .expect("the chat opens");
    let jobs = chat.jobs().upgrade().expect("the chat");
    let next = || {
        let (reply, commands) = tokio::sync::oneshot::channel();
        let job = Job::Next {
            link: "one".to_owned(),
            reply,
        };
        jobs.try_send(job).ok();
        commands
    };
    let send = |request: ChatRequest| serde_json::to_vec(&request).expect("a request");

    let waiting = next();
    let message = send(ChatRequest::Send {
        text: "And in French?".to_owned(),
    });
    viewer.sender.send(message).await.expect("the pipe is open");
    assert_eq!(
        waiting.await,
        Ok(Some(vec![
            json!({"do": "prompt", "n": 1, "text": "And in French?"})
        ]))
    );

    let (running, _) = record!("command-allow")
        .split_once("{\"k\":\"ask\"")
        .expect("the record has a question");
    assert!(report(&chat, "one", 2, events(running)).await);
    viewer
        .sender
        .send(send(ChatRequest::Stop))
        .await
        .expect("the pipe is open");
    assert_eq!(next().await, Ok(Some(vec![json!({"do": "stop"})])));
}

#[tokio::test]
async fn a_batch_that_the_module_sends_again_changes_the_chat_once() {
    let chat = ChatHandle::spawn();
    assert!(report(&chat, "one", 1, events(record!("text"))).await);
    assert!(report(&chat, "one", 1, events(record!("text"))).await);

    let mut viewer = chat
        .attach(String::new(), String::new())
        .await
        .expect("the chat opens");
    let ChatFrame::Snapshot(snapshot) = frame(&mut viewer).await else {
        panic!("the chat as it is comes first");
    };
    assert_eq!(snapshot.items.len(), 3);
}

#[tokio::test]
async fn reports_of_a_module_that_the_chat_does_not_follow_are_refused() {
    let chat = ChatHandle::spawn();
    assert!(report(&chat, "new", 1, events(record!("text"))).await);

    let late = events(record!("command-allow"));
    assert!(!report(&chat, "old", 9, late).await);

    let mut viewer = chat
        .attach(String::new(), String::new())
        .await
        .expect("the chat opens");
    let ChatFrame::Snapshot(snapshot) = frame(&mut viewer).await else {
        panic!("the chat as it is comes first");
    };
    assert_eq!(
        snapshot.items.len(),
        3,
        "only the rows of the followed module"
    );
}

fn sam() -> ChatPerson {
    ChatPerson {
        id: "sam".to_owned(),
        name: "Sam".to_owned(),
    }
}

/// The calls of the play to one method of Codex's app-server.
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
        facts.models,
        ["gpt-6-luna", "gpt-5.6-terra", "gpt-5.6-luna"]
    );
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
    // The server refuses the first request to follow: the conversation has no turn yet. The
    // conversation that Codex makes to name this one is not followed.
    let followed = calls(&play, "thread/resume");
    assert_eq!(followed.len(), 2);
    assert!(
        followed
            .iter()
            .all(|call| call["p"]["threadId"] == "00000001-0000-7000-8000-000000000000")
    );
}

#[test]
fn a_command_of_codex_shows_as_the_agent_asked_for_it_with_what_it_printed() {
    let play = Play::codex(&[codex!("first"), codex!("read")], &[]);

    let tools = play.tools();
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool.target.as_str())
            .collect::<Vec<_>>(),
        [
            "pwd && rg --files -g 'app.py' -g 'notes.md'",
            "cat app.py notes.md"
        ]
    );
    assert!(tools.iter().all(|tool| tool.state == ToolState::Done));
    let Some(ToolDetail::Command { output, .. }) = &tools[1].detail else {
        panic!("a command");
    };
    assert!(output.contains("TAX = 1.24"), "{output}");
    assert_eq!(play.answers().last(), Some(&"The tax rate is 24%."));
}

#[test]
fn a_person_allows_a_command_of_codex_in_the_chat() {
    let record = codex!("allow");
    let asks = record
        .lines()
        .position(|line| line.contains("\"k\":\"request\""))
        .expect("the request");
    let before = record.lines().take(asks + 1).collect::<Vec<_>>().join("\n");
    let mut play = Play::codex(&[codex!("first"), &before], &[]);

    assert_eq!(play.view.state, ChatState::Asking);
    let [ask] = play.view.asks.as_slice() else {
        panic!("one question");
    };
    let AskBody::Permission { tool, always, .. } = &ask.body else {
        panic!("a permission");
    };
    assert_eq!(tool.target, "printf 'hi\\n' > hello.txt");
    assert_eq!(tool.state, ToolState::Asking);
    assert_eq!(
        *always,
        Some(AskAlways::Rule {
            tool: "shell".to_owned(),
            text: "printf 'hi\\n' > hello.txt".to_owned()
        })
    );

    let after = record.lines().skip(asks + 1).collect::<Vec<_>>().join("\n");
    play.more(&after, &[ChatAnswer::Allow]);

    assert_eq!(play.view.asks, []);
    assert_eq!(play.closed, Vec::<String>::new(), "the chat decided");
    assert_eq!(
        play.tools().last().map(|tool| tool.state),
        Some(ToolState::Done)
    );
    assert_eq!(play.answers().last(), Some(&"Done."));
}

#[test]
fn a_refusal_with_words_ends_the_turn_of_codex_and_the_words_are_its_next_message() {
    let words = "Do not write the file. Say only the word: skipped";
    let play = Play::codex(
        &[codex!("first"), codex!("refuse")],
        &[ChatAnswer::Deny {
            text: Some(words.to_owned()),
        }],
    );

    assert_eq!(
        play.tools().last().map(|tool| tool.state),
        Some(ToolState::Denied)
    );
    assert_eq!(
        play.ends(),
        [TurnEnd::Done, TurnEnd::Stopped, TurnEnd::Done]
    );
    let sent = calls(&play, "turn/start");
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["p"]["input"][0]["text"], words);
    assert!(matches!(
        play.view.items.iter().rev().find_map(|item| match &item.body {
            ChatBody::Person { text, by } => Some((text.as_str(), by.clone())),
            _ => None,
        }),
        Some((said, Some(by))) if said == words && by == sam()
    ));
    assert_eq!(play.answers().last(), Some(&"skipped"));
}

#[test]
fn an_answer_in_the_terminal_closes_the_question_of_codex_in_the_chat() {
    let play = Play::codex(&[codex!("first"), codex!("terminal")], &[]);

    assert_eq!(play.closed, ["r2"]);
    assert_eq!(play.view.asks, []);
    assert_eq!(
        play.tools().last().map(|tool| tool.state),
        Some(ToolState::Done)
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

    // The record holds Codex's own word for "from now on" for these files.
    Play::codex(&[codex!("first"), codex!("session")], &[ChatAnswer::Always]);
}

/// The record of a turn that ended while its command ran, cut after the command's first output.
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
fn the_model_and_the_mode_of_codex_show_as_its_server_reports_them() {
    let mut play = Play::codex(&[codex!("first"), codex!("model")], &[]);
    assert_eq!(play.view.facts.model.as_deref(), Some("gpt-5.6-luna"));
    assert_eq!(play.view.facts.mode, Some(ChatMode::Ask));

    play.more(codex!("settings"), &[]);
    assert_eq!(play.view.facts.model.as_deref(), Some("gpt-6-luna"));
}

#[test]
fn a_new_conversation_of_codex_in_the_terminal_starts_the_chat_again() {
    let play = Play::codex(&[codex!("first"), codex!("new")], &[]);

    assert_eq!(play.view.items, []);
    assert_eq!(
        calls(&play, "thread/unsubscribe")
            .iter()
            .map(|call| &call["p"]["threadId"])
            .collect::<Vec<_>>(),
        ["00000001-0000-7000-8000-000000000000"]
    );
    assert_eq!(
        calls(&play, "thread/resume")
            .last()
            .map(|call| &call["p"]["threadId"]),
        Some(&json!("00000003-0000-7000-8000-000000000000"))
    );
}

#[test]
fn a_conversation_of_codex_that_a_person_resumes_shows_its_last_turns_in_order() {
    let play = Play::codex(&[codex!("history")], &[]);

    assert_eq!(
        play.people(),
        [
            "Run this exact shell command and nothing else: python3 -c \"import time; time.sleep(43); print('finished')\"",
            "Run this exact shell command and nothing else. It is safe and takes 25 seconds: python3 -u -c \"import time; [print(i) or time.sleep(1) for i in range(25)]\"",
            "Say only the word: fourth",
            "I want to pick a currency. Ask me with the request_user_input tool which currency to use: EUR or USD. After my answer, say only the chosen currency.",
        ]
    );
    assert_eq!(
        play.ends(),
        [
            TurnEnd::Done,
            TurnEnd::Stopped,
            TurnEnd::Done,
            TurnEnd::Done
        ]
    );
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_message_of_the_chat_goes_to_codex_when_no_turn_runs_and_shows_with_its_person() {
    let mut play = Play::codex(&[], &[]);
    let sam = sam();
    let queued = play
        .chat
        .queue("Say hello in five words.".to_owned(), Some(sam.clone()))
        .expect("room in the queue");

    let sent = play.agent.commands(&play.chat);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["m"], "turn/start");
    assert_eq!(sent[0]["p"]["input"][0]["text"], "Say hello in five words.");
    assert!(play.agent.has(&queued), "Codex has the message");
    assert_eq!(play.agent.commands(&play.chat), Vec::<Value>::new());

    play.more(codex!("first"), &[]);

    assert_eq!(play.view.queued, []);
    assert!(play.view.items.iter().any(|item| matches!(
        &item.body,
        ChatBody::Person { text, by } if text == "Say hello in five words." && by.as_ref() == Some(&sam)
    )));
}

/// The first change of the chat that has what the test waits for.
#[cfg(unix)]
async fn change<T>(
    viewer: &mut crate::canvas::PipeEnds,
    wanted: impl Fn(&ChatChange) -> Option<T>,
) -> T {
    loop {
        if let ChatFrame::Update(update) = frame(viewer).await
            && let Some(found) = update.changes.iter().find_map(&wanted)
        {
            return found;
        }
    }
}

/// The messages of a record of Codex as its server sends them on a connection.
#[cfg(unix)]
fn server_says(record: &str) -> Vec<Value> {
    events(record)
        .into_iter()
        .filter_map(|event| match event["k"].as_str()? {
            "note" => Some(json!({"method": event["m"], "params": event["p"]})),
            "request" => {
                Some(json!({"method": event["m"], "id": event["id"], "params": event["p"]}))
            }
            _ => None,
        })
        .collect()
}

/// The answer of a record to one call of the adapter.
#[cfg(unix)]
fn answer_of(record: &str, tag: &str) -> Value {
    events(record)
        .into_iter()
        .find(|event| {
            event["tag"]
                .as_str()
                .is_some_and(|given| given.starts_with(tag))
                && !event["r"].is_null()
        })
        .map(|event| event["r"].clone())
        .expect("the record has the answer")
}

#[cfg(unix)]
#[tokio::test]
async fn the_link_of_codex_carries_the_servers_messages_to_the_chat_and_a_persons_decision_to_the_server()
 {
    use futures_util::{SinkExt as _, StreamExt as _};
    use tokio_tungstenite::tungstenite::Message;

    let chat = ChatHandle::spawn();
    let room = chat.jobs().upgrade().expect("the chat");
    let (ours, theirs) = tokio::net::UnixStream::pair().expect("two connected sockets");
    let left = tokio_util::sync::CancellationToken::new();
    let link = tokio::spawn(codex_link::report(ours, room, left.clone()));
    let mut server = tokio_tungstenite::accept_async(theirs)
        .await
        .expect("the link connects");
    macro_rules! heard {
        () => {
            loop {
                let message =
                    tokio::time::timeout(std::time::Duration::from_secs(5), server.next())
                        .await
                        .expect("the link speaks in time")
                        .expect("the link is there")
                        .expect("a message");
                if let Message::Text(text) = message {
                    break serde_json::from_str::<Value>(text.as_str())
                        .expect("a message of the protocol");
                }
            }
        };
    }
    macro_rules! say {
        ($message:expr) => {
            server
                .send(Message::text($message.to_string()))
                .await
                .expect("the link listens")
        };
    }

    let first = heard!();
    assert_eq!(first["method"], "initialize");
    assert_eq!(first["params"]["capabilities"]["experimentalApi"], true);
    say!(
        json!({"id": first["id"], "result": {"userAgent": "codex-tui/0.171.0 (Mac OS 27.2.1; arm64)"}})
    );
    assert_eq!(heard!()["method"], "initialized");

    // The adapter asks for the models, the limits and the conversations of the server.
    let asked = [heard!(), heard!(), heard!()];
    let loaded = asked
        .iter()
        .find(|call| call["method"] == "thread/loaded/list")
        .expect("the link asks for the conversations");
    say!(json!({"id": loaded["id"], "result": answer_of(codex!("start"), "loaded")}));
    let read = heard!();
    assert_eq!(read["method"], "thread/read");
    say!(json!({"id": read["id"], "result": answer_of(codex!("start"), "read")}));
    let follow = heard!();
    assert_eq!(follow["method"], "thread/resume");
    say!(json!({"id": follow["id"], "result": answer_of(codex!("first"), "follow")}));
    let history = heard!();
    assert_eq!(history["method"], "thread/turns/list");
    say!(json!({"id": history["id"], "result": {"data": [], "nextCursor": null}}));

    let mut viewer = chat
        .attach("sam".to_owned(), "Sam".to_owned())
        .await
        .expect("the chat opens");
    assert!(matches!(frame(&mut viewer).await, ChatFrame::Snapshot(_)));
    for message in server_says(codex!("allow")) {
        let asks = !message["id"].is_null();
        say!(message);
        if asks {
            break;
        }
    }
    let ask = change(&mut viewer, |change| match change {
        ChatChange::Asks { asks } => asks.first().map(|ask| ask.id.clone()),
        _ => None,
    })
    .await;
    let answer = serde_json::to_vec(&ChatRequest::Answer {
        ask,
        answer: ChatAnswer::Allow,
    })
    .expect("a request");
    viewer.sender.send(answer).await.expect("the pipe is open");
    assert_eq!(heard!(), json!({"id": 0, "result": {"decision": "accept"}}));

    // Codex leaves the terminal.
    left.cancel();
    link.await.expect("the link ends");
    change(&mut viewer, |change| {
        matches!(
            change,
            ChatChange::State {
                state: ChatState::Away,
                ..
            }
        )
        .then_some(())
    })
    .await;
}
