//! The desktop's portal, `xdg-desktop-portal`: a remote desktop session that shares every display
//! and takes the pointer and the keyboard, with the clipboard where the desktop offers it. The
//! desktop asks the person at the computer once; after that, the token that it gives back with
//! each session, or KDE's pre-authorization, starts the next session without asking.

use std::{
    collections::HashMap,
    os::{fd::OwnedFd, unix::fs::OpenOptionsExt as _},
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
    time::Duration,
};

use futures_util::StreamExt as _;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::time::Instant;
use zbus::{
    Connection, MatchRule, MessageStream,
    message::Type as MessageType,
    names::BusName,
    zvariant::{self, DeserializeDict, DynamicType, ObjectPath, OwnedObjectPath, Type, Value},
};

pub(super) const DESKTOP: &str = "org.freedesktop.portal.Desktop";
pub(super) const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
const REGISTRY: &str = "org.freedesktop.host.portal.Registry";
const REMOTE_DESKTOP: &str = "org.freedesktop.portal.RemoteDesktop";
const SCREEN_CAST: &str = "org.freedesktop.portal.ScreenCast";
pub(super) const CLIPBOARD: &str = "org.freedesktop.portal.Clipboard";
const INHIBIT: &str = "org.freedesktop.portal.Inhibit";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const PERMISSION_STORE: &str = "org.freedesktop.impl.portal.PermissionStore";
const PERMISSION_STORE_PATH: &str = "/org/freedesktop/impl/portal/PermissionStore";
const KDE_BACKEND: &str = "org.freedesktop.impl.portal.desktop.kde";
/// The id of Kodosi's desktop entry, by which the portal names Kodosi and keeps its grant.
const APP_ID: &str = "com.kodosi.Kodosi";
const KEYBOARD: u32 = 1;
const POINTER: u32 = 2;
const MONITOR: u32 = 1;
const CURSOR_EMBEDDED: u32 = 2;
/// The pointer's position and picture come apart from the frames, so viewers draw it.
const CURSOR_METADATA: u32 = 4;
/// The grant lasts until the person at the computer revokes it.
const PERSIST: u32 = 2;
/// The inhibit portal's flag that keeps the screen from blanking and locking.
const IDLE: u32 = 8;
/// How long any one call over D-Bus may take.
pub(super) const CALL_LIMIT: Duration = Duration::from_secs(10);

static HANDLES: AtomicU32 = AtomicU32::new(1);

#[derive(Debug)]
pub(super) enum Problem {
    /// The person at the computer did not allow the sharing, or did not answer in time.
    Declined,
    Failed(String),
}

impl From<zbus::Error> for Problem {
    fn from(error: zbus::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<zvariant::Error> for Problem {
    fn from(error: zvariant::Error) -> Self {
        Self::Failed(error.to_string())
    }
}

#[derive(Clone)]
pub(super) struct Portal {
    bus: Connection,
    /// The app id that the portal knows the helper by, once it registered as Kodosi.
    app: Option<&'static str>,
    /// KDE's portal serves the desktop, which can take the word of the person at the computer.
    kde: bool,
}

impl Portal {
    /// Registers with the portal as Kodosi, whose name the desktop shows when it asks, before
    /// any other call to it. `None` without a session bus.
    pub(super) async fn connect() -> Option<Self> {
        let bus = zbus::connection::Builder::session()
            .ok()?
            .method_timeout(CALL_LIMIT)
            .build()
            .await
            .ok()?;
        let registered = bus
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(REGISTRY),
                "Register",
                &(APP_ID, options()),
            )
            .await
            .is_ok();
        let kde = on_kde(&bus).await;
        Some(Self {
            bus,
            app: registered.then_some(APP_ID),
            kde,
        })
    }

    /// Whether KDE lets Kodosi share the screen without asking, as the person at the computer
    /// allowed in Kodosi's settings or in KDE's.
    pub(super) async fn preauthorized(&self) -> bool {
        let Some(app) = self.app.filter(|_| self.kde) else {
            return false;
        };
        let Ok(reply) = self
            .bus
            .call_method(
                Some(PERMISSION_STORE),
                PERMISSION_STORE_PATH,
                Some(PERMISSION_STORE),
                "Lookup",
                &("kde-authorized", "remote-desktop"),
            )
            .await
        else {
            return false;
        };
        reply
            .body()
            .deserialize::<(HashMap<String, Vec<String>>, zvariant::OwnedValue)>()
            .is_ok_and(|(apps, _)| {
                apps.get(app)
                    .is_some_and(|granted| granted.iter().any(|grant| grant == "yes"))
            })
    }

    /// Pre-authorizes Kodosi on KDE, as KDE's own remote desktop settings do, so that the
    /// desktop asks nobody again. Other desktops have to ask the person at the computer.
    pub(super) async fn preauthorize(&self) -> bool {
        let Some(app) = self.app.filter(|_| self.kde) else {
            return false;
        };
        self.bus
            .call_method(
                Some(PERMISSION_STORE),
                PERMISSION_STORE_PATH,
                Some(PERMISSION_STORE),
                "SetPermission",
                &("kde-authorized", true, "remote-desktop", app, vec!["yes"]),
            )
            .await
            .is_ok()
    }

    /// Starts a session that shares every display and takes input. With `restore`, the desktop
    /// starts it as it granted before; otherwise, or when that grant no longer holds, it asks
    /// the person at the computer, who has `limit` to answer.
    pub(super) async fn start(
        &self,
        restore: Option<String>,
        limit: Duration,
    ) -> Result<Desktop, Problem> {
        let deadline = Instant::now() + limit;
        let created: Created = self
            .request(REMOTE_DESKTOP, "CreateSession", deadline, |token| {
                (with_handle(
                    token,
                    [("session_handle_token", handle().into())],
                ),)
            })
            .await?;
        let session = created
            .session_handle
            .and_then(|path| OwnedObjectPath::try_from(path).ok())
            .ok_or_else(|| Problem::Failed("the portal made no session".to_owned()))?;
        match self.share(&session, restore, deadline).await {
            Ok(started) => Ok(Desktop {
                bus: self.bus.clone(),
                displays: displays(&started),
                token: started.restore_token,
                input: started.devices.unwrap_or_default() & (KEYBOARD | POINTER) != 0,
                clipboard: started.clipboard_enabled.unwrap_or_default(),
                session,
            }),
            Err(problem) => {
                close(&self.bus, &session).await;
                Err(problem)
            }
        }
    }

    async fn share(
        &self,
        session: &ObjectPath<'_>,
        restore: Option<String>,
        deadline: Instant,
    ) -> Result<Started, Problem> {
        // The clipboard comes with the session where the desktop offers it.
        drop(
            self.bus
                .call_method(
                    Some(DESKTOP),
                    DESKTOP_PATH,
                    Some(CLIPBOARD),
                    "RequestClipboard",
                    &(session, options()),
                )
                .await,
        );
        let mut devices = vec![
            ("types", Value::from(KEYBOARD | POINTER)),
            ("persist_mode", Value::from(PERSIST)),
        ];
        if let Some(restore) = restore {
            devices.push(("restore_token", Value::from(restore)));
        }
        let _: Answer = self
            .request(REMOTE_DESKTOP, "SelectDevices", deadline, |token| {
                (session, with_handle(token, devices))
            })
            .await?;
        let cursor = if self.cursor_modes().await & CURSOR_METADATA == 0 {
            CURSOR_EMBEDDED
        } else {
            CURSOR_METADATA
        };
        let _: Answer = self
            .request(SCREEN_CAST, "SelectSources", deadline, |token| {
                (
                    session,
                    with_handle(
                        token,
                        [
                            ("types", Value::from(MONITOR)),
                            ("multiple", Value::from(true)),
                            ("cursor_mode", Value::from(cursor)),
                        ],
                    ),
                )
            })
            .await?;
        self.request(REMOTE_DESKTOP, "Start", deadline, |token| {
            (session, "", with_handle(token, []))
        })
        .await
    }

    async fn cursor_modes(&self) -> u32 {
        let Ok(reply) = self
            .bus
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(PROPERTIES),
                "Get",
                &(SCREEN_CAST, "AvailableCursorModes"),
            )
            .await
        else {
            return 0;
        };
        reply
            .body()
            .deserialize::<zvariant::OwnedValue>()
            .ok()
            .and_then(|modes| u32::try_from(modes).ok())
            .unwrap_or_default()
    }

    /// Keeps the screen from blanking and locking while someone watches it, until the returned
    /// request closes.
    pub(super) async fn inhibit(&self) -> Option<OwnedObjectPath> {
        let reply = self
            .bus
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(INHIBIT),
                "Inhibit",
                &("", IDLE, with_handle(&handle(), [])),
            )
            .await
            .ok()?;
        reply.body().deserialize::<OwnedObjectPath>().ok()
    }

    pub(super) async fn uninhibit(&self, request: &ObjectPath<'_>) {
        drop(
            self.bus
                .call_method(Some(DESKTOP), request, Some(REQUEST), "Close", &())
                .await,
        );
    }

    /// Calls a method that answers through a request object, and waits for the answer until the
    /// deadline, when the desktop's question to the person at the computer goes away unanswered.
    async fn request<B, T>(
        &self,
        interface: &'static str,
        method: &'static str,
        deadline: Instant,
        body: impl FnOnce(&str) -> B + Send,
    ) -> Result<T, Problem>
    where
        B: Serialize + DynamicType + Send + Sync,
        T: DeserializeOwned + Type + Send,
    {
        let token = handle();
        let sender = self.bus.unique_name().map_or_else(String::new, |name| {
            name.trim_start_matches(':').replace('.', "_")
        });
        let path = format!("{DESKTOP_PATH}/request/{sender}/{token}");
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(REQUEST)?
            .member("Response")?
            .path(path.as_str())?
            .build();
        let mut answers = MessageStream::for_match_rule(rule, &self.bus, Some(1)).await?;
        self.bus
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(interface),
                method,
                &body(&token),
            )
            .await?;
        let Ok(answer) = tokio::time::timeout_at(deadline, answers.next()).await else {
            drop(
                self.bus
                    .call_method(Some(DESKTOP), path.as_str(), Some(REQUEST), "Close", &())
                    .await,
            );
            return Err(Problem::Declined);
        };
        let answer =
            answer.ok_or_else(|| Problem::Failed("the portal did not answer".to_owned()))??;
        let (code, results) = answer.body().deserialize::<(u32, T)>()?;
        match code {
            0 => Ok(results),
            1 => Err(Problem::Declined),
            _ => Err(Problem::Failed(format!(
                "the portal could not {method}: {code}"
            ))),
        }
    }
}

/// A started portal session.
pub(super) struct Desktop {
    bus: Connection,
    session: OwnedObjectPath,
    /// The shared displays, in the portal's order, which puts the main display first.
    pub(super) displays: Vec<Shared>,
    /// The token that starts the next session without asking, once.
    pub(super) token: Option<String>,
    /// Whether the desktop lets the helper play input.
    pub(super) input: bool,
    pub(super) clipboard: bool,
}

/// A display that the session shares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Shared {
    /// The `PipeWire` node of the display's stream.
    pub(super) node: u32,
    /// Where the display is on the desktop and its size, in logical pixels.
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) width: f64,
    pub(super) height: f64,
}

impl Desktop {
    pub(super) fn session(&self) -> &ObjectPath<'_> {
        &self.session
    }

    pub(super) fn bus(&self) -> &Connection {
        &self.bus
    }

    /// The connection to `PipeWire` that carries the displays' streams.
    pub(super) async fn pipewire(&self) -> Result<OwnedFd, Problem> {
        let reply = self
            .bus
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(SCREEN_CAST),
                "OpenPipeWireRemote",
                &(&self.session, options()),
            )
            .await?;
        Ok(reply.body().deserialize::<zvariant::OwnedFd>()?.into())
    }

    /// The connection to the desktop's libei server, which takes the input.
    pub(super) async fn eis(&self) -> Result<OwnedFd, Problem> {
        let reply = self
            .bus
            .call_method(
                Some(DESKTOP),
                DESKTOP_PATH,
                Some(REMOTE_DESKTOP),
                "ConnectToEIS",
                &(&self.session, options()),
            )
            .await?;
        Ok(reply.body().deserialize::<zvariant::OwnedFd>()?.into())
    }

    /// Resolves once the desktop closes the session, as when the person at the computer ends it.
    pub(super) async fn closing(&self) -> Result<MessageStream, Problem> {
        let rule = MatchRule::builder()
            .msg_type(MessageType::Signal)
            .interface(SESSION)?
            .member("Closed")?
            .path(self.session.as_ref())?
            .build();
        Ok(MessageStream::for_match_rule(rule, &self.bus, Some(1)).await?)
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        let (bus, session) = (self.bus.clone(), self.session.clone());
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move { close(&bus, &session).await });
        }
    }
}

async fn on_kde(bus: &Connection) -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if desktop
        .split(':')
        .any(|name| name.eq_ignore_ascii_case("KDE"))
    {
        return true;
    }
    let (Ok(bus), Ok(backend)) = (
        zbus::fdo::DBusProxy::new(bus).await,
        BusName::try_from(KDE_BACKEND),
    ) else {
        return false;
    };
    bus.name_has_owner(backend).await.unwrap_or(false)
}

async fn close(bus: &Connection, session: &ObjectPath<'_>) {
    drop(
        bus.call_method(Some(DESKTOP), session, Some(SESSION), "Close", &())
            .await,
    );
}

#[derive(Debug, Default, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}", crate = "zbus::zvariant")]
struct Created {
    session_handle: Option<String>,
}

#[derive(Debug, Default, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}", crate = "zbus::zvariant")]
struct Answer {}

#[derive(Debug, Default, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}", crate = "zbus::zvariant")]
struct Started {
    devices: Option<u32>,
    clipboard_enabled: Option<bool>,
    restore_token: Option<String>,
    streams: Option<Vec<(u32, Stream)>>,
}

#[derive(Debug, Default, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}", crate = "zbus::zvariant")]
struct Stream {
    position: Option<(i32, i32)>,
    size: Option<(i32, i32)>,
}

fn displays(started: &Started) -> Vec<Shared> {
    started
        .streams
        .iter()
        .flatten()
        .filter_map(|(node, stream)| {
            let (x, y) = stream.position.unwrap_or_default();
            let (width, height) = stream.size?;
            Some(Shared {
                node: *node,
                x: f64::from(x),
                y: f64::from(y),
                width: f64::from(width),
                height: f64::from(height),
            })
        })
        .collect()
}

fn handle() -> String {
    format!(
        "kodosi{}x{}",
        std::process::id(),
        HANDLES.fetch_add(1, Ordering::Relaxed)
    )
}

fn options() -> HashMap<&'static str, Value<'static>> {
    HashMap::new()
}

fn with_handle<'v>(
    token: &str,
    entries: impl IntoIterator<Item = (&'static str, Value<'v>)>,
) -> HashMap<&'static str, Value<'v>> {
    let mut options: HashMap<&'static str, Value<'v>> = entries.into_iter().collect();
    options.insert("handle_token", Value::from(token.to_owned()));
    options
}

/// What the desktop granted, kept in a private file: the token that starts the next session
/// without asking, which the desktop replaces with each session it starts.
pub(super) struct Grant {
    path: PathBuf,
    kept: Option<Kept>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Kept {
    token: String,
    input: bool,
}

impl Grant {
    pub(super) fn read(path: PathBuf) -> Self {
        let kept = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        Self { path, kept }
    }

    pub(super) fn token(&self) -> Option<String> {
        self.kept.as_ref().map(|kept| kept.token.clone())
    }

    /// Whether the kept grant lets the helper play input.
    pub(super) fn input(&self) -> bool {
        self.kept.as_ref().is_some_and(|kept| kept.input)
    }

    pub(super) async fn keep(&mut self, token: Option<String>, input: bool) {
        self.kept = token.map(|token| Kept { token, input });
        let path = self.path.clone();
        let kept = self.kept.clone();
        drop(tokio::task::spawn_blocking(move || write(&path, kept.as_ref())).await);
    }

    pub(super) async fn forget(&mut self) {
        self.keep(None, false).await;
    }
}

fn write(path: &std::path::Path, kept: Option<&Kept>) {
    let Some(kept) = kept else {
        drop(std::fs::remove_file(path));
        return;
    };
    let (Some(folder), Ok(bytes)) = (path.parent(), serde_json::to_vec(kept)) else {
        return;
    };
    let fresh = path.with_extension("new");
    let written = std::fs::create_dir_all(folder).and_then(|()| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&fresh)?;
        std::io::Write::write_all(&mut file, &bytes)?;
        file.sync_all()
    });
    if written.is_ok() {
        drop(std::fs::rename(&fresh, path));
    }
}
