//! The debug connection that the agent's driver gets for one window. A person's terminals share
//! one browser, so a driver sees only the tabs of its own window, and the commands that would
//! change the browser for every window get their answer here.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    io,
    net::Ipv4Addr,
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::{SinkExt as _, StreamExt as _};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::{AbortHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

use crate::{engine::Engine, link::ToEngine};

/// A driver keeps one connection while it works, so more are left over from earlier drivers.
const MOST_DRIVERS: usize = 8;
const MOST_OPENING: usize = 8;
const MOST_ASKED: usize = 1024;
const MOST_HELD: usize = 256;
/// How long a new page waits to be placed in a window. The host's own pages never are.
const HOLD_LIMIT: Duration = Duration::from_secs(5);
const BLANK: &str = "about:blank";
/// The host attaches to each tab itself, because CEF's browser-wide auto-attach holds back the
/// pages that a page opens, and it keeps the downloads of every window in one place.
const ANSWERED: [&str; 2] = ["Target.setAutoAttach", "Browser.setDownloadBehavior"];
/// Commands that would end the browser or reach past the windows.
const REFUSED: [&str; 6] = [
    "Browser.close",
    "Browser.crash",
    "Browser.crashGpuProcess",
    "Target.attachToBrowserTarget",
    "Target.createBrowserContext",
    "Target.disposeBrowserContext",
];

/// The driver port of a window, bound before the window opens.
pub(crate) struct Listener {
    listener: TcpListener,
    port: u16,
}

impl Listener {
    pub(crate) fn bind() -> io::Result<Self> {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok(Self {
            listener: TcpListener::from_std(listener)?,
            port,
        })
    }

    /// Serves the drivers of a window until the window closes, and gives their address.
    pub(crate) fn serve(
        self,
        engine: Arc<Engine>,
        window: i64,
        mut open: watch::Receiver<bool>,
    ) -> String {
        let Self { listener, port } = self;
        tokio::spawn(async move {
            let mut drivers = JoinSet::new();
            let mut started: VecDeque<AbortHandle> = VecDeque::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let stream = match accepted {
                            Ok((stream, _)) => stream,
                            Err(problem) => {
                                tracing::warn!(%problem, "the driver port of a window closed");
                                break;
                            }
                        };
                        if started.len() == MOST_DRIVERS
                            && let Some(oldest) = started.pop_front()
                        {
                            oldest.abort();
                        }
                        started.push_back(drivers.spawn(serve(stream, Arc::clone(&engine), window)));
                    }
                    Some(_) = drivers.join_next(), if !drivers.is_empty() => {
                        started.retain(|driver| !driver.is_finished());
                    }
                    _ = open.wait_for(|open| !open) => break,
                }
            }
        });
        format!("ws://127.0.0.1:{port}/devtools/browser/{window}")
    }
}

/// Carries one driver's connection through its view, until either side or the browser ends.
async fn serve(stream: TcpStream, engine: Arc<Engine>, window: i64) {
    let Ok(driver) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let Ok((browser, _)) = tokio_tungstenite::connect_async(engine.address()).await else {
        return;
    };
    let (mut to_driver, mut from_driver) = driver.split();
    let (mut to_browser, mut from_browser) = browser.split();
    let mut placed = engine.placed();
    let mut opening = JoinSet::new();
    let mut view = View::new(window);
    let place = |target: &str| engine.window_of(target);
    loop {
        let mut out = Vec::new();
        tokio::select! {
            message = from_driver.next() => {
                let Some(Ok(message)) = message else { break };
                let Message::Text(text) = message else {
                    if message.is_close() {
                        break;
                    }
                    continue;
                };
                match view.ask(&text, place) {
                    Ask::Pass => {
                        if to_browser.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Ask::Show(target) => {
                        if let Some(tab) = engine.tab_of(&target) {
                            engine.command(ToEngine::Activate { tab });
                        }
                        if to_browser.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Ask::Answer(answer) => out.push(answer),
                    Ask::Dialog { target, accept, text, answer } => {
                        if let Some(tab) = engine.tab_of(&target) {
                            engine.command(ToEngine::Dialog { tab, accept, text });
                        }
                        out.push(answer);
                    }
                    Ask::Open { id, address } => {
                        if opening.len() == MOST_OPENING {
                            out.push(failure(id, None, "Too many tabs are opening."));
                        } else {
                            let engine = Arc::clone(&engine);
                            opening.spawn(async move { (id, engine.open_tab(window, &address).await) });
                        }
                    }
                }
            }
            message = from_browser.next() => {
                let Some(Ok(message)) = message else { break };
                let Message::Text(text) = message else {
                    if message.is_close() {
                        break;
                    }
                    continue;
                };
                let now = Instant::now();
                out.extend(view.release(place, now));
                match view.tell(&text, place, || engine.active_target(window), now) {
                    Tell::Pass => {
                        if to_driver.send(Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    Tell::Changed(changed) => out.push(changed),
                    Tell::Drop => {}
                }
            }
            changed = placed.changed() => {
                if changed.is_err() {
                    break;
                }
                out.extend(view.release(place, Instant::now()));
            }
            Some(opened) = opening.join_next(), if !opening.is_empty() => {
                let Ok((id, target)) = opened else { continue };
                out.push(view.opened(id, target));
                out.extend(view.release(place, Instant::now()));
            }
        }
        for message in out {
            if to_driver.send(Message::text(message)).await.is_err() {
                return;
            }
        }
    }
}

/// What becomes of a driver's command.
#[derive(Debug, PartialEq, Eq)]
enum Ask {
    Pass,
    /// It goes to the browser, and the window shows this tab to its people too.
    Show(String),
    /// The view answers it.
    Answer(String),
    /// The engine answers the dialog of this tab, and the view answers the driver.
    Dialog {
        target: String,
        accept: bool,
        text: Option<String>,
        answer: String,
    },
    /// A new tab of this window, whose target answers the command.
    Open {
        id: u64,
        address: String,
    },
}

/// What becomes of a message of the browser.
#[derive(Debug, PartialEq, Eq)]
enum Tell {
    Pass,
    Drop,
    Changed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Whose {
    Ours,
    Theirs,
    /// The engine has not placed it yet, or it is no tab.
    Unknown,
}

enum Asked {
    Targets,
    Attach(String),
}

struct Held {
    target: String,
    message: String,
    at: Instant,
}

#[derive(Deserialize)]
struct Command {
    id: Option<u64>,
    method: String,
    #[serde(rename = "sessionId")]
    session: Option<String>,
    #[serde(default)]
    params: Value,
}

/// The parts of a browser message that decide its way, read without its body.
#[derive(Deserialize)]
struct Head<'a> {
    id: Option<u64>,
    #[serde(borrow)]
    method: Option<Cow<'a, str>>,
    #[serde(rename = "sessionId", borrow)]
    session: Option<Cow<'a, str>>,
}

/// One driver's view of the browser: the tabs of its window and the sessions that it has with
/// them.
struct View {
    window: i64,
    /// Tabs that this driver opened, before the engine places them.
    opened: HashSet<String>,
    /// Targets that the driver knows of.
    told: HashSet<String>,
    /// The driver's sessions, with their targets.
    sessions: HashMap<String, String>,
    asked: HashMap<u64, Asked>,
    /// News of pages that are not placed yet, in their order.
    held: VecDeque<Held>,
}

impl View {
    fn new(window: i64) -> Self {
        Self {
            window,
            opened: HashSet::new(),
            told: HashSet::new(),
            sessions: HashMap::new(),
            asked: HashMap::new(),
            held: VecDeque::new(),
        }
    }

    fn whose(&self, target: &str, place: impl Fn(&str) -> Option<i64>) -> Whose {
        if self.opened.contains(target) {
            return Whose::Ours;
        }
        match place(target) {
            Some(window) if window == self.window => Whose::Ours,
            Some(_) => Whose::Theirs,
            None => Whose::Unknown,
        }
    }

    fn ask(&mut self, text: &str, place: impl Fn(&str) -> Option<i64>) -> Ask {
        let Ok(command) = serde_json::from_str::<Command>(text) else {
            return Ask::Answer(failure(0, None, "The message is not a command."));
        };
        let id = command.id.unwrap_or_default();
        if let Some(session) = &command.session {
            return match self.sessions.get(session) {
                Some(target) if command.method == "Page.bringToFront" => Ask::Show(target.clone()),
                Some(target) if command.method == "Page.handleJavaScriptDialog" => Ask::Dialog {
                    target: target.clone(),
                    accept: command.params["accept"].as_bool().unwrap_or_default(),
                    text: command.params["promptText"].as_str().map(str::to_owned),
                    answer: answer_in(id, session, &json!({})),
                },
                Some(_) => Ask::Pass,
                None => Ask::Answer(failure(id, Some(session), "No session with given id")),
            };
        }
        let method = command.method.as_str();
        if REFUSED.contains(&method) {
            return Ask::Answer(failure(
                id,
                None,
                "Kodosi keeps this for the browser of every window.",
            ));
        }
        if ANSWERED.contains(&method) {
            return Ask::Answer(answer(id, &json!({})));
        }
        if method == "Target.createTarget" {
            let address = command.params["url"]
                .as_str()
                .filter(|address| !address.is_empty())
                .unwrap_or(BLANK);
            return Ask::Open {
                id,
                address: address.to_owned(),
            };
        }
        let target = command.params["targetId"].as_str();
        if target.is_some_and(|target| self.whose(target, &place) != Whose::Ours) {
            return Ask::Answer(failure(id, None, "No target with given id found"));
        }
        if command.params["sessionId"]
            .as_str()
            .is_some_and(|session| !self.sessions.contains_key(session))
        {
            return Ask::Answer(failure(id, None, "No session with given id"));
        }
        let asked = match (method, target) {
            ("Target.getTargets", _) => Some(Asked::Targets),
            ("Target.attachToTarget", Some(target)) => Some(Asked::Attach(target.to_owned())),
            _ => None,
        };
        if let Some(asked) = asked {
            if self.asked.len() == MOST_ASKED {
                return Ask::Answer(failure(id, None, "Too many commands wait."));
            }
            self.asked.insert(id, asked);
        }
        match target {
            Some(target) if method == "Target.activateTarget" => Ask::Show(target.to_owned()),
            _ => Ask::Pass,
        }
    }

    fn tell(
        &mut self,
        text: &str,
        place: impl Fn(&str) -> Option<i64>,
        active: impl FnOnce() -> Option<String>,
        now: Instant,
    ) -> Tell {
        let Ok(head) = serde_json::from_str::<Head<'_>>(text) else {
            return Tell::Drop;
        };
        if let Some(session) = head.session.as_deref() {
            if !self.sessions.contains_key(session) {
                return Tell::Drop;
            }
            match head.method.as_deref() {
                Some("Target.attachedToTarget") => {
                    let params = params(text);
                    if let (Some(child), Some(target)) = (
                        params["sessionId"].as_str(),
                        params["targetInfo"]["targetId"].as_str(),
                    ) {
                        self.sessions.insert(child.to_owned(), target.to_owned());
                    }
                }
                Some("Target.detachedFromTarget") => {
                    if let Some(child) = params(text)["sessionId"].as_str() {
                        self.sessions.remove(child);
                    }
                }
                _ => {}
            }
            return Tell::Pass;
        }
        if let Some(method) = head.method.as_deref() {
            return self.event(method, text, place, now);
        }
        match head.id.and_then(|id| self.asked.remove(&id)) {
            Some(Asked::Targets) => self.targets(text, place, active().as_deref()),
            Some(Asked::Attach(target)) => {
                if let Ok(answer) = serde_json::from_str::<Value>(text)
                    && let Some(session) = answer["result"]["sessionId"].as_str()
                {
                    self.sessions.insert(session.to_owned(), target);
                }
                Tell::Pass
            }
            None => Tell::Pass,
        }
    }

    fn event(
        &mut self,
        method: &str,
        text: &str,
        place: impl Fn(&str) -> Option<i64>,
        now: Instant,
    ) -> Tell {
        match method {
            "Target.targetCreated" | "Target.targetInfoChanged" => {
                let params = params(text);
                let info = &params["targetInfo"];
                let Some(target) = info["targetId"].as_str() else {
                    return Tell::Drop;
                };
                if info["type"] != "page" {
                    return Tell::Drop;
                }
                match self.whose(target, place) {
                    Whose::Ours => {
                        self.told.insert(target.to_owned());
                        Tell::Pass
                    }
                    Whose::Theirs => Tell::Drop,
                    Whose::Unknown => {
                        if self.held.len() == MOST_HELD {
                            self.held.pop_front();
                        }
                        self.held.push_back(Held {
                            target: target.to_owned(),
                            message: text.to_owned(),
                            at: now,
                        });
                        Tell::Drop
                    }
                }
            }
            "Target.targetDestroyed" => {
                let params = params(text);
                let Some(target) = params["targetId"].as_str() else {
                    return Tell::Drop;
                };
                self.held.retain(|held| held.target != target);
                self.opened.remove(target);
                if self.told.remove(target) {
                    Tell::Pass
                } else {
                    Tell::Drop
                }
            }
            "Target.targetCrashed" => {
                let told = params(text)["targetId"]
                    .as_str()
                    .is_some_and(|target| self.told.contains(target));
                if told { Tell::Pass } else { Tell::Drop }
            }
            "Target.attachedToTarget" => {
                let params = params(text);
                let (Some(session), Some(target)) = (
                    params["sessionId"].as_str(),
                    params["targetInfo"]["targetId"].as_str(),
                ) else {
                    return Tell::Drop;
                };
                if self.whose(target, place) == Whose::Ours {
                    self.sessions.insert(session.to_owned(), target.to_owned());
                    Tell::Pass
                } else {
                    Tell::Drop
                }
            }
            "Target.detachedFromTarget" => {
                let gone = params(text)["sessionId"]
                    .as_str()
                    .is_some_and(|session| self.sessions.remove(session).is_some());
                if gone { Tell::Pass } else { Tell::Drop }
            }
            "Target.receivedMessageFromTarget" => {
                let ours = params(text)["sessionId"]
                    .as_str()
                    .is_some_and(|session| self.sessions.contains_key(session));
                if ours { Tell::Pass } else { Tell::Drop }
            }
            _ => Tell::Pass,
        }
    }

    /// The window's pages, with its active tab first, which a driver takes when it starts.
    fn targets(
        &mut self,
        text: &str,
        place: impl Fn(&str) -> Option<i64>,
        active: Option<&str>,
    ) -> Tell {
        let Ok(mut answer) = serde_json::from_str::<Value>(text) else {
            return Tell::Drop;
        };
        let Some(infos) = answer["result"]["targetInfos"].as_array_mut() else {
            return Tell::Pass;
        };
        infos.retain(|info| {
            info["type"] == "page"
                && info["targetId"]
                    .as_str()
                    .is_some_and(|target| self.whose(target, &place) == Whose::Ours)
        });
        infos.sort_by_key(|info| info["targetId"].as_str() != active);
        for info in infos.iter() {
            if let Some(target) = info["targetId"].as_str() {
                self.told.insert(target.to_owned());
            }
        }
        Tell::Changed(answer.to_string())
    }

    /// News of pages that are now placed, in their order. The rest wait a little longer.
    fn release(&mut self, place: impl Fn(&str) -> Option<i64>, now: Instant) -> Vec<String> {
        let mut released = Vec::new();
        for held in std::mem::take(&mut self.held) {
            match self.whose(&held.target, &place) {
                Whose::Ours => {
                    self.told.insert(held.target);
                    released.push(held.message);
                }
                Whose::Unknown if now.duration_since(held.at) < HOLD_LIMIT => {
                    self.held.push_back(held);
                }
                Whose::Theirs | Whose::Unknown => {}
            }
        }
        released
    }

    /// The answer to a driver's new tab.
    fn opened(&mut self, id: u64, target: Option<String>) -> String {
        match target {
            Some(target) => {
                let result = json!({ "targetId": target });
                self.opened.insert(target);
                answer(id, &result)
            }
            None => failure(id, None, "The tab did not open."),
        }
    }
}

fn params(text: &str) -> Value {
    serde_json::from_str::<Value>(text)
        .map(|mut message| message["params"].take())
        .unwrap_or_default()
}

fn answer(id: u64, result: &Value) -> String {
    json!({ "id": id, "result": result }).to_string()
}

fn answer_in(id: u64, session: &str, result: &Value) -> String {
    json!({ "id": id, "sessionId": session, "result": result }).to_string()
}

fn failure(id: u64, session: Option<&str>, message: &str) -> String {
    let mut failure = json!({ "id": id, "error": { "code": -32000, "message": message } });
    if let Some(session) = session {
        failure["sessionId"] = session.into();
    }
    failure.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINE: i64 = 1;

    fn place(target: &str) -> Option<i64> {
        match target {
            "A" | "B" => Some(MINE),
            "X" => Some(2),
            _ => None,
        }
    }

    fn page(target: &str) -> Value {
        json!({ "targetId": target, "type": "page", "url": "about:blank" })
    }

    fn told(view: &mut View, text: &Value) -> Tell {
        view.tell(&text.to_string(), place, || None, Instant::now())
    }

    fn attached(view: &mut View) {
        let asked = json!({"id": 3, "method": "Target.attachToTarget", "params": {"targetId": "A", "flatten": true}});
        assert_eq!(view.ask(&asked.to_string(), place), Ask::Pass);
        let answered = json!({"id": 3, "result": {"sessionId": "S"}});
        assert_eq!(told(view, &answered), Tell::Pass);
    }

    #[test]
    fn a_driver_sees_the_pages_of_its_window_with_the_active_one_first() {
        let mut view = View::new(MINE);
        let asked = json!({"id": 1, "method": "Target.getTargets"});
        assert_eq!(view.ask(&asked.to_string(), place), Ask::Pass);
        let answered = json!({"id": 1, "result": {"targetInfos": [
            page("A"), page("X"), page("B"), page("host"),
            {"targetId": "U", "type": "browser_ui"},
        ]}});
        let Tell::Changed(changed) = view.tell(
            &answered.to_string(),
            place,
            || Some("B".to_owned()),
            Instant::now(),
        ) else {
            panic!("the answer must change");
        };
        let changed: Value = serde_json::from_str(&changed).unwrap();
        let targets: Vec<&str> = changed["result"]["targetInfos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|info| info["targetId"].as_str().unwrap())
            .collect();
        assert_eq!(targets, ["B", "A"]);
    }

    #[test]
    fn a_tab_of_another_window_is_out_of_reach() {
        let mut view = View::new(MINE);
        for method in [
            "Target.attachToTarget",
            "Target.activateTarget",
            "Target.closeTarget",
        ] {
            let asked = json!({"id": 4, "method": method, "params": {"targetId": "X"}});
            let Ask::Answer(answer) = view.ask(&asked.to_string(), place) else {
                panic!("{method} must be refused");
            };
            assert!(answer.contains("No target"));
        }
        let shown =
            json!({"id": 5, "method": "Target.activateTarget", "params": {"targetId": "B"}});
        assert_eq!(
            view.ask(&shown.to_string(), place),
            Ask::Show("B".to_owned())
        );
        let created =
            json!({"method": "Target.targetCreated", "params": {"targetInfo": page("X")}});
        assert_eq!(told(&mut view, &created), Tell::Drop);
    }

    #[test]
    fn only_the_sessions_of_its_own_tabs_reach_the_driver() {
        let mut view = View::new(MINE);
        let foreign = json!({"id": 6, "sessionId": "T", "method": "Page.navigate", "params": {"url": "about:blank"}});
        assert!(matches!(
            view.ask(&foreign.to_string(), place),
            Ask::Answer(_)
        ));
        attached(&mut view);
        let own = json!({"id": 7, "sessionId": "S", "method": "Page.navigate", "params": {"url": "about:blank"}});
        assert_eq!(view.ask(&own.to_string(), place), Ask::Pass);
        let front = json!({"id": 15, "sessionId": "S", "method": "Page.bringToFront"});
        assert_eq!(
            view.ask(&front.to_string(), place),
            Ask::Show("A".to_owned())
        );
        let dialog = json!({"id": 16, "sessionId": "S", "method": "Page.handleJavaScriptDialog", "params": {"accept": true, "promptText": "yes"}});
        assert_eq!(
            view.ask(&dialog.to_string(), place),
            Ask::Dialog {
                target: "A".to_owned(),
                accept: true,
                text: Some("yes".to_owned()),
                answer: answer_in(16, "S", &json!({})),
            }
        );
        let child = json!({"sessionId": "S", "method": "Target.attachedToTarget", "params": {"sessionId": "C", "targetInfo": {"targetId": "F", "type": "iframe"}}});
        assert_eq!(told(&mut view, &child), Tell::Pass);
        let from_child =
            json!({"sessionId": "C", "method": "Runtime.consoleAPICalled", "params": {}});
        assert_eq!(told(&mut view, &from_child), Tell::Pass);
        let from_other =
            json!({"sessionId": "T", "method": "Runtime.consoleAPICalled", "params": {}});
        assert_eq!(told(&mut view, &from_other), Tell::Drop);
        let gone = json!({"method": "Target.detachedFromTarget", "params": {"sessionId": "S"}});
        assert_eq!(told(&mut view, &gone), Tell::Pass);
        assert!(matches!(view.ask(&own.to_string(), place), Ask::Answer(_)));
    }

    #[test]
    fn a_new_page_waits_until_the_engine_places_it() {
        let mut view = View::new(MINE);
        let now = Instant::now();
        for target in ["N", "M", "Q"] {
            let created =
                json!({"method": "Target.targetCreated", "params": {"targetInfo": page(target)}});
            assert_eq!(
                view.tell(&created.to_string(), place, || None, now),
                Tell::Drop
            );
        }
        let placed = |target: &str| match target {
            "N" => Some(MINE),
            "M" => Some(2),
            _ => None,
        };
        let released = view.release(placed, now);
        assert_eq!(released.len(), 1);
        assert!(released[0].contains("\"N\""));
        assert_eq!(view.release(placed, now + HOLD_LIMIT), Vec::<String>::new());
        assert_eq!(view.held.len(), 0);
        let destroyed = json!({"method": "Target.targetDestroyed", "params": {"targetId": "N"}});
        assert_eq!(
            view.tell(&destroyed.to_string(), placed, || None, now),
            Tell::Pass
        );
        let unknown = json!({"method": "Target.targetDestroyed", "params": {"targetId": "M"}});
        assert_eq!(
            view.tell(&unknown.to_string(), placed, || None, now),
            Tell::Drop
        );
    }

    #[test]
    fn a_new_tab_opens_in_the_window_and_is_its_own_at_once() {
        let mut view = View::new(MINE);
        let asked = json!({"id": 8, "method": "Target.createTarget", "params": {"url": "https://example.com/"}});
        assert_eq!(
            view.ask(&asked.to_string(), place),
            Ask::Open {
                id: 8,
                address: "https://example.com/".to_owned()
            }
        );
        let blank = json!({"id": 9, "method": "Target.createTarget", "params": {"url": ""}});
        assert_eq!(
            view.ask(&blank.to_string(), place),
            Ask::Open {
                id: 9,
                address: BLANK.to_owned()
            }
        );
        assert!(
            view.opened(8, Some("N".to_owned()))
                .contains("\"targetId\":\"N\"")
        );
        let attach = json!({"id": 10, "method": "Target.attachToTarget", "params": {"targetId": "N", "flatten": true}});
        assert_eq!(view.ask(&attach.to_string(), place), Ask::Pass);
        assert!(view.opened(9, None).contains("error"));
    }

    #[test]
    fn commands_for_the_whole_browser_stay_with_kodosi() {
        let mut view = View::new(MINE);
        let downloads = json!({"id": 11, "method": "Browser.setDownloadBehavior", "params": {"behavior": "allow", "downloadPath": "/tmp"}});
        assert_eq!(
            view.ask(&downloads.to_string(), place),
            Ask::Answer(answer(11, &json!({})))
        );
        let auto = json!({"id": 12, "method": "Target.setAutoAttach", "params": {"autoAttach": true, "waitForDebuggerOnStart": true}});
        assert_eq!(
            view.ask(&auto.to_string(), place),
            Ask::Answer(answer(12, &json!({})))
        );
        let close = json!({"id": 13, "method": "Browser.close"});
        assert!(matches!(
            view.ask(&close.to_string(), place),
            Ask::Answer(answer) if answer.contains("error")
        ));
        let version = json!({"id": 14, "method": "Browser.getVersion"});
        assert_eq!(view.ask(&version.to_string(), place), Ask::Pass);
    }
}
