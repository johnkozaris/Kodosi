//! The notice while people watch: an item in the desktop's tray, a `StatusNotifierItem` with its
//! menu, that says who watches and stops the sharing. Desktops without a tray show their own
//! sign that the screen is shared, which also ends it.

#![expect(
    clippy::needless_pass_by_value,
    clippy::unnecessary_literal_bound,
    clippy::unused_self,
    clippy::used_underscore_binding,
    reason = "the tray calls these methods of its D-Bus interfaces with what they declare"
)]

use std::collections::HashMap;

use screen::link::Notice;
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use zbus::{
    Connection,
    object_server::SignalEmitter,
    zvariant::{self, ObjectPath, OwnedValue, Type, Value},
};

use super::{portal::CALL_LIMIT, session::Event};

const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
/// Kodosi's icon, which its package installs.
const ICON: &str = "com.kodosi.Kodosi";
const ROOT: i32 = 0;
const WHO: i32 = 1;
const STOP: i32 = 2;

/// Shows the notice on a task of its own, so that a tray that is slow to answer never holds up
/// the link to the host.
pub(super) struct Tray {
    notice: watch::Sender<Option<Notice>>,
    task: JoinHandle<()>,
}

impl Tray {
    pub(super) fn start(events: mpsc::UnboundedSender<Event>) -> Self {
        let (notice, mut changes) = watch::channel(None);
        let task = tokio::spawn(async move {
            let mut shown: Option<Shown> = None;
            while changes.changed().await.is_ok() {
                let notice = changes.borrow_and_update().clone();
                shown = match (notice, shown) {
                    (Some(notice), Some(shown)) => {
                        shown.update(&notice).await;
                        Some(shown)
                    }
                    (Some(notice), None) => Shown::show(&notice, events.clone()).await,
                    (None, _) => None,
                };
            }
        });
        Self { notice, task }
    }

    /// Shows who watches, or takes the notice away once nobody does.
    pub(super) fn show(&self, notice: Option<Notice>) {
        self.notice.send_replace(notice);
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The shown notice, which goes once dropped.
struct Shown {
    bus: Connection,
}

impl Shown {
    /// `None` where the desktop has no tray.
    async fn show(notice: &Notice, events: mpsc::UnboundedSender<Event>) -> Option<Self> {
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        let bus = zbus::connection::Builder::session()
            .ok()?
            .method_timeout(CALL_LIMIT)
            .name(name.as_str())
            .ok()?
            .serve_at(
                ITEM_PATH,
                Item {
                    watching: notice.watching.clone(),
                },
            )
            .ok()?
            .serve_at(
                MENU_PATH,
                Menu {
                    revision: 1,
                    watching: notice.watching.clone(),
                    stop: notice.stop.clone(),
                    events,
                },
            )
            .ok()?
            .build()
            .await
            .ok()?;
        bus.call_method(
            Some(WATCHER),
            WATCHER_PATH,
            Some(WATCHER),
            "RegisterStatusNotifierItem",
            &(name.as_str(),),
        )
        .await
        .ok()?;
        Some(Self { bus })
    }

    async fn update(&self, notice: &Notice) {
        let server = self.bus.object_server();
        if let Ok(item) = server.interface::<_, Item>(ITEM_PATH).await {
            item.get_mut().await.watching.clone_from(&notice.watching);
            drop(Item::new_title(item.signal_emitter()).await);
            drop(Item::new_tool_tip(item.signal_emitter()).await);
        }
        if let Ok(menu) = server.interface::<_, Menu>(MENU_PATH).await {
            let revision = {
                let mut menu = menu.get_mut().await;
                menu.watching.clone_from(&notice.watching);
                menu.stop.clone_from(&notice.stop);
                menu.revision += 1;
                menu.revision
            };
            drop(Menu::layout_updated(menu.signal_emitter(), revision, ROOT).await);
        }
    }
}

struct Item {
    watching: String,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "SystemServices"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "kodosi-screen"
    }

    #[zbus(property)]
    fn title(&self) -> String {
        self.watching.clone()
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ICON
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        (
            ICON.to_owned(),
            Vec::new(),
            self.watching.clone(),
            String::new(),
        )
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'_> {
        ObjectPath::from_static_str_unchecked(MENU_PATH)
    }

    #[zbus(signal)]
    async fn new_title(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn new_tool_tip(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}

/// The item's menu: who watches, then the stop button.
struct Menu {
    revision: u32,
    watching: String,
    stop: String,
    events: mpsc::UnboundedSender<Event>,
}

/// A menu item with its children, as `com.canonical.dbusmenu` lays them out.
#[derive(Debug, Clone, serde::Serialize, Type, zvariant::Value, zvariant::OwnedValue)]
#[zvariant(crate = "zbus::zvariant")]
struct Layout {
    id: i32,
    properties: HashMap<String, OwnedValue>,
    children: Vec<OwnedValue>,
}

impl Menu {
    /// The item's properties, or those of them that are `named` when any are.
    fn properties(&self, id: i32, named: &[String]) -> HashMap<String, OwnedValue> {
        let mut properties = HashMap::new();
        let mut set = |name: &str, value: Value<'_>| {
            if (named.is_empty() || named.iter().any(|wanted| wanted == name))
                && let Ok(value) = value.try_into_owned()
            {
                properties.insert(name.to_owned(), value);
            }
        };
        match id {
            ROOT => set("children-display", Value::from("submenu")),
            WHO => {
                set("label", Value::from(self.watching.as_str()));
                set("enabled", Value::from(false));
            }
            STOP => set("label", Value::from(self.stop.as_str())),
            _ => {}
        }
        properties
    }

    /// The item and its children, `depth` levels down, or all of them for -1.
    fn layout(&self, id: i32, depth: i32, named: &[String]) -> Layout {
        let children = if id == ROOT && depth != 0 {
            [WHO, STOP]
                .into_iter()
                .filter_map(|child| OwnedValue::try_from(self.layout(child, depth - 1, named)).ok())
                .collect()
        } else {
            Vec::new()
        };
        Layout {
            id,
            properties: self.properties(id, named),
            children,
        }
    }
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }

    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }

    fn get_layout(&self, parent: i32, depth: i32, properties: Vec<String>) -> (u32, Layout) {
        (self.revision, self.layout(parent, depth, &properties))
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        properties: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let ids = if ids.is_empty() {
            vec![ROOT, WHO, STOP]
        } else {
            ids
        };
        ids.into_iter()
            .map(|id| (id, self.properties(id, &properties)))
            .collect()
    }

    fn get_property(&self, id: i32, name: &str) -> zbus::fdo::Result<OwnedValue> {
        self.properties(id, &[])
            .remove(name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs(name.to_owned()))
    }

    fn event(&self, id: i32, event: &str, _data: Value<'_>, _timestamp: u32) {
        if id == STOP && event == "clicked" {
            drop(self.events.send(Event::Stopped));
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, event, _, _) in &events {
            if *id == STOP && event == "clicked" {
                drop(self.events.send(Event::Stopped));
            }
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(signal)]
    async fn layout_updated(
        emitter: &SignalEmitter<'_>,
        revision: u32,
        parent: i32,
    ) -> zbus::Result<()>;
}
