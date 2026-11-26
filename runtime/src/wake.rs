use std::{collections::BTreeSet, io, net::Ipv4Addr, time::Duration};

use futures_util::{StreamExt as _, stream::FuturesUnordered};
use kodosi_os::network::{Hardware, Link};
use tokio::{sync::watch, time::Instant};
use uuid::Uuid;

use crate::{
    Command, EventBody,
    network::BackendClient,
    protocol::{WakeDetails, WakeMedium, WakeSetting, WakeSignal, WakeTest},
    text::Text,
};

/// A broadcast from a phone or over Wi-Fi gets lost easily, and a computer that just fell asleep
/// does not listen yet. The signal goes again until the computer is back.
const BETWEEN_SIGNALS: Duration = Duration::from_secs(10);
pub(crate) const WAKE_WAIT: Duration = Duration::from_secs(90);
const TEST_SLEEP_WAIT: Duration = Duration::from_mins(1);
/// A computer leaves the network before its card listens for the wake signal.
const TEST_SETTLE: Duration = Duration::from_secs(20);
/// A computer that hibernated takes longer to come back than one that slept.
const TEST_WAKE_WAIT: Duration = Duration::from_mins(2);

pub(crate) fn hardware(text: &str) -> Option<Hardware> {
    let mut hardware = [0; 6];
    let mut parts = text.split(':');
    for byte in &mut hardware {
        let part = parts.next().filter(|part| part.len() == 2)?;
        *byte = u8::from_str_radix(part, 16).ok()?;
    }
    parts.next().is_none().then_some(hardware)
}

#[cfg(feature = "host")]
fn hardware_text(hardware: Hardware) -> String {
    hardware.map(|byte| format!("{byte:02x}")).join(":")
}

/// Where a card is. Two cards at the same place hear the broadcasts of each other.
#[derive(Clone, Copy)]
struct Place {
    network: u32,
    prefix: u8,
    router: Option<Hardware>,
    public: Option<Ipv4Addr>,
}

impl Place {
    fn of(
        address: Ipv4Addr,
        prefix: u8,
        router: Option<Hardware>,
        public: Option<Ipv4Addr>,
    ) -> Self {
        let mask = u32::MAX
            .checked_shl(32 - u32::from(prefix))
            .unwrap_or_default();
        Self {
            network: address.to_bits() & mask,
            prefix,
            router,
            public,
        }
    }

    /// Many homes use the same addresses. The router, or else the public address, tells two such
    /// networks apart when both cards know it.
    fn reaches(&self, other: &Self) -> bool {
        self.prefix == other.prefix
            && self.network == other.network
            && match (self.router, other.router) {
                (Some(own), Some(other)) => own == other,
                _ => match (self.public, other.public) {
                    (Some(own), Some(other)) => own == other,
                    _ => true,
                },
            }
    }
}

fn places(details: &WakeDetails) -> Vec<(Hardware, Place)> {
    let public = details
        .public
        .as_deref()
        .and_then(|public| public.parse().ok());
    details
        .links
        .iter()
        .filter_map(|link| {
            let router = link.router.as_deref().and_then(hardware);
            let place = Place::of(link.address.parse().ok()?, link.prefix, router, public);
            Some((hardware(&link.hardware)?, place))
        })
        .collect()
}

/// Whether a computer with these details is on the network of the sleeping computer.
pub(crate) fn helps(helper: &WakeDetails, sleeper: &WakeDetails) -> bool {
    let there = places(sleeper);
    places(helper)
        .iter()
        .any(|(_, here)| there.iter().any(|(_, place)| here.reaches(place)))
}

/// The cards of this device, read now.
#[derive(Clone, Default)]
pub(crate) struct OwnNetwork {
    cards: Vec<Link>,
    public: Option<Ipv4Addr>,
    #[cfg(feature = "host")]
    fast_startup: bool,
}

impl OwnNetwork {
    /// The public address costs a question to a server on the internet, so `public` says whether
    /// these cards need it.
    pub(crate) async fn read(
        network: &BackendClient,
        public: impl FnOnce(&[Link]) -> bool,
    ) -> Self {
        let read = tokio::task::spawn_blocking(|| Self {
            cards: kodosi_os::network::links(),
            public: None,
            #[cfg(feature = "host")]
            fast_startup: kodosi_os::network::fast_startup(),
        });
        let mut own = read.await.unwrap_or_default();
        if public(&own.cards) {
            own.public = network.public_ipv4().await;
        }
        own
    }

    pub(crate) async fn read_for(network: &BackendClient, sleeper: &WakeDetails) -> Self {
        Self::read(network, |cards| {
            sleeper.public.is_some()
                && (cards.iter().any(|card| card.router.is_none())
                    || sleeper.links.iter().any(|link| link.router.is_none()))
        })
        .await
    }

    fn place(&self, card: &Link) -> Place {
        Place::of(card.address, card.prefix, card.router, self.public)
    }

    pub(crate) fn reaches(&self, sleeper: &WakeDetails) -> bool {
        let there = places(sleeper);
        self.cards.iter().any(|card| {
            let here = self.place(card);
            there.iter().any(|(_, place)| here.reaches(place))
        })
    }

    #[cfg(feature = "host")]
    pub(crate) fn details(
        &self,
        keep_awake: crate::protocol::KeepAwake,
        tested: Option<WakeTest>,
    ) -> WakeDetails {
        use kodosi_os::network::{Medium, Wakes};

        use crate::protocol::{ComputerSystem, WAKE_LINKS_MOST, WakeLink};

        WakeDetails {
            system: if cfg!(target_os = "macos") {
                ComputerSystem::Mac
            } else if cfg!(windows) {
                ComputerSystem::Windows
            } else {
                ComputerSystem::Linux
            },
            links: self
                .cards
                .iter()
                .filter_map(|card| {
                    Some(WakeLink {
                        hardware: hardware_text(card.hardware?),
                        medium: match card.medium {
                            Medium::Cable => WakeMedium::Cable,
                            Medium::Radio => WakeMedium::Radio,
                        },
                        address: card.address.to_string(),
                        prefix: card.prefix,
                        router: card.router.map(hardware_text),
                        wakes: match card.wakes {
                            Wakes::On => WakeSetting::On,
                            Wakes::Off => WakeSetting::Off,
                            Wakes::Unknown => WakeSetting::Unknown,
                        },
                    })
                })
                .take(WAKE_LINKS_MOST)
                .collect(),
            public: self.public.map(|public| public.to_string()),
            fast_startup: self.fast_startup,
            keep_awake,
            tested,
        }
    }

    /// Sends the wake signal out of each card that is on the network of the sleeping computer.
    pub(crate) fn signal(&self, sleeper: &WakeDetails) -> WakeSignal {
        let there = places(sleeper);
        let mut outcome = WakeSignal::Elsewhere;
        for card in &self.cards {
            let here = self.place(card);
            let sleepers = there
                .iter()
                .filter(|(_, place)| here.reaches(place))
                .map(|(hardware, _)| *hardware)
                .collect::<Vec<_>>();
            if sleepers.is_empty() {
                continue;
            }
            match kodosi_os::network::wake(card, &sleepers) {
                Ok(()) => outcome = WakeSignal::Sent,
                Err(error) => {
                    tracing::warn!(%error, "the wake signal did not leave this device");
                    // macOS answers this to an app that the person did not allow on the local
                    // network.
                    if cfg!(target_os = "macos")
                        && error.kind() == io::ErrorKind::HostUnreachable
                        && outcome == WakeSignal::Elsewhere
                    {
                        outcome = WakeSignal::Refused;
                    }
                }
            }
        }
        outcome
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WakeAbility {
    /// A test woke it.
    Tested,
    /// Its settings allow a wake, and no test ran.
    Ready,
    /// A test did not wake it.
    Failed,
    /// A setting of its operating system must change first.
    Fix,
    /// It is only on Wi-Fi.
    Radio,
    Unknown,
}

/// A test counts for as long as the computer still has the card that the test woke it through.
pub(crate) fn ability(details: &WakeDetails, unreported: Option<&WakeTest>) -> WakeAbility {
    let tested = unreported.or(details.tested.as_ref()).filter(|test| {
        details
            .links
            .iter()
            .any(|link| link.hardware == test.hardware)
    });
    if let Some(test) = tested {
        return if test.woke {
            WakeAbility::Tested
        } else {
            WakeAbility::Failed
        };
    }
    let mut cables = details
        .links
        .iter()
        .filter(|link| link.medium == WakeMedium::Cable)
        .peekable();
    if cables.peek().is_none() {
        return if details.links.is_empty() {
            WakeAbility::Unknown
        } else {
            WakeAbility::Radio
        };
    }
    if cables.any(|link| link.wakes != WakeSetting::Off) {
        WakeAbility::Ready
    } else {
        WakeAbility::Fix
    }
}

/// The card that a test names: the one that the wake signal most probably used.
pub(crate) fn tested_card(details: &WakeDetails) -> Option<&str> {
    details
        .links
        .iter()
        .find(|link| link.medium == WakeMedium::Cable)
        .or_else(|| details.links.first())
        .map(|link| link.hardware.as_str())
}

#[derive(Default)]
pub(crate) struct Roused {
    pub(crate) woke: bool,
    pub(crate) signaled: bool,
    /// A Mac of the person whose system did not let Kodosi send on the local network.
    pub(crate) refused: Option<String>,
}

pub(crate) enum Tested {
    /// The computer did not take the request to sleep, or says that it did not really sleep.
    Refused(Text),
    StayedAwake,
    /// The computer woke and has the result.
    Recorded(WakeDetails),
    /// The computer does not have the result yet.
    Roused(Roused),
}

pub(crate) struct Rouse {
    pub(crate) network: BackendClient,
    pub(crate) own_device: String,
    pub(crate) sleeper: String,
    pub(crate) details: WakeDetails,
    /// The awake computers of the person, which can be on the network of the sleeping one.
    pub(crate) helpers: Vec<String>,
    pub(crate) online: watch::Receiver<BTreeSet<String>>,
}

impl Rouse {
    pub(crate) async fn wake(self, wait: Duration) -> Roused {
        let Self {
            network,
            own_device,
            sleeper,
            details,
            mut helpers,
            mut online,
        } = self;
        let until = Instant::now() + wait;
        let mut roused = Roused::default();
        loop {
            let own = OwnNetwork::read_for(&network, &details).await;
            let here = {
                let details = details.clone();
                tokio::task::spawn_blocking(move || own.signal(&details)).await
            };
            match here {
                Ok(WakeSignal::Sent) => roused.signaled = true,
                Ok(WakeSignal::Refused) => {
                    roused.refused.get_or_insert_with(|| own_device.clone());
                }
                Ok(WakeSignal::Elsewhere) | Err(_) => {}
            }
            let mut asks = helpers
                .iter()
                .map(|helper| ask_signal(network.clone(), helper.clone(), details.clone()))
                .collect::<FuturesUnordered<_>>();
            let mut elsewhere = BTreeSet::new();
            let round = tokio::time::sleep(BETWEEN_SIGNALS);
            tokio::pin!(round);
            loop {
                tokio::select! {
                    awake = online.wait_for(|online| online.contains(&sleeper)) => {
                        roused.woke = awake.is_ok();
                        return roused;
                    }
                    Some((helper, signal)) = asks.next() => match signal {
                        Some(WakeSignal::Sent) => roused.signaled = true,
                        Some(WakeSignal::Refused) => {
                            roused.refused.get_or_insert(helper);
                        }
                        Some(WakeSignal::Elsewhere) => {
                            elsewhere.insert(helper);
                        }
                        None => {}
                    },
                    () = &mut round => break,
                    () = tokio::time::sleep_until(until) => return roused,
                }
            }
            drop(asks);
            helpers.retain(|helper| !elsewhere.contains(helper));
        }
    }

    /// Puts the computer to sleep through its own Sleep, waits until it is gone, and wakes it.
    pub(crate) async fn test(mut self) -> Tested {
        let asleep = Command::Sleep {
            request_id: Uuid::now_v7().to_string(),
        };
        let answer = match serde_json::to_value(asleep) {
            Ok(request) => self.network.ask_computer(&self.sleeper, request).await,
            Err(error) => return Tested::Refused(Text::plain(error.to_string())),
        };
        match answer.map(serde_json::from_value::<EventBody>) {
            Ok(Ok(EventBody::PowerResult { .. })) => {}
            Ok(Ok(EventBody::PowerError { message, .. })) => return Tested::Refused(message),
            Ok(_) => return Tested::Refused(crate::protocol::other_version()),
            Err(error) => return Tested::Refused(error.text()),
        }
        let gone = self
            .online
            .wait_for(|online| !online.contains(&self.sleeper));
        if !matches!(tokio::time::timeout(TEST_SLEEP_WAIT, gone).await, Ok(Ok(_))) {
            return Tested::StayedAwake;
        }
        tokio::time::sleep(TEST_SETTLE).await;
        // A computer that is back before the first signal did not stay asleep.
        if self.online.borrow().contains(&self.sleeper) {
            return Tested::StayedAwake;
        }
        let network = self.network.clone();
        let sleeper = self.sleeper.clone();
        let card = tested_card(&self.details).map(str::to_owned);
        let roused = self.wake(TEST_WAKE_WAIT).await;
        // A computer that came back with no signal sent woke by itself.
        let Some(hardware) = card.filter(|_| roused.woke && roused.signaled) else {
            return Tested::Roused(roused);
        };
        // Only the computer knows if it really slept: one that stayed awake also comes back.
        let record = serde_json::to_value(Command::RecordWakeTest {
            request_id: Uuid::now_v7().to_string(),
            hardware,
            woke: true,
        });
        let answer = match record {
            Ok(request) => network.ask_computer(&sleeper, request).await.ok(),
            Err(_) => None,
        };
        match answer.map(serde_json::from_value::<EventBody>) {
            Some(Ok(EventBody::PowerResult {
                wake: Some(details),
                ..
            })) if details.validate().is_ok() => Tested::Recorded(details),
            Some(Ok(EventBody::PowerError { message, .. })) => Tested::Refused(message),
            _ => Tested::Roused(roused),
        }
    }
}

async fn ask_signal(
    network: BackendClient,
    helper: String,
    sleeper: WakeDetails,
) -> (String, Option<WakeSignal>) {
    let request = serde_json::to_value(Command::SignalWake {
        request_id: Uuid::now_v7().to_string(),
        sleeper,
    });
    let answer = match request {
        Ok(request) => network.ask_computer(&helper, request).await.ok(),
        Err(_) => None,
    };
    let signal = match answer.map(serde_json::from_value::<EventBody>) {
        Some(Ok(EventBody::PowerResult { signal, .. })) => signal,
        _ => None,
    };
    (helper, signal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ComputerSystem, WakeLink};

    fn link(hardware: &str, medium: WakeMedium, address: &str, router: Option<&str>) -> WakeLink {
        WakeLink {
            hardware: hardware.to_owned(),
            medium,
            address: address.to_owned(),
            prefix: 24,
            router: router.map(str::to_owned),
            wakes: WakeSetting::On,
        }
    }

    fn computer(links: Vec<WakeLink>, public: Option<&str>) -> WakeDetails {
        WakeDetails {
            system: ComputerSystem::Linux,
            links,
            public: public.map(str::to_owned),
            fast_startup: false,
            keep_awake: crate::protocol::KeepAwake::default(),
            tested: None,
        }
    }

    const HOME: Option<&str> = Some("02:00:5e:00:00:01");
    const OFFICE: Option<&str> = Some("02:00:5e:00:00:02");

    #[test]
    fn a_computer_helps_only_on_the_network_of_the_sleeping_computer() {
        let cable = WakeMedium::Cable;
        let sleeper = computer(
            vec![link("02:00:5e:10:20:30", cable, "192.168.1.20", HOME)],
            Some("203.0.113.7"),
        );
        let at_home = computer(
            vec![link("02:00:5e:10:20:31", cable, "192.168.1.5", HOME)],
            Some("203.0.113.7"),
        );
        // An office with the same addresses as the home is another network.
        let at_the_office = computer(
            vec![link("02:00:5e:10:20:32", cable, "192.168.1.5", OFFICE)],
            Some("198.51.100.9"),
        );
        let on_another_net = computer(
            vec![link("02:00:5e:10:20:33", cable, "192.168.2.5", HOME)],
            Some("203.0.113.7"),
        );
        assert!(helps(&at_home, &sleeper));
        assert!(!helps(&at_the_office, &sleeper));
        assert!(!helps(&on_another_net, &sleeper));

        // A phone knows no router. The public address tells its network then.
        let phone_at_home = computer(
            vec![link(
                "02:00:5e:10:20:34",
                WakeMedium::Radio,
                "192.168.1.9",
                None,
            )],
            Some("203.0.113.7"),
        );
        let phone_in_a_cafe = computer(
            vec![link(
                "02:00:5e:10:20:34",
                WakeMedium::Radio,
                "192.168.1.9",
                None,
            )],
            Some("198.51.100.9"),
        );
        let phone_that_knows_nothing = computer(
            vec![link(
                "02:00:5e:10:20:34",
                WakeMedium::Radio,
                "192.168.1.9",
                None,
            )],
            None,
        );
        assert!(helps(&phone_at_home, &sleeper));
        assert!(!helps(&phone_in_a_cafe, &sleeper));
        assert!(helps(&phone_that_knows_nothing, &sleeper));
    }

    #[test]
    fn a_test_counts_while_the_computer_has_the_tested_card() {
        let cable = link("02:00:5e:10:20:30", WakeMedium::Cable, "192.168.1.20", HOME);
        let radio = link("02:00:5e:10:20:40", WakeMedium::Radio, "192.168.1.21", HOME);
        let test = |woke| WakeTest {
            hardware: "02:00:5e:10:20:30".to_owned(),
            woke,
        };

        let mut details = computer(vec![cable.clone(), radio.clone()], None);
        assert_eq!(ability(&details, None), WakeAbility::Ready);
        details.tested = Some(test(true));
        assert_eq!(ability(&details, None), WakeAbility::Tested);
        // A newer result that the computer did not get yet counts first.
        assert_eq!(ability(&details, Some(&test(false))), WakeAbility::Failed);

        // The cable is out: the test of the cable card says nothing about Wi-Fi.
        details.links = vec![radio];
        assert_eq!(ability(&details, None), WakeAbility::Radio);

        let mut off = cable;
        off.wakes = WakeSetting::Off;
        assert_eq!(ability(&computer(vec![off], None), None), WakeAbility::Fix);
        assert_eq!(
            ability(&computer(Vec::new(), None), None),
            WakeAbility::Unknown
        );
    }

    #[test]
    fn a_hardware_address_has_six_bytes_of_two_digits() {
        assert_eq!(
            hardware("02:00:5e:10:20:30"),
            Some([0x02, 0x00, 0x5e, 0x10, 0x20, 0x30])
        );
        assert_eq!(hardware("2:0:5e:10:20:30"), None);
        assert_eq!(hardware("02:00:5e:10:20"), None);
        assert_eq!(hardware("02:00:5e:10:20:30:40"), None);
        assert_eq!(hardware("02:00:5e:10:20:zz"), None);
    }
}
