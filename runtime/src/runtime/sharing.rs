use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use serde_json::{Value, json};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Completion, Runtime};
use crate::{
    Command, Error, Event, EventBody, Result,
    identity::{
        friends::{FriendList, fingerprint, parse_invite},
        link_code::typed_code,
    },
    network::{self, BackendClient, ProjectMember, RemoteSession},
    protocol::{
        APPROVAL_WAIT, ApprovalOutcome, ClientKind, FriendEntry, FriendRequestEntry, SessionEntry,
        answer_time, parse_id, short_id,
    },
    text::{Arg, Text},
};

const MOST_APPROVALS: usize = 8;
const MOST_NAME: usize = 64;
/// How long a command may hold the lane while this device reads what its question needs.
const READING: Duration = Duration::from_mins(5);

/// Where a command comes from. A command of an app or of a program on this device that lets
/// someone new into the owner's terminals or projects waits for the owner's OK in an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    App,
    Command,
    /// Another device of the owner, which asked the owner before it sent the command.
    Device,
}

impl From<ClientKind> for Origin {
    fn from(kind: ClientKind) -> Self {
        match kind {
            ClientKind::App => Self::App,
            ClientKind::Command => Self::Command,
        }
    }
}

pub(super) struct Joined {
    pub(super) origin: Origin,
    pub(super) lifetime: CancellationToken,
}

pub(super) struct Requester {
    pub(super) consumer: Uuid,
    pub(super) lifetime: CancellationToken,
    pub(super) origin: Origin,
}

/// A command that waits before it runs, as its client sent it.
pub(super) struct Held {
    command: Command,
    computer: Option<String>,
    requester: Requester,
}

impl Held {
    pub(super) fn new(command: &Command, computer: Option<&str>, requester: Requester) -> Self {
        Self {
            command: command.clone(),
            computer: computer.map(str::to_owned),
            requester,
        }
    }
}

/// What waits for the owner's answer in an app.
enum Asked {
    Command(Box<Held>),
    /// A sign-in whose account would take the terminals that run on this computer.
    SignIn(network::SignIn),
}

impl Asked {
    fn gone(&self) -> bool {
        match self {
            Self::Command(held) => held.requester.lifetime.is_cancelled(),
            Self::SignIn(sign_in) => sign_in.reply.is_closed(),
        }
    }

    fn end(self, runtime: &Runtime, why: &Error) {
        match self {
            Self::Command(held) => runtime.error(&held.command, why),
            Self::SignIn(sign_in) => drop(sign_in.reply.send(Err(match why {
                Error::Stale => network::Error::Stale,
                Error::NotAllowed(text) => network::Error::NotAllowed(text.clone()),
                why => network::Error::Text(why.text()),
            }))),
        }
    }
}

pub(super) struct Approval {
    asked: Asked,
    question: Value,
    until: Instant,
}

/// The commands that change who reaches this account's terminals run one after another, so that
/// each question counts what the commands before it did.
#[derive(Default)]
pub(super) struct Lane {
    running: Option<Running>,
    queued: VecDeque<Queued>,
}

struct Running {
    request: String,
    until: Instant,
    ended: AtomicBool,
}

struct Queued {
    held: Held,
    until: Instant,
}

impl Lane {
    /// The final answer of the running command went out, so the next command may decide.
    pub(super) fn ended(&self, request: &str) {
        if let Some(running) = self
            .running
            .as_ref()
            .filter(|running| running.request == request)
        {
            running.ended.store(true, Ordering::Release);
        }
    }

    fn due(&self) -> Option<Instant> {
        let running = self.running.as_ref().map(|running| {
            if running.ended.load(Ordering::Acquire) {
                Instant::now()
            } else {
                running.until
            }
        });
        running
            .into_iter()
            .chain(self.queued.iter().map(|queued| queued.until))
            .min()
    }
}

/// What this device read for a question. A part stays missing when this device could not read
/// it, and a missing part asks the owner.
#[derive(Default)]
pub(super) struct Facts {
    pub(super) friends: Option<Friends>,
    /// This account's terminals as the server lists them now.
    pub(super) terminals: Option<Vec<RemoteSession>>,
    pub(super) members: Members,
    /// The device that shows the code of an approval, or why it cannot be approved.
    pub(super) device: Option<std::result::Result<String, Text>>,
}

/// The friend list that this account's devices signed, and the friends whose identity changed.
pub(super) struct Friends {
    pub(super) list: FriendList,
    pub(super) changed: BTreeSet<String>,
}

#[derive(Default)]
struct Needs {
    friends: bool,
    terminals: bool,
    projects: BTreeSet<Uuid>,
    /// The projects of this computer's terminals, when the question counts every project that
    /// holds a terminal of this account.
    holding: Option<Vec<Uuid>>,
    names: bool,
    code: Option<String>,
}

/// What this device knows of the people of a project.
#[derive(Default)]
pub(super) struct Membership {
    /// The people of the newest key state of the project that this device verified, when this
    /// account is one of them. Only these people can enter the project's terminals.
    pub(super) verified: Option<BTreeSet<String>>,
    /// The members as the server lists them, only for their handles.
    pub(super) listed: Vec<ProjectMember>,
}

pub(super) type Members = BTreeMap<Uuid, Membership>;

pub(super) enum Gate {
    Open,
    Ask(Question),
}

pub(super) struct Question {
    pub(super) title: Text,
    pub(super) message: Text,
    pub(super) confirm: Text,
}

/// Where a terminal of this account is, and who has it.
struct Spot {
    project: Option<Uuid>,
    people: Vec<String>,
}

/// What the server says about people: their handles and names.
#[derive(Default)]
struct Directory {
    friends: Vec<FriendEntry>,
    requests: Vec<FriendRequestEntry>,
}

impl Directory {
    /// The handle and the name of a person.
    fn about(&self, user: &str) -> Option<(&str, &str)> {
        let friends = self
            .friends
            .iter()
            .map(|friend| (&friend.user_id, &friend.handle, &friend.display_name));
        let requests = self
            .requests
            .iter()
            .map(|request| (&request.user_id, &request.handle, &request.display_name));
        friends
            .chain(requests)
            .find(|(id, ..)| *id == user)
            .map(|(_, handle, name)| (handle.as_str(), name.as_str()))
    }
}

impl Runtime {
    pub(super) fn admit(
        &mut self,
        command: &Command,
        computer: Option<&str>,
        requester: Requester,
    ) -> Result<()> {
        if matches!(command, Command::AnswerSharing { .. }) {
            return self.answer_sharing(command, requester.origin);
        }
        #[cfg(feature = "host")]
        if matches!(command, Command::Login { .. } | Command::FinishLogin { .. }) {
            self.sign_in_origin = Some(requester.origin);
        }
        if requester.origin == Origin::Device || !guarded(command, requester.origin) {
            return self.carry_out(command, computer, requester);
        }
        let held = Held::new(command, computer, requester);
        if !changes_access(command) {
            return self.decide(held);
        }
        self.advance_lane();
        if self.lane.running.is_none() {
            return self.run_in_lane(held);
        }
        if self.lane.queued.len() >= MOST_APPROVALS {
            return Err(Error::Refused(text!(
                "Other requests wait for your OK in the Kodosi app. Answer them first."
            )));
        }
        self.tell_waiting(
            command.request_id(),
            command.operation(),
            &text!("Waiting for an earlier request to end."),
        );
        self.lane.queued.push_back(Queued {
            held,
            until: Instant::now() + APPROVAL_WAIT,
        });
        Ok(())
    }

    fn run_in_lane(&mut self, held: Held) -> Result<()> {
        self.lane.running = Some(Running {
            request: held.command.request_id().to_owned(),
            until: Instant::now() + READING,
            ended: AtomicBool::new(false),
        });
        self.decide(held)
    }

    /// The next command decides once the command before it ended, or took too long. A command
    /// whose wait ran out ends at once, even while another one runs, since the run loop wakes for
    /// its deadline until it ends.
    pub(super) fn advance_lane(&mut self) {
        let now = Instant::now();
        let mut overdue = Vec::new();
        self.lane.queued.retain(|queued| {
            let waits = queued.until > now;
            if !waits && !queued.held.requester.lifetime.is_cancelled() {
                overdue.push(queued.held.command.clone());
            }
            waits
        });
        for command in overdue {
            self.error(
                &command,
                &Error::Refused(text!(
                    "Other requests wait for your OK in the Kodosi app. Answer them first."
                )),
            );
        }
        if let Some(running) = &self.lane.running {
            let ended = running.ended.load(Ordering::Acquire);
            if !ended && running.until > now {
                return;
            }
            if !ended {
                tracing::warn!(
                    "a request that changes who has access took too long, so the next one goes on"
                );
            }
            self.lane.running = None;
        }
        while self.lane.running.is_none()
            && let Some(Queued { held, .. }) = self.lane.queued.pop_front()
        {
            if held.requester.lifetime.is_cancelled() {
                continue;
            }
            let command = held.command.clone();
            if let Err(error) = self.run_in_lane(held) {
                self.error(&command, &error);
            }
        }
    }

    fn hold_lane(&mut self, request: &str, until: Instant) {
        if let Some(running) = self
            .lane
            .running
            .as_mut()
            .filter(|running| running.request == request)
        {
            running.until = until;
        }
    }

    pub(super) fn lane_due(&self) -> Option<Instant> {
        self.lane.due()
    }

    /// A guarded command makes the checks that this device can make at once, so that the owner is
    /// asked only about a command that can run. Then it reads what its question needs.
    fn decide(&mut self, held: Held) -> Result<()> {
        if self.scope.user.is_none() {
            // The credentials that run a command come before the account settles.
            return Err(network::Error::SignedOut.into());
        }
        self.precheck(&held)?;
        match self.needs(&held.command, held.requester.origin)? {
            None => self.ask_first(held, &Facts::default()),
            Some(needs) => {
                self.job_capacity()?;
                let network = self.network.clone();
                self.spawn(async move {
                    let facts = read_facts(&network, needs).await;
                    Completion::Checked { held, facts }
                });
                Ok(())
            }
        }
    }

    fn precheck(&mut self, held: &Held) -> Result<()> {
        let elsewhere = self.destination(
            &held.command,
            held.computer.as_deref(),
            held.requester.origin,
        )?;
        match (&held.command, elsewhere) {
            (Command::ShareSession { .. } | Command::AttachSession { .. }, None) => {
                self.membership_checks(&held.command).map(drop)
            }
            (Command::CreateSession { .. }, Some(computer)) => self.computer_online(&computer),
            #[cfg(feature = "host")]
            (Command::CreateSession { request_id, .. }, None) => {
                if self.started_by(request_id) {
                    Ok(())
                } else {
                    self.can_start(&held.command).map(drop)
                }
            }
            _ => Ok(()),
        }
    }

    fn computer_online(&self, computer: &str) -> Result<()> {
        let online = self
            .cached_events
            .get("devices.hosts")
            .and_then(|hosts| hosts["deviceIds"].as_array())
            .is_none_or(|hosts| hosts.iter().any(|host| host.as_str() == Some(computer)));
        if online {
            Ok(())
        } else {
            Err(Error::Text(text!("That computer is not online.")))
        }
    }

    fn needs(&self, command: &Command, origin: Origin) -> Result<Option<Needs>> {
        let mut needs = Needs::default();
        match command {
            Command::ShareSession { .. } => {
                let me = self.scope.user.as_deref().unwrap_or_default();
                if self.added(command, me)?.is_empty() {
                    return Ok(None);
                }
                needs.friends = true;
            }
            Command::AttachSession {
                project_id: Some(project),
                ..
            } => {
                needs.friends = true;
                needs.terminals = !self.hosts(&self.command_session(command)?);
                needs.projects.insert(parse_id(project)?);
            }
            Command::CreateSession {
                project_id: Some(project),
                ..
            } => {
                needs.friends = true;
                needs.projects.insert(parse_id(project)?);
            }
            Command::InviteProject { .. } => {
                needs.friends = true;
                needs.terminals = true;
            }
            Command::RequestFriend { .. }
            | Command::AcceptFriend { .. }
            | Command::VerifyFriend { .. }
            | Command::TrustFriend { .. } => {
                needs.friends = true;
                needs.terminals = true;
                needs.holding = Some(self.local_projects());
                needs.names = !matches!(command, Command::TrustFriend { .. });
            }
            Command::ApproveDevice { code, .. } if origin == Origin::Command => {
                needs.code = Some(code.clone());
            }
            _ => return Ok(None),
        }
        Ok(Some(needs))
    }

    pub(super) fn checked(&mut self, held: Held, facts: &Facts, current: bool) {
        let command = held.command.clone();
        let outcome = if !current || held.requester.lifetime.is_cancelled() {
            Err(Error::Stale)
        } else if changes_access(&command)
            && self
                .lane
                .running
                .as_ref()
                .is_none_or(|running| running.request != command.request_id())
        {
            Err(Error::Busy)
        } else {
            self.precheck(&held)
                .and_then(|()| self.ask_first(held, facts))
        };
        if let Err(error) = outcome {
            self.error(&command, &error);
        }
    }

    fn ask_first(&mut self, held: Held, facts: &Facts) -> Result<()> {
        match self.gate(&held.command, held.requester.origin, facts)? {
            Gate::Open => {
                let Held {
                    command,
                    computer,
                    requester,
                } = held;
                self.hold_lane(
                    command.request_id(),
                    Instant::now() + answer_time(command.operation()),
                );
                self.carry_out(&command, computer.as_deref(), requester)
            }
            Gate::Ask(question) => self.ask_about(held, question),
        }
    }

    fn carry_out(
        &mut self,
        command: &Command,
        computer: Option<&str>,
        requester: Requester,
    ) -> Result<()> {
        match self.destination(command, computer, requester.origin)? {
            Some(computer) => self.ask_computer(command, computer),
            None => self.apply_for_consumer(command, requester.consumer, requester.lifetime),
        }
    }

    /// The computer that runs the command when it is another one. A command from another device
    /// runs here, so it never travels on.
    pub(super) fn destination(
        &self,
        command: &Command,
        computer: Option<&str>,
        origin: Origin,
    ) -> Result<Option<String>> {
        if origin == Origin::Device {
            return Ok(None);
        }
        let own = self.network.identity().map(|identity| identity.device_id);
        if let Some(computer) = computer.filter(|computer| own.as_deref() != Some(*computer)) {
            return Ok(Some(computer.to_owned()));
        }
        if !matches!(command, Command::ShareSession { .. }) {
            return Ok(None);
        }
        let id = self.command_session(command)?;
        let Some(entry) = self.remotes.get(&id) else {
            return Ok(None);
        };
        if !entry.is_owner {
            return Err(Error::Refused(text!(
                "Only the owner can share this terminal."
            )));
        }
        if !self.host_online(id) {
            return Err(entry.host_name.as_deref().map_or_else(
                || self.offline(id),
                |computer| {
                    Error::Text(text!(
                        "{computer} is offline. Share when it is back.",
                        computer = shown(computer)
                    ))
                },
            ));
        }
        entry
            .host_device_id
            .clone()
            .map(Some)
            .ok_or(Error::NotFound)
    }

    pub(super) fn gate(&self, command: &Command, origin: Origin, facts: &Facts) -> Result<Gate> {
        let me = self
            .scope
            .user
            .as_deref()
            .ok_or_else(|| Error::from(network::Error::SignedOut))?;
        match command {
            Command::ShareSession { user_ids, .. } => self.share_gate(command, me, user_ids, facts),
            Command::AttachSession {
                project_id: Some(project),
                ..
            } => self.attach_gate(command, me, project, facts),
            Command::CreateSession {
                project_id: Some(project),
                request_id,
                ..
            } => self.create_gate(me, project, request_id, facts),
            Command::InviteProject {
                project_id,
                user_id,
                ..
            } => Ok(Gate::Ask(self.invite_question(project_id, user_id, facts))),
            Command::RequestFriend { username, .. } | Command::AcceptFriend { username, .. } => Ok(
                Gate::Ask(self.friend_question(command, me, username, facts)),
            ),
            Command::TrustFriend { username, .. } => Ok(self.trust_gate(username, facts)),
            Command::VerifyFriend {
                username, invite, ..
            } => Ok(self.verify_gate(command, me, username, invite, facts)),
            Command::ApproveDevice { code, .. } if origin == Origin::Command => {
                device_gate(code, facts)
            }
            Command::OfferDevice { .. } if origin == Origin::Command => Ok(Gate::Ask(Question {
                title: text!("Show a code that adds a device to your account?"),
                message: text!(
                    "A device that uses the code joins your account and gets full control of your terminals."
                ),
                confirm: text!("Show code"),
            })),
            Command::CreateRecoveryKey { .. } if origin == Origin::Command => {
                Ok(Gate::Ask(Question {
                    title: text!("Make a new recovery key?"),
                    message: text!(
                        "With the key and your sign-in, a device can join your account and get full control of your terminals. Your earlier key stops working."
                    ),
                    confirm: text!("Make key"),
                }))
            }
            _ => Ok(Gate::Open),
        }
    }

    /// People who get a terminal directly: only those who did not have it ask the owner, and only
    /// when the computer that hosts the terminal would let them in.
    fn share_gate(
        &self,
        command: &Command,
        me: &str,
        users: &[String],
        facts: &Facts,
    ) -> Result<Gate> {
        let id = self.command_session(command)?;
        let Some(terminal) = self.owned_terminal(id) else {
            return Ok(Gate::Open);
        };
        let added = self.added(command, me)?;
        if added.is_empty() {
            return Ok(Gate::Open);
        }
        if let Some(friends) = &facts.friends {
            let users = users.iter().cloned().collect();
            network::shareable(&friends.list, &friends.changed, &users)?;
        }
        let directory = self.directory();
        let named = added
            .iter()
            .map(|user| person(facts, &directory, user))
            .collect();
        let Some((people, one)) = listed(named) else {
            return Ok(Gate::Open);
        };
        let computer = computer_of(terminal);
        let message = if one {
            text!(
                "{person} gets full control: they can type anything as you on {computer}.",
                person = people.clone(),
                computer = &computer
            )
        } else {
            text!(
                "{people} get full control: they can type anything as you on {computer}.",
                people = people.clone(),
                computer = &computer
            )
        };
        Ok(Gate::Ask(Question {
            title: text!(
                "Share “{terminal}” with {people}?",
                terminal = shown(&terminal.name),
                people = people
            ),
            message,
            confirm: text!("Share"),
        }))
    }

    /// The people whom a share adds. Only the computer that hosts a terminal knows who has it. It
    /// refuses a share whose expected people are not those, so another computer's list counts
    /// only with them.
    fn added(&self, command: &Command, me: &str) -> Result<Vec<String>> {
        let Command::ShareSession {
            user_ids,
            expected_user_ids,
            ..
        } = command
        else {
            return Ok(Vec::new());
        };
        let id = self.command_session(command)?;
        let Some(terminal) = self.owned_terminal(id) else {
            return Ok(Vec::new());
        };
        let expected = expected_user_ids.as_deref();
        let known = self.hosts(&id) || expected.is_some();
        let kept = |user: &String| {
            known
                && terminal.shared_with.contains(user)
                && expected.is_none_or(|expected| expected.contains(user))
        };
        Ok(user_ids
            .iter()
            .filter(|user| user.as_str() != me && !kept(user))
            .cloned()
            .collect())
    }

    /// Who gets a terminal through a project: its verified people who are friends and do not have
    /// the terminal yet. Where the terminal is now comes from this computer for its own terminals
    /// and fresh from the server for the others.
    fn attach_gate(
        &self,
        command: &Command,
        me: &str,
        project: &str,
        facts: &Facts,
    ) -> Result<Gate> {
        let id = self.command_session(command)?;
        let Some(terminal) = self.owned_terminal(id) else {
            return Ok(Gate::Open);
        };
        let target = parse_id(project)?;
        let now = self.placement(id, facts);
        if now
            .as_ref()
            .is_some_and(|spot| spot.project == Some(target))
        {
            return Ok(Gate::Open);
        }
        let access = now.as_ref().map_or(&[][..], |spot| spot.people.as_slice());
        let label = self.project_label(project);
        let mut message = match self.gaining(me, target, access, facts).map(listed) {
            Some(None) if now.is_some() => return Ok(Gate::Open),
            Some(Some((person, true))) => text!(
                "{person} gets full control, and so will friends of yours who join later.",
                person = person
            ),
            Some(Some((people, false))) => text!(
                "{people} get full control, and so will friends of yours who join later.",
                people = people
            ),
            None | Some(None) => text!(
                "Members of {project} who are your friends get full control, and so will friends of yours who join later.",
                project = &label
            ),
        };
        if let Some(left) = now.and_then(|spot| spot.project) {
            let leaving = text!(
                "It leaves {project}.",
                project = self.project_label(&left.to_string())
            );
            message = sentences(message, leaving);
        }
        Ok(Gate::Ask(Question {
            title: text!(
                "Share “{terminal}” with {project}?",
                terminal = shown(&terminal.name),
                project = label
            ),
            message,
            confirm: text!("Share"),
        }))
    }

    fn create_gate(&self, me: &str, project: &str, request: &str, facts: &Facts) -> Result<Gate> {
        if self.started_by(request) {
            return Ok(Gate::Open);
        }
        let label = self.project_label(project);
        let message = match self.gaining(me, parse_id(project)?, &[], facts).map(listed) {
            Some(None) => return Ok(Gate::Open),
            Some(Some((person, true))) => text!(
                "{person} gets full control of it, and so will friends of yours who join later.",
                person = person
            ),
            Some(Some((people, false))) => text!(
                "{people} get full control of it, and so will friends of yours who join later.",
                people = people
            ),
            None => text!(
                "Members of {project} who are your friends get full control of it, and so will friends of yours who join later.",
                project = &label
            ),
        };
        Ok(Gate::Ask(Question {
            title: text!("Start the new terminal in {project}?", project = label),
            message,
            confirm: text!("Start"),
        }))
    }

    /// The verified people of a project who are friends of the signed list and do not have the
    /// terminal yet, or nothing when this device could not read who they are.
    fn gaining(
        &self,
        me: &str,
        project: Uuid,
        access: &[String],
        facts: &Facts,
    ) -> Option<Vec<String>> {
        let verified = facts.members.get(&project)?.verified.as_ref()?;
        let friends = &facts.friends.as_ref()?.list.friends;
        let directory = self.directory();
        Some(
            verified
                .iter()
                .filter(|user| {
                    user.as_str() != me && !access.contains(user) && friends.contains_key(*user)
                })
                .map(|user| person(facts, &directory, user))
                .collect(),
        )
    }

    fn invite_question(&self, project: &str, user: &str, facts: &Facts) -> Question {
        let person = person(facts, &self.directory(), user);
        let target = Uuid::parse_str(project).ok();
        let terminals = self.spots(facts).map_or_else(
            || {
                self.owned()
                    .filter(|entry| entry.project_id.as_deref() == Some(project))
                    .count()
            },
            |spots| {
                spots
                    .iter()
                    .filter(|spot| target.is_some() && spot.project == target)
                    .count()
            },
        );
        let message = match terminals {
            0 => text!(
                "{person} sees its chat and tasks and gets full control of your terminals in it.",
                person = &person
            ),
            1 => text!(
                "{person} sees its chat and tasks and gets full control of your terminal in it.",
                person = &person
            ),
            count => text!(
                "{person} sees its chat and tasks and gets full control of your {count} terminals in it.",
                person = &person,
                count = count
            ),
        };
        Question {
            title: text!(
                "Invite {person} to {project}?",
                person = person,
                project = self.project_label(project)
            ),
            message,
            confirm: text!("Invite"),
        }
    }

    /// Making a friend always asks: a friend gets the terminals of the projects that both people
    /// are in, and each terminal that the owner shares with them later. Who a handle is comes
    /// from the server, so its list only names the projects in the question.
    fn friend_question(
        &self,
        command: &Command,
        me: &str,
        username: &str,
        facts: &Facts,
    ) -> Question {
        let (handle, root) = parse_invite(username).map_or_else(
            || (username.trim().to_ascii_lowercase(), None),
            |(handle, root)| (handle, Some(root)),
        );
        let signed = facts.friends.as_ref().map(|friends| &friends.list);
        let stranger = |user: &String| {
            user.as_str() != me && !signed.is_some_and(|list| list.friends.contains_key(user))
        };
        let mut projects = Vec::new();
        let mut terminals = 0;
        for (project, count) in self.holding(facts).unwrap_or_default() {
            let Some(membership) = facts.members.get(&project) else {
                continue;
            };
            let Some(verified) = &membership.verified else {
                continue;
            };
            if membership.listed.iter().any(|member| {
                member.handle.eq_ignore_ascii_case(&handle)
                    && verified.contains(&member.user_id)
                    && stranger(&member.user_id)
            }) {
                projects.push(project);
                terminals += count;
            }
        }
        let person = self.friend_named(facts, &handle);
        let reach = match projects.as_slice() {
            [] => text!("Friends you share a terminal or project with get full control of it."),
            [project] if terminals == 1 => text!(
                "{person} is in {project} with your terminal there. As your friend, they get full control of it.",
                person = &person,
                project = self.project_label(&project.to_string())
            ),
            [project] => text!(
                "{person} is in {project} with your terminals there. As your friend, they get full control of them.",
                person = &person,
                project = self.project_label(&project.to_string())
            ),
            _ => text!(
                "{person} is in projects with {count} of your terminals. As your friend, they get full control of them.",
                person = &person,
                count = terminals
            ),
        };
        let mut message = sentences(
            reach,
            text!("Be friends only with people you would let use your computer."),
        );
        if let Some(root) = root {
            message = sentences(message, identity_in(&root));
        }
        let (title, confirm) = if matches!(command, Command::AcceptFriend { .. }) {
            (
                text!("Accept {person} as a friend?", person = &person),
                text!("Accept"),
            )
        } else {
            (
                text!("Add {person} as a friend?", person = &person),
                text!("Add"),
            )
        };
        Question {
            title,
            message,
            confirm,
        }
    }

    /// Trusting a new identity always asks, unless the command cannot run: it trusts only a
    /// friend whom nobody verified with an invite.
    fn trust_gate(&self, username: &str, facts: &Facts) -> Gate {
        let handle = username.trim();
        let user = match &facts.friends {
            Some(friends) => match friends.list.trustable(handle) {
                Some((user, _)) => Some(user.as_str()),
                None => return Gate::Open,
            },
            None => None,
        };
        let person = self.friend_named(facts, handle);
        Gate::Ask(Question {
            title: text!("Trust the new identity of {person}?", person = &person),
            message: sentences(
                self.holder(facts, user, &person),
                text!("Trust it only if they told you that their identity changed."),
            ),
            confirm: text!("Trust"),
        })
    }

    /// Verifying with an invite always asks: for a person outside the signed list it makes a
    /// friend, and for a friend it trusts the identity of the invite.
    fn verify_gate(
        &self,
        command: &Command,
        me: &str,
        username: &str,
        invite: &str,
        facts: &Facts,
    ) -> Gate {
        let Some((handle, root)) =
            parse_invite(invite).filter(|(handle, _)| handle.eq_ignore_ascii_case(username.trim()))
        else {
            return Gate::Open;
        };
        let signed = facts
            .friends
            .as_ref()
            .map(|friends| (friends.list.by_handle(&handle), &friends.changed));
        let mut question = if let Some((None, _)) = signed {
            self.friend_question(command, me, &handle, facts)
        } else {
            let person = self.friend_named(facts, &handle);
            let known = signed.and_then(|(friend, changed)| {
                friend.map(|(user, friend)| {
                    (user, friend.root() == Some(root) && !changed.contains(user))
                })
            });
            let (title, confirm) = if known.is_some_and(|(_, same)| same) {
                (text!("Verify {person}?", person = &person), text!("Verify"))
            } else {
                (
                    text!("Trust the new identity of {person}?", person = &person),
                    text!("Trust"),
                )
            };
            Question {
                title,
                message: self.holder(facts, known.map(|(user, _)| user.as_str()), &person),
                confirm,
            }
        };
        question.message = sentences(question.message, identity_in(&root));
        Gate::Ask(question)
    }

    /// What whoever holds the identity of a friend gets. A friend whom this device cannot find
    /// may have terminals already.
    fn holder(&self, facts: &Facts, user: Option<&str>, person: &str) -> Text {
        if user.is_none_or(|user| self.reached(facts, |shared| shared == user)) {
            text!(
                "Whoever holds it gets full control of your terminals shared with {person}.",
                person = person
            )
        } else {
            text!(
                "Whoever holds it can act as {person}, with full control of each terminal and project that you share with them.",
                person = person
            )
        }
    }

    /// A handle as a question shows it, with the name of a friend of the signed list.
    fn friend_named(&self, facts: &Facts, handle: &str) -> String {
        facts
            .friends
            .as_ref()
            .and_then(|friends| friends.list.by_handle(handle))
            .map_or_else(
                || called(handle, None),
                |(user, _)| person(facts, &self.directory(), user),
            )
    }

    /// Whether one of these people may reach a terminal of this account: one shared with them, one
    /// in a project that has them, or one that this device could not read.
    fn reached(&self, facts: &Facts, who: impl Fn(&String) -> bool) -> bool {
        let Some(spots) = self.spots(facts) else {
            return true;
        };
        spots.iter().any(|spot| {
            spot.people.iter().any(&who)
                || spot.project.is_some_and(|project| {
                    facts
                        .members
                        .get(&project)
                        .and_then(|membership| membership.verified.as_ref())
                        .is_none_or(|verified| verified.iter().any(&who))
                })
        })
    }

    /// Where a terminal of this account is now, and who has it.
    fn placement(&self, id: Uuid, facts: &Facts) -> Option<Spot> {
        self.spot_here(id).or_else(|| {
            facts
                .terminals
                .as_ref()?
                .iter()
                .find(|terminal| terminal.id == id)
                .map(listed_spot)
        })
    }

    #[cfg(feature = "host")]
    fn spot_here(&self, id: Uuid) -> Option<Spot> {
        self.local.get(&id).map(|local| local_spot(&local.entry))
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer hosts no terminals")]
    const fn spot_here(&self, _id: Uuid) -> Option<Spot> {
        None
    }

    /// This account's terminals: this computer's own as it keeps them, and the others as the
    /// server lists them now.
    fn spots(&self, facts: &Facts) -> Option<Vec<Spot>> {
        let others = facts
            .terminals
            .as_ref()?
            .iter()
            .filter(|terminal| !self.hosts(&terminal.id))
            .map(listed_spot);
        #[cfg(feature = "host")]
        let others = others.chain(self.local.values().map(|local| local_spot(&local.entry)));
        Some(others.collect())
    }

    /// The projects that hold terminals of this account, with how many each holds.
    fn holding(&self, facts: &Facts) -> Option<BTreeMap<Uuid, usize>> {
        let mut holding = BTreeMap::new();
        for project in self
            .spots(facts)?
            .into_iter()
            .filter_map(|spot| spot.project)
        {
            *holding.entry(project).or_default() += 1;
        }
        Some(holding)
    }

    #[cfg(feature = "host")]
    fn local_projects(&self) -> Vec<Uuid> {
        self.local
            .values()
            .filter_map(|local| Uuid::parse_str(local.entry.project_id.as_deref()?).ok())
            .collect()
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer hosts no terminals")]
    const fn local_projects(&self) -> Vec<Uuid> {
        Vec::new()
    }

    fn ask_about(&mut self, held: Held, question: Question) -> Result<()> {
        self.can_ask()?;
        let operation = held.command.operation();
        let request = held.command.request_id().to_owned();
        let origin = held.requester.origin;
        let until = self.ask_owner(
            Asked::Command(Box::new(held)),
            question,
            origin,
            operation,
            &request,
        );
        self.hold_lane(&request, until + answer_time(operation));
        Ok(())
    }

    fn can_ask(&self) -> Result<()> {
        if self.apps().next().is_none() {
            return Err(Error::Refused(text!(
                "Open the Kodosi app on this computer to allow this."
            )));
        }
        if self.approvals.len() >= MOST_APPROVALS {
            return Err(Error::Refused(text!(
                "Other requests wait for your OK in the Kodosi app. Answer them first."
            )));
        }
        Ok(())
    }

    /// Asks every app of this computer, and gives until when the question waits.
    fn ask_owner(
        &mut self,
        asked: Asked,
        question: Question,
        origin: Origin,
        operation: &str,
        request: &str,
    ) -> Instant {
        let id = Uuid::now_v7();
        let (from, message) = match origin {
            Origin::App => (ClientKind::App, question.message),
            Origin::Command | Origin::Device => (
                ClientKind::Command,
                sentences(
                    question.message,
                    text!("The kodosi command asked for this."),
                ),
            ),
        };
        let shown = json!({"type":"sharing.approval","approvalId":id,"operation":operation,
            "title":question.title,"message":message,"confirm":question.confirm,"from":from});
        self.tell_apps(&shown);
        if from == ClientKind::Command {
            self.tell_waiting(
                request,
                operation,
                &text!("Waiting for your OK in the Kodosi app."),
            );
        }
        let until = Instant::now() + APPROVAL_WAIT;
        self.approvals.insert(
            id,
            Approval {
                asked,
                question: shown,
                until,
            },
        );
        until
    }

    fn tell_waiting(&self, request: &str, operation: &str, message: &Text) {
        self.emit(json!({"type":"sharing.waiting","requestId":request,
            "operation":operation,"message":message}));
    }

    fn answer_sharing(&mut self, command: &Command, origin: Origin) -> Result<()> {
        let Command::AnswerSharing {
            approval_id, allow, ..
        } = command
        else {
            return Err(Error::Invalid(
                "expected an answer to a question".to_owned(),
            ));
        };
        if origin != Origin::App {
            return Err(Error::Refused(text!(
                "Only the Kodosi app can allow sharing."
            )));
        }
        let outcome = if *allow {
            ApprovalOutcome::Allowed
        } else {
            ApprovalOutcome::Refused
        };
        let asked = self
            .end_approval(parse_id(approval_id)?, outcome)
            .ok_or_else(|| Error::Refused(text!("This request has already ended.")))?
            .asked;
        self.result(command);
        let refused = Error::NotAllowed(text!("You did not allow this in Kodosi."));
        match asked {
            Asked::Command(held) => {
                let Held {
                    command: asked,
                    computer,
                    requester,
                } = *held;
                let outcome = if *allow {
                    self.hold_lane(
                        asked.request_id(),
                        Instant::now() + answer_time(asked.operation()),
                    );
                    self.carry_out(&asked, computer.as_deref(), requester)
                } else {
                    Err(refused)
                };
                if let Err(error) = outcome {
                    self.error(&asked, &error);
                }
            }
            Asked::SignIn(sign_in) if *allow => drop(sign_in.reply.send(Ok(()))),
            asked @ Asked::SignIn(_) => asked.end(self, &refused),
        }
        Ok(())
    }

    fn end_approval(&mut self, id: Uuid, outcome: ApprovalOutcome) -> Option<Approval> {
        let approval = self.approvals.remove(&id)?;
        self.tell_apps(&json!({"type":"sharing.approvalEnded","approvalId":id,"outcome":outcome}));
        Some(approval)
    }

    /// Questions whose time ran out, or whose asker left, end without an answer.
    pub(super) fn expire_approvals(&mut self) {
        let now = Instant::now();
        let ended = self
            .approvals
            .iter()
            .filter(|(_, approval)| approval.until <= now || approval.asked.gone())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in ended {
            if let Some(approval) = self.end_approval(id, ApprovalOutcome::Expired) {
                approval.asked.end(
                    self,
                    &Error::NotAllowed(text!("Kodosi did not get your OK in time.")),
                );
            }
        }
    }

    pub(super) fn forget_approvals(&mut self) {
        let ids = self.approvals.keys().copied().collect::<Vec<_>>();
        for id in ids {
            if let Some(approval) = self.end_approval(id, ApprovalOutcome::Expired) {
                approval.asked.end(self, &Error::Stale);
            }
        }
        for Queued { held, .. } in std::mem::take(&mut self.lane.queued) {
            self.error(&held.command, &Error::Stale);
        }
        self.lane.running = None;
    }

    pub(super) fn approvals_due(&self) -> Option<Instant> {
        self.approvals
            .values()
            .map(|approval| approval.until)
            .chain(self.lane_due())
            .min()
    }

    /// The open questions, for an app that joins or reads the whole state again.
    pub(super) fn open_questions(&self, consumer: Uuid) -> Vec<Event> {
        if !self
            .consumers
            .get(&consumer)
            .is_some_and(|joined| joined.origin == Origin::App)
        {
            return Vec::new();
        }
        self.approvals
            .values()
            .filter_map(|approval| {
                let mut event = self.event(approval.question.clone()).ok()?;
                event.recipient = Some(consumer);
                Some(event)
            })
            .collect()
    }

    pub(super) fn show_approvals(&self, consumer: Uuid) {
        for event in self.open_questions(consumer) {
            self.send_event(event);
        }
    }

    /// A sign-in that the kodosi command started asks the owner before its account takes the
    /// terminals that run here from the account that ran them.
    pub(super) fn sign_in_asked(&mut self, sign_in: network::SignIn) {
        let running = self.taken_by(&sign_in.user_id);
        if running == 0 {
            drop(sign_in.reply.send(Ok(())));
            return;
        }
        let refusal = if self.apps().next().is_none() {
            Some(text!(
                "Close the terminals on this computer, or sign in from the Kodosi app."
            ))
        } else if self.approvals.len() >= MOST_APPROVALS {
            Some(text!(
                "Other requests wait for your OK in the Kodosi app. Answer them first."
            ))
        } else {
            None
        };
        if let Some(refusal) = refusal {
            drop(sign_in.reply.send(Err(network::Error::Text(refusal))));
            return;
        }
        let account = if sign_in.handle.is_empty() {
            short_id(&sign_in.user_id).to_owned()
        } else {
            shown(&sign_in.handle)
        };
        let message = if running == 1 {
            text!(
                "The terminal that runs on this computer moves to that account, which gets full control of it."
            )
        } else {
            text!(
                "The {count} terminals that run on this computer move to that account, which gets full control of them.",
                count = running
            )
        };
        let question = Question {
            title: text!("Sign in as {account}?", account = account),
            message,
            confirm: text!("Sign in"),
        };
        let request = sign_in.request_id.clone();
        self.ask_owner(
            Asked::SignIn(sign_in),
            question,
            Origin::Command,
            "auth.login.start",
            &request,
        );
    }

    /// How many terminals that run here a sign-in of the kodosi command would take from the
    /// account that ran them.
    #[cfg(feature = "host")]
    fn taken_by(&self, user: &str) -> usize {
        if self.sign_in_origin != Some(Origin::Command)
            || self
                .terminals_owner
                .as_ref()
                .is_none_or(|owner| owner == user)
        {
            return 0;
        }
        self.local
            .values()
            .filter(|local| local.comes_back())
            .count()
    }

    #[cfg(not(feature = "host"))]
    #[expect(clippy::unused_self, reason = "a viewer runs no terminals")]
    const fn taken_by(&self, _user: &str) -> usize {
        0
    }

    fn apps(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.consumers
            .iter()
            .filter(|(_, joined)| joined.origin == Origin::App && !joined.lifetime.is_cancelled())
            .map(|(consumer, _)| *consumer)
    }

    fn tell_apps(&self, value: &Value) {
        for app in self.apps() {
            self.tell(app, value.clone());
        }
    }

    fn owned_terminal(&self, id: Uuid) -> Option<&SessionEntry> {
        #[cfg(feature = "host")]
        if let Some(local) = self.local.get(&id) {
            return Some(&local.entry);
        }
        self.remotes.get(&id).filter(|entry| entry.is_owner)
    }

    fn owned(&self) -> impl Iterator<Item = &SessionEntry> {
        let owned = self.remotes.values().filter(|entry| entry.is_owner);
        #[cfg(feature = "host")]
        let owned = owned.chain(self.local.values().map(|local| &local.entry));
        owned
    }

    fn directory(&self) -> Directory {
        let snapshot = self.cached_events.get("friends.snapshot").cloned();
        let Some(Ok(EventBody::FriendsSnapshot {
            friends,
            incoming,
            outgoing,
            ..
        })) = snapshot.map(EventBody::new)
        else {
            return Directory::default();
        };
        Directory {
            friends,
            requests: incoming.into_iter().chain(outgoing).collect(),
        }
    }

    fn project_label(&self, id: &str) -> String {
        self.project_name(id)
            .map_or_else(|| short_id(id).to_owned(), |name| shown(&name))
    }
}

/// The commands that the owner may have to allow.
fn guarded(command: &Command, origin: Origin) -> bool {
    changes_access(command)
        || origin == Origin::Command
            && matches!(
                command,
                Command::ApproveDevice { .. }
                    | Command::OfferDevice { .. }
                    | Command::CreateRecoveryKey { .. }
            )
}

/// The commands that change who reaches this account's terminals.
const fn changes_access(command: &Command) -> bool {
    matches!(
        command,
        Command::ShareSession { .. }
            | Command::AttachSession { .. }
            | Command::CreateSession {
                project_id: Some(_),
                ..
            }
            | Command::InviteProject { .. }
            | Command::RequestFriend { .. }
            | Command::AcceptFriend { .. }
            | Command::TrustFriend { .. }
            | Command::VerifyFriend { .. }
    )
}

/// A device that joins the account gets every terminal, so a command that approves one asks.
fn device_gate(code: &str, facts: &Facts) -> Result<Gate> {
    let device = match &facts.device {
        Some(Ok(label)) => shown(label),
        Some(Err(why)) => return Err(Error::Text(why.clone())),
        None => return Ok(Gate::Open),
    };
    let message = typed_code(code).map_or_else(
        || text!("It gets full control of your terminals. Approve it only if it is yours."),
        |code| {
            text!(
                "It gets full control of your terminals. Approve it only if it is yours and shows the code {code}.",
                code = code
            )
        },
    );
    Ok(Gate::Ask(Question {
        title: text!(
            "Add the device “{device}” to your account?",
            device = device
        ),
        message,
        confirm: text!("Approve"),
    }))
}

/// Reads what a question needs.
async fn read_facts(network: &BackendClient, needs: Needs) -> Facts {
    let friends = if needs.friends {
        network
            .signed_friends()
            .await
            .inspect_err(|error| {
                tracing::info!(%error, "this device could not read the signed friend list");
            })
            .ok()
            .map(|(list, changed)| Friends { list, changed })
    } else {
        None
    };
    let terminals = if needs.terminals {
        network
            .listed_terminals()
            .await
            .inspect_err(|error| {
                tracing::info!(%error, "this device could not read the terminals of the account");
            })
            .ok()
    } else {
        None
    };
    let mut projects = needs.projects;
    if let Some(local) = needs.holding {
        projects.extend(local);
        projects.extend(
            terminals
                .iter()
                .flatten()
                .filter_map(|terminal| terminal.project_id),
        );
    }
    let members = members_of(network, projects, needs.names).await;
    let device = match needs.code {
        Some(code) => Some(
            network
                .link_label(&code)
                .await
                .map_err(|error| error.text()),
        ),
        None => None,
    };
    Facts {
        friends,
        terminals,
        members,
        device,
    }
}

/// A project whose people this device cannot verify counts as one that anybody may be in.
async fn members_of(network: &BackendClient, projects: BTreeSet<Uuid>, names: bool) -> Members {
    let mut members = Members::new();
    for project in projects {
        let verified = network
            .verified_members(project)
            .await
            .unwrap_or_else(|error| {
                tracing::info!(%error, "this device could not verify the people of a project");
                None
            });
        let listed = if names {
            network.project_members(project).await.unwrap_or_default()
        } else {
            Vec::new()
        };
        members.insert(project, Membership { verified, listed });
    }
    members
}

#[cfg(feature = "host")]
fn local_spot(entry: &SessionEntry) -> Spot {
    Spot {
        project: entry
            .project_id
            .as_deref()
            .and_then(|project| Uuid::parse_str(project).ok()),
        people: entry.shared_with.clone(),
    }
}

fn listed_spot(terminal: &RemoteSession) -> Spot {
    Spot {
        project: terminal.project_id,
        people: terminal.shared_with.clone(),
    }
}

fn computer_of(terminal: &SessionEntry) -> String {
    let name = terminal.host_name.clone();
    #[cfg(feature = "host")]
    let name = name.or_else(|| Some(network::host_label()));
    name.as_deref().map_or_default(shown)
}

/// How a question names a person: by handle, since anybody can take any name, and with the
/// name too for a friend whom this account's devices signed.
fn person(facts: &Facts, directory: &Directory, user: &str) -> String {
    let signed = facts
        .friends
        .as_ref()
        .and_then(|friends| friends.list.friends.get(user));
    match (signed, directory.about(user)) {
        (Some(friend), about) => called(&friend.handle, about.map(|(_, name)| name)),
        (None, Some((handle, _))) => called(handle, None),
        (None, None) => short_id(user).to_owned(),
    }
}

/// A handle as the apps show it, with the name in front when the name says something more.
fn called(handle: &str, name: Option<&str>) -> String {
    let handle = shown(handle);
    name.map(shown)
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case(&handle))
        .map_or_else(
            || format!("@{handle}"),
            |name| format!("{name} (@{handle})"),
        )
}

fn identity_in(root: &crate::identity::pins::Root) -> Text {
    text!(
        "The identity in the invite starts with {fingerprint}.",
        fingerprint = fingerprint(root)
    )
}

/// Names come from other people and from agents, so they are clipped and lose hidden letters.
fn shown(name: &str) -> String {
    crate::terminal::display_text(name, MOST_NAME).unwrap_or_default()
}

/// The names of people, and whether there is only one.
fn listed(mut names: Vec<String>) -> Option<(Arg, bool)> {
    names.sort_by_key(|name| name.to_lowercase());
    let list = match names.as_slice() {
        [] => return None,
        [only] => Arg::from(only),
        [first, second] => Arg::from(text!(
            "{first} and {second}",
            first = first,
            second = second
        )),
        [first, second, third] => Arg::from(text!(
            "{first}, {second} and {third}",
            first = first,
            second = second,
            third = third
        )),
        [first, second, rest @ ..] => Arg::from(text!(
            "{first}, {second} and {count} others",
            first = first,
            second = second,
            count = rest.len()
        )),
    };
    Some((list, names.len() == 1))
}

fn sentences(sentence: Text, next: Text) -> Text {
    text!("{sentence} {next}", sentence = sentence, next = next)
}
