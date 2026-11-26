use std::{io, path::Path, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

use super::{Completion, Runtime, saver::Saver};
use crate::{
    Command, Error, Result,
    network::Moment,
    protocol::{KeepAwake, SessionEntry, SessionKind},
};

/// An agent that stops often asks its person a question, or gets its next task at once. The
/// computer stays awake for this time, so that the answer from another device still arrives.
const AFTER_WORK: Duration = Duration::from_mins(5);
const REASON: &str = "Kodosi keeps this computer awake";
/// The answer to the device that asked must leave before the connection closes.
const ANSWER_LEAVES: Duration = Duration::from_secs(1);
/// A computer that is still awake after this time did not go to sleep.
const SLEEP_STARTS: Duration = Duration::from_mins(1);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wanted {
    Awake,
    Idle,
    Off,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    keep_awake: KeepAwake,
}

pub(super) struct Power {
    keep_awake: KeepAwake,
    agents_work: bool,
    wanted: watch::Sender<Wanted>,
    saver: Saver,
}

impl Power {
    pub(super) fn load(data_root: &Path) -> Self {
        Self::with(data_root, || {
            kodosi_os::power::Awake::hold(REASON)
                .inspect_err(|error| {
                    tracing::warn!(%error, "this computer can go to sleep by itself");
                })
                .ok()
        })
    }

    #[cfg(test)]
    pub(super) fn unheld(data_root: &Path) -> Self {
        Self::with(data_root, || None::<()>)
    }

    fn with<Held: Send + 'static>(
        data_root: &Path,
        hold: impl Fn() -> Option<Held> + Send + Sync + 'static,
    ) -> Self {
        let file = data_root.join("power.json");
        let keep_awake = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .map_or_else(KeepAwake::default, |saved| saved.keep_awake);
        let (wanted, changes) = watch::channel(Wanted::Off);
        tokio::spawn(keep(changes, std::sync::Arc::new(hold)));
        let power = Self {
            keep_awake,
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
        match serde_json::to_vec(&Saved { keep_awake }) {
            Ok(bytes) => self.saver.save(bytes),
            Err(error) => tracing::warn!(%error, "could not save the power setting"),
        }
        self.want();
        true
    }

    pub(super) fn agents_work(&mut self, working: bool) {
        self.agents_work = working;
        self.want();
    }

    fn want(&self) {
        let wanted = match self.keep_awake {
            KeepAwake::Always => Wanted::Awake,
            KeepAwake::Agents if self.agents_work => Wanted::Awake,
            KeepAwake::Agents => Wanted::Idle,
            KeepAwake::Never => Wanted::Off,
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
    pub(super) fn power_event(&self) -> Value {
        json!({"type":"power.state", "keepAwake":self.power.keeps_awake()})
    }

    pub(super) fn follow_agents(&mut self, entries: &[SessionEntry]) {
        self.power.agents_work(entries.iter().any(agent_works));
    }

    pub(super) fn keep_awake(&mut self, command: &Command, keep_awake: KeepAwake) {
        if self.power.keep_awake(keep_awake) {
            self.emit(self.power_event());
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
            if matches!(result, Ok(kodosi_os::power::Sleep::Starts)) {
                let until = tokio::time::Instant::now() + SLEEP_STARTS;
                while tokio::time::Instant::now() < until && !before.slept_until(Moment::now()) {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
            Completion::Slept {
                result: result.map(drop),
            }
        });
    }

    pub(super) fn slept(&mut self, result: &io::Result<()>) {
        if let Err(error) = result {
            tracing::warn!(%error, "this computer did not go to sleep");
        }
        self.network.resume();
        self.retry_views_now();
    }
}

async fn keep<Held: Send + 'static>(
    mut wanted: watch::Receiver<Wanted>,
    hold: std::sync::Arc<dyn Fn() -> Option<Held> + Send + Sync>,
) {
    let mut awake = None;
    loop {
        let now = *wanted.borrow_and_update();
        match now {
            Wanted::Awake if awake.is_none() => {
                let hold = std::sync::Arc::clone(&hold);
                awake = tokio::task::spawn_blocking(move || hold())
                    .await
                    .ok()
                    .flatten();
            }
            Wanted::Idle if awake.is_some() => {
                tokio::select! {
                    changed = wanted.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        continue;
                    }
                    () = tokio::time::sleep(AFTER_WORK) => release(&mut awake).await,
                }
            }
            Wanted::Off => release(&mut awake).await,
            Wanted::Awake | Wanted::Idle => {}
        }
        if wanted.changed().await.is_err() {
            break;
        }
    }
    release(&mut awake).await;
}

/// Linux releases with calls on the message bus, which can block.
async fn release<Held: Send + 'static>(awake: &mut Option<Held>) {
    if let Some(awake) = awake.take() {
        drop(tokio::task::spawn_blocking(move || drop(awake)).await);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

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
        let holds = Arc::clone(holds);
        Power::with(folder, move || {
            holds.fetch_add(1, Ordering::SeqCst);
            Some(Held(Arc::clone(&holds)))
        })
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
}
