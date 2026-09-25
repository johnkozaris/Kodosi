use crate::{Config, Error, EventBody, HostKind, Result, local_host, protocol::SessionKind};
use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use uuid::Uuid;

mod terminal;

#[derive(Parser)]
#[command(
    name = "kodosi",
    version,
    about = "Real terminals on your machines, shared with trusted people."
)]
struct Arguments {
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    #[command(about = "Run the local terminal host until interrupted.")]
    Host,
    #[command(about = "Show the current local runtime and sessions.")]
    Status,
    #[command(subcommand)]
    Session(SessionAction),
    #[command(subcommand)]
    Auth(AuthAction),
    #[command(subcommand)]
    Devices(DeviceAction),
    #[command(subcommand)]
    Friends(FriendAction),
    #[command(subcommand)]
    Mission(MissionAction),
    #[command(subcommand)]
    Provider(ProviderAction),
    #[command(hide = true)]
    InternalHost,
}
#[derive(Subcommand)]
enum SessionAction {
    List,
    Show {
        session: Uuid,
    },
    Start {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        directory: Option<PathBuf>,
    },
    Resume {
        provider: String,
        conversation: Uuid,
        directory: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    Attach {
        session: Uuid,
    },
    Input {
        session: Uuid,
        text: String,
        #[arg(long)]
        enter: bool,
    },
    Rename {
        session: Uuid,
        name: String,
    },
    Close {
        session: Uuid,
    },
    Interrupt {
        session: Uuid,
    },
    Share {
        session: Uuid,
        #[arg(required = false)]
        users: Vec<Uuid>,
    },
    Leave {
        session: Uuid,
    },
    Mission {
        session: Uuid,
        mission: Option<Uuid>,
    },
}
#[derive(Subcommand)]
enum AuthAction {
    Login,
    Logout,
    Status,
}
#[derive(Subcommand)]
enum DeviceAction {
    List,
    Link,
    CancelLink,
    Approve { code: String },
    Revoke { device: String },
}
#[derive(Subcommand)]
enum FriendAction {
    List,
    Add { username: String },
    Accept { username: String },
    Reject { username: String },
    Cancel { username: String },
    Remove { username: String },
}
#[derive(Subcommand)]
enum MissionAction {
    List,
    Show { mission: Uuid },
    Create { name: String },
    Rename { mission: Uuid, name: String },
    Delete { mission: Uuid },
    Invite { mission: Uuid, user: Uuid },
    Accept { invitation: Uuid },
    Reject { invitation: Uuid },
    RemoveMember { mission: Uuid, user: Uuid },
    Leave { mission: Uuid },
}
#[derive(Subcommand)]
enum ProviderAction {
    Inspect {
        provider: String,
        #[arg(long)]
        directory: Option<PathBuf>,
    },
    History {
        provider: String,
        directory: PathBuf,
    },
    Read {
        provider: String,
        conversation: Uuid,
        directory: PathBuf,
    },
}

#[expect(
    clippy::future_not_send,
    reason = "CLI execution keeps the Ghostty mirror on the main thread"
)]
pub async fn run_with(args: Vec<OsString>) -> u8 {
    let args = match Arguments::try_parse_from(args) {
        Ok(args) => args,
        Err(error) => {
            drop(error.print());
            return u8::try_from(error.exit_code()).unwrap_or(2);
        }
    };
    let json_output = args.json;
    match dispatch(args).await {
        Ok(()) => 0,
        Err(error) => {
            if json_output {
                drop(print_json(&json!({"error":error.to_string()})));
            } else {
                drop(writeln!(io::stderr().lock(), "Kodosi: {error}"));
            }
            1
        }
    }
}

pub fn is_invocation(args: &[OsString]) -> bool {
    if args
        .first()
        .map(Path::new)
        .and_then(Path::file_name)
        .is_some_and(|name| name == "kodosi")
    {
        return true;
    }
    let Some(first) = args.get(1).and_then(|argument| argument.to_str()) else {
        return false;
    };
    matches!(
        first,
        "--help" | "-h" | "--version" | "-V" | "--json" | "help"
    ) || Arguments::command()
        .get_subcommands()
        .any(|command| command.get_name() == first)
}

const fn hosted_kind(action: &Action) -> Option<HostKind> {
    match action {
        Action::Host => Some(HostKind::Foreground),
        Action::InternalHost => Some(HostKind::Background),
        _ => None,
    }
}

#[expect(
    clippy::future_not_send,
    reason = "terminal attach runs the Ghostty mirror on the main thread"
)]
async fn dispatch(args: Arguments) -> Result<()> {
    let mut config = Config::load()?;
    if let Some(kind) = hosted_kind(&args.command) {
        config.host = kind;
        return run_host(config, args.json).await;
    }
    let mut client = connect_or_start(&config).await?;
    let mut context = Context::from_events(&client.initial_events);
    if matches!(
        args.command,
        Action::Status | Action::Session(SessionAction::List) | Action::Auth(AuthAction::Status)
    ) {
        return print_json(
            &json!({"accountUserId":context.user,"accountEpoch":context.epoch,"sessions":context.sessions}),
        );
    }
    if let Action::Session(SessionAction::Show { session }) = &args.command {
        return print_json(context.session(*session)?);
    }
    let request = Uuid::now_v7().to_string();
    let command = match args.command {
        Action::Session(SessionAction::Attach { session }) => {
            open_remote(&mut client, &mut context, session).await?;
            return attach_with_demand(&config.data_root, session, &mut client).await;
        }
        Action::Session(SessionAction::Input {
            session,
            text,
            enter,
        }) => {
            open_remote(&mut client, &mut context, session).await?;
            return send_input(&config.data_root, session, text, enter, args.json).await;
        }
        Action::Session(SessionAction::Start { name, directory }) => {
            json!({"type":"session.create","requestId":request,"name":name.unwrap_or_else(||"Terminal".into()),"workingDir":path(directory)?})
        }
        Action::Session(SessionAction::Resume {
            provider,
            conversation,
            directory,
            name,
        }) => {
            json!({"type":"session.create","requestId":request,"name":name.unwrap_or_else(||"Terminal".into()),"workingDir":path(Some(directory))?,"resume":{"provider":provider_name(provider)?,"nativeConversationId":conversation}})
        }
        Action::Session(SessionAction::Rename { session, name }) => {
            json!({"type":"session.rename","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?,"name":name})
        }
        Action::Session(SessionAction::Close { session }) => {
            open_remote(&mut client, &mut context, session).await?;
            json!({"type":"session.close","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?})
        }
        Action::Session(SessionAction::Interrupt { session }) => {
            open_remote(&mut client, &mut context, session).await?;
            json!({"type":"session.interrupt","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?})
        }
        Action::Session(SessionAction::Share { session, users }) => {
            json!({"type":"session.share","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?,"userIds":users})
        }
        Action::Session(SessionAction::Leave { session }) => {
            json!({"type":"session.leave","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?})
        }
        Action::Session(SessionAction::Mission { session, mission }) => {
            json!({"type":"session.attachMission","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?,"missionId":mission})
        }
        Action::Auth(AuthAction::Login) => json!({"type":"auth.login.start"}),
        Action::Auth(AuthAction::Logout) => json!({"type":"auth.logout"}),
        Action::Devices(action) => device_command(action),
        Action::Friends(action) => friend_command(action, &request),
        Action::Mission(action) => mission_command(action, &request),
        Action::Provider(action) => provider_command(action, &request)?,
        Action::Host
        | Action::InternalHost
        | Action::Status
        | Action::Session(SessionAction::List | SessionAction::Show { .. })
        | Action::Auth(AuthAction::Status) => {
            return Err(Error::Invalid("unexpected command".into()));
        }
    };
    let operation = command["type"]
        .as_str()
        .ok_or_else(|| Error::Invalid("command has no type".into()))?
        .to_owned();
    let request_id = command
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    client.command(context.envelope(command)?).await?;
    wait_result(
        &mut client,
        &mut context,
        &operation,
        request_id.as_deref(),
        args.json,
    )
    .await
}

fn device_command(action: DeviceAction) -> Value {
    match action {
        DeviceAction::List => json!({"type":"devices.refresh"}),
        DeviceAction::Link => json!({"type":"devices.link.startSelf"}),
        DeviceAction::CancelLink => json!({"type":"devices.link.cancelSelf"}),
        DeviceAction::Approve { code } => json!({"type":"devices.link.approve","userCode":code}),
        DeviceAction::Revoke { device } => json!({"type":"devices.revoke","deviceId":device}),
    }
}

struct IdleWatch {
    enabled: bool,
    since: Option<tokio::time::Instant>,
}
impl IdleWatch {
    const GRACE: Duration = Duration::from_secs(30);
    const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            since: None,
        }
    }
    fn deadline(
        &mut self,
        local_sessions: usize,
        clients: usize,
        now: tokio::time::Instant,
    ) -> Option<tokio::time::Instant> {
        if !self.enabled || local_sessions > 0 || clients > 0 {
            self.since = None;
            return None;
        }
        Some(*self.since.get_or_insert(now) + Self::GRACE)
    }
}

async fn run_host(config: Config, json_output: bool) -> Result<()> {
    let mut idle = IdleWatch::new(config.host == HostKind::Background);
    let runtime = crate::start(config).await?;
    let result = async {
        let mut events = runtime.subscribe_events();
        let mut clients = runtime.local_clients();
        let mut local_sessions = local_host::local_sessions(&runtime.snapshot().await?);
        loop {
            let deadline = idle.deadline(local_sessions, *clients.borrow(), tokio::time::Instant::now());
            tokio::select! {
                result=tokio::signal::ctrl_c()=>{result?;break;}
                ()=runtime.stopped()=>break,
                ()=async{match deadline{Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await}}=>break,
                changed=clients.changed()=>{if changed.is_err(){break;}}
                event=events.recv()=>match event{
                    Ok(event)=>{
                        if let EventBody::SessionsSnapshot{sessions}=&event.event{
                            local_sessions=sessions.iter().filter(|session|session.kind==SessionKind::Local).count();
                        }
                        if json_output{print_json(&event)?;}
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{
                        local_sessions=local_host::local_sessions(&runtime.snapshot().await?);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed)=>break,
                }
            }
        }
        Ok(())
    }
    .await;
    runtime.shutdown().await;
    drop(runtime);
    result
}

#[expect(
    clippy::future_not_send,
    reason = "the CLI mirror remains on its main thread"
)]
async fn attach_with_demand(
    root: &Path,
    session: Uuid,
    client: &mut local_host::Client,
) -> Result<()> {
    let attached = terminal::attach(root, session);
    tokio::pin!(attached);
    loop {
        tokio::select! {
            result = &mut attached => return result,
            event = client.next() => { event?; }
        }
    }
}

async fn send_input(
    root: &Path,
    session: Uuid,
    text: String,
    enter: bool,
    json_output: bool,
) -> Result<()> {
    let mut terminal = local_host::TerminalClient::connect(root, session).await?;
    let subscribed = tokio::time::timeout(
        Duration::from_secs(5),
        local_host::read_terminal(&mut terminal.reader),
    )
    .await
    .map_err(|_| Error::Other("terminal snapshot timed out; no input was sent".into()))??;
    if !matches!(subscribed, local_host::TerminalFrame::Checkpoint { .. }) {
        return Err(Error::Invalid(
            "terminal did not provide an initial snapshot".into(),
        ));
    }
    let mut bytes = text.into_bytes();
    if enter {
        bytes.push(b'\r');
    }
    local_host::write_input(&mut terminal.writer, &bytes).await?;
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            match local_host::read_terminal(&mut terminal.reader).await? {
                local_host::TerminalFrame::InputAck { accepted: true, .. } => {
                    return Ok::<(), Error>(());
                }
                local_host::TerminalFrame::InputAck {
                    accepted: false,
                    message,
                } => {
                    return Err(Error::Other(
                        message.unwrap_or_else(|| "Input was rejected".into()),
                    ));
                }
                local_host::TerminalFrame::Closed { reason, .. } => {
                    return Err(Error::Other(reason));
                }
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| {
        Error::Other("Input delivery was not confirmed; do not retry automatically.".into())
    })??;
    if json_output {
        print_json(&json!({"submitted":true,"sessionId":session}))
    } else {
        Ok(())
    }
}

fn friend_command(action: FriendAction, request: &str) -> Value {
    match action {
        FriendAction::List => json!({"type":"friends.refresh"}),
        FriendAction::Add { username } => {
            json!({"type":"friends.request.send","requestId":request,"username":username})
        }
        FriendAction::Accept { username } => {
            json!({"type":"friends.request.accept","username":username})
        }
        FriendAction::Reject { username } => {
            json!({"type":"friends.request.reject","username":username})
        }
        FriendAction::Cancel { username } => {
            json!({"type":"friends.request.cancel","username":username})
        }
        FriendAction::Remove { username } => {
            json!({"type":"friends.remove","username":username})
        }
    }
}

fn mission_command(action: MissionAction, request: &str) -> Value {
    match action {
        MissionAction::List => json!({"type":"mission.list"}),
        MissionAction::Show { mission } => {
            json!({"type":"mission.open","requestId":request,"missionId":mission})
        }
        MissionAction::Create { name } => {
            json!({"type":"mission.create","requestId":request,"name":name})
        }
        MissionAction::Rename { mission, name } => {
            json!({"type":"mission.rename","requestId":request,"missionId":mission,"name":name})
        }
        MissionAction::Delete { mission } => {
            json!({"type":"mission.delete","requestId":request,"missionId":mission})
        }
        MissionAction::Invite { mission, user } => {
            json!({"type":"mission.invite","requestId":request,"missionId":mission,"userId":user})
        }
        MissionAction::Accept { invitation } => {
            json!({"type":"mission.invitation.accept","requestId":request,"invitationId":invitation})
        }
        MissionAction::Reject { invitation } => {
            json!({"type":"mission.invitation.reject","requestId":request,"invitationId":invitation})
        }
        MissionAction::RemoveMember { mission, user } => {
            json!({"type":"mission.removeMember","requestId":request,"missionId":mission,"userId":user})
        }
        MissionAction::Leave { mission } => {
            json!({"type":"mission.leave","requestId":request,"missionId":mission})
        }
    }
}
fn provider_command(action: ProviderAction, request: &str) -> Result<Value> {
    Ok(match action {
        ProviderAction::Inspect {
            provider,
            directory,
        } => {
            json!({"type":"provider.inspect","requestId":request,"provider":provider_name(provider)?,"workingDirectory":path(directory)?})
        }
        ProviderAction::History {
            provider,
            directory,
        } => {
            json!({"type":"provider.discoverConversations","requestId":request,"provider":provider_name(provider)?,"workingDirectory":path(Some(directory))?})
        }
        ProviderAction::Read {
            provider,
            conversation,
            directory,
        } => {
            json!({"type":"provider.readConversation","requestId":request,"provider":provider_name(provider)?,"nativeConversationId":conversation,"workingDirectory":path(Some(directory))?})
        }
    })
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, value)?;
    output.write_all(b"\n")?;
    Ok(())
}
fn path(value: Option<PathBuf>) -> Result<Option<String>> {
    value
        .map(|path| {
            path.canonicalize().and_then(|p| {
                p.into_os_string()
                    .into_string()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path is not UTF-8"))
            })
        })
        .transpose()
        .map_err(Into::into)
}
fn provider_name(value: String) -> Result<String> {
    if matches!(value.as_str(), "claude" | "copilot") {
        Ok(value)
    } else {
        Err(Error::Invalid("provider must be claude or copilot".into()))
    }
}

struct Context {
    user: Option<String>,
    epoch: u64,
    sessions: Vec<Value>,
}
impl Context {
    fn from_events(events: &[Value]) -> Self {
        let mut value = Self {
            user: None,
            epoch: 0,
            sessions: vec![],
        };
        for event in events {
            value.observe(event);
        }
        value
    }
    fn observe(&mut self, event: &Value) -> bool {
        if let Some(epoch) = event.get("accountEpoch").and_then(Value::as_u64) {
            if epoch < self.epoch {
                return false;
            }
            if epoch > self.epoch {
                self.sessions.clear();
            }
            self.epoch = epoch;
            self.user = event
                .get("accountUserId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        if event.get("type").and_then(Value::as_str) == Some("sessions.snapshot") {
            self.sessions = event
                .get("sessions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
        }
        true
    }
    fn session(&self, id: Uuid) -> Result<&Value> {
        let id = id.to_string();
        self.sessions
            .iter()
            .find(|s| s.get("id").and_then(Value::as_str) == Some(id.as_str()))
            .ok_or(Error::NotFound)
    }
    fn incarnation(&self, id: Uuid) -> Result<String> {
        self.session(id)?
            .get("incarnationId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(Error::Stale)
    }
    fn envelope(&self, mut command: Value) -> Result<Value> {
        let value = command
            .as_object_mut()
            .ok_or_else(|| Error::Invalid("command must be object".into()))?;
        value.insert("accountUserId".into(), json!(self.user));
        value.insert("accountEpoch".into(), json!(self.epoch));
        Ok(command)
    }
}
async fn wait_result(
    client: &mut local_host::Client,
    context: &mut Context,
    operation: &str,
    request_id: Option<&str>,
    json_output: bool,
) -> Result<()> {
    let budget = if operation == "auth.login.start" {
        Duration::from_mins(10)
    } else {
        Duration::from_secs(30)
    };
    tokio::time::timeout(budget, async {
        loop {
            let frame = client.next().await?;
            if frame.get("kind").and_then(Value::as_str) == Some("rejected") {
                return Err(Error::Other(
                    frame["message"]
                        .as_str()
                        .unwrap_or("command rejected")
                        .to_owned(),
                ));
            }
            let Some(event) = frame.get("event") else {
                continue;
            };
            if !context.observe(event) {
                continue;
            }
            let kind = event
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let matching = request_id
                .is_none_or(|id| event.get("requestId").and_then(Value::as_str) == Some(id));
            let family_matches = kind.split('.').next() == operation.split('.').next();
            if kind.rsplit('.').next() == Some("error") && matching && family_matches {
                return Err(Error::Other(
                    event
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("command failed")
                        .to_owned(),
                ));
            }
            if kind == "auth.device_code" {
                if json_output {
                    print_json(event)?;
                } else {
                    writeln!(
                        io::stdout().lock(),
                        "Open {} and enter {}",
                        event["verificationUri"].as_str().unwrap_or_default(),
                        event["userCode"].as_str().unwrap_or_default()
                    )?;
                }
            }
            let done = match operation {
                "auth.login.start" => kind == "auth.ready",
                "auth.logout" => kind == "auth.required",
                "devices.refresh" | "devices.revoke" => kind == "devices.list",
                "devices.link.startSelf" => matches!(
                    kind,
                    "devices.link.selfPending" | "devices.link.selfResolved"
                ),
                "devices.link.cancelSelf" => kind == "devices.link.selfResolved",
                "devices.link.approve" => kind == "devices.link.resolved",
                op if op.starts_with("friends.") => kind == "friends.snapshot",
                "mission.list" => kind == "missions.snapshot",
                "mission.open" => kind == "mission.snapshot" && matching,
                op if op.starts_with("mission.") => kind == "mission.result" && matching,
                op if op.starts_with("provider.") => kind == "provider.reply" && matching,
                _ => kind == "session.result" && matching,
            };
            if done {
                return print_json(event);
            }
        }
    })
    .await
    .map_err(|_| {
        Error::Other("No authoritative result arrived. The command was not retried.".into())
    })?
}
async fn open_remote(
    client: &mut local_host::Client,
    context: &mut Context,
    session: Uuid,
) -> Result<()> {
    let entry = context.session(session)?;
    if entry.get("kind").and_then(Value::as_str) != Some("remote") {
        return Ok(());
    }
    let request = Uuid::now_v7().to_string();
    client
        .command(context.envelope(
            json!({"type":"session.openRemote","requestId":request,"sessionId":session}),
        )?)
        .await?;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let frame = client.next().await?;
            if frame.get("kind").and_then(Value::as_str) == Some("rejected") {
                return Err(Error::Other(
                    frame["message"]
                        .as_str()
                        .unwrap_or("remote connection rejected")
                        .to_owned(),
                ));
            }
            let Some(event) = frame.get("event") else {
                continue;
            };
            if !context.observe(event) {
                continue;
            }
            if event.get("type").and_then(Value::as_str) == Some("session.error")
                && event.get("requestId").and_then(Value::as_str) == Some(&request)
            {
                return Err(Error::Other(
                    event["message"]
                        .as_str()
                        .unwrap_or("remote connection failed")
                        .to_owned(),
                ));
            }
            if event.get("type").and_then(Value::as_str) == Some("session.result")
                && event.get("requestId").and_then(Value::as_str) == Some(&request)
            {
                return Ok(());
            }
        }
    })
    .await
    .map_err(|_| Error::Other("The remote host did not connect.".into()))?
}
async fn connect_or_start(config: &Config) -> Result<local_host::Client> {
    match local_host::Client::connect(&config.data_root).await {
        Ok(client) => return Ok(client),
        Err(Error::Io(error))
            if matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) => {}
        Err(error) => return Err(error),
    }
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("internal-host")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Ok(client) = local_host::Client::connect(&config.data_root).await {
            return Ok(client);
        }
        if let Some(status) = child.try_wait()? {
            return Err(Error::Other(format!(
                "local host exited before startup ({status})"
            )));
        }
    }
    drop(child.kill());
    drop(child.wait());
    Err(Error::Other("local host startup timed out".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_executables_recognise_cli_invocations() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>();
        let app = "/Applications/KodosiDesktop.app/Contents/MacOS/KodosiDesktop";
        assert!(is_invocation(&args(&["/opt/homebrew/bin/kodosi"])));
        assert!(is_invocation(&args(&["kodosi", "statsu"])));
        assert!(is_invocation(&args(&[app, "status"])));
        assert!(is_invocation(&args(&[app, "internal-host"])));
        assert!(is_invocation(&args(&[app, "--json", "status"])));
        assert!(is_invocation(&args(&[app, "--version"])));
        assert!(is_invocation(&args(&[app, "help", "session"])));
        assert!(!is_invocation(&args(&[app])));
        assert!(!is_invocation(&args(&[
            app,
            "-NSDocumentRevisionsDebugMode",
            "YES"
        ])));
        assert!(!is_invocation(&args(&[app, "statsu"])));
        assert!(!is_invocation(&[]));
    }
    #[test]
    fn background_hosts_only_exit_after_an_idle_grace_period() {
        let now = tokio::time::Instant::now();
        let mut foreground = IdleWatch::new(false);
        assert!(foreground.deadline(0, 0, now).is_none());
        let mut background = IdleWatch::new(true);
        assert!(background.deadline(1, 0, now).is_none());
        assert!(background.deadline(0, 1, now).is_none());
        let deadline = background.deadline(0, 0, now).unwrap();
        assert_eq!(deadline, now + IdleWatch::GRACE);
        assert_eq!(
            background
                .deadline(0, 0, now + Duration::from_secs(5))
                .unwrap(),
            deadline
        );
        assert!(
            background
                .deadline(0, 1, now + Duration::from_secs(6))
                .is_none()
        );
        assert_eq!(
            background
                .deadline(0, 0, now + Duration::from_secs(7))
                .unwrap(),
            now + Duration::from_secs(7) + IdleWatch::GRACE
        );
    }
    #[test]
    fn obsolete_products_are_not_cli_commands() {
        for args in [
            vec!["kodosi", "agent"],
            vec!["kodosi", "msg"],
            vec!["kodosi", "repair"],
            vec!["kodosi", "session", "reopen"],
            vec!["kodosi", "session", "run"],
            vec![
                "kodosi",
                "session",
                "stop",
                "01900000-0000-7000-8000-000000000001",
            ],
        ] {
            assert!(Arguments::try_parse_from(args).is_err());
        }
    }
    #[test]
    fn older_account_events_cannot_restore_old_sessions() {
        let mut context = Context::from_events(&[
            json!({"type":"sessions.snapshot","accountUserId":"old","accountEpoch":1,"sessions":[{"id":"old-session"}]}),
        ]);
        assert!(
            context.observe(&json!({"type":"auth.required","accountUserId":null,"accountEpoch":2}))
        );
        assert!(context.sessions.is_empty());
        assert!(!context.observe(&json!({"type":"sessions.snapshot","accountUserId":"old","accountEpoch":1,"sessions":[{"id":"old-session"}]})));
        assert_eq!(context.epoch, 2);
        assert!(context.user.is_none());
        assert!(context.sessions.is_empty());
    }
    #[test]
    fn terminal_close_and_native_resume_remain() {
        assert!(
            Arguments::try_parse_from([
                "kodosi",
                "session",
                "close",
                "01900000-0000-7000-8000-000000000001"
            ])
            .is_ok()
        );
        assert!(
            Arguments::try_parse_from([
                "kodosi",
                "session",
                "resume",
                "claude",
                "01900000-0000-7000-8000-000000000001",
                "/tmp"
            ])
            .is_ok()
        );
    }
}
