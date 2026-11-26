use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{sync::watch, time::Instant};
use uuid::Uuid;

use super::{Completion, MAX_JOBS, Runtime, saver::Saver};
use crate::{
    Command, Error, EventBody, Result,
    protocol::{ComputerPower, ComputerSystem, KeepAwake, PowerBusy, WakeDetails, WakeTest},
    text::Text,
    wake::{self, OwnNetwork, Rouse, Roused, Tested, WakeAbility},
};

const MOST_BYTES: usize = 1024 * 1024;
/// A computer can change its network while it is on, for example from a cable to Wi-Fi.
const REFRESH: Duration = Duration::from_mins(10);
const RETRY: Duration = Duration::from_mins(2);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Known {
    details: WakeDetails,
    /// The result of a test that the computer does not have yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unreported: Option<WakeTest>,
}

#[derive(Default)]
struct Ask {
    /// The number of the question that is on its way. An answer to an earlier question is old.
    asking: Option<u64>,
    next: Option<Instant>,
}

pub(super) struct Wakes {
    /// What each computer of each account said about its wake. A sleeping computer answers
    /// nothing, so this device keeps the last answer.
    accounts: BTreeMap<String, BTreeMap<String, Known>>,
    saver: Saver,
    asks: HashMap<String, Ask>,
    asked: u64,
    online: watch::Sender<BTreeSet<String>>,
    /// Whether the server said in this session which computers are online. Before that, a
    /// computer that is not in the list is not known to sleep.
    heard: bool,
    busy: HashMap<String, PowerBusy>,
    /// What the person can do about a computer that did not wake or did not go to sleep. It
    /// stays until that computer comes or goes.
    failed: HashMap<String, Text>,
    own: OwnNetwork,
    /// The count of connections to the server when this device last read its cards. A new
    /// connection can mean a new network.
    own_read: Option<u64>,
    reading_own: bool,
    published: Option<Value>,
}

impl Wakes {
    pub(super) fn load(data_root: &Path) -> Self {
        let file = data_root.join("network").join("wake.json");
        let accounts = std::fs::read(&file)
            .ok()
            .filter(|bytes| bytes.len() <= MOST_BYTES)
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            accounts,
            saver: Saver::file(file, "how the computers wake"),
            asks: HashMap::new(),
            asked: 0,
            online: watch::Sender::new(BTreeSet::new()),
            heard: false,
            busy: HashMap::new(),
            failed: HashMap::new(),
            own: OwnNetwork::default(),
            own_read: None,
            reading_own: false,
            published: None,
        }
    }

    fn save(&self) {
        match serde_json::to_vec(&self.accounts) {
            Ok(bytes) => self.saver.save(bytes),
            Err(error) => tracing::warn!(%error, "could not save how the computers wake"),
        }
    }

    pub(super) async fn saved(&self) {
        self.saver.saved().await;
    }

    pub(super) fn forget_account(&mut self) {
        self.asks.clear();
        self.online.send_replace(BTreeSet::new());
        self.heard = false;
        self.busy.clear();
        self.failed.clear();
        self.published = None;
    }

    fn is_online(&self, device: &str) -> bool {
        self.online.borrow().contains(device)
    }

    #[cfg(test)]
    pub(super) fn unreported(&self, user: &str, device: &str) -> Option<WakeTest> {
        self.accounts.get(user)?.get(device)?.unreported.clone()
    }
}

#[cfg(feature = "host")]
fn result_with(command: &Command, name: &str, value: impl Serialize) -> Value {
    json!({"type":"power.result", "requestId":command.request_id(),
        "operation":command.operation(), name:value})
}

fn details_of(answer: Value) -> Result<WakeDetails> {
    match serde_json::from_value(answer) {
        Ok(EventBody::PowerResult {
            wake: Some(details),
            ..
        }) => {
            details.validate()?;
            Ok(details)
        }
        Ok(EventBody::PowerError { message, .. }) => Err(Error::Refused(message)),
        _ => Err(Error::Text(crate::protocol::other_version())),
    }
}

fn expectation(ability: WakeAbility, details: &WakeDetails, computer: &str) -> Option<Text> {
    Some(match ability {
        WakeAbility::Tested => text!("Wakes from sleep. Tested."),
        WakeAbility::Ready => text!("Should wake from sleep. Not tested."),
        WakeAbility::Failed => text!("Did not wake from sleep in the test."),
        WakeAbility::Radio => text!("Does not wake from sleep on Wi-Fi. Use a network cable."),
        WakeAbility::Fix => match details.system {
            ComputerSystem::Mac => text!(
                "To let {computer} wake from sleep, turn on “Wake for network access” in System Settings on it.",
                computer = computer
            ),
            ComputerSystem::Windows => text!(
                "To let {computer} wake from sleep, let its network adapter wake the computer, in Device Manager on it.",
                computer = computer
            ),
            ComputerSystem::Linux => text!(
                "To let {computer} wake from sleep, turn on Wake-on-LAN for its wired connection.",
                computer = computer
            ),
        },
        WakeAbility::Unknown => return None,
    })
}

/// `nobody` is known only once this device has read its own networks.
fn sleep_warning(ability: WakeAbility, nobody: bool, computer: &str) -> Option<Text> {
    match ability {
        WakeAbility::Tested | WakeAbility::Ready => nobody.then(|| {
            text!(
                "None of your other devices on the network of {computer} can wake it now. You may have to wake it by hand.",
                computer = computer
            )
        }),
        WakeAbility::Failed => Some(text!(
            "{computer} did not wake in its last test. You may have to wake it by hand.",
            computer = computer
        )),
        WakeAbility::Radio | WakeAbility::Fix | WakeAbility::Unknown => Some(text!(
            "Kodosi cannot wake {computer}. Wake it by hand.",
            computer = computer
        )),
    }
}

fn did_not_wake(
    roused: &Roused,
    details: &WakeDetails,
    computer: &str,
    helper: Option<&str>,
) -> Text {
    if roused.signaled {
        match details.system {
            ComputerSystem::Mac => {
                text!(
                    "{computer} did not wake. Wake it by hand.",
                    computer = computer
                )
            }
            ComputerSystem::Windows if details.fast_startup => text!(
                "{computer} did not wake. If it is shut down, it wakes only with Fast Startup off in Windows.",
                computer = computer
            ),
            ComputerSystem::Windows | ComputerSystem::Linux => text!(
                "{computer} did not wake. Turn on Wake-on-LAN in its firmware (BIOS) settings.",
                computer = computer
            ),
        }
    } else if let Some(helper) = helper {
        text!(
            "Allow Kodosi to use the local network on {helper}, in System Settings › Privacy & Security.",
            helper = helper
        )
    } else {
        nobody_there(computer)
    }
}

fn did_not_stay_asleep(computer: &str) -> Text {
    text!(
        "{computer} did not stay asleep. The test has no result.",
        computer = computer
    )
}

fn nobody_there(computer: &str) -> Text {
    text!(
        "None of your devices is awake on the network of {computer}.",
        computer = computer
    )
}

impl Runtime {
    #[cfg(test)]
    pub(super) const fn wakes_asked(&self) -> u64 {
        self.wakes.asked
    }

    pub(super) fn power_event(&self) -> Value {
        #[cfg_attr(
            not(feature = "host"),
            expect(unused_mut, reason = "a viewer has no setting")
        )]
        let mut event = json!({"type":"power.state", "computers":self.computers()});
        #[cfg(feature = "host")]
        {
            event["keepAwake"] = json!(self.power.keeps_awake());
        }
        event
    }

    pub(super) fn publish_power(&mut self) {
        let event = self.power_event();
        if self.wakes.published.as_ref() != Some(&event) {
            self.wakes.published = Some(event.clone());
            self.emit(event);
        }
    }

    fn device_labels(&self) -> BTreeMap<String, String> {
        self.cached_events
            .get("devices.list")
            .and_then(|list| list["devices"].as_array())
            .into_iter()
            .flatten()
            .filter(|device| device["recoveryKey"] != true)
            .filter_map(|device| {
                Some((
                    device["deviceId"].as_str()?.to_owned(),
                    device["label"].as_str()?.to_owned(),
                ))
            })
            .collect()
    }

    fn known(&self, device: &str) -> Option<&Known> {
        self.wakes
            .accounts
            .get(self.scope.user.as_ref()?)?
            .get(device)
    }

    /// Whether this device, or an awake computer of the person, is on the network of the
    /// sleeping computer now.
    fn sender_for(&self, sleeper: &str, details: &WakeDetails) -> bool {
        if self.wakes.own.reaches(details) {
            return true;
        }
        let online = self.wakes.online.borrow();
        let Some(known) = self
            .scope
            .user
            .as_ref()
            .and_then(|user| self.wakes.accounts.get(user))
        else {
            return false;
        };
        online
            .iter()
            .filter(|helper| *helper != sleeper)
            .filter_map(|helper| known.get(helper))
            .any(|helper| wake::helps(&helper.details, details))
    }

    fn computers(&self) -> Vec<ComputerPower> {
        let Some(identity) = self.network.identity().filter(|identity| identity.enrolled) else {
            return Vec::new();
        };
        let labels = self.device_labels();
        // Until this device has read its own cards, it does not know which networks it is on.
        let connected = self.wakes.heard && self.online && self.wakes.own_read.is_some();
        let mut computers = Vec::new();
        #[cfg(feature = "host")]
        if self.wakes.own_read.is_some() {
            let details = self
                .wakes
                .own
                .details(self.power.keeps_awake(), self.power.tested().cloned());
            let ability = wake::ability(&details, None);
            let label = crate::network::host_label();
            computers.push(ComputerPower {
                device_id: identity.device_id.clone(),
                note: expectation(ability, &details, &label),
                problem: None,
                sleep_warning: None,
                can_wake: false,
                can_test: false,
                busy: None,
                keep_awake: details.keep_awake,
            });
        }
        let known = self
            .scope
            .user
            .as_ref()
            .and_then(|user| self.wakes.accounts.get(user));
        for (device, known) in known.into_iter().flatten() {
            let Some(label) = labels.get(device).filter(|_| *device != identity.device_id) else {
                continue;
            };
            let online = self.wakes.is_online(device);
            let ability = wake::ability(&known.details, known.unreported.as_ref());
            let busy = self.wakes.busy.get(device).copied();
            let usable = matches!(
                ability,
                WakeAbility::Tested | WakeAbility::Ready | WakeAbility::Failed
            );
            let sender = usable && connected && self.sender_for(device, &known.details);
            let problem = self.wakes.failed.get(device).cloned().or_else(|| {
                (usable && connected && !online && !sender).then(|| nobody_there(label))
            });
            let sleep_warning = if online {
                sleep_warning(ability, connected && !sender, label)
            } else {
                None
            };
            computers.push(ComputerPower {
                device_id: device.clone(),
                note: expectation(ability, &known.details, label),
                problem,
                sleep_warning,
                can_wake: sender && !online && busy.is_none(),
                can_test: sender && online && busy.is_none(),
                busy,
                keep_awake: known.details.keep_awake,
            });
        }
        computers
    }

    pub(super) fn devices_changed(&mut self, kind: &str, event: &Value) {
        match kind {
            "devices.hosts" => self.hosts_heard(event),
            "devices.list" => self.devices_listed(),
            _ => {}
        }
    }

    fn hosts_heard(&mut self, event: &Value) {
        let online = event["deviceIds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        let before = self.wakes.online.send_replace(online.clone());
        self.wakes.heard = true;
        for device in before.symmetric_difference(&online) {
            self.wakes.asks.remove(device);
            self.wakes.failed.remove(device);
        }
        self.ask_wakes();
        self.publish_power();
    }

    /// A device that the person removed has no place in the list.
    fn devices_listed(&mut self) {
        let labels = self.device_labels();
        if let Some(user) = &self.scope.user
            && let Some(known) = self.wakes.accounts.get_mut(user)
            && known.keys().any(|device| !labels.contains_key(device))
            && self
                .cached_events
                .get("devices.list")
                .is_some_and(|list| list["localDeviceEnrolled"] == true)
        {
            known.retain(|device, _| labels.contains_key(device));
            self.wakes.save();
        }
        self.publish_power();
    }

    /// Asks each awake computer of the person how it wakes, when it comes online and from time
    /// to time after that. A computer that did not get the result of its test gets it first.
    pub(super) fn ask_wakes(&mut self) {
        let Some(identity) = self.network.identity().filter(|identity| identity.enrolled) else {
            return;
        };
        self.read_own();
        let online = self.wakes.online.borrow().clone();
        let now = Instant::now();
        let mut room = MAX_JOBS.saturating_sub(self.jobs.len());
        for device in online {
            let unreported = self
                .known(&device)
                .and_then(|known| known.unreported.clone());
            let ask = self.wakes.asks.entry(device.clone()).or_default();
            if room == 0
                || device == identity.device_id
                || ask.asking.is_some()
                || ask.next.is_some_and(|next| next > now)
            {
                continue;
            }
            let request_id = Uuid::now_v7().to_string();
            let command = match unreported.clone() {
                Some(WakeTest { hardware, woke }) => Command::RecordWakeTest {
                    request_id,
                    hardware,
                    woke,
                },
                None => Command::DescribeWake { request_id },
            };
            let Ok(request) = serde_json::to_value(command) else {
                continue;
            };
            self.wakes.asked += 1;
            let number = self.wakes.asked;
            ask.asking = Some(number);
            room -= 1;
            let network = self.network.clone();
            self.spawn(async move {
                let result = network
                    .ask_computer(&device, request)
                    .await
                    .map_err(Error::from)
                    .and_then(details_of);
                Completion::WakeDescribed {
                    device,
                    number,
                    reported: unreported,
                    result,
                }
            });
        }
    }

    pub(super) fn wake_described(
        &mut self,
        device: String,
        number: u64,
        reported: Option<&WakeTest>,
        result: Result<WakeDetails>,
    ) {
        let Some(user) = self.scope.user.clone() else {
            return;
        };
        let now = Instant::now();
        let Some(ask) = self
            .wakes
            .asks
            .get_mut(&device)
            .filter(|ask| ask.asking == Some(number))
        else {
            return;
        };
        ask.asking = None;
        match result {
            Ok(details) => {
                ask.next = Some(now + REFRESH);
                let known = self.wakes.accounts.entry(user).or_default();
                let unreported = known
                    .get(&device)
                    .and_then(|known| known.unreported.clone())
                    .filter(|unreported| Some(unreported) != reported);
                let fresh = Known {
                    details,
                    unreported,
                };
                let changed = known.get(&device).is_none_or(|known| {
                    known.details != fresh.details || known.unreported != fresh.unreported
                });
                if changed {
                    known.insert(device, fresh);
                    self.wakes.save();
                    self.publish_power();
                }
            }
            Err(error) => {
                ask.next = Some(now + RETRY);
                tracing::debug!(%error, "a computer did not say how it wakes");
                // The computer did not take a wake as the result of its test: it did not
                // really sleep.
                if let Error::Refused(reason) = error
                    && let Some(reported) = reported.filter(|reported| reported.woke)
                    && let Some(known) = self
                        .wakes
                        .accounts
                        .get_mut(&user)
                        .and_then(|known| known.get_mut(&device))
                    && known.unreported.as_ref() == Some(reported)
                {
                    known.unreported = None;
                    self.wakes.failed.insert(device, reason);
                    self.wakes.save();
                    self.publish_power();
                }
            }
        }
    }

    /// Reads the cards of this device again after each new connection to the server. A device
    /// with no connection can wake nothing, so it reads nothing.
    fn read_own(&mut self) {
        let connects = self.network.inner.link.connects();
        if !self.online
            || self.wakes.reading_own
            || self.wakes.own_read == Some(connects)
            || self.jobs.len() >= MAX_JOBS
        {
            return;
        }
        self.wakes.reading_own = true;
        let network = self.network.clone();
        self.spawn(async move {
            Completion::OwnNetwork {
                own: OwnNetwork::read(&network, |_| false).await,
                connects: Some(connects),
            }
        });
    }

    pub(super) fn own_network_read(&mut self, own: OwnNetwork, connects: Option<u64>) {
        if let Some(connects) = connects {
            self.wakes.reading_own = false;
            self.wakes.own_read = Some(connects);
        }
        self.wakes.own = own;
        self.publish_power();
    }

    pub(super) fn wake(&mut self, command: &Command, device: &str) -> Result<()> {
        let rouse = self.rouse(device, false)?;
        self.result(command);
        self.wakes.busy.insert(device.to_owned(), PowerBusy::Waking);
        self.wakes.failed.remove(device);
        self.publish_power();
        let device = device.to_owned();
        self.spawn(async move {
            let details = rouse.details.clone();
            let roused = rouse.wake(wake::WAKE_WAIT).await;
            Completion::Roused {
                device,
                details,
                roused,
            }
        });
        Ok(())
    }

    pub(super) fn test_wake(&mut self, command: &Command, device: &str) -> Result<()> {
        let rouse = self.rouse(device, true)?;
        self.result(command);
        self.wakes
            .busy
            .insert(device.to_owned(), PowerBusy::Testing);
        self.wakes.failed.remove(device);
        self.publish_power();
        let device = device.to_owned();
        self.spawn(async move {
            let details = rouse.details.clone();
            let tested = rouse.test().await;
            Completion::WakeTested {
                device,
                details,
                tested,
            }
        });
        Ok(())
    }

    fn rouse(&self, device: &str, awake: bool) -> Result<Rouse> {
        self.job_capacity()?;
        let identity = self
            .network
            .identity()
            .filter(|identity| identity.enrolled)
            .ok_or_else(|| Error::Refused(text!("This device is not approved.")))?;
        let computer = self.device_labels().remove(device).ok_or_else(|| {
            Error::Refused(text!("That computer is not an approved device of yours."))
        })?;
        let Some(known) = self.known(device) else {
            return Err(Error::Refused(text!(
                "Kodosi does not know yet how {computer} wakes. Open Kodosi on it one time.",
                computer = computer
            )));
        };
        if self.wakes.busy.contains_key(device) {
            return Err(Error::Busy);
        }
        if self.wakes.is_online(device) != awake {
            return Err(Error::Refused(if awake {
                text!("{computer} is not online.", computer = computer)
            } else {
                text!("{computer} is awake.", computer = computer)
            }));
        }
        if !self.sender_for(device, &known.details) {
            return Err(Error::Refused(nobody_there(&computer)));
        }
        let helpers = self
            .wakes
            .online
            .borrow()
            .iter()
            .filter(|helper| *helper != device && **helper != identity.device_id)
            .cloned()
            .collect();
        Ok(Rouse {
            network: self.network.clone(),
            own_device: identity.device_id,
            sleeper: device.to_owned(),
            details: known.details.clone(),
            helpers,
            online: self.wakes.online.subscribe(),
        })
    }

    /// Another computer took this device's choice of when it stays awake.
    pub(super) fn kept_awake(&mut self, device: &str, keep_awake: KeepAwake) {
        let Some(user) = self.scope.user.clone() else {
            return;
        };
        let Some(known) = self
            .wakes
            .accounts
            .get_mut(&user)
            .and_then(|known| known.get_mut(device))
        else {
            return;
        };
        known.details.keep_awake = keep_awake;
        self.wakes.save();
        self.publish_power();
    }

    pub(super) fn roused(&mut self, device: &str, details: &WakeDetails, roused: &Roused) {
        self.wakes.busy.remove(device);
        let labels = self.device_labels();
        if !roused.woke
            && let Some(computer) = labels.get(device)
        {
            let helper = roused
                .refused
                .as_ref()
                .and_then(|helper| labels.get(helper));
            self.wakes.failed.insert(
                device.to_owned(),
                did_not_wake(roused, details, computer, helper.map(String::as_str)),
            );
        }
        self.publish_power();
    }

    pub(super) fn wake_tested(&mut self, device: &str, details: &WakeDetails, tested: Tested) {
        let roused = match tested {
            Tested::Roused(roused) => roused,
            Tested::Refused(reason) => {
                self.wakes.busy.remove(device);
                self.wakes.failed.insert(device.to_owned(), reason);
                self.publish_power();
                return;
            }
            Tested::StayedAwake => {
                self.wakes.busy.remove(device);
                if let Some(computer) = self.device_labels().get(device) {
                    self.wakes
                        .failed
                        .insert(device.to_owned(), did_not_stay_asleep(computer));
                }
                self.publish_power();
                return;
            }
            Tested::Recorded(fresh) => {
                self.wakes.busy.remove(device);
                if let Some(user) = self.scope.user.clone() {
                    let known = Known {
                        details: fresh,
                        unreported: None,
                    };
                    self.wakes
                        .accounts
                        .entry(user)
                        .or_default()
                        .insert(device.to_owned(), known);
                    self.wakes.save();
                }
                self.publish_power();
                return;
            }
        };
        if let Some(user) = self.scope.user.clone()
            && let Some(hardware) = wake::tested_card(details)
            && let Some(known) = self
                .wakes
                .accounts
                .get_mut(&user)
                .and_then(|known| known.get_mut(device))
        {
            known.unreported = Some(WakeTest {
                hardware: hardware.to_owned(),
                woke: roused.woke,
            });
            self.wakes.save();
        }
        self.roused(device, details, &roused);
        // The computer that woke is online again, and gets the result of its test now.
        self.wakes.asks.remove(device);
        self.ask_wakes();
    }
}

#[cfg(feature = "host")]
impl Runtime {
    /// Answers with what this computer reads about its wake now.
    pub(super) fn describe_wake(&mut self, command: &Command) -> Result<()> {
        self.job_capacity()?;
        let network = self.network.clone();
        let command = command.clone();
        self.spawn(async move {
            Completion::WakeRead {
                command,
                own: OwnNetwork::read(&network, |_| true).await,
            }
        });
        Ok(())
    }

    /// An error answer always means that this computer did not keep the result.
    pub(super) fn record_wake_test(&mut self, command: &Command, test: WakeTest) -> Result<()> {
        self.job_capacity()?;
        if !self.power.record_test(test) {
            return Err(Error::Refused(did_not_stay_asleep(
                &crate::network::host_label(),
            )));
        }
        self.describe_wake(command)
    }

    pub(super) fn wake_read(&mut self, command: &Command, own: OwnNetwork) {
        let details = own.details(self.power.keeps_awake(), self.power.tested().cloned());
        self.emit(result_with(command, "wake", details));
        self.own_network_read(own, None);
    }

    pub(super) fn signal_wake(&mut self, command: &Command, sleeper: &WakeDetails) -> Result<()> {
        self.job_capacity()?;
        let network = self.network.clone();
        let command = command.clone();
        let sleeper = sleeper.clone();
        self.spawn(async move {
            let own = OwnNetwork::read_for(&network, &sleeper).await;
            let signal = tokio::task::spawn_blocking(move || own.signal(&sleeper))
                .await
                .map_err(|error| Error::Text(Text::plain(error.to_string())));
            Completion::WakeSignaled { command, signal }
        });
        Ok(())
    }

    pub(super) fn wake_signaled(
        &self,
        command: &Command,
        signal: Result<crate::protocol::WakeSignal>,
    ) {
        match signal {
            Ok(signal) => self.emit(result_with(command, "signal", signal)),
            Err(error) => self.error(command, &error),
        }
    }
}
