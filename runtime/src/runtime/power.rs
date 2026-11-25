use std::{io, path::Path, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{sync::watch, time::Instant};

use super::{Completion, Runtime, saver::Saver};
use crate::{
    Command, Error, Result,
    network::Moment,
    protocol::{KeepAwake, SessionEntry, SessionKind, WakeTest},
};

/// An agent that stops often asks its person a question, or gets its next task at once. The
/// computer stays awake for this time, so that the answer from another device still arrives.
const AFTER_WORK: Duration = Duration::from_mins(5);
const REASON: &str = "Kodosi keeps this computer awake";
/// The answer to the device that asked must leave before the connection closes.
const ANSWER_LEAVES: Duration = Duration::from_secs(1);
/// A computer that is still awake after this time did not go to sleep.
const SLEEP_STARTS: Duration = Duration::from_mins(1);
/// How often a computer set to stay awake always reads whether it is plugged in.
const POWER_CHECK: Duration = Duration::from_mins(1);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wanted {
    Awake,
    /// Awake while the computer is plugged in. On its battery, as `Idle`.
    Plugged,
    Idle,
    Off,
}

type Hold<Held> = Arc<dyn Fn() -> Option<Held> + Send + Sync>;
type OnBattery = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    keep_awake: KeepAwake,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tested: Option<WakeTest>,
}

pub(super) struct Power {
    keep_awake: KeepAwake,
    /// Whether a test from another device woke this computer through one of its cards.
    tested: Option<WakeTest>,
    /// Whether the last sleep that another device asked for was a real sleep.
    slept: bool,
    agents_work: bool,
    wanted: watch::Sender<Wanted>,
    saver: Saver,
}

impl Power {
    pub(super) fn load(data_root: &Path) -> Self {
        Self::with(
            data_root,
            || {
                kodosi_os::power::Awake::hold(REASON)
                    .inspect_err(|error| {
                        tracing::warn!(%error, "this computer can go to sleep by itself");
                    })
                    .ok()
            },
            kodosi_os::power::on_battery,
        )
    }

    #[cfg(test)]
    pub(super) fn unheld(data_root: &Path) -> Self {
        Self::with(data_root, || None::<()>, || false)
    }

    fn with<Held: Send + 'static>(
        data_root: &Path,
        hold: impl Fn() -> Option<Held> + Send + Sync + 'static,
        on_battery: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        let file = data_root.join("power.json");
        let saved = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .unwrap_or_default();
        let (wanted, changes) = watch::channel(Wanted::Off);
        tokio::spawn(keep(changes, Arc::new(hold), Arc::new(on_battery)));
        let power = Self {
            keep_awake: saved.keep_awake,
            tested: saved.tested,
            slept: false,
            agents_work: false,
            wanted,
            saver: Saver::file(file, "the power setting"),
        };
        power.want();
        power
    }

    pub(super) const fn keeps_awake(&self) -> KeepAwake {
        self.keep_awake
    }

    pub(super) fn keep_awake(&mut self, keep_awake: KeepAwake) -> bool {
        if self.keep_awake == keep_awake {
            return false;
        }
        self.keep_awake = keep_awake;
        self.save();
        self.want();
        true
    }

    pub(super) const fn tested(&self) -> Option<&WakeTest> {
        self.tested.as_ref()
    }

    /// A computer that did not sleep also comes back, so a wake counts only after a real sleep.
    pub(super) fn record_test(&mut self, test: WakeTest) -> bool {
        if test.woke && !self.slept {
            return false;
        }
        self.tested = Some(test);
        self.save();
        true
    }

    fn save(&self) {
        let saved = Saved {
            keep_awake: self.keep_awake,
            tested: self.tested.clone(),
        };
        match serde_json::to_vec(&saved) {
            Ok(bytes) => self.saver.save(bytes),
            Err(error) => tracing::warn!(%error, "could not save the power setting"),
        }
    }

    pub(super) fn agents_work(&mut self, working: bool) {
        self.agents_work = working;
        self.want();
    }

    fn want(&self) {
        let wanted = match self.keep_awake {
            KeepAwake::Never => Wanted::Off,
            _ if self.agents_work => Wanted::Awake,
            KeepAwake::Always => Wanted::Plugged,
            KeepAwake::Agents => Wanted::Idle,
        };
        self.wanted
            .send_if_modified(|now| std::mem::replace(now, wanted) != wanted);
    }

    pub(super) async fn saved(&self) {
        self.saver.saved().await;
    }
}

fn agent_works(entry: &SessionEntry) -> bool {
    entry.kind == SessionKind::Local
        && entry.program.is_some()
        && (entry.working || entry.helpers > 0)
}

impl Runtime {
    pub(super) fn follow_agents(&mut self, entries: &[SessionEntry]) {
        self.power.agents_work(entries.iter().any(agent_works));
    }

    pub(super) fn keep_awake(&mut self, command: &Command, keep_awake: KeepAwake) {
        if self.power.keep_awake(keep_awake) {
            self.publish_power();
        }
        self.result(command);
    }

    pub(super) fn sleep(&mut self, command: &Command) -> Result<()> {
        self.job_capacity()?;
        let command = command.clone();
        self.spawn(async move {
            let result = tokio::task::spawn_blocking(kodosi_os::power::can_sleep)
                .await
                .map_err(io::Error::other)
                .flatten();
            Completion::SleepChecked { command, result }
        });
        Ok(())
    }

    /// The connection to the server closes before the sleep, so that the person's other devices
    /// see at once that this computer is gone, and opens again when the computer is awake.
    pub(super) fn sleep_checked(&mut self, command: &Command, result: io::Result<()>) {
        if let Err(error) = result {
            tracing::warn!(%error, "this computer cannot be put to sleep");
            self.error(
                command,
                &Error::Refused(text!(
                    "{computer} does not let Kodosi put it to sleep.",
                    computer = crate::network::host_label()
                )),
            );
            return;
        }
        self.result(command);
        let network = self.network.clone();
        self.spawn(async move {
            tokio::time::sleep(ANSWER_LEAVES).await;
            network.pause().await;
            let before = Moment::now();
            let result = tokio::task::spawn_blocking(kodosi_os::power::sleep)
                .await
                .map_err(io::Error::other)
                .flatten();
            let slept = match &result {
                Ok(kodosi_os::power::Sleep::Starts) => {
                    let until = Instant::now() + SLEEP_STARTS;
                    while Instant::now() < until && !before.slept_until(Moment::now()) {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    before.slept_until(Moment::now())
                }
                Ok(kodosi_os::power::Sleep::Ended) => before.slept_through(Moment::now()),
                Err(_) => false,
            };
            Completion::Slept {
                result: result.map(drop),
                slept,
            }
        });
    }

    pub(super) fn slept(&mut self, result: &io::Result<()>, slept: bool) {
        if let Err(error) = result {
            tracing::warn!(%error, "this computer did not go to sleep");
        }
        self.power.slept = slept;
        self.network.resume();
        self.retry_views_now();
    }
}

async fn keep<Held: Send + 'static>(
    mut wanted: watch::Receiver<Wanted>,
    hold: Hold<Held>,
    on_battery: OnBattery,
) {
    let mut awake = None;
    // When the hold ends, once agents stopped or the computer was unplugged.
    let mut until = None;
    loop {
        let now = *wanted.borrow_and_update();
        let keeps = match now {
            Wanted::Awake => true,
            Wanted::Plugged => !runs_on_battery(&on_battery).await,
            Wanted::Idle | Wanted::Off => false,
        };
        if keeps {
            until = None;
            if awake.is_none() {
                let hold = Arc::clone(&hold);
                awake = tokio::task::spawn_blocking(move || hold())
                    .await
                    .ok()
                    .flatten();
            }
        } else if now == Wanted::Off {
            until = None;
            release(&mut awake).await;
        } else if awake.is_some() && until.is_none() {
            until = Some(Instant::now() + AFTER_WORK);
        }
        if until.is_some_and(|at| at <= Instant::now()) {
            until = None;
            release(&mut awake).await;
        }
        let check = (now == Wanted::Plugged).then(|| Instant::now() + POWER_CHECK);
        let next = until.into_iter().chain(check).min();
        tokio::select! {
            changed = wanted.changed() => {
                if changed.is_err() {
                    break;
                }
            }
            () = wait_until(next) => {}
        }
    }
    release(&mut awake).await;
}

async fn runs_on_battery(on_battery: &OnBattery) -> bool {
    let on_battery = Arc::clone(on_battery);
    tokio::task::spawn_blocking(move || on_battery())
        .await
        .unwrap_or(false)
}

async fn wait_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Linux releases with calls on the message bus, which can block.
async fn release<Held: Send + 'static>(awake: &mut Option<Held>) {
    if let Some(awake) = awake.take() {
        drop(tokio::task::spawn_blocking(move || drop(awake)).await);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    struct Held(Arc<AtomicUsize>);

    impl Drop for Held {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    async fn settle() {
        for _ in 0..20 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }

    fn power(folder: &Path, holds: &Arc<AtomicUsize>) -> Power {
        with_battery(folder, holds, &Arc::new(AtomicBool::new(false)))
    }

    fn with_battery(folder: &Path, holds: &Arc<AtomicUsize>, battery: &Arc<AtomicBool>) -> Power {
        let holds = Arc::clone(holds);
        let battery = Arc::clone(battery);
        Power::with(
            folder,
            move || {
                holds.fetch_add(1, Ordering::SeqCst);
                Some(Held(Arc::clone(&holds)))
            },
            move || battery.load(Ordering::SeqCst),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn the_computer_stays_awake_while_an_agent_works_and_for_a_time_after() {
        let folder = tempfile::tempdir().unwrap();
        let holds = Arc::new(AtomicUsize::new(0));
        let mut power = power(folder.path(), &holds);
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 0);

        power.agents_work(true);
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 1);

        power.agents_work(false);
        tokio::time::sleep(AFTER_WORK.mul_f32(0.75)).await;
        assert_eq!(holds.load(Ordering::SeqCst), 1);
        power.agents_work(true);
        settle().await;
        power.agents_work(false);
        tokio::time::sleep(AFTER_WORK.mul_f32(0.75)).await;
        assert_eq!(holds.load(Ordering::SeqCst), 1);

        tokio::time::sleep(AFTER_WORK.mul_f32(0.5)).await;
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_setting_takes_effect_at_once_and_is_kept() {
        let folder = tempfile::tempdir().unwrap();
        let holds = Arc::new(AtomicUsize::new(0));
        let mut power = power(folder.path(), &holds);
        power.agents_work(true);
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 1);

        assert!(power.keep_awake(KeepAwake::Never));
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 0);

        assert!(power.keep_awake(KeepAwake::Always));
        power.agents_work(false);
        tokio::time::sleep(AFTER_WORK.mul_f32(2.0)).await;
        assert_eq!(
            holds.load(Ordering::SeqCst),
            1,
            "a computer set to stay awake always stays awake without agents"
        );
        power.saved().await;
        drop(power);
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 0);

        let again = self::power(folder.path(), &holds);
        assert_eq!(again.keeps_awake(), KeepAwake::Always);
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn always_keeps_the_computer_awake_only_while_it_is_plugged_in() {
        let folder = tempfile::tempdir().unwrap();
        let holds = Arc::new(AtomicUsize::new(0));
        let battery = Arc::new(AtomicBool::new(true));
        let mut power = with_battery(folder.path(), &holds, &battery);
        assert!(power.keep_awake(KeepAwake::Always));
        settle().await;
        assert_eq!(
            holds.load(Ordering::SeqCst),
            0,
            "on its battery, it can sleep"
        );

        power.agents_work(true);
        settle().await;
        assert_eq!(
            holds.load(Ordering::SeqCst),
            1,
            "a working agent keeps it awake"
        );
        power.agents_work(false);
        tokio::time::sleep(AFTER_WORK + POWER_CHECK).await;
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 0);

        battery.store(false, Ordering::SeqCst);
        tokio::time::sleep(POWER_CHECK).await;
        settle().await;
        assert_eq!(
            holds.load(Ordering::SeqCst),
            1,
            "plugged in, it stays awake"
        );

        battery.store(true, Ordering::SeqCst);
        tokio::time::sleep(POWER_CHECK.mul_f32(0.5)).await;
        settle().await;
        assert_eq!(
            holds.load(Ordering::SeqCst),
            1,
            "it does not sleep at once when unplugged"
        );
        tokio::time::sleep(POWER_CHECK + AFTER_WORK).await;
        settle().await;
        assert_eq!(holds.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_wake_counts_only_after_a_real_sleep() {
        let folder = tempfile::tempdir().unwrap();
        let mut power = power(folder.path(), &Arc::new(AtomicUsize::new(0)));
        let test = |woke| WakeTest {
            hardware: "02:00:5e:10:20:30".to_owned(),
            woke,
        };

        assert!(
            !power.record_test(test(true)),
            "a computer that stayed awake also comes back"
        );
        assert!(power.tested().is_none());
        assert!(power.record_test(test(false)));

        power.slept = true;
        assert!(power.record_test(test(true)));
        assert_eq!(power.tested().map(|test| test.woke), Some(true));
    }
}
