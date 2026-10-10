use crate::{Config, Error, EventBody, HostKind, Result, local_host, protocol::SessionKind};
use clap::{CommandFactory, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{self, IsTerminal as _, Write},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use uuid::Uuid;

mod render;
mod rooms;
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
    #[command(about = "Work with a room's conversation, tasks, repositories, and terminals.")]
    Room(rooms::Arguments),
    #[command(subcommand)]
    Provider(ProviderAction),
    #[command(hide = true)]
    InternalHost,
}
#[derive(Subcommand)]
enum SessionAction {
    /// List the terminals of this computer and the terminals that you can open.
    List,
    /// Show one terminal.
    Show { session: String },
    /// Start a terminal on this computer.
    Start {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        directory: Option<PathBuf>,
        #[arg(long)]
        room: Option<String>,
        /// Make a new branch of the repository in --directory (or the current folder), with its own folder, and start the terminal there.
        #[arg(long)]
        branch: Option<String>,
    },
    /// Start a terminal that resumes a saved provider conversation.
    Resume {
        provider: String,
        conversation: Uuid,
        directory: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Open a terminal in this window. Press Ctrl-] and then . to leave; the terminal keeps running.
    Attach { session: String },
    /// Type text into a terminal.
    Input {
        session: String,
        text: String,
        #[arg(long)]
        enter: bool,
    },
    /// Give a terminal a new name.
    Rename { session: String, name: String },
    /// End a terminal and the programs in it.
    Close { session: String },
    /// Send Ctrl-C to a terminal.
    Interrupt { session: String },
    /// Give friends full control of a terminal of this computer.
    Share {
        session: String,
        #[arg(required = true)]
        friends: Vec<String>,
    },
    /// Take a terminal back from the given friends, or from all friends.
    Unshare {
        session: String,
        friends: Vec<String>,
    },
    /// Leave a terminal that a friend shared with you.
    Leave { session: String },
    /// Put a terminal in a Mission, or take it out when no Mission is given.
    Mission {
        session: String,
        mission: Option<String>,
    },
}
#[derive(Subcommand)]
enum AuthAction {
    /// Sign in. Terminals on this computer work without an account.
    Login,
    /// Sign out. This ends the sharing of the terminals of this computer.
    Logout,
    /// Show the account and the terminals.
    Status,
    /// Delete this account and its devices, friends, shared terminals and missions on the server.
    DeleteAccount,
}
#[derive(Subcommand)]
enum DeviceAction {
    /// List the approved devices of your account.
    List,
    /// Show a code for this device and wait until an approved device accepts it.
    Link,
    /// Cancel the request of this device.
    CancelLink,
    /// Approve the device that shows this code.
    Approve { code: String },
    /// Remove a device from your account.
    Revoke { device: String },
    /// Start fresh on this device: every other device loses access until it is approved again.
    Reset,
    /// Make a recovery key and show it one time. An earlier recovery key stops working.
    RecoveryKey,
    /// Approve this device with your recovery key when you have no other approved device.
    /// Kodosi reads the key from standard input.
    Recover,
}
#[derive(Subcommand)]
enum FriendAction {
    /// List your friends and the open requests.
    List,
    /// Ask a person to be your friend, by username or invite text.
    Add { username: String },
    /// Accept a request.
    Accept { username: String },
    /// Reject a request.
    Reject { username: String },
    /// Cancel a request that you sent.
    Cancel { username: String },
    /// Remove a friend. Terminals that you shared with this friend close for them.
    Remove { username: String },
    /// Print your invite text. A friend who adds you with it is verified at once.
    Invite,
    /// Verify a friend with the invite text that they sent you.
    Verify { username: String, invite: String },
    /// Trust the new identity of a friend who set up Kodosi again.
    Trust { username: String },
}
#[derive(Subcommand)]
enum MissionAction {
    /// List your Missions and invitations.
    List,
    /// Show a Mission and its members.
    Show { mission: String },
    /// Make a Mission.
    Create { name: String },
    /// Give a Mission a new name.
    Rename { mission: String, name: String },
    /// Delete a Mission. Its terminals stay.
    Delete { mission: String },
    /// Invite a friend to a Mission.
    Invite { mission: String, friend: String },
    /// Accept the invitation to a Mission.
    Accept { mission: String },
    /// Reject the invitation to a Mission.
    Reject { mission: String },
    /// Remove a member from a Mission.
    RemoveMember { mission: String, member: String },
    /// Leave a Mission.
    Leave { mission: String },
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
    let json_output = args.json;
    match args.command {
        Action::Room(arguments) => {
            rooms::run(arguments, &mut client, &mut context, json_output).await
        }
        Action::Status
        | Action::Session(SessionAction::List)
        | Action::Auth(AuthAction::Status) => {
            if json_output {
                print_json(
                    &json!({"accountUserId":context.user,"accountEpoch":context.epoch,"sessions":context.sessions}),
                )
            } else {
                print_text(&render::status(&context))
            }
        }
        Action::Session(SessionAction::Show { session }) => {
            let entry = context.session(context.session_id(&session)?)?;
            if json_output {
                print_json(entry)
            } else {
                print_text(&render::session(entry, &context))
            }
        }
        Action::Session(SessionAction::Attach { session }) => {
            let session = context.session_id(&session)?;
            open_remote(&mut client, &mut context, session).await?;
            attach_with_demand(&config.data_root, session, &mut client).await
        }
        Action::Session(SessionAction::Input {
            session,
            text,
            enter,
        }) => {
            let session = context.session_id(&session)?;
            open_remote(&mut client, &mut context, session).await?;
            send_input(&config.data_root, session, text, enter, json_output).await
        }
        Action::Session(SessionAction::Share { session, friends }) => {
            share(
                &mut client,
                &mut context,
                &session,
                &friends,
                true,
                json_output,
            )
            .await
        }
        Action::Session(SessionAction::Unshare { session, friends }) => {
            share(
                &mut client,
                &mut context,
                &session,
                &friends,
                false,
                json_output,
            )
            .await
        }
        Action::Devices(DeviceAction::Link) => {
            link_device(&mut client, &mut context, json_output).await
        }
        action => {
            let command = command(action, &mut client, &mut context).await?;
            let event = request(&mut client, &mut context, command, json_output).await?;
            if json_output {
                print_json(&event)
            } else {
                print_text(&render::result(&event, &context))
            }
        }
    }
}

async fn command(
    action: Action,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    let request = Uuid::now_v7().to_string();
    Ok(match action {
        Action::Session(action) => session_command(action, &request, client, context).await?,
        Action::Auth(AuthAction::Login) => json!({"type":"auth.login.start"}),
        Action::Auth(AuthAction::Logout) => json!({"type":"auth.logout"}),
        Action::Auth(AuthAction::DeleteAccount) => json!({"type":"auth.deleteAccount"}),
        Action::Devices(action) => device_command(action, client, context).await?,
        Action::Friends(action) => friend_command(action, &request),
        Action::Mission(action) => mission_command(action, &request, client, context).await?,
        Action::Provider(action) => provider_command(action, &request)?,
        Action::Room(_)
        | Action::Host
        | Action::InternalHost
        | Action::Status
        | Action::Auth(AuthAction::Status) => {
            return Err(Error::Invalid("unexpected command".into()));
        }
    })
}

async fn session_command(
    action: SessionAction,
    request: &str,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    let located = |context: &Context, session: &str| {
        let id = context.session_id(session)?;
        Ok::<_, Error>((id, context.incarnation(id)?))
    };
    Ok(match action {
        SessionAction::Start {
            name,
            directory,
            room,
            branch,
        } => {
            let room = if let Some(reference) = room {
                Some(mission_id(&missions(client, context).await?, &reference)?)
            } else {
                None
            };
            let (name, directory) = start_defaults(name, directory, branch.as_deref());
            json!({"type":"session.create","requestId":request,"name":name,"workingDir":path(directory)?,"missionId":room,"branch":branch})
        }
        SessionAction::Resume {
            provider,
            conversation,
            directory,
            name,
        } => {
            json!({"type":"session.create","requestId":request,"name":name.unwrap_or_else(||"Terminal".into()),"workingDir":path(Some(directory))?,"resume":{"provider":provider_name(provider)?,"nativeConversationId":conversation}})
        }
        SessionAction::Rename { session, name } => {
            let (id, incarnation) = located(context, &session)?;
            json!({"type":"session.rename","requestId":request,"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"name":name})
        }
        SessionAction::Close { session } => {
            let (id, incarnation) = located(context, &session)?;
            open_remote(client, context, id).await?;
            json!({"type":"session.close","requestId":request,"sessionId":id,"expectedRuntimeIncarnationId":incarnation})
        }
        SessionAction::Interrupt { session } => {
            let (id, incarnation) = located(context, &session)?;
            open_remote(client, context, id).await?;
            json!({"type":"session.interrupt","requestId":request,"sessionId":id,"expectedRuntimeIncarnationId":incarnation})
        }
        SessionAction::Leave { session } => {
            let (id, incarnation) = located(context, &session)?;
            json!({"type":"session.leave","requestId":request,"sessionId":id,"expectedRuntimeIncarnationId":incarnation})
        }
        SessionAction::Mission { session, mission } => {
            let (id, incarnation) = located(context, &session)?;
            let mission = match mission {
                Some(mission) => Some(mission_id(&missions(client, context).await?, &mission)?),
                None => None,
            };
            json!({"type":"session.attachMission","requestId":request,"sessionId":id,"expectedRuntimeIncarnationId":incarnation,"missionId":mission})
        }
        SessionAction::List
        | SessionAction::Show { .. }
        | SessionAction::Attach { .. }
        | SessionAction::Input { .. }
        | SessionAction::Share { .. }
        | SessionAction::Unshare { .. } => {
            return Err(Error::Invalid("unexpected command".into()));
        }
    })
}

async fn share(
    client: &mut local_host::Client,
    context: &mut Context,
    session: &str,
    friends: &[String],
    add: bool,
    json_output: bool,
) -> Result<()> {
    let id = context.session_id(session)?;
    let known = self::friends(client, context).await?;
    let chosen = friends
        .iter()
        .map(|friend| friend_id(&known, friend))
        .collect::<Result<BTreeSet<_>>>()?;
    let current = context.session(id)?["sharedWith"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|user| user.as_str().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let users = if add {
        &current | &chosen
    } else if chosen.is_empty() {
        BTreeSet::new()
    } else {
        &current - &chosen
    };
    let started = tokio::time::Instant::now();
    let event = loop {
        let command = json!({"type":"session.share","requestId":Uuid::now_v7().to_string(),"sessionId":id,"expectedRuntimeIncarnationId":context.incarnation(id)?,"userIds":users});
        match request(client, context, command, json_output).await {
            Err(Error::Other(message))
                if message == crate::runtime::NOT_PUBLISHED
                    && started.elapsed() < Duration::from_secs(10) =>
            {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            result => break result?,
        }
    };
    if json_output {
        print_json(&event)
    } else {
        print_text(&render::sharing(context.session(id)?, &users, &known))
    }
}

async fn link_device(
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let mut event = request(
        client,
        context,
        json!({"type":"devices.link.startSelf"}),
        json_output,
    )
    .await?;
    if event["type"] == "devices.link.selfPending" {
        if json_output {
            print_json(&event)?;
        } else {
            print_text(&render::link_code(&event))?;
        }
        event = tokio::time::timeout(Duration::from_mins(11), async {
            loop {
                let frame = client.next().await?;
                if let Some(event) = frame.get("event")
                    && context.observe(event)
                    && event["type"] == "devices.link.selfResolved"
                {
                    return Ok::<_, Error>(event.clone());
                }
            }
        })
        .await
        .map_err(|_| Error::Other("The code expired. Run `kodosi devices link` again.".into()))??;
    }
    if json_output {
        return print_json(&event);
    }
    match event["outcome"].as_str() {
        Some("approved") => print_text("This device is approved."),
        Some("expired") => Err(Error::Other(
            "The code expired. Run `kodosi devices link` again.".into(),
        )),
        _ => Err(Error::Other("The request was cancelled.".into())),
    }
}

async fn request(
    client: &mut local_host::Client,
    context: &mut Context,
    command: Value,
    json_output: bool,
) -> Result<Value> {
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
        client,
        context,
        &operation,
        request_id.as_deref(),
        json_output,
    )
    .await
}

async fn friends(client: &mut local_host::Client, context: &mut Context) -> Result<Vec<Value>> {
    let snapshot = request(client, context, json!({"type":"friends.refresh"}), true).await?;
    Ok(snapshot["friends"].as_array().cloned().unwrap_or_default())
}

async fn missions(client: &mut local_host::Client, context: &mut Context) -> Result<Value> {
    request(client, context, json!({"type":"mission.list"}), true).await
}

fn friend_id(friends: &[Value], reference: &str) -> Result<String> {
    let name = reference.trim_start_matches('@');
    let matches = friends
        .iter()
        .filter(|friend| {
            friend["userId"] == reference
                || friend["handle"]
                    .as_str()
                    .is_some_and(|handle| handle.eq_ignore_ascii_case(name))
        })
        .filter_map(|friend| friend["userId"].as_str())
        .collect::<Vec<_>>();
    only(&matches, "friend", reference, "kodosi friends list").map(str::to_owned)
}

fn mission_id(snapshot: &Value, reference: &str) -> Result<String> {
    let matches = snapshot["missions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|mission| mission["id"] == reference || mission["name"] == reference)
        .filter_map(|mission| mission["id"].as_str())
        .collect::<Vec<_>>();
    only(&matches, "Mission", reference, "kodosi mission list").map(str::to_owned)
}

fn only<'a>(matches: &[&'a str], kind: &str, reference: &str, list: &str) -> Result<&'a str> {
    match matches {
        [one] => Ok(one),
        [] => Err(Error::Invalid(format!(
            "No {kind} matches \"{reference}\". Run `{list}`."
        ))),
        _ => Err(Error::Invalid(format!(
            "More than one {kind} matches \"{reference}\". Use the ID from `{list}`."
        ))),
    }
}

async fn device_command(
    action: DeviceAction,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    Ok(match action {
        DeviceAction::List => json!({"type":"devices.refresh"}),
        DeviceAction::Link => json!({"type":"devices.link.startSelf"}),
        DeviceAction::CancelLink => json!({"type":"devices.link.cancelSelf"}),
        DeviceAction::Approve { code } => json!({"type":"devices.link.approve","code":code}),
        DeviceAction::Revoke { device } => {
            let list = request(client, context, json!({"type":"devices.refresh"}), true).await?;
            let matches = list["devices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| {
                    entry["label"] == device.as_str()
                        || entry["deviceId"].as_str().is_some_and(|id| {
                            id == device || (device.len() >= 4 && id.ends_with(&device))
                        })
                })
                .filter_map(|entry| entry["deviceId"].as_str())
                .collect::<Vec<_>>();
            let id = only(&matches, "device", &device, "kodosi devices list")?;
            json!({"type":"devices.revoke","deviceId":id})
        }
        DeviceAction::Reset => json!({"type":"devices.reset"}),
        DeviceAction::RecoveryKey => json!({"type":"devices.recovery.create"}),
        DeviceAction::Recover => json!({"type":"devices.recovery.use","key":recovery_key()?}),
    })
}

fn recovery_key() -> Result<String> {
    if io::stdin().is_terminal() {
        let mut prompt = io::stderr().lock();
        write!(prompt, "Recovery key: ")?;
        prompt.flush()?;
    }
    let mut key = String::new();
    io::stdin().read_line(&mut key)?;
    Ok(key.trim().to_owned())
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
    crate::diagnostics::install(&config.data_root);
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
        FriendAction::Invite => json!({"type":"friends.invite"}),
        FriendAction::Verify { username, invite } => {
            json!({"type":"friends.verify","username":username,"invite":invite})
        }
        FriendAction::Trust { username } => {
            json!({"type":"friends.identity.trust","username":username})
        }
    }
}

async fn mission_command(
    action: MissionAction,
    request: &str,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    let (operation, mission, extra) = match action {
        MissionAction::List => return Ok(json!({"type":"mission.list"})),
        MissionAction::Create { name } => {
            return Ok(json!({"type":"mission.create","requestId":request,"name":name}));
        }
        MissionAction::Accept { mission } => {
            return invitation_command(
                "mission.invitation.accept",
                &mission,
                request,
                client,
                context,
            )
            .await;
        }
        MissionAction::Reject { mission } => {
            return invitation_command(
                "mission.invitation.reject",
                &mission,
                request,
                client,
                context,
            )
            .await;
        }
        MissionAction::Show { mission } => ("mission.open", mission, json!({})),
        MissionAction::Rename { mission, name } => {
            ("mission.rename", mission, json!({"name":name}))
        }
        MissionAction::Delete { mission } => ("mission.delete", mission, json!({})),
        MissionAction::Leave { mission } => ("mission.leave", mission, json!({})),
        MissionAction::Invite { mission, friend } => {
            let user = friend_id(&friends(client, context).await?, &friend)?;
            ("mission.invite", mission, json!({"userId":user}))
        }
        MissionAction::RemoveMember { mission, member } => {
            let user = if Uuid::parse_str(&member).is_ok() {
                member
            } else {
                friend_id(&friends(client, context).await?, &member)?
            };
            ("mission.removeMember", mission, json!({"userId":user}))
        }
    };
    let mut command = extra;
    command["type"] = json!(operation);
    command["requestId"] = json!(request);
    command["missionId"] = json!(mission_id(&missions(client, context).await?, &mission)?);
    Ok(command)
}

async fn invitation_command(
    operation: &str,
    reference: &str,
    request: &str,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    let snapshot = missions(client, context).await?;
    let matches = snapshot["invitations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|invitation| {
            invitation["id"] == reference
                || invitation["missionId"] == reference
                || invitation["missionName"] == reference
        })
        .filter_map(|invitation| invitation["id"].as_str())
        .collect::<Vec<_>>();
    let id = only(&matches, "invitation", reference, "kodosi mission list")?;
    Ok(json!({"type":operation,"requestId":request,"invitationId":id}))
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

fn print_text(text: &str) -> Result<()> {
    let mut output = io::stdout().lock();
    output.write_all(text.as_bytes())?;
    output.write_all(b"\n")?;
    Ok(())
}
fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, value)?;
    output.write_all(b"\n")?;
    Ok(())
}
fn start_defaults(
    name: Option<String>,
    directory: Option<PathBuf>,
    branch: Option<&str>,
) -> (String, Option<PathBuf>) {
    (
        name.or_else(|| branch.map(str::to_owned))
            .unwrap_or_else(|| "Terminal".into()),
        directory.or_else(|| branch.map(|_| PathBuf::from("."))),
    )
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
    account: Option<Value>,
    friends: Vec<Value>,
}
impl Context {
    fn from_events(events: &[Value]) -> Self {
        let mut value = Self {
            user: None,
            epoch: 0,
            sessions: vec![],
            account: None,
            friends: vec![],
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
                self.friends.clear();
                self.account = None;
            }
            self.epoch = epoch;
            self.user = event
                .get("accountUserId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        match event.get("type").and_then(Value::as_str) {
            Some("sessions.snapshot") => {
                self.sessions = event["sessions"].as_array().cloned().unwrap_or_default();
            }
            Some("friends.snapshot") => {
                self.friends = event["friends"].as_array().cloned().unwrap_or_default();
            }
            Some("auth.ready") => self.account = Some(event.clone()),
            Some("auth.required") => self.account = None,
            _ => {}
        }
        true
    }
    fn session_id(&self, reference: &str) -> Result<Uuid> {
        let matches = self
            .sessions
            .iter()
            .filter(|session| {
                session["name"] == reference
                    || session["id"].as_str().is_some_and(|id| {
                        id == reference || (reference.len() >= 4 && id.ends_with(reference))
                    })
            })
            .filter_map(|session| session["id"].as_str())
            .collect::<Vec<_>>();
        let id = only(&matches, "terminal", reference, "kodosi session list")?;
        Uuid::parse_str(id).map_err(|_| Error::Stale)
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
) -> Result<Value> {
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
            if kind == "auth.deletion_pending" {
                if json_output {
                    print_json(event)?;
                } else {
                    writeln!(
                        io::stdout().lock(),
                        "Open {} and confirm the deletion",
                        event["confirmationUri"].as_str().unwrap_or_default()
                    )?;
                }
            }
            let done = match operation {
                "auth.login.start" => kind == "auth.ready",
                "auth.logout" | "auth.deleteAccount" => kind == "auth.required",
                "devices.refresh" | "devices.revoke" | "devices.reset" | "devices.recovery.use" => {
                    kind == "devices.list"
                }
                "devices.recovery.create" => kind == "devices.recovery.created",
                "devices.link.startSelf" => matches!(
                    kind,
                    "devices.link.selfPending" | "devices.link.selfResolved"
                ),
                "session.share" => kind == "session.result" && matching,
                "devices.link.cancelSelf" => kind == "devices.link.selfResolved",
                "devices.link.approve" => kind == "devices.link.resolved",
                "friends.invite" => kind == "friends.invite",
                op if op.starts_with("friends.") => kind == "friends.snapshot",
                "mission.list" => kind == "missions.snapshot",
                "mission.open" => kind == "mission.snapshot" && matching,
                op if op.starts_with("mission.") => kind == "mission.result" && matching,
                op if op.starts_with("provider.") => kind == "provider.reply" && matching,
                _ => kind == "session.result" && matching,
            };
            if done {
                return Ok(event.clone());
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
    fn a_branch_terminal_starts_from_the_current_folder_with_the_name_of_its_branch() {
        assert!(
            Arguments::try_parse_from(["kodosi", "session", "start", "--branch", "fix/login"])
                .is_ok()
        );
        assert_eq!(
            start_defaults(None, None, Some("fix/login")),
            ("fix/login".to_owned(), Some(PathBuf::from(".")))
        );
        assert_eq!(
            start_defaults(Some("Work".to_owned()), Some("/repo".into()), Some("fix")),
            ("Work".to_owned(), Some(PathBuf::from("/repo")))
        );
        assert_eq!(
            start_defaults(None, None, None),
            ("Terminal".to_owned(), None)
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
    fn a_terminal_and_a_friend_are_found_by_name_or_identifier() {
        let context = Context::from_events(&[
            json!({"type":"sessions.snapshot","accountUserId":"me","accountEpoch":1,"sessions":[
                {"id":"01a10d06-92a7-7367-8536-65c4895d05ae","name":"work"},
                {"id":"01a10d06-92a7-7367-8536-000000000002","name":"Terminal"},
                {"id":"01a10d06-92a7-7367-8536-000000000003","name":"Terminal"},
            ]}),
        ]);
        let work = Uuid::parse_str("01a10d06-92a7-7367-8536-65c4895d05ae").unwrap();
        assert_eq!(context.session_id("work").unwrap(), work);
        assert_eq!(context.session_id("5d05ae").unwrap(), work);
        assert_eq!(context.session_id(&work.to_string()).unwrap(), work);
        assert!(context.session_id("Terminal").is_err());
        assert!(context.session_id("000003").is_ok());
        assert!(context.session_id("ae").is_err());
        assert!(context.session_id("other").is_err());
        let friends = [json!({"userId":"u-bob","handle":"bob"})];
        for reference in ["bob", "@bob", "Bob", "u-bob"] {
            assert_eq!(friend_id(&friends, reference).unwrap(), "u-bob");
        }
        assert!(friend_id(&friends, "carol").is_err());
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
