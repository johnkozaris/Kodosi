use crate::{Config, Error, EventBody, HostKind, Result, local_host, protocol::SessionKind};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::{self, IsTerminal as _, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

mod dosi;
mod render;
mod rooms;
mod terminal;

#[derive(Parser)]
#[command(
    name = "kodosi",
    version = env!("KODOSI_VERSION"),
    about = "Shared terminals and rooms for people and their coding agents."
)]
struct Arguments {
    /// Print JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    #[command(about = "Run the local terminal host until interrupted, or stop the host that runs.")]
    Host {
        #[command(subcommand)]
        action: Option<HostAction>,
    },
    #[command(about = "Show the account and the terminals.")]
    Status,
    #[command(subcommand, about = "Start, read, type into and share terminals.")]
    Terminal(TerminalAction),
    #[command(
        subcommand,
        about = "List the agents, their accounts, models and efforts."
    )]
    Agent(AgentAction),
    #[command(subcommand, about = "Sign in or out, or delete your account.")]
    Auth(AuthAction),
    #[command(
        subcommand,
        about = "Approve and remove your devices, and make a recovery key."
    )]
    Devices(DeviceAction),
    #[command(subcommand, about = "Add, verify and remove friends.")]
    Friends(FriendAction),
    #[command(
        about = "Work with rooms and their members, conversation, tasks, repositories and terminals."
    )]
    Room(rooms::Arguments),
    #[command(subcommand, about = "Work with Dosi, an agent for your other agents.")]
    Dosi(DosiAction),
    #[command(
        subcommand,
        about = "Read the saved conversations of Claude Code and Copilot CLI."
    )]
    Provider(ProviderAction),
    #[command(hide = true)]
    InternalHost,
}
#[derive(Subcommand)]
enum DosiAction {
    /// Print the skill that lets an agent act as Dosi: it sees your terminals and rooms and acts on them.
    Skill,
    /// Tell Dosi something and print what it says and does until it answers.
    Say {
        text: String,
        /// Stop waiting after this many seconds; Dosi goes on in Kodosi.
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Show Dosi's conversation, which all your computers share.
    Show,
    /// Stop what Dosi does now, on this computer or on another one.
    Stop,
    /// Ask the person a question with choices in Dosi's conversation. Only Dosi in the Kodosi app asks this way.
    Ask {
        question: String,
        /// One choice; repeat it for each choice. With --terminal, write the words to show, = and what to type, as Yes=y.
        #[arg(long = "option", required = true)]
        options: Vec<String>,
        /// Let the person pick more than one choice.
        #[arg(long)]
        multiple: bool,
        /// Type what the chosen option types, and Enter, into this terminal.
        #[arg(long)]
        terminal: Option<String>,
    },
}
#[derive(Subcommand)]
enum HostAction {
    /// Stop the host that runs in the background. It refuses while terminals run, unless --force ends them.
    Stop {
        /// End the terminals that run, then stop.
        #[arg(long)]
        force: bool,
    },
}
#[derive(Subcommand)]
enum TerminalAction {
    /// List the terminals of this computer and the terminals that you can open.
    List,
    /// Show the details of one terminal.
    Info { terminal: String },
    /// Show a terminal in the Kodosi app.
    Show { terminal: String },
    /// Print the text on the screen of a terminal.
    Read {
        terminal: String,
        /// Also print the earlier lines that scrolled away.
        #[arg(long)]
        history: bool,
    },
    /// Press keys in a terminal, for example enter, escape, up or ctrl-c. It stops with a message while a person types there.
    Keys {
        terminal: String,
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// Wait until the program in a terminal reaches a state.
    Wait {
        terminal: String,
        /// One or more states, separated by commas.
        #[arg(long, value_delimiter = ',', required = true)]
        until: Vec<WaitState>,
        /// The longest wait, in seconds.
        #[arg(long, default_value_t = 600)]
        timeout: u64,
    },
    /// Start a terminal on this computer, or on another computer of yours with --computer.
    Start {
        /// The name of the terminal. The default is Terminal, or the name of the new branch.
        #[arg(long)]
        name: Option<String>,
        /// The folder to start in. The default is your home folder.
        #[arg(long)]
        directory: Option<PathBuf>,
        /// Start the terminal on another computer of yours, by the name that `kodosi devices list` shows. There, a relative --directory, or one that starts with ~ in quotes, is in your home.
        #[arg(long)]
        computer: Option<String>,
        /// Start the terminal in this room, by its name or ID.
        #[arg(long)]
        room: Option<String>,
        /// Make a new branch of the repository in --directory (or the current folder), with its own folder, and start the terminal there.
        #[arg(long)]
        branch: Option<String>,
        /// Start an agent in the terminal, for example claude or codex, with your last choice of account, model and effort.
        #[arg(long)]
        agent: Option<String>,
        /// The agent's account, by the name that `kodosi agent list` shows.
        #[arg(long, requires = "agent")]
        account: Option<String>,
        /// The agent's model, by the ID that `kodosi agent models` shows. An empty value starts without a model.
        #[arg(long, requires = "agent")]
        model: Option<String>,
        /// The agent's effort. An empty value starts without one.
        #[arg(long, requires = "agent")]
        effort: Option<String>,
        /// The agent's first prompt.
        #[arg(long, requires = "agent")]
        prompt: Option<String>,
    },
    /// Start an agent in a terminal that waits at its shell prompt.
    Run {
        terminal: String,
        /// The agent, for example claude or codex. It starts with your last choice of account, model and effort.
        #[arg(long)]
        agent: String,
        /// The agent's account, by the name that `kodosi agent list` shows.
        #[arg(long)]
        account: Option<String>,
        /// The agent's model, by the ID that `kodosi agent models` shows. An empty value starts without a model.
        #[arg(long)]
        model: Option<String>,
        /// The agent's effort. An empty value starts without one.
        #[arg(long)]
        effort: Option<String>,
    },
    /// Start a terminal that resumes a saved provider conversation.
    Resume {
        provider: String,
        conversation: Uuid,
        directory: PathBuf,
        /// The name of the terminal.
        #[arg(long)]
        name: Option<String>,
    },
    /// Open a terminal in this window. Press Ctrl-] and then . to leave; the terminal keeps running.
    Attach { terminal: String },
    /// Type text into a terminal. It stops with a message while a person types there.
    Input {
        terminal: String,
        text: String,
        /// Press Enter after the text.
        #[arg(long)]
        enter: bool,
    },
    /// Give a terminal a new name.
    Rename { terminal: String, name: String },
    /// End a terminal and the programs in it.
    Close { terminal: String },
    /// Send Ctrl-C to a terminal.
    Interrupt { terminal: String },
    /// Give friends full control of a terminal of this computer.
    Share {
        terminal: String,
        #[arg(required = true)]
        friends: Vec<String>,
    },
    /// Take a terminal back from the given friends, or from all friends.
    Unshare {
        terminal: String,
        friends: Vec<String>,
    },
    /// Leave a terminal that a friend shared with you.
    Leave { terminal: String },
}
#[derive(Subcommand)]
enum AgentAction {
    /// List the agents, their accounts and your last choice.
    List,
    /// List the models and efforts of an agent.
    Models { agent: String },
    /// Show how much of the plan limits each signed-in account has used.
    Usage,
}
#[derive(Subcommand)]
enum AuthAction {
    /// Sign in. Terminals on this computer work without an account.
    Login,
    /// Sign out. This ends the sharing of the terminals of this computer.
    Logout,
    /// Show the account and the terminals.
    Status,
    /// Delete this account and its devices, friends, shared terminals and rooms on the server.
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
enum ProviderAction {
    /// Show the program and the settings files of an agent, for a folder if you give one.
    Inspect {
        provider: String,
        /// The folder whose project settings to show.
        #[arg(long)]
        directory: Option<PathBuf>,
    },
    /// List the saved conversations of an agent in a folder.
    History {
        provider: String,
        directory: PathBuf,
    },
    /// Print a saved conversation.
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
            let message = match error {
                Error::HostOutdated {
                    kind: HostKind::Background,
                    terminals,
                    ..
                } => format!(
                    "{error} Run `kodosi host stop --force` to stop it and its {terminals} terminal(s). The next command starts the new host."
                ),
                Error::HostOutdated { .. } => {
                    format!("{error} Stop `kodosi host` in its terminal and start it again.")
                }
                error => error.to_string(),
            };
            if json_output {
                drop(print_json(&json!({"error":message})));
            } else {
                drop(writeln!(io::stderr().lock(), "Kodosi: {message}"));
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
        Action::Host { action: None } => Some(HostKind::Foreground),
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
    if let Action::Host {
        action: Some(HostAction::Stop { force }),
    } = args.command
    {
        return stop_host(&config, force, args.json).await;
    }
    if matches!(args.command, Action::Dosi(DosiAction::Skill)) {
        return print_text(dosi::SKILL);
    }
    refuse_for_dosi(&args.command)?;
    let mut client = local_host::connect_or_start(&config.data_root).await?;
    let mut context = Context::from_events(&client.initial_events);
    list_terminals(&args.command, &mut client, &mut context).await;
    let json_output = args.json;
    match args.command {
        Action::Room(arguments) => {
            rooms::run(arguments, &mut client, &mut context, json_output).await
        }
        Action::Status
        | Action::Terminal(TerminalAction::List)
        | Action::Auth(AuthAction::Status) => print_status(&context, json_output),
        Action::Terminal(TerminalAction::Wait {
            terminal,
            until,
            timeout,
        }) => {
            let session = context.session_id(&terminal)?;
            wait_for(
                &mut client,
                &mut context,
                session,
                &until,
                timeout,
                json_output,
            )
            .await
        }
        Action::Terminal(TerminalAction::Info { terminal }) => {
            let entry = context.session(context.session_id(&terminal)?)?;
            if json_output {
                print_json(entry)
            } else {
                print_text(&render::session(entry, &context))
            }
        }
        Action::Terminal(TerminalAction::Attach { terminal }) => {
            let session = context.session_id(&terminal)?;
            open_remote(&mut client, &mut context, session).await?;
            request(
                &mut client,
                &mut context,
                json!({"type":"session.visible","sessionIds":[session]}),
                false,
            )
            .await?;
            attach_with_demand(&config.data_root, session, &mut client).await
        }
        Action::Terminal(TerminalAction::Share { terminal, friends }) => {
            share(
                &mut client,
                &mut context,
                &terminal,
                &friends,
                true,
                json_output,
            )
            .await
        }
        Action::Terminal(TerminalAction::Unshare { terminal, friends }) => {
            share(
                &mut client,
                &mut context,
                &terminal,
                &friends,
                false,
                json_output,
            )
            .await
        }
        Action::Devices(DeviceAction::Link) => {
            link_device(&mut client, &mut context, json_output).await
        }
        Action::Dosi(action) => dosi::run(action, &mut client, &mut context, json_output).await,
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

fn refuse_for_dosi(action: &Action) -> Result<()> {
    if std::env::var_os(crate::dosi::TOKEN_VARIABLE).is_none() {
        return Ok(());
    }
    if matches!(
        action,
        Action::Dosi(DosiAction::Say { .. } | DosiAction::Stop)
    ) {
        return Err(Error::Invalid(
            "Dosi answers in its own conversation; it does not send itself messages or stop itself."
                .to_owned(),
        ));
    }
    if guarded(action) {
        return Err(Error::Invalid(
            "Dosi does not sign in or out, change devices, use recovery keys or trust friends. Ask the person to do it in Kodosi.".to_owned(),
        ));
    }
    Ok(())
}

const fn guarded(action: &Action) -> bool {
    matches!(
        action,
        Action::Auth(AuthAction::Login | AuthAction::Logout | AuthAction::DeleteAccount)
            | Action::Devices(
                DeviceAction::Link
                    | DeviceAction::CancelLink
                    | DeviceAction::Approve { .. }
                    | DeviceAction::Revoke { .. }
                    | DeviceAction::Reset
                    | DeviceAction::RecoveryKey
                    | DeviceAction::Recover
            )
            | Action::Friends(FriendAction::Trust { .. } | FriendAction::Verify { .. })
    )
}

async fn command(
    action: Action,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    let request = Uuid::now_v7().to_string();
    Ok(match action {
        Action::Terminal(action) => terminal_command(action, &request, client, context).await?,
        Action::Agent(AgentAction::List) => json!({"type":"provider.agents","requestId":request}),
        Action::Agent(AgentAction::Models { agent }) => {
            json!({"type":"provider.agentModels","requestId":request,"agent":agent_name(&agent)?})
        }
        Action::Agent(AgentAction::Usage) => json!({"type":"provider.usage","requestId":request}),
        Action::Auth(AuthAction::Login) => json!({"type":"auth.login.start"}),
        Action::Auth(AuthAction::Logout) => json!({"type":"auth.logout"}),
        Action::Auth(AuthAction::DeleteAccount) => json!({"type":"auth.deleteAccount"}),
        Action::Devices(action) => device_command(action, client, context).await?,
        Action::Friends(action) => friend_command(action, &request),
        Action::Provider(action) => provider_command(action, &request)?,
        Action::Room(_)
        | Action::Dosi(_)
        | Action::Host { .. }
        | Action::InternalHost
        | Action::Status
        | Action::Auth(AuthAction::Status) => {
            return Err(Error::Invalid("unexpected command".into()));
        }
    })
}

#[expect(clippy::too_many_arguments, reason = "the choices of one new terminal")]
async fn start_command(
    request: &str,
    client: &mut local_host::Client,
    context: &mut Context,
    name: Option<String>,
    (directory, computer): (Option<PathBuf>, Option<String>),
    room: Option<String>,
    branch: Option<String>,
    agent: Option<Value>,
    prompt: Option<String>,
) -> Result<Value> {
    let room = if let Some(reference) = room {
        Some(mission_id(&missions(client, context).await?, &reference)?)
    } else {
        None
    };
    let name = name.or_else(|| {
        agent.as_ref().and_then(|agent| {
            crate::agents::Agent::ALL
                .into_iter()
                .find(|known| known.name() == agent["agent"])
                .map(|known| known.label().to_owned())
        })
    });
    let computer = match computer {
        Some(reference) => computer_id(client, context, &reference).await?,
        None => None,
    };
    if computer.is_some() && branch.is_some() && directory.is_none() {
        return Err(Error::Invalid(
            "Give --directory, the repository on that computer, for a new branch there.".into(),
        ));
    }
    let (name, directory) = start_defaults(name, directory, branch.as_deref());
    let directory = match &computer {
        Some(computer) => remote_folder(client, context, computer, directory).await?,
        None => path(directory)?,
    };
    Ok(
        json!({"type":"session.create","requestId":request,"name":name,"workingDir":directory,"missionId":room,"branch":branch,"agent":agent,"prompt":prompt,"computer":computer}),
    )
}

async fn device_id(
    client: &mut local_host::Client,
    context: &mut Context,
    reference: &str,
    kind: &str,
) -> Result<String> {
    request(client, context, json!({"type":"devices.refresh"}), true).await?;
    let list = context
        .latest
        .get("devices.list")
        .cloned()
        .unwrap_or_default();
    let matches = list["devices"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry["label"]
                .as_str()
                .is_some_and(|label| label.eq_ignore_ascii_case(reference))
                || entry["deviceId"].as_str().is_some_and(|id| {
                    id == reference || (reference.len() >= 4 && id.ends_with(reference))
                })
        })
        .filter_map(|entry| entry["deviceId"].as_str())
        .collect::<Vec<_>>();
    only(&matches, kind, reference, "kodosi devices list").map(str::to_owned)
}

async fn computer_id(
    client: &mut local_host::Client,
    context: &mut Context,
    reference: &str,
) -> Result<Option<String>> {
    let id = device_id(client, context, reference, "computer").await?;
    let latest = |kind: &str| context.latest.get(kind).cloned().unwrap_or_default();
    if latest("devices.list")["selfDeviceId"].as_str() == Some(id.as_str()) {
        return Ok(None);
    }
    let online = latest("devices.hosts")["deviceIds"]
        .as_array()
        .is_some_and(|hosts| hosts.iter().any(|host| host.as_str() == Some(id.as_str())));
    if online {
        Ok(Some(id))
    } else {
        Err(Error::Invalid(format!(
            "\"{reference}\" is not online. Open Kodosi or run `kodosi host` there."
        )))
    }
}

async fn remote_folder(
    client: &mut local_host::Client,
    context: &mut Context,
    computer: &str,
    directory: Option<PathBuf>,
) -> Result<Option<String>> {
    let Some(directory) = directory else {
        return Ok(None);
    };
    let directory = directory
        .into_os_string()
        .into_string()
        .map_err(|_| Error::Invalid("The folder must be text.".into()))?;
    let bytes = directory.as_bytes();
    if directory.starts_with(['/', '\\'])
        || bytes.len() > 1 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
    {
        return Ok(Some(directory));
    }
    let listed = request(
        client,
        context,
        json!({"type":"folders.list","computer":computer}),
        true,
    )
    .await?;
    let home = listed["folders"]["home"]
        .as_str()
        .ok_or_else(|| Error::Other("The other computer did not name its home folder.".into()))?;
    let rest = match directory.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => rest,
        _ => directory.as_str(),
    }
    .trim_start_matches(['/', '\\']);
    let separator = if home.contains('\\') { '\\' } else { '/' };
    Ok(Some(if rest.is_empty() {
        home.to_owned()
    } else {
        format!("{}{separator}{rest}", home.trim_end_matches(separator))
    }))
}

async fn terminal_command(
    action: TerminalAction,
    request: &str,
    client: &mut local_host::Client,
    context: &mut Context,
) -> Result<Value> {
    match action {
        TerminalAction::Start {
            name,
            directory,
            computer,
            room,
            branch,
            agent,
            account,
            model,
            effort,
            prompt,
        } => {
            let agent = agent
                .map(|agent| {
                    agent_start(
                        &agent,
                        account.as_deref(),
                        model.as_deref(),
                        effort.as_deref(),
                    )
                })
                .transpose()?;
            start_command(
                request,
                client,
                context,
                name,
                (directory, computer),
                room,
                branch,
                agent,
                prompt,
            )
            .await
        }
        TerminalAction::Resume {
            provider,
            conversation,
            directory,
            name,
        } => Ok(
            json!({"type":"session.create","requestId":request,"name":name.unwrap_or_else(||"Terminal".into()),"workingDir":path(Some(directory))?,"resume":{"provider":provider_name(provider)?,"nativeConversationId":conversation}}),
        ),
        TerminalAction::Show { terminal } => {
            let id = context.session_id(&terminal)?;
            Ok(json!({"type":"session.show","requestId":request,"sessionId":id}))
        }
        action => {
            let (terminal, kind, remote, fields) = on_terminal(action)?;
            let id = context.session_id(&terminal)?;
            let incarnation = context.incarnation(id)?;
            if remote {
                open_remote(client, context, id).await?;
            }
            let mut command = json!({"type":kind,"requestId":request,"sessionId":id,"expectedRuntimeIncarnationId":incarnation});
            if let (Some(command), Value::Object(fields)) = (command.as_object_mut(), fields) {
                command.extend(fields);
            }
            Ok(command)
        }
    }
}

fn on_terminal(action: TerminalAction) -> Result<(String, &'static str, bool, Value)> {
    Ok(match action {
        TerminalAction::Run {
            terminal,
            agent,
            account,
            model,
            effort,
        } => {
            let agent = agent_start(
                &agent,
                account.as_deref(),
                model.as_deref(),
                effort.as_deref(),
            )?;
            (terminal, "session.run", false, json!({"agent":agent}))
        }
        TerminalAction::Read { terminal, history } => {
            (terminal, "session.read", true, json!({"history":history}))
        }
        TerminalAction::Keys { terminal, keys } => {
            let text = keys.iter().map(|key| key_text(key));
            let text = text.collect::<Result<String>>()?;
            (terminal, "session.input", true, json!({"text":text}))
        }
        TerminalAction::Input {
            terminal,
            text,
            enter,
        } => {
            let text = if enter { format!("{text}\r") } else { text };
            (terminal, "session.input", true, json!({"text":text}))
        }
        TerminalAction::Rename { terminal, name } => {
            (terminal, "session.rename", false, json!({"name":name}))
        }
        TerminalAction::Close { terminal } => (terminal, "session.close", true, json!({})),
        TerminalAction::Interrupt { terminal } => (terminal, "session.interrupt", true, json!({})),
        TerminalAction::Leave { terminal } => (terminal, "session.leave", false, json!({})),
        _ => return Err(Error::Invalid("unexpected command".into())),
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
    if event["code"].is_string() {
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
    mut command: Value,
    json_output: bool,
) -> Result<Value> {
    let operation = command["type"]
        .as_str()
        .ok_or_else(|| Error::Invalid("command has no type".into()))?
        .to_owned();
    let request_id = command
        .as_object_mut()
        .ok_or_else(|| Error::Invalid("command must be object".into()))?
        .entry("requestId")
        .or_insert_with(|| json!(Uuid::now_v7().to_string()))
        .as_str()
        .ok_or_else(|| Error::Invalid("requestId must be text".into()))?
        .to_owned();
    client.command(context.envelope(command)?).await?;
    wait_answer(client, context, &operation, &request_id, json_output).await
}

async fn list_terminals(action: &Action, client: &mut local_host::Client, context: &mut Context) {
    if context.user.is_some()
        && matches!(
            action,
            Action::Status | Action::Terminal(_) | Action::Auth(AuthAction::Status)
        )
    {
        drop(
            tokio::time::timeout(
                Duration::from_secs(5),
                request(client, context, json!({"type":"session.list"}), false),
            )
            .await,
        );
    }
}

async fn friends(client: &mut local_host::Client, context: &mut Context) -> Result<Vec<Value>> {
    request(client, context, json!({"type":"friends.refresh"}), true).await?;
    Ok(context.friends.clone())
}

async fn missions(client: &mut local_host::Client, context: &mut Context) -> Result<Value> {
    request(client, context, json!({"type":"mission.list"}), true).await?;
    context
        .latest
        .get("missions.snapshot")
        .cloned()
        .ok_or_else(|| Error::Other("Kodosi did not list the rooms.".into()))
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
    only(&matches, "room", reference, "kodosi room list").map(str::to_owned)
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
            let id = device_id(client, context, &device, "device").await?;
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
    let mut end = kodosi_os::process::EndRequest::new()?;
    crate::diagnostics::install(&config.data_root, crate::diagnostics::Role::Host);
    let runtime = crate::start(config).await?;
    let result = async {
        let mut events = runtime.subscribe_events();
        let mut clients = runtime.local_clients();
        let mut local_sessions = local_host::local_sessions(&runtime.snapshot().await?);
        loop {
            let deadline = idle.deadline(local_sessions, *clients.borrow(), tokio::time::Instant::now());
            tokio::select! {
                result=tokio::signal::ctrl_c()=>{result?;break;}
                ()=end.received()=>break,
                ()=runtime.stopped()=>break,
                ()=async{match deadline{Some(at)=>tokio::time::sleep_until(at).await,None=>std::future::pending().await}}=>break,
                changed=clients.changed()=>{if changed.is_err(){break;}}
                event=events.recv()=>match event{
                    Ok(event)=>{
                        if let EventBody::SessionsSnapshot{sessions}=&event.event{
                            local_sessions=sessions.iter().filter(|session|session.kind==SessionKind::Local).count();
                        }
                        if json_output && event.recipient.is_none(){print_json(&event)?;}
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

fn agent_name(agent: &str) -> Result<&'static str> {
    crate::agents::Agent::ALL
        .into_iter()
        .map(crate::agents::Agent::name)
        .find(|name| *name == agent)
        .ok_or_else(|| {
            Error::Invalid(format!(
                "Kodosi knows these agents: {}.",
                crate::agents::Agent::ALL
                    .map(crate::agents::Agent::name)
                    .join(", ")
            ))
        })
}

fn agent_start(
    agent: &str,
    account: Option<&str>,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Value> {
    Ok(json!({"agent":agent_name(agent)?,"account":account,"model":model,"effort":effort}))
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
            kodosi_os::fs::canonicalize(&path).and_then(|p| {
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
    restoring: Option<Value>,
    friends: Vec<Value>,
    latest: BTreeMap<String, Value>,
    rooms: BTreeMap<String, Value>,
}
impl Context {
    fn from_events(events: &[Value]) -> Self {
        let mut value = Self {
            user: None,
            epoch: 0,
            sessions: vec![],
            account: None,
            restoring: None,
            friends: vec![],
            latest: BTreeMap::new(),
            rooms: BTreeMap::new(),
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
                self.restoring = None;
                self.latest.clear();
                self.rooms.clear();
            }
            self.epoch = epoch;
            self.user = event
                .get("accountUserId")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        let kind = event
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if event.get("requestId").is_none() {
            self.latest.insert(kind.to_owned(), event.clone());
        }
        match event.get("type").and_then(Value::as_str) {
            Some("room.snapshot") => {
                if let Some(room) = event["room"]["roomId"].as_str() {
                    self.rooms.insert(room.to_owned(), event["room"].clone());
                }
            }
            Some("sessions.snapshot") => {
                self.sessions = event["sessions"].as_array().cloned().unwrap_or_default();
            }
            Some("friends.snapshot") => {
                self.friends = event["friends"].as_array().cloned().unwrap_or_default();
            }
            Some("auth.ready") => {
                self.account = Some(event.clone());
                self.restoring = None;
            }
            Some("auth.required") => {
                self.account = None;
                self.restoring = None;
            }
            Some("auth.restoring") => {
                self.restoring = Some(event.clone());
            }
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
        let id = only(&matches, "terminal", reference, "kodosi terminal list")?;
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
async fn wait_answer(
    client: &mut local_host::Client,
    context: &mut Context,
    operation: &str,
    request_id: &str,
    json_output: bool,
) -> Result<Value> {
    tokio::time::timeout(crate::protocol::answer_time(operation), async {
        loop {
            let frame = client.next().await?;
            if frame.get("kind").and_then(Value::as_str) == Some("rejected")
                && frame.get("requestId").and_then(Value::as_str) == Some(request_id)
            {
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
            if let Some(answer) = crate::protocol::answer(event, request_id) {
                return answer.map_err(Error::Other);
            }
            if event.get("requestId").and_then(Value::as_str) != Some(request_id) {
                continue;
            }
            match event.get("type").and_then(Value::as_str) {
                Some("auth.device_code") if !json_output => writeln!(
                    io::stdout().lock(),
                    "Open {} and enter {}",
                    event["verificationUri"].as_str().unwrap_or_default(),
                    event["userCode"].as_str().unwrap_or_default()
                )?,
                Some("auth.deletion_pending") if !json_output => writeln!(
                    io::stdout().lock(),
                    "Open {} and confirm the deletion",
                    event["confirmationUri"].as_str().unwrap_or_default()
                )?,
                _ if json_output => print_json(event)?,
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| Error::Other("No answer arrived in time. The command was not retried.".into()))?
}

#[derive(Clone, Copy, ValueEnum)]
enum WaitState {
    Working,
    Waiting,
    Done,
    Failed,
    Prompt,
    Closed,
}

impl WaitState {
    const fn name(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Prompt => "prompt",
            Self::Closed => "closed",
        }
    }

    fn reached(self, entry: Option<&Value>) -> bool {
        let Some(entry) = entry else {
            return matches!(self, Self::Closed);
        };
        let state = entry["programStatus"]["state"].as_str();
        match self {
            Self::Working => state == Some("working"),
            Self::Waiting => state == Some("blocked"),
            Self::Done => state == Some("done"),
            Self::Failed => state == Some("error"),
            Self::Prompt => entry["prompt"] == true,
            Self::Closed => false,
        }
    }
}

async fn wait_for(
    client: &mut local_host::Client,
    context: &mut Context,
    session: Uuid,
    until: &[WaitState],
    seconds: u64,
    json_output: bool,
) -> Result<()> {
    let id = session.to_string();
    let reached = tokio::time::timeout(Duration::from_secs(seconds), async {
        loop {
            let entry = context
                .sessions
                .iter()
                .find(|entry| entry["id"] == id.as_str());
            if let Some(state) = until.iter().find(|state| state.reached(entry)) {
                return Ok::<_, Error>((*state, entry.cloned().unwrap_or_default()));
            }
            if let Some(offline) = entry.and_then(render::offline) {
                return Err(Error::Other(offline));
            }
            let frame = client.next().await?;
            if let Some(event) = frame.get("event") {
                context.observe(event);
            }
        }
    })
    .await
    .map_err(|_| Error::Other("The terminal did not reach that state in time.".into()))?;
    let (state, entry) = reached?;
    if json_output {
        print_json(&json!({"state":state.name(),"session":entry}))
    } else {
        print_text(state.name())
    }
}

fn print_status(context: &Context, json_output: bool) -> Result<()> {
    if json_output {
        print_json(
            &json!({"accountUserId":context.user,"accountEpoch":context.epoch,"sessions":context.sessions}),
        )
    } else {
        print_text(&render::status(context))
    }
}

async fn stop_host(config: &Config, force: bool, json_output: bool) -> Result<()> {
    local_host::stop_other_host(&config.data_root, force).await?;
    if json_output {
        print_json(&json!({"stopped":true}))
    } else {
        print_text("The Kodosi host stopped.")
    }
}

fn key_text(key: &str) -> Result<String> {
    let named = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => "\r",
        "tab" => "\t",
        "escape" | "esc" => "\u{1b}",
        "backspace" => "\u{7f}",
        "delete" => "\u{1b}[3~",
        "up" => "\u{1b}[A",
        "down" => "\u{1b}[B",
        "right" => "\u{1b}[C",
        "left" => "\u{1b}[D",
        "home" => "\u{1b}[H",
        "end" => "\u{1b}[F",
        "pageup" => "\u{1b}[5~",
        "pagedown" => "\u{1b}[6~",
        "space" => " ",
        control => {
            return control
                .strip_prefix("ctrl-")
                .and_then(|letter| {
                    let [letter] = letter.as_bytes() else {
                        return None;
                    };
                    letter
                        .is_ascii_lowercase()
                        .then(|| char::from(letter - b'a' + 1).to_string())
                })
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "Unknown key {key}. Use enter, tab, escape, backspace, delete, up, down, left, right, home, end, pageup, pagedown, space or ctrl- and a letter."
                    ))
                });
        }
    };
    Ok(named.to_owned())
}

async fn open_remote(
    client: &mut local_host::Client,
    context: &mut Context,
    session: Uuid,
) -> Result<()> {
    if context
        .session(session)?
        .get("kind")
        .and_then(Value::as_str)
        != Some("remote")
    {
        return Ok(());
    }
    request(
        client,
        context,
        json!({"type":"session.openRemote","requestId":Uuid::now_v7().to_string(),"sessionId":session}),
        false,
    )
    .await
    .map(drop)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_executables_recognise_cli_invocations() {
        let args = |list: &[&str]| list.iter().map(OsString::from).collect::<Vec<_>>();
        let app = "/Applications/Kodosi.app/Contents/MacOS/Kodosi";
        assert!(is_invocation(&args(&["/opt/homebrew/bin/kodosi"])));
        assert!(is_invocation(&args(&["kodosi", "statsu"])));
        assert!(is_invocation(&args(&[app, "status"])));
        assert!(is_invocation(&args(&[app, "internal-host"])));
        assert!(is_invocation(&args(&[app, "--json", "status"])));
        assert!(is_invocation(&args(&[app, "--version"])));
        assert!(is_invocation(&args(&[app, "help", "terminal"])));
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
    fn every_command_in_a_skill_is_a_command_of_this_cli() {
        let words = |command: &str| {
            let mut words = vec![String::new()];
            let mut quoted = false;
            for letter in command.chars() {
                match letter {
                    '"' => quoted = !quoted,
                    letter if letter.is_whitespace() && !quoted => words.push(String::new()),
                    letter => words.last_mut().unwrap().push(letter),
                }
            }
            words
                .into_iter()
                .filter(|word| !word.is_empty() && !word.starts_with('['))
                .map(|word| match word.as_str() {
                    "<sequence>" | "<issue-number>" => "1".to_owned(),
                    "<conversation>" => Uuid::nil().to_string(),
                    word if word.starts_with('<') => "x".to_owned(),
                    word => word.to_owned(),
                })
                .collect::<Vec<_>>()
        };
        for skill in [
            include_str!("../../skills/kodosi-dosi/INSTRUCTIONS.md"),
            include_str!("../../skills/kodosi-dosi/SKILL.md"),
            include_str!("../../skills/kodosi-room/SKILL.md"),
        ] {
            let commands = skill.split('`').skip(1).step_by(2);
            for command in commands.filter(|code| code.starts_with("kodosi ")) {
                assert!(
                    Arguments::try_parse_from(words(command)).is_ok(),
                    "{command}"
                );
            }
        }
    }
    #[test]
    fn named_keys_become_the_bytes_a_terminal_reads() {
        assert_eq!(key_text("ctrl-c").unwrap(), "\u{3}");
        assert_eq!(key_text("Enter").unwrap(), "\r");
        assert_eq!(key_text("up").unwrap(), "\u{1b}[A");
        assert!(key_text("ctrl-1").is_err());
        assert!(key_text("hyper").is_err());
    }
    #[test]
    fn a_wait_ends_on_the_state_the_program_reports() {
        let entry = json!({"id":"t","prompt":false,"programStatus":{"state":"blocked"}});
        assert!(WaitState::Waiting.reached(Some(&entry)));
        assert!(!WaitState::Done.reached(Some(&entry)));
        assert!(!WaitState::Prompt.reached(Some(&entry)));
        assert!(WaitState::Closed.reached(None));
        assert!(!WaitState::Waiting.reached(None));
        assert_eq!(render::offline(&entry), None);
        let away = json!({"id":"t","status":"offline","hostName":"Studio"});
        assert_eq!(render::offline(&away).unwrap(), "Studio is offline.");
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
            Arguments::try_parse_from(["kodosi", "terminal", "start", "--branch", "fix/login"])
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
    fn older_account_events_cannot_restore_old_sessions() {
        let mut context = Context::from_events(&[
            json!({"type":"sessions.snapshot","accountUserId":"old","accountEpoch":1,"sessions":[{"id":"old-session"}]}),
        ]);
        assert!(
            context.observe(&json!({"type":"auth.required","accountUserId":null,"accountEpoch":2}))
        );
        assert_eq!(context.sessions, [] as [Value; 0]);
        assert!(!context.observe(&json!({"type":"sessions.snapshot","accountUserId":"old","accountEpoch":1,"sessions":[{"id":"old-session"}]})));
        assert_eq!(context.epoch, 2);
        assert!(context.user.is_none());
        assert_eq!(context.sessions, [] as [Value; 0]);
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
    fn every_command_says_terminal_and_room() {
        fn help(command: &mut clap::Command) -> String {
            let mut text = command.render_long_help().to_string();
            for subcommand in command.get_subcommands_mut() {
                text.push_str(&help(subcommand));
            }
            text
        }
        let mut command = Arguments::command();
        command.build();
        let text = help(&mut command).to_lowercase();
        assert!(text.contains("terminal") && text.contains("room"));
        assert!(!text.contains("session") && !text.contains("mission"));
    }
    #[test]
    fn terminal_close_and_native_resume_remain() {
        assert!(
            Arguments::try_parse_from([
                "kodosi",
                "terminal",
                "close",
                "01900000-0000-7000-8000-000000000001"
            ])
            .is_ok()
        );
        assert!(
            Arguments::try_parse_from([
                "kodosi",
                "terminal",
                "resume",
                "claude",
                "01900000-0000-7000-8000-000000000001",
                "/tmp"
            ])
            .is_ok()
        );
    }
}
