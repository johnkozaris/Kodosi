//! Whether the screen can be seen: the lock and the account in front of the display, from logind
//! and from the desktop's screen saver, whichever says so first. The screen saver also wakes the
//! display.

use std::sync::{Arc, Mutex, PoisonError};

use futures_util::StreamExt as _;
use screen::link::Away;
use tokio::{sync::mpsc, task::JoinHandle};
use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

use super::session::Event;

const LOGIN: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
/// The caller's session, or the user's graphical one when the caller runs outside of a session.
const AUTO_SESSION: &str = "/org/freedesktop/login1/session/auto";
const LOGIN_MANAGER: &str = "org.freedesktop.login1.Manager";
const LOGIN_SESSION: &str = "org.freedesktop.login1.Session";
/// KDE's screen saver, which is its lock, and GNOME's.
const SCREEN_SAVERS: [(&str, &str); 2] = [
    (
        "org.freedesktop.ScreenSaver",
        "/org/freedesktop/ScreenSaver",
    ),
    ("org.gnome.ScreenSaver", "/org/gnome/ScreenSaver"),
];

#[derive(Debug, Default, Clone, Copy)]
struct Seen {
    /// Another session has the display, as when another account is in front of it.
    inactive: bool,
    locked: bool,
    saver: bool,
}

pub(super) struct Watch {
    seen: Arc<Mutex<Seen>>,
    /// The desktop's screen savers that answered.
    savers: Vec<Proxy<'static>>,
    tasks: Vec<JoinHandle<()>>,
}

impl Watch {
    pub(super) async fn start(events: mpsc::UnboundedSender<Event>) -> Self {
        let seen: Arc<Mutex<Seen>> = Arc::default();
        let mut savers = Vec::new();
        let mut tasks = Vec::new();
        if let Some(session) = login_session().await {
            let active = session.get_property::<bool>("Active").await;
            let locked = session.get_property::<bool>("LockedHint").await;
            note(&seen, |seen| {
                seen.inactive = active.is_ok_and(|active| !active);
                seen.locked = locked.unwrap_or_default();
            });
            tasks.push(tokio::spawn(follow_login(
                session,
                Arc::clone(&seen),
                events.clone(),
            )));
        }
        if let Ok(bus) = Connection::session().await {
            for (name, path) in SCREEN_SAVERS {
                let Ok(saver) = Proxy::new(&bus, name, path, name).await else {
                    continue;
                };
                let Ok(active) = saver.call::<_, _, bool>("GetActive", &()).await else {
                    continue;
                };
                note(&seen, |seen| seen.saver |= active);
                tasks.push(tokio::spawn(follow_saver(
                    saver.clone(),
                    Arc::clone(&seen),
                    events.clone(),
                )));
                savers.push(saver);
            }
        }
        Self {
            seen,
            savers,
            tasks,
        }
    }

    /// Turns the display on, as a touch of the computer's mouse would, where the desktop lets
    /// the helper: KDE does.
    pub(super) async fn wake(&self) {
        for saver in &self.savers {
            drop(saver.call_method("SimulateUserActivity", &()).await);
        }
    }

    pub(super) fn away(&self) -> Option<Away> {
        let seen = *self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if seen.inactive {
            Some(Away::AnotherAccount)
        } else if seen.locked || seen.saver {
            Some(Away::Locked)
        } else {
            None
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

fn note(seen: &Mutex<Seen>, change: impl FnOnce(&mut Seen)) {
    change(&mut seen.lock().unwrap_or_else(PoisonError::into_inner));
}

async fn login_session() -> Option<Proxy<'static>> {
    let system = Connection::system().await.ok()?;
    let auto = Proxy::new(&system, LOGIN, AUTO_SESSION, LOGIN_SESSION)
        .await
        .ok()?;
    let id = auto.get_property::<String>("Id").await.ok()?;
    let manager = Proxy::new(&system, LOGIN, LOGIN_PATH, LOGIN_MANAGER)
        .await
        .ok()?;
    let path = manager
        .call::<_, _, OwnedObjectPath>("GetSession", &(id,))
        .await
        .ok()?;
    Proxy::new(&system, LOGIN, path, LOGIN_SESSION).await.ok()
}

async fn follow_login(
    session: Proxy<'static>,
    seen: Arc<Mutex<Seen>>,
    events: mpsc::UnboundedSender<Event>,
) {
    let mut active = session.receive_property_changed::<bool>("Active").await;
    let mut locked = session.receive_property_changed::<bool>("LockedHint").await;
    loop {
        tokio::select! {
            Some(change) = active.next() => {
                if let Ok(active) = change.get().await {
                    note(&seen, |seen| seen.inactive = !active);
                }
            }
            Some(change) = locked.next() => {
                if let Ok(locked) = change.get().await {
                    note(&seen, |seen| seen.locked = locked);
                }
            }
            else => return,
        }
        if events.send(Event::Changed).is_err() {
            return;
        }
    }
}

async fn follow_saver(
    saver: Proxy<'static>,
    seen: Arc<Mutex<Seen>>,
    events: mpsc::UnboundedSender<Event>,
) {
    let Ok(mut changes) = saver.receive_signal("ActiveChanged").await else {
        return;
    };
    while let Some(change) = changes.next().await {
        if let Ok(active) = change.body().deserialize::<bool>() {
            note(&seen, |seen| seen.saver = active);
            if events.send(Event::Changed).is_err() {
                return;
            }
        }
    }
}
