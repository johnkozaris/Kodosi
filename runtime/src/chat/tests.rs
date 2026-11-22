//! The records in `fixtures/claude` are what the link module sent from real sessions of Claude Code.
//! Those in `fixtures/codex` are the messages of Codex's app-server in real sessions, as the link
//! gives them to the chat, without the messages that the chat does not read. Those in
//! `fixtures/copilot` are what the extension sent from real sessions of Copilot CLI 1.0.115.
//! Those in `fixtures/antigravity` are the messages of `agy acp` 1.5.14 in real sessions, as
//! `kodosi acp` gives them to the chat.
//!
//! The names of private places are replaced. A line with `answer` holds the decision that the
//! recording gave the agent for a question, which the agent then took.

use serde_json::{Value, json};

use super::{
    acp,
    agent::{Agent, Stop, shown_rule},
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

    /// Plays records of Copilot after the start of its extension.
    fn copilot(records: &[&str], answers: &[ChatAnswer]) -> Self {
        let mut play = Self::new(Box::new(Copilot::default()));
        play.more(copilot!("start"), &[]);
        play.more(&records.join(""), answers);
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
            if matches!(self.agent.name(), Codex::NAME | Antigravity::NAME) {
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
                ChatChange::Tasks { tasks } => self.view.tasks.clone_from(tasks),
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
    assert_said_after_refusal(&play, "Do not change the file.");
}

#[test]
fn a_rule_for_an_absolute_path_names_only_its_folder() {
    assert_eq!(shown_rule("//home/sam/shop/**"), "shop/**");
    assert_eq!(shown_rule("/home/sam/shop"), "shop");
    assert_eq!(shown_rule("~/work/notes/*.md"), "notes/*.md");
    assert_eq!(shown_rule("npm test:*"), "npm test:*");
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
fn a_command_that_claude_code_cannot_run_ends_with_its_reason_and_the_chat_goes_on() {
    let mut play = Play::of(record!("text"), &[]);
    play.chat.queue("/nonexistent".to_owned(), None);
    let [command] = &play.agent.commands(&play.chat)[..] else {
        panic!("the message goes to Claude Code");
    };

    play.take(
        &json!({"k": "did", "n": command["n"], "error": "Error: no command named nonexistent"}),
    );

    assert_eq!(play.view.queued, []);
    assert!(
        play.view.items.iter().any(|item| matches!(&item.body,
        ChatBody::Note { note: ChatNote::Failed { error, .. } } if error.ends_with("nonexistent")))
    );
    play.chat.queue("Hello.".to_owned(), None);
    play.follow();
    assert!(
        play.view.state != ChatState::Away && !play.agent.commands(&play.chat).is_empty(),
        "the next message goes on"
    );
}

#[test]
fn a_session_in_auto_mode_goes_back_to_it_after_its_plan_as_claude_code_itself_does() {
    let play = Play::of(record!("plan-auto"), &[]);

    let [ask] = &play.view.asks[..] else {
        panic!("the plan waits for approval");
    };
    assert!(matches!(&ask.body, AskBody::Plan { choices, .. }
        if choices == &[PlanChoice::Auto, PlanChoice::Ask]));
    let mut agent = play.agent;
    let auto = ChatAnswer::Plan {
        choice: PlanChoice::Auto,
    };
    let decision = agent.decision(&ask.id, &auto, None).expect("a decision");
    assert_eq!(decision["behavior"], "allow");
    assert_eq!(decision.get("updatedPermissions"), None);
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
fn a_clear_from_the_chat_lets_the_next_message_go() {
    let mut play = Play::of(record!("text"), &[]);
    play.chat.queue("/clear".to_owned(), None);
    let [command] = &play.agent.commands(&play.chat)[..] else {
        panic!("the command goes to Claude Code");
    };
    let n = command["n"].clone();

    play.more(record!("clear"), &[]);
    play.take(&json!({"k": "did", "n": n}));

    assert_eq!(play.view.queued, []);
    play.chat.queue("Hello.".to_owned(), None);
    assert_eq!(play.agent.commands(&play.chat).len(), 1);
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
async fn the_agent_gets_the_start_and_the_end_of_a_message_longer_than_the_chat_takes() {
    let chat = ChatHandle::spawn();
    assert!(report(&chat, "one", 1, events(record!("text"))).await);
    let viewer = chat
        .attach("7".to_owned(), "Sam".to_owned())
        .await
        .expect("the chat opens");
    let (reply, commands) = tokio::sync::oneshot::channel();
    let job = Job::Next {
        link: "one".to_owned(),
        reply,
    };
    chat.jobs()
        .upgrade()
        .expect("the chat")
        .send(job)
        .await
        .ok();
    let log = format!(
        "Build log:\n{}Why does it fail?",
        "error: link\n".repeat(10_000)
    );

    let message = serde_json::to_vec(&ChatRequest::Send { text: log }).expect("a request");
    viewer.sender.send(message).await.expect("the pipe is open");

    let Ok(Ok(Some(commands))) =
        tokio::time::timeout(std::time::Duration::from_secs(5), commands).await
    else {
        panic!("the message goes to the module");
    };
    let text = commands[0]["text"].as_str().expect("the text");
    assert!(text.len() <= MOST_MESSAGE + 8);
    assert!(text.starts_with("Build log:"));
    assert!(text.ends_with("Why does it fail?"));
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
async fn another_agent_in_the_terminal_starts_the_chat_again() {
    let chat = ChatHandle::spawn();
    assert!(report(&chat, "copilot", 1, events(copilot!("history"))).await);
    assert!(report(&chat, "claude", 1, events(record!("text"))).await);

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

/// The server says of a conversation that the chat does not follow only that Codex works in it.
fn codex_works_in(thread: &str) -> String {
    json!({
        "k": "note",
        "m": "thread/status/changed",
        "p": {"threadId": thread, "status": {"type": "active", "activeFlags": []}},
    })
    .to_string()
}

#[test]
fn the_chat_of_codex_goes_to_the_conversation_that_a_person_goes_to_in_the_terminal() {
    let mut play = Play::codex(&[codex!("first")], &[]);
    let started: Value =
        serde_json::from_str(codex!("new").lines().next().expect("the start")).expect("a line");
    let other = started["p"]["thread"].clone();
    let id = other["id"].as_str().expect("the name").to_owned();

    play.more(&codex_works_in(&id), &[]);
    assert_eq!(
        calls(&play, "thread/read")
            .last()
            .map(|call| &call["p"]["threadId"]),
        Some(&json!(id))
    );
    assert_eq!(
        play.people(),
        ["Say hello in five words."],
        "the chat stays until it knows what the conversation is"
    );

    play.more(
        &json!({"k": "reply", "tag": "read", "r": {"thread": other}}).to_string(),
        &[],
    );
    assert_eq!(play.view.items, []);
    assert_eq!(
        calls(&play, "thread/resume")
            .last()
            .map(|call| &call["p"]["threadId"]),
        Some(&json!(id))
    );
}

/// When Codex resumes a conversation, the server tells the other programs only that the
/// conversation has no goal now.
fn codex_resumed(thread: &str) -> String {
    json!({"k": "note", "m": "thread/goal/cleared", "p": {"threadId": thread}}).to_string()
}

#[test]
fn the_chat_of_codex_goes_to_a_conversation_that_codex_resumes_before_a_turn_runs() {
    let mut play = Play::codex(&[codex!("first")], &[]);
    let started: Value =
        serde_json::from_str(codex!("new").lines().next().expect("the start")).expect("a line");
    let other = started["p"]["thread"].clone();
    let id = other["id"].as_str().expect("the name").to_owned();

    play.more(&codex_resumed(&id), &[]);
    assert_eq!(
        calls(&play, "thread/read")
            .last()
            .map(|call| &call["p"]["threadId"]),
        Some(&json!(id))
    );
    play.more(
        &json!({"k": "reply", "tag": "read", "r": {"thread": other}}).to_string(),
        &[],
    );
    assert_eq!(
        calls(&play, "thread/resume")
            .last()
            .map(|call| &call["p"]["threadId"]),
        Some(&json!(id))
    );
}

#[test]
fn the_chat_of_codex_stays_when_a_helper_of_codex_works_in_its_own_conversation() {
    let mut play = Play::codex(&[codex!("first")], &[]);
    let started: Value =
        serde_json::from_str(codex!("new").lines().next().expect("the start")).expect("a line");
    let mut helper = started["p"]["thread"].clone();
    helper["parentThreadId"] = json!("00000001-0000-7000-8000-000000000000");
    let id = helper["id"].as_str().expect("the name").to_owned();

    play.more(&codex_works_in(&id), &[]);
    play.more(
        &json!({"k": "reply", "tag": "read", "r": {"thread": helper}}).to_string(),
        &[],
    );
    play.more(&codex_works_in(&id), &[]);

    assert_eq!(play.people(), ["Say hello in five words."]);
    assert_eq!(
        calls(&play, "thread/read").len(),
        2,
        "one at the start, one for the helper"
    );
    assert_eq!(calls(&play, "thread/unsubscribe"), Vec::<&Value>::new());
}

#[test]
fn a_message_that_codex_writes_down_in_other_words_leaves_the_queue_when_its_turn_ends() {
    let mut play = Play::codex(&[], &[]);
    play.chat
        .queue("Say hello.".to_owned(), Some(sam()))
        .expect("room in the queue");
    assert_eq!(play.agent.commands(&play.chat).len(), 1);

    play.more(codex!("first"), &[]);

    assert_eq!(play.view.queued, []);
    play.chat
        .queue("And again.".to_owned(), Some(sam()))
        .expect("room in the queue");
    let sent = play.agent.commands(&play.chat);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0]["p"]["input"][0]["text"], "And again.");
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

    // The adapter asks for the modes, the limits and the conversations of the server.
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

#[test]
fn codex_names_the_conversation_that_its_terminal_shows() {
    let play = Play::codex(&[codex!("first")], &[]);

    assert_eq!(
        play.agent.conversation(),
        Some("00000001-0000-7000-8000-000000000000")
    );
}

#[test]
fn copilot_shows_a_message_and_the_answer_as_copilot_writes_it() {
    let play = Play::copilot(&[copilot!("text")], &[]);

    assert_eq!(play.people(), ["Say hello in five words."]);
    assert_eq!(play.answers(), ["Hello, it’s wonderful to meet you."]);
    assert_eq!(play.ends(), [TurnEnd::Done]);
    assert_eq!(play.view.state, ChatState::Idle);
    let facts = &play.view.facts;
    assert_eq!(facts.agent.as_deref(), Some("copilot"));
    assert_eq!(facts.version.as_deref(), Some("1.0.115-3"));
    assert_eq!(facts.model.as_deref(), Some("mai-code-2-flash"));
    assert_eq!(facts.folder.as_deref(), Some("/home/sam/shop"));
    assert!(
        facts
            .context
            .is_some_and(|context| context.window == 922_000)
    );
}

#[test]
fn a_person_allows_a_command_of_copilot_in_the_chat() {
    let play = Play::copilot(&[copilot!("command")], &[ChatAnswer::Allow]);

    let [command] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(
        (command.kind, command.state, command.target.as_str()),
        (ToolKind::Command, ToolState::Done, "python3 app.py")
    );
    let Some(ToolDetail::Command { output, .. }) = &command.detail else {
        panic!("a command");
    };
    assert!(output.contains("Total       33.83"), "{output}");
    assert_eq!(play.answers(), ["The total is 33.83."]);
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
}

#[test]
fn the_words_of_a_refusal_go_to_copilot_and_the_command_shows_as_refused() {
    let refusal = ChatAnswer::Deny {
        text: Some("Do not run it. Say in one short sentence what it would print.".to_owned()),
    };
    let play = Play::copilot(&[copilot!("refuse")], &[refusal]);

    let [command] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(command.state, ToolState::Denied);
    assert_eq!(
        play.answers(),
        ["It prints the Mug, Notebook, Pen, and total of 33.83."],
        "the agent followed the person's words"
    );
    assert_eq!(play.ends(), [TurnEnd::Done]);
    assert_said_after_refusal(&play, "Do not run it.");
}

fn assert_said_after_refusal(play: &Play, words: &str) {
    let items = &play.view.items;
    let refused = items.iter().position(
        |item| matches!(&item.body, ChatBody::Tool { tool } if tool.state == ToolState::Denied),
    );
    let said = items.iter().position(
        |item| matches!(&item.body, ChatBody::Person { text, .. } if text.starts_with(words)),
    );
    let answered = items
        .iter()
        .rposition(|item| matches!(&item.body, ChatBody::Text { .. }));
    assert!(
        refused.is_some() && refused < said && said < answered,
        "{items:#?}"
    );
}

#[test]
fn a_change_of_a_file_by_copilot_shows_its_lines() {
    let play = Play::copilot(&[copilot!("edit")], &[ChatAnswer::Allow]);

    let [read, edit] = play.tools()[..] else {
        panic!("two calls");
    };
    assert_eq!((read.kind, read.state), (ToolKind::Read, ToolState::Done));
    assert_eq!(
        (edit.kind, edit.state, edit.target.as_str()),
        (ToolKind::Edit, ToolState::Done, "app.py")
    );
    let Some(ToolDetail::Change {
        path,
        added,
        removed,
        lines,
    }) = &edit.detail
    else {
        panic!("a change");
    };
    assert_eq!(
        (path.as_str(), *added, *removed),
        ("/home/sam/shop/app.py", 1, 1)
    );
    assert!(lines.contains(&line(ChangeSign::Removed, "    return subtotal * TAX")));
    assert!(lines.contains(&line(ChangeSign::Added, "    return subtotal")));
}

#[test]
fn a_stop_of_the_chat_ends_the_turn_of_copilot_and_its_command() {
    let play = Play::copilot(&[copilot!("stop")], &[ChatAnswer::Allow]);

    let [command] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(command.state, ToolState::Stopped);
    assert_eq!(play.ends(), [TurnEnd::Stopped]);
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn an_answer_in_the_terminal_closes_the_question_of_copilot_in_the_chat() {
    let play = Play::copilot(&[copilot!("terminal")], &[]);

    assert_eq!(play.closed.len(), 1, "the question closed without the chat");
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
    let [command] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(command.state, ToolState::Done);
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
fn a_resumed_conversation_of_copilot_shows_its_earlier_turns() {
    let mut play = Play::new(Box::new(Copilot::default()));
    play.more(copilot!("history"), &[]);

    assert_eq!(play.people().len(), 7);
    assert_eq!(play.people()[0], "Say hello in five words.");
    assert_eq!(
        play.ends(),
        [
            TurnEnd::Done,
            TurnEnd::Done,
            TurnEnd::Done,
            TurnEnd::Done,
            TurnEnd::Stopped,
            TurnEnd::Done,
            TurnEnd::Done
        ]
    );
    let refused = play
        .tools()
        .iter()
        .filter(|tool| tool.state == ToolState::Denied)
        .count();
    assert_eq!(refused, 1, "the refusal of the third turn");
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_message_of_the_chat_goes_to_copilot_only_when_copilot_is_idle() {
    let mut play = Play::copilot(&[], &[]);
    play.chat.queue("Say hello.".to_owned(), Some(sam()));
    assert_eq!(
        play.agent.commands(&play.chat),
        [json!({"do": "prompt", "n": 1, "text": "Say hello."})]
    );
    assert_eq!(
        play.agent.commands(&play.chat),
        Vec::<Value>::new(),
        "Copilot has it"
    );

    play.more(copilot!("text"), &[]);
    assert_eq!(play.people(), ["Say hello in five words."]);
}

#[test]
fn a_person_answers_the_question_of_codex_in_plan_mode_and_its_plan_shows() {
    let mut codex = Play::codex(&[codex!("first")], &[]);
    let lines = codex!("question").lines().collect::<Vec<_>>();
    let asked = lines
        .iter()
        .position(|line| line.contains(r#""k":"request""#))
        .expect("a question");
    codex.more(&lines[..=asked].join("\n"), &[]);
    let [ask] = &codex.view.asks[..] else {
        panic!("one question");
    };
    let AskBody::Question { questions } = &ask.body else {
        panic!("a question with choices");
    };
    let [question] = &questions[..] else {
        panic!("one question");
    };
    assert_eq!(
        (question.header.as_str(), question.question.as_str()),
        (
            "Filename",
            "Which file name should I use for the greeting file?"
        )
    );
    assert_eq!(
        question
            .options
            .iter()
            .map(|option| option.label.as_str())
            .collect::<Vec<_>>(),
        ["hello.txt (Recommended)", "greet.txt"]
    );
    assert_eq!(codex.view.facts.mode, Some(ChatMode::Plan));
    assert!(
        matches!(codex.agent.stop(&codex.chat), Stop::Command(stop) if stop["m"] == "turn/interrupt"),
        "a question of Codex takes no refusal, so a stop ends the turn"
    );
    assert_eq!(
        codex.agent.decision(
            &ask.id,
            &ChatAnswer::Deny {
                text: Some("a name of your own".to_owned())
            },
            None
        ),
        Some(json!({"answers": {"greeting_filename": {"answers": ["a name of your own"]}}}))
    );

    codex.more(
        &lines[asked + 1..].join("\n"),
        &[ChatAnswer::Answers {
            answers: vec![vec!["greet.txt".to_owned()]],
        }],
    );
    let tools = codex.tools();
    let [question, plan] = &tools[tools.len() - 2..] else {
        panic!("the question and the plan");
    };
    assert_eq!(question.state, ToolState::Done);
    assert!(
        matches!(&question.detail, Some(ToolDetail::Question { answers, .. }) if answers == &["greet.txt"])
    );
    assert_eq!((plan.kind, plan.state), (ToolKind::Plan, ToolState::Asking));
    assert!(
        matches!(&plan.detail, Some(ToolDetail::Plan { text }) if text.starts_with("## Create greeting file")),
        "{:?}",
        plan.detail
    );
    let grew = codex.changes.iter().any(|change| {
        matches!(change, ChatChange::Item { item } if matches!(&item.body, ChatBody::Tool { tool }
            if tool.state == ToolState::Running && matches!(&tool.detail, Some(ToolDetail::Plan { text }) if text.starts_with("## Create")))
        )
    });
    assert!(grew, "the plan showed while Codex wrote it");
    assert_eq!(codex.ends(), [TurnEnd::Done, TurnEnd::Done]);
    let offer = codex
        .view
        .asks
        .first()
        .cloned()
        .expect("the question about the plan");
    assert!(
        matches!(&offer.body, AskBody::Plan { text, choices } if text.starts_with("## Create greeting file")
            && choices == &[PlanChoice::Go]),
        "{offer:?}"
    );
    assert_eq!(codex.view.state, ChatState::Asking);
    assert!(
        matches!(codex.agent.stop(&codex.chat), Stop::Decisions(decisions) if decisions.len() == 1),
        "a stop ends the question and starts nothing"
    );
}

#[test]
fn a_plan_command_of_the_chat_goes_to_codex_in_its_plan_mode_as_the_terminal_sends_it() {
    let mut codex = Play::codex(&[codex!("first")], &[]);
    codex.chat.queue("/plan Add a discount.".to_owned(), None);

    let calls = codex.agent.commands(&codex.chat);
    let [start] = &calls[..] else {
        panic!("one turn: {calls:?}");
    };
    assert_eq!(start["m"], "turn/start");
    assert_eq!(start["p"]["input"][0]["text"], "Add a discount.");
    assert_eq!(start["p"]["collaborationMode"]["mode"], "plan");

    let mut codex = Play::codex(&[codex!("first")], &[]);
    codex.chat.queue("/plan".to_owned(), None);
    let calls = codex.agent.commands(&codex.chat);
    let [change] = &calls[..] else {
        panic!("one change of the mode: {calls:?}");
    };
    assert_eq!(change["m"], "thread/settings/update");
    assert_eq!(change["p"]["collaborationMode"]["mode"], "plan");
}

#[test]
fn a_plan_command_of_the_chat_goes_to_copilot_in_its_plan_mode() {
    let mut copilot = Play::copilot(&[], &[]);
    copilot.chat.queue("/plan Add a discount.".to_owned(), None);

    assert_eq!(
        copilot.agent.commands(&copilot.chat),
        [json!({"do": "prompt", "n": 1, "text": "Add a discount.", "mode": "plan"})]
    );
}

#[test]
fn a_person_carries_out_the_plan_of_codex_from_the_chat_as_its_terminal_does() {
    let mut codex = Play::codex(
        &[codex!("first"), codex!("question")],
        &[ChatAnswer::Answers {
            answers: vec![vec!["greet.txt".to_owned()]],
        }],
    );
    let offer = codex
        .view
        .asks
        .first()
        .cloned()
        .expect("the question about the plan");
    let go = ChatAnswer::Plan {
        choice: PlanChoice::Go,
    };

    let decision = codex
        .agent
        .decision(&offer.id, &go, Some(&sam()))
        .expect("a decision");
    assert!(
        !codex.agent.answered(&mut codex.chat, &offer.id, &decision),
        "the adapter takes the decision, and no link waits for it"
    );
    codex.follow();
    let calls = codex.agent.commands(&codex.chat);
    let [start] = &calls[..] else {
        panic!("one turn: {calls:?}");
    };
    assert_eq!(start["m"], "turn/start");
    assert_eq!(start["p"]["input"][0]["text"], "Implement the plan.");
    assert_eq!(
        start["p"]["collaborationMode"],
        json!({"mode": "default", "settings": {"model": "gpt-6-luna", "reasoning_effort": "low", "developer_instructions": null}})
    );

    codex.more(codex!("implement"), &[]);
    assert_eq!(codex.people().last(), Some(&"Implement the plan."));
    let said = codex
        .view
        .items
        .iter()
        .rev()
        .find_map(|item| match &item.body {
            ChatBody::Person { by, .. } => Some(by.clone()),
            _ => None,
        });
    assert_eq!(
        said,
        Some(Some(sam())),
        "the message of the person who chose"
    );
    assert_eq!(codex.view.asks, [] as [ChatAsk; 0]);
    assert_eq!(codex.ends().last(), Some(&TurnEnd::Done));
}

#[test]
fn the_plan_tool_of_codex_shows_as_its_list_of_the_work() {
    let codex = Play::codex(&[codex!("first"), codex!("tasks")], &[]);

    let working = codex.changes.iter().any(|change| {
        matches!(change, ChatChange::Tasks { tasks } if tasks.first().is_some_and(|task| task.state == TaskState::Working))
    });
    assert!(working, "the list showed the step that Codex worked on");
    assert_eq!(
        codex
            .view
            .tasks
            .iter()
            .map(|task| (task.text.as_str(), task.state))
            .collect::<Vec<_>>(),
        [
            ("Write “one” into one.txt", TaskState::Done),
            ("Write “two” into two.txt", TaskState::Done),
            ("Say done", TaskState::Done),
        ]
    );
}

#[test]
fn a_person_answers_a_question_of_copilot_with_one_of_its_choices() {
    let mut play = Play::copilot(&[], &[]);
    let lines = copilot!("question").lines().collect::<Vec<_>>();
    let asked = lines
        .iter()
        .position(|line| line.contains(r#""k":"ask""#))
        .expect("a question");
    play.more(&lines[..=asked].join("\n"), &[]);
    let [ask] = &play.view.asks[..] else {
        panic!("one question");
    };
    let AskBody::Question { questions } = &ask.body else {
        panic!("a question with choices");
    };
    assert_eq!(
        questions
            .iter()
            .map(|question| (
                question.question.as_str(),
                question
                    .options
                    .iter()
                    .map(|option| option.label.as_str())
                    .collect::<Vec<_>>()
            ))
            .collect::<Vec<_>>(),
        [(
            "Which fruit should I add to the price list?",
            vec!["pear", "plum", "kiwi"]
        )]
    );
    assert_eq!(play.view.state, ChatState::Asking);

    play.more(
        &lines[asked + 1..].join("\n"),
        &[ChatAnswer::Answers {
            answers: vec![vec!["plum".to_owned()]],
        }],
    );
    let [question] = play.tools()[..] else {
        panic!("one call");
    };
    assert_eq!(question.kind, ToolKind::Question);
    assert_eq!(question.state, ToolState::Done);
    assert!(
        matches!(&question.detail, Some(ToolDetail::Question { answers, .. }) if answers == &["plum"])
    );
    assert_eq!(play.answers(), ["plum"]);
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
}

#[test]
fn an_answer_in_the_terminal_closes_a_question_of_copilot_and_shows_in_the_chat() {
    let play = Play::copilot(&[copilot!("question-terminal")], &[]);

    assert_eq!(play.closed.len(), 1, "the question closed without the chat");
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
    let [question] = play.tools()[..] else {
        panic!("one call");
    };
    assert!(
        matches!(&question.detail, Some(ToolDetail::Question { answers, .. }) if answers == &["tea"])
    );
    assert_eq!(play.answers(), ["tea"]);
}

/// A question of Copilot that asks `fields` of the schema of its form, as the extension reports it.
fn copilot_form(fields: &Value) -> Play {
    let mut play = Play::copilot(&[], &[]);
    let lines = copilot!("question").lines().collect::<Vec<_>>();
    let asked = lines
        .iter()
        .position(|line| line.contains(r#""k":"ask""#))
        .expect("a question");
    let mut ask: Value = serde_json::from_str(lines[asked]).expect("the question");
    ask["form"]["requestedSchema"]["properties"] = fields.clone();
    let record = lines[..asked]
        .iter()
        .map(|line| (*line).to_owned())
        .chain([ask.to_string()])
        .collect::<Vec<_>>();
    play.more(&record.join("\n"), &[]);
    play
}

#[test]
fn a_number_that_a_person_writes_answers_the_form_of_copilot_that_asks_for_it() {
    let play = copilot_form(&json!({"count": {"type": "integer", "title": "How many?"}}));
    let ask = play.view.asks[0].id.clone();
    let mut agent = play.agent;

    assert_eq!(
        agent.decision(
            &ask,
            &ChatAnswer::Deny {
                text: Some("3".to_owned())
            },
            None
        ),
        Some(json!({"action": "accept", "content": {"count": 3}}))
    );
}

#[test]
fn a_form_of_copilot_with_words_and_other_fields_is_for_its_terminal() {
    let play = copilot_form(&json!({
        "name": {"type": "string", "title": "Name"},
        "fruit": {"type": "string", "enum": ["pear", "plum"]},
    }));

    assert_eq!(play.view.asks, []);
}

#[test]
fn words_in_place_of_an_answer_go_to_copilot_as_the_person_s_own_answer() {
    let mut play = Play::copilot(&[], &[]);
    let lines = copilot!("question").lines().collect::<Vec<_>>();
    let asked = lines
        .iter()
        .position(|line| line.contains(r#""k":"ask""#))
        .expect("a question");
    play.more(&lines[..=asked].join("\n"), &[]);
    let ask = play.view.asks[0].id.clone();
    let words = ChatAnswer::Deny {
        text: Some(" a ripe mango ".to_owned()),
    };

    assert_eq!(
        play.agent.decision(&ask, &words, None),
        Some(json!({"action": "accept", "content": {"fruit": "a ripe mango"}}))
    );
    assert_eq!(
        play.agent
            .decision(&ask, &ChatAnswer::Deny { text: None }, None),
        Some(json!({"action": "decline"}))
    );
}

#[test]
fn copilot_takes_the_words_of_a_refused_plan_and_then_its_approval() {
    let answers = [
        ChatAnswer::Deny {
            text: Some("Use only two steps".to_owned()),
        },
        ChatAnswer::Plan {
            choice: PlanChoice::Go,
        },
        ChatAnswer::Allow,
        ChatAnswer::Allow,
        ChatAnswer::Allow,
    ];
    let play = Play::copilot(&[copilot!("plan")], &answers);

    let plans = play
        .changes
        .iter()
        .filter_map(|change| match change {
            ChatChange::Asks { asks } => asks.iter().find_map(|ask| match &ask.body {
                AskBody::Plan { text, choices } => Some((text.clone(), choices.clone())),
                _ => None,
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(plans.len(), 2, "the plan, and the plan in two steps");
    assert!(
        plans
            .iter()
            .all(|(_, choices)| choices == &[PlanChoice::Go, PlanChoice::Alone, PlanChoice::Leave]),
        "Copilot's own ways on, the recommended one first: {plans:?}"
    );
    assert!(
        plans[1].0.starts_with("1. Update `total()`"),
        "{}",
        plans[1].0
    );
    assert_eq!(
        play.tools()
            .iter()
            .filter(|tool| tool.kind == ToolKind::Plan)
            .map(|tool| tool.state)
            .collect::<Vec<_>>(),
        [ToolState::Denied, ToolState::Done]
    );
    let planning = play.changes.iter().any(|change| {
        matches!(change, ChatChange::Facts { facts } if facts.mode == Some(ChatMode::Plan))
    });
    assert!(planning, "the chat showed the plan mode");
    assert_eq!(play.view.facts.mode, None);
    assert_eq!(
        play.view.tasks,
        [
            ChatTask {
                text: "Adding total discount".to_owned(),
                state: TaskState::Done
            },
            ChatTask {
                text: "Verifying discounted total".to_owned(),
                state: TaskState::Done
            },
        ]
    );
    let worked = play.changes.iter().any(|change| {
        matches!(change, ChatChange::Tasks { tasks } if tasks.first().is_some_and(|task| task.state == TaskState::Working))
    });
    assert!(worked, "the list showed the step that Copilot worked on");
}

#[test]
fn the_steps_of_a_helper_of_copilot_show_in_the_call_that_started_it() {
    let play = Play::copilot(&[copilot!("helper")], &[ChatAnswer::Allow]);

    let [task] = play.tools()[..] else {
        panic!("one call of the main agent");
    };
    assert_eq!(
        (task.kind, task.state, task.target.as_str()),
        (ToolKind::Agent, ToolState::Done, "Run arithmetic command")
    );
    let Some(ToolDetail::Agent {
        name,
        steps,
        doing,
        said,
    }) = &task.detail
    else {
        panic!("a helper");
    };
    assert_eq!((name.as_str(), *steps, said.as_str()), ("task", 1, "42"));
    assert_eq!(
        doing.as_ref().map(|step| (step.kind, step.target.as_str())),
        Some((ToolKind::Command, "python3 -c 'print(6*7)'"))
    );
    assert_eq!(
        play.people().len(),
        1,
        "the helper's task is no message of a person"
    );
    assert_eq!(play.answers(), ["42"]);
    let helper_asked = play.changes.iter().any(|change| {
        matches!(change, ChatChange::Asks { asks } if asks.iter().any(|ask| ask.item.is_none()
            && matches!(&ask.body, AskBody::Permission { helper, .. } if helper == "Run arithmetic command")))
    });
    assert!(helper_asked, "the helper's question named its task");
    assert_eq!(play.view.facts.helpers, 0);
    assert_eq!(play.view.facts.model.as_deref(), Some("mai-code-2-flash"));
}

#[test]
fn copilot_that_starts_again_in_its_terminal_shows_its_conversation_once() {
    let mut play = Play::new(Box::new(Copilot::default()));
    play.more(copilot!("history"), &[]);
    let people = play.people().len();

    play.more(copilot!("history"), &[]);

    assert_eq!(play.people().len(), people);
}

#[test]
fn a_resumed_conversation_of_copilot_keeps_its_helper_in_the_call_that_started_it() {
    let mut play = Play::new(Box::new(Copilot::default()));
    play.more(copilot!("resume"), &[]);

    let tasks = play
        .tools()
        .into_iter()
        .filter(|tool| tool.kind == ToolKind::Agent)
        .collect::<Vec<_>>();
    let [task] = tasks[..] else {
        panic!("one helper");
    };
    assert!(
        matches!(&task.detail, Some(ToolDetail::Agent { steps: 1, said, .. }) if said.contains("report.py"))
    );
    assert!(
        play.people()
            .iter()
            .all(|said| !said.starts_with("Search the current workspace")),
        "the helper's task is no message of a person"
    );
    assert_eq!(play.view.state, ChatState::Idle);
    assert_eq!(play.view.asks, [] as [ChatAsk; 0]);
}

#[test]
fn copilot_carries_out_a_plan_alone_as_its_autopilot_and_asks_nothing() {
    let play = Play::copilot(
        &[copilot!("plan-alone")],
        &[ChatAnswer::Plan {
            choice: PlanChoice::Alone,
        }],
    );

    let asked = play
        .changes
        .iter()
        .filter(|change| matches!(change, ChatChange::Asks { asks } if !asks.is_empty()))
        .count();
    assert_eq!(asked, 1, "only the plan asked");
    let edit = play
        .tools()
        .into_iter()
        .find(|tool| tool.kind == ToolKind::Edit)
        .expect("the change of the file");
    assert_eq!(edit.state, ToolState::Done);
    assert_eq!(play.ends(), [TurnEnd::Done]);
}

#[test]
fn copilot_leaves_plan_mode_without_carrying_out_the_plan() {
    let play = Play::copilot(
        &[copilot!("plan-leave")],
        &[ChatAnswer::Plan {
            choice: PlanChoice::Leave,
        }],
    );

    let plans = play
        .tools()
        .into_iter()
        .filter(|tool| tool.kind == ToolKind::Plan)
        .map(|tool| tool.state)
        .collect::<Vec<_>>();
    assert_eq!(plans, [ToolState::Done]);
    let changed = play.tools().into_iter().any(|tool| {
        matches!(tool.kind, ToolKind::Edit | ToolKind::Write) && tool.target == "prices.py"
    });
    assert!(!changed, "the plan was not carried out");
    assert_eq!(play.view.facts.mode, None);
    assert_eq!(play.view.state, ChatState::Idle);
}

#[test]
fn a_signed_out_antigravity_signs_in_with_its_own_steps_in_its_terminal() {
    let mut play = Play::new(Box::new(Antigravity::default()));
    play.more(antigravity!("signed-out"), &[]);
    let new = play
        .calls
        .iter()
        .find(|call| call["m"] == "session/new")
        .expect("a new session");
    assert_eq!(new["p"]["cwd"], "/home/ana/work");
    assert!(matches!(
        &play.view.items[..],
        [ChatItem {
            body: ChatBody::Note {
                note: ChatNote::SignedOut
            },
            ..
        }]
    ));
    assert!(play.calls.iter().any(|call| call["m"] == acp::SIGN_IN));
    assert_eq!(play.view.facts.version.as_deref(), Some("1.5.14"));
    assert_eq!(play.view.state, ChatState::Idle);
}
