use crate::{Config, Error, Result, headless};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::{ExitCode, Stdio},
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
    Stop {
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
        room: Option<Uuid>,
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
    Show { room: Uuid },
    Create { name: String, slug: String },
    Rename { room: Uuid, name: String },
    Delete { room: Uuid },
    Invite { room: Uuid, user: Uuid },
    Accept { invitation: Uuid },
    Reject { invitation: Uuid },
    RemoveMember { room: Uuid, user: Uuid },
    Leave { room: Uuid },
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
pub async fn run() -> ExitCode {
    let args = Arguments::parse();
    let json_output = args.json;
    match dispatch(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json_output {
                drop(print_json(&json!({"error":error.to_string()})));
            } else {
                drop(writeln!(io::stderr().lock(), "Kodosi: {error}"));
            }
            ExitCode::FAILURE
        }
    }
}

#[expect(
    clippy::future_not_send,
    reason = "terminal attach runs the Ghostty mirror on the main thread"
)]
async fn dispatch(args: Arguments) -> Result<()> {
    let config = Config::load()?;
    if matches!(args.command, Action::Host | Action::InternalHost) {
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
            return terminal::attach(&config.data_root, session).await;
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
        Action::Session(SessionAction::Stop { session }) => {
            open_remote(&mut client, &mut context, session).await?;
            json!({"type":"session.stop","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?})
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
        Action::Session(SessionAction::Mission { session, room }) => {
            json!({"type":"session.attachMission","requestId":request,"sessionId":session,"expectedRuntimeIncarnationId":context.incarnation(session)?,"roomId":room})
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

async fn run_host(config: Config, json_output: bool) -> Result<()> {
    let runtime = crate::start(config).await?;
    let result = async {
        let mut events = runtime.subscribe_events();
        loop {
            tokio::select! {
                result=tokio::signal::ctrl_c()=>{result?;break;}
                ()=runtime.stopped()=>break,
                event=events.recv()=>match event{
                    Ok(event) if json_output=>print_json(&event)?,
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{},
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

async fn send_input(
    root: &std::path::Path,
    session: Uuid,
    text: String,
    enter: bool,
    json_output: bool,
) -> Result<()> {
    let mut terminal = headless::TerminalClient::connect(root, session).await?;
    let subscribed = tokio::time::timeout(
        Duration::from_secs(5),
        headless::read_terminal(&mut terminal.reader),
    )
    .await
    .map_err(|_| Error::Other("terminal snapshot timed out; no input was sent".into()))??;
    if !matches!(subscribed, headless::TerminalFrame::Checkpoint { .. }) {
        return Err(Error::Invalid(
            "terminal did not provide an initial snapshot".into(),
        ));
    }
    let mut bytes = text.into_bytes();
    if enter {
        bytes.push(b'\r');
    }
    headless::write_input(&mut terminal.writer, &bytes).await?;
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            match headless::read_terminal(&mut terminal.reader).await? {
                headless::TerminalFrame::InputAck { accepted: true, .. } => {
                    return Ok::<(), Error>(());
                }
                headless::TerminalFrame::InputAck {
                    accepted: false,
                    message,
                } => {
                    return Err(Error::Other(
                        message.unwrap_or_else(|| "Input was rejected".into()),
                    ));
                }
                headless::TerminalFrame::Closed { reason, .. } => return Err(Error::Other(reason)),
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| {
        Error::Other("Input admission was not confirmed; do not retry automatically.".into())
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
        MissionAction::List => json!({"type":"room.list"}),
        MissionAction::Show { room } => {
            json!({"type":"room.open","requestId":request,"roomId":room})
        }
        MissionAction::Create { name, slug } => {
            json!({"type":"room.create","requestId":request,"name":name,"slug":slug})
        }
        MissionAction::Rename { room, name } => {
            json!({"type":"room.rename","requestId":request,"roomId":room,"name":name})
        }
        MissionAction::Delete { room } => {
            json!({"type":"room.delete","requestId":request,"roomId":room})
        }
        MissionAction::Invite { room, user } => {
            json!({"type":"room.invite","requestId":request,"roomId":room,"userId":user})
        }
        MissionAction::Accept { invitation } => {
            json!({"type":"room.invitation.accept","requestId":request,"invitationId":invitation})
        }
        MissionAction::Reject { invitation } => {
            json!({"type":"room.invitation.reject","requestId":request,"invitationId":invitation})
        }
        MissionAction::RemoveMember { room, user } => {
            json!({"type":"room.removeMember","requestId":request,"roomId":room,"userId":user})
        }
        MissionAction::Leave { room } => {
            json!({"type":"room.leave","requestId":request,"roomId":room})
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
    client: &mut headless::Client,
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
                "room.list" => kind == "rooms.snapshot",
                "room.open" => kind == "room.snapshot" && matching,
                op if op.starts_with("room.") => kind == "room.result" && matching,
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
    client: &mut headless::Client,
    context: &mut Context,
    session: Uuid,
) -> Result<()> {
    let entry = context.session(session)?;
    if entry.get("kind").and_then(Value::as_str) != Some("remote") {
        return Ok(());
    }
    if entry.get("connectionState").and_then(Value::as_str) == Some("connected") {
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
            if context
                .session(session)?
                .get("connectionState")
                .and_then(Value::as_str)
                == Some("connected")
            {
                return Ok(());
            }
        }
    })
    .await
    .map_err(|_| Error::Other("The remote host did not connect.".into()))?
}
async fn connect_or_start(config: &Config) -> Result<headless::Client> {
    match headless::Client::connect(&config.data_root).await {
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
        if let Ok(client) = headless::Client::connect(&config.data_root).await {
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
    fn obsolete_products_are_not_cli_commands() {
        for args in [
            vec!["kodosi", "agent"],
            vec!["kodosi", "msg"],
            vec!["kodosi", "repair"],
            vec!["kodosi", "session", "reopen"],
            vec!["kodosi", "session", "run"],
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
    fn terminal_stop_and_native_resume_remain() {
        assert!(
            Arguments::try_parse_from([
                "kodosi",
                "session",
                "stop",
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
