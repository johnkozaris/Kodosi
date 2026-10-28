use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::atomic::AtomicU64,
};

use kodosi_runtime::local_host::{self, TerminalFrame};
use serde_json::{Value, json};
use tokio::sync::mpsc::{self, error::TrySendError};

use super::{
    Arc, Bytes, CommandEnvelope, Config, Duration, Error, HashMap, Instance, KODOSI_FFI_BUSY,
    KODOSI_FFI_DESER_FAILED, KODOSI_FFI_OK, KODOSI_FFI_PAYLOAD_TOO_LARGE,
    KODOSI_FFI_RUNTIME_STOPPED, KODOSI_FFI_SESSION_NOT_FOUND,
    KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED, KodosiStartFailure, Mutex, Ordering, State,
    SubscriptionState, Uuid, code, lock, start_error, terminal,
};

const COMMAND_QUEUE: usize = 128;
const MAX_PENDING: usize = 256;
const TERMINAL_QUEUE: usize = 64;
const RECONNECT_FIRST: Duration = Duration::from_millis(250);
const RECONNECT_LAST: Duration = Duration::from_secs(30);
const SWEEP: Duration = Duration::from_secs(1);
const LOST: &str =
    "The Kodosi host stopped before it answered, and Kodosi did not send the request again.";
const LATE: &str = "No answer arrived in time, and Kodosi did not send the request again.";

pub(super) struct Link {
    queue: mpsc::Sender<Value>,
    pending: Arc<Pending>,
    root: PathBuf,
}

pub(super) struct Runner {
    client: local_host::Client,
    queue: mpsc::Receiver<Value>,
    pending: Arc<Pending>,
    root: PathBuf,
}

pub(super) struct Connected {
    pub(super) incarnation: Uuid,
    outbox: mpsc::Sender<Outgoing>,
    unacknowledged: Arc<Mutex<Unacknowledged>>,
}

enum Outgoing {
    Input(Bytes),
    Control(Value),
}

#[derive(Default)]
struct Unacknowledged {
    bytes: usize,
    frames: VecDeque<usize>,
}

struct Request {
    operation: &'static str,
    deadline: tokio::time::Instant,
}

#[derive(Default)]
struct Pending {
    requests: Mutex<HashMap<String, Request>>,
    scope: Mutex<(Option<String>, u64)>,
}

impl Pending {
    fn scoped(&self, mut event: Value) -> Value {
        let (user, epoch) = lock(&self.scope).clone();
        event["accountUserId"] = json!(user);
        event["accountEpoch"] = json!(epoch);
        event
    }
    fn deliver(&self, state: &State, event: &Value) {
        if let Some(request) = event.get("requestId").and_then(Value::as_str) {
            let kind = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mut requests = lock(&self.requests);
            let waiting = if kodosi_runtime::protocol::is_final(kind) {
                requests.remove(request).is_some()
            } else {
                requests.contains_key(request)
            };
            drop(requests);
            if !waiting {
                return;
            }
        }
        if let Some(epoch) = event.get("accountEpoch").and_then(Value::as_u64) {
            let user = event
                .get("accountUserId")
                .and_then(Value::as_str)
                .map(str::to_owned);
            *lock(&self.scope) = (user, epoch);
        }
        state.event(event);
    }
    fn refuse(&self, state: &State, request: &str, operation: &str, message: &str) {
        let area = operation.split('.').next().unwrap_or(operation);
        state.event(&self.scoped(json!({"type":format!("{area}.error"),
            "requestId":request,"operation":operation,"message":message})));
    }
    fn fail(&self, state: &State, request: &str, message: &str) {
        let entry = lock(&self.requests).remove(request);
        if let Some(entry) = entry {
            self.refuse(state, request, entry.operation, message);
        }
    }
    fn fail_all(&self, state: &State, message: &str) {
        let requests: Vec<_> = lock(&self.requests).drain().collect();
        for (request, entry) in requests {
            self.refuse(state, &request, entry.operation, message);
        }
    }
    fn expire(&self, state: &State) {
        let now = tokio::time::Instant::now();
        let due: Vec<_> = lock(&self.requests)
            .extract_if(|_, entry| entry.deadline <= now)
            .collect();
        for (request, entry) in due {
            self.refuse(state, &request, entry.operation, LATE);
        }
    }
    fn system_error(&self, state: &State, message: &str) {
        state.event(&self.scoped(json!({"type":"system.error","message":message})));
    }
}

impl Link {
    pub(super) async fn open(
        config: &Config,
    ) -> std::result::Result<(Self, Runner), Box<KodosiStartFailure>> {
        let client = local_host::connect_or_start(&config.data_root)
            .await
            .map_err(|error| {
                tracing::error!(%error, "could not reach the Kodosi host");
                start_error(&error)
            })?;
        let (queue, receiver) = mpsc::channel(COMMAND_QUEUE);
        let pending = Arc::new(Pending::default());
        let link = Self {
            queue,
            pending: Arc::clone(&pending),
            root: config.data_root.clone(),
        };
        let runner = Runner {
            client,
            queue: receiver,
            pending,
            root: config.data_root.clone(),
        };
        Ok((link, runner))
    }

    pub(super) fn command(&self, value: Value, envelope: &CommandEnvelope) -> i32 {
        let request = envelope.command.request_id().to_owned();
        let operation = envelope.command.operation();
        {
            let mut requests = lock(&self.pending.requests);
            if requests.contains_key(&request) {
                return KODOSI_FFI_DESER_FAILED;
            }
            if requests.len() >= MAX_PENDING {
                return KODOSI_FFI_BUSY;
            }
            requests.insert(
                request.clone(),
                Request {
                    operation,
                    deadline: tokio::time::Instant::now()
                        + kodosi_runtime::protocol::answer_time(operation),
                },
            );
        }
        match self.queue.try_send(value) {
            Ok(()) => KODOSI_FFI_OK,
            Err(error) => {
                lock(&self.pending.requests).remove(&request);
                match error {
                    TrySendError::Full(_) => KODOSI_FFI_BUSY,
                    TrySendError::Closed(_) => KODOSI_FFI_RUNTIME_STOPPED,
                }
            }
        }
    }

    pub(super) fn connect_terminal(
        &self,
        instance: &Instance,
        session: Uuid,
        subscription: Arc<SubscriptionState>,
    ) -> bool {
        let root = self.root.clone();
        let state = Arc::clone(&instance.state);
        let pending = Arc::clone(&self.pending);
        instance.spawn(async move {
            tokio::select! {
                biased;
                () = subscription.cancel.cancelled() => {}
                () = stream_terminal(&root, &state, &pending, session, &subscription) => {}
            }
            subscription.retire();
            lock(&subscription.connected).take();
            state.remove(session, &subscription);
        })
    }
}

pub(super) fn stopped(_: &Link) -> std::future::Pending<()> {
    std::future::pending()
}

pub(super) fn shutdown(_: &Link) -> std::future::Ready<()> {
    std::future::ready(())
}

impl Connected {
    pub(super) fn control(
        &self,
        _instance: &Instance,
        _subscription: &Arc<SubscriptionState>,
        envelope: &CommandEnvelope,
    ) -> i32 {
        let Ok(value) = serde_json::to_value(envelope) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        match self.outbox.try_send(Outgoing::Control(value)) {
            Ok(()) => KODOSI_FFI_OK,
            Err(TrySendError::Full(_)) => KODOSI_FFI_BUSY,
            Err(TrySendError::Closed(_)) => KODOSI_FFI_SESSION_NOT_FOUND,
        }
    }

    pub(super) fn input(&self, _instance: &Instance, _session: Uuid, bytes: &[u8]) -> i32 {
        if bytes.len() > terminal::TERMINAL_INPUT_MAX_BYTES {
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }
        let mut waiting = lock(&self.unacknowledged);
        if waiting.bytes + bytes.len() > terminal::TERMINAL_INPUT_MAX_BYTES {
            return KODOSI_FFI_BUSY;
        }
        match self
            .outbox
            .try_send(Outgoing::Input(Bytes::copy_from_slice(bytes)))
        {
            Ok(()) => {
                waiting.bytes += bytes.len();
                waiting.frames.push_back(bytes.len());
                KODOSI_FFI_OK
            }
            Err(TrySendError::Full(_)) => KODOSI_FFI_BUSY,
            Err(TrySendError::Closed(_)) => KODOSI_FFI_SESSION_NOT_FOUND,
        }
    }
}

impl Runner {
    pub(super) fn spawn(self, instance: &Instance) -> bool {
        let pending = Arc::clone(&self.pending);
        let state = Arc::clone(&instance.state);
        let swept = Arc::clone(&instance.state);
        instance.spawn(async move {
            let mut tick = tokio::time::interval(SWEEP);
            loop {
                tokio::select! {
                    biased;
                    () = swept.cancel.cancelled() => return,
                    _ = tick.tick() => pending.expire(&swept),
                }
            }
        }) && instance.spawn(async move {
            let cancel = state.cancel.clone();
            tokio::select! {
                biased;
                () = cancel.cancelled() => {}
                () = self.run(&state) => {}
            }
        })
    }

    async fn run(self, state: &State) {
        let Self {
            client,
            mut queue,
            pending,
            root,
        } = self;
        let mut client = Some(client);
        loop {
            let current = match client.take() {
                Some(current) => current,
                None => match reconnect(&root, &mut queue, &pending, state).await {
                    Some(current) => current,
                    None => return,
                },
            };
            if !serve(current, &mut queue, &pending, state).await {
                return;
            }
            pending.fail_all(state, LOST);
            pending.system_error(
                state,
                "The Kodosi host stopped, and its terminals ended. Kodosi starts it again.",
            );
        }
    }
}

async fn serve(
    mut client: local_host::Client,
    queue: &mut mpsc::Receiver<Value>,
    pending: &Pending,
    state: &State,
) -> bool {
    for event in std::mem::take(&mut client.initial_events) {
        pending.deliver(state, &event);
    }
    let (mut events, mut commands) = client.split();
    let reading = async {
        loop {
            match events.next().await {
                Ok(frame) => receive(&frame, pending, state),
                Err(error) => {
                    tracing::info!(%error, "the connection to the Kodosi host closed");
                    return;
                }
            }
        }
    };
    let writing = async {
        while let Some(command) = queue.recv().await {
            let request = command
                .get("requestId")
                .and_then(Value::as_str)
                .map(str::to_owned);
            match commands.command(command).await {
                Ok(()) => {}
                Err(Error::Invalid(message)) => {
                    if let Some(request) = request {
                        pending.fail(state, &request, &message);
                    }
                }
                Err(error) => {
                    tracing::info!(%error, "a command did not reach the Kodosi host");
                    return true;
                }
            }
        }
        false
    };
    tokio::select! {
        () = reading => true,
        open = writing => open,
    }
}

fn receive(frame: &Value, pending: &Pending, state: &State) {
    match frame.get("kind").and_then(Value::as_str) {
        Some("event") => {
            if let Some(event) = frame.get("event") {
                pending.deliver(state, event);
            }
        }
        Some("rejected") => {
            if let (Some(request), Some(message)) = (
                frame.get("requestId").and_then(Value::as_str),
                frame.get("message").and_then(Value::as_str),
            ) {
                pending.fail(state, request, message);
            }
        }
        _ => {}
    }
}

async fn reconnect(
    root: &Path,
    queue: &mut mpsc::Receiver<Value>,
    pending: &Pending,
    state: &State,
) -> Option<local_host::Client> {
    let mut delay = RECONNECT_FIRST;
    let mut reported = false;
    loop {
        let attempt = local_host::connect_or_start(root);
        tokio::pin!(attempt);
        let result = loop {
            tokio::select! {
                result = &mut attempt => break result,
                command = queue.recv() => refuse(&command?, pending, state),
            }
        };
        match result {
            Ok(client) => return Some(client),
            Err(error) => {
                tracing::warn!(%error, "could not start the Kodosi host again");
                if !reported {
                    reported = true;
                    pending
                        .system_error(state, &format!("Kodosi could not start its host: {error}"));
                }
            }
        }
        let pause = tokio::time::sleep(delay);
        tokio::pin!(pause);
        loop {
            tokio::select! {
                () = &mut pause => break,
                command = queue.recv() => refuse(&command?, pending, state),
            }
        }
        delay = (delay * 2).min(RECONNECT_LAST);
    }
}

fn refuse(command: &Value, pending: &Pending, state: &State) {
    if let Some(request) = command.get("requestId").and_then(Value::as_str) {
        pending.fail(state, request, LOST);
    }
}

async fn stream_terminal(
    root: &Path,
    state: &State,
    pending: &Pending,
    session: Uuid,
    token: &SubscriptionState,
) {
    let mut client = match local_host::TerminalClient::connect(root, session).await {
        Ok(client) => client,
        Err(error) => {
            state.result(token, connect_code(&error));
            return;
        }
    };
    let restored = match local_host::read_terminal(&mut client.reader).await {
        Ok(TerminalFrame::Checkpoint {
            bytes,
            console,
            next_sequence,
        }) => restore(state, token, bytes, console, next_sequence).then_some(next_sequence),
        _ => None,
    };
    let Some(first) = restored else {
        state.result(token, KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED);
        return;
    };
    let next = AtomicU64::new(first);
    let (outbox, mut outgoing) = mpsc::channel(TERMINAL_QUEUE);
    let unacknowledged = Arc::new(Mutex::new(Unacknowledged::default()));
    *lock(&token.connected) = Some(Connected {
        incarnation: client.incarnation_id,
        outbox,
        unacknowledged: Arc::clone(&unacknowledged),
    });
    state.result(token, KODOSI_FFI_OK);
    let local_host::TerminalClient {
        mut reader,
        mut writer,
        ..
    } = client;
    let (reason, final_sequence) = tokio::select! {
        closed = read_frames(&mut reader, state, pending, token, &next, &unacknowledged) => closed,
        () = write_frames(&mut writer, session, token, &mut outgoing) => ("the terminal connection closed".to_owned(), next.load(Ordering::Acquire)),
    };
    state.control(
        token,
        &json!({"type":"term.closed","reason":reason,"finalSequence":final_sequence}),
    );
}

async fn read_frames<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut local_host::FrameReader<R>,
    state: &State,
    pending: &Pending,
    token: &SubscriptionState,
    next: &AtomicU64,
    unacknowledged: &Mutex<Unacknowledged>,
) -> (String, u64) {
    loop {
        let frame = match local_host::read_terminal(reader).await {
            Ok(frame) => frame,
            Err(error) => return (error.to_string(), next.load(Ordering::Acquire)),
        };
        match frame {
            TerminalFrame::Data { bytes, sequence } => {
                next.store(sequence.saturating_add(1), Ordering::Release);
                state.data(token, sequence, &bytes);
            }
            TerminalFrame::Resize {
                rows,
                cols,
                cell,
                at_sequence,
            } => state.control(
                token,
                &json!({"type":"term.resize","rows":rows,"cols":cols,
                    "cellWidthPixels":cell.width,"cellHeightPixels":cell.height,"atSequence":at_sequence}),
            ),
            TerminalFrame::Checkpoint {
                bytes,
                console,
                next_sequence,
            } => {
                if !restore(state, token, bytes, console, next_sequence) {
                    return (
                        "the terminal snapshot could not be restored".to_owned(),
                        next.load(Ordering::Acquire),
                    );
                }
                next.store(next_sequence, Ordering::Release);
            }
            TerminalFrame::Closed {
                reason,
                final_sequence,
            } => return (reason, final_sequence),
            TerminalFrame::Answer(answer) => state.control(token, &answer),
            TerminalFrame::InputAck { accepted, message } => {
                {
                    let mut waiting = lock(unacknowledged);
                    if let Some(size) = waiting.frames.pop_front() {
                        waiting.bytes -= size;
                    }
                }
                if !accepted {
                    pending.system_error(
                        state,
                        &format!(
                            "The terminal did not confirm the input, and Kodosi did not send it again: {}",
                            message.as_deref().unwrap_or("the host refused it")
                        ),
                    );
                }
            }
        }
    }
}

async fn write_frames<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    session: Uuid,
    token: &SubscriptionState,
    outgoing: &mut mpsc::Receiver<Outgoing>,
) {
    loop {
        let written = tokio::select! {
            () = token.refresh.notified() => local_host::write_refresh(writer).await,
            item = outgoing.recv() => match item {
                Some(Outgoing::Input(bytes)) => local_host::write_input(writer, &bytes).await,
                Some(Outgoing::Control(command)) => local_host::write_control(writer, &command).await,
                None => return,
            },
        };
        if let Err(error) = written {
            tracing::info!(%session, %error, "the terminal connection closed");
            return;
        }
    }
}

fn restore(
    state: &State,
    token: &SubscriptionState,
    bytes: Vec<u8>,
    console: bool,
    next_sequence: u64,
) -> bool {
    terminal::Checkpoint::new(bytes, console)
        .is_ok_and(|checkpoint| state.checkpoint(token, &checkpoint, next_sequence))
}

fn connect_code(error: &Error) -> i32 {
    match error {
        Error::Io(_) => KODOSI_FFI_RUNTIME_STOPPED,
        error => code(error),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{CancellationToken, Gate, KodosiCallbacks, Subscriptions, UserData, c_void};

    unsafe extern "C" fn record(json: *const u8, length: usize, userdata: *mut c_void) {
        let events = unsafe { &*userdata.cast::<Mutex<Vec<Value>>>() };
        let json = unsafe { std::slice::from_raw_parts(json, length) };
        lock(events).push(serde_json::from_slice(json).unwrap());
    }

    #[test]
    fn each_request_hears_one_answer_in_the_current_account() {
        let events = Mutex::new(Vec::<Value>::new());
        let state = State {
            callbacks: KodosiCallbacks {
                on_event: Some(record),
                on_terminal_data: None,
                on_terminal_control: None,
                on_terminal_connect_result: None,
                on_terminal_checkpoint: None,
            },
            userdata: UserData((&raw const events).cast_mut().cast()),
            gate: Gate::default(),
            serial: Mutex::new(()),
            cancel: CancellationToken::new(),
            subscriptions: Mutex::new(Subscriptions::default()),
        };
        let pending = Pending::default();
        let now = tokio::time::Instant::now();
        for (request, operation, deadline) in [
            ("asked", "friends.refresh", now + SWEEP),
            ("waiting", "auth.login.start", now),
        ] {
            lock(&pending.requests).insert(
                request.to_owned(),
                Request {
                    operation,
                    deadline,
                },
            );
        }
        let answer = json!({"type":"friends.result","requestId":"asked","accountUserId":"me","accountEpoch":7});
        pending.deliver(
            &state,
            &json!({"type":"auth.ready","accountUserId":"me","accountEpoch":7,"userId":"me"}),
        );
        pending.deliver(&state, &answer);
        pending.deliver(&state, &answer);
        pending.deliver(&state, &json!({"type":"friends.result","requestId":"foreign","accountUserId":"me","accountEpoch":7}));
        pending.expire(&state);
        pending.deliver(&state, &json!({"type":"auth.browser","requestId":"waiting","accountUserId":"me","accountEpoch":7}));
        let heard = lock(&events).clone();
        assert_eq!(heard.len(), 3, "{heard:?}");
        assert_eq!(heard[1], answer);
        assert_eq!(
            heard[2],
            json!({"type":"auth.error","requestId":"waiting","operation":"auth.login.start",
                "message":LATE,"accountUserId":"me","accountEpoch":7})
        );
    }
}
