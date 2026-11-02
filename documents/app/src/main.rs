//! A desktop app for Word, Excel, `PowerPoint` and PDF files. Each window shows one file as one
//! viewer of its live document, so two windows on one file edit it together.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use documents::{Documents, OpenError, Viewer};
use tauri::{
    AppHandle, Manager as _, RunEvent, State, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent, Wry,
    ipc::{Channel, InvokeBody, InvokeResponseBody, Request},
    menu::{Menu, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder},
    webview::NewWindowResponse,
};
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;
use tokio::sync::mpsc;

const PAGE: &str = include_str!("page.js");
const VIEWS: [(&str, &str); 4] = [
    ("docx", "word"),
    ("xlsx", "excel"),
    ("pptx", "powerpoint"),
    ("pdf", "pdf"),
];
const OPEN: &str = "open";
const OPEN_AGAIN: &str = "open-again";
const CLOSE: &str = "close";
const QUIT: &str = "quit";

struct App {
    documents: Documents,
    person: String,
    windows: Mutex<HashMap<String, Shown>>,
    next_window: AtomicU64,
}

/// A window's file, and the way to its pipe while its page is open.
struct Shown {
    file: PathBuf,
    frames: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

impl App {
    fn new(state_folder: PathBuf) -> Self {
        Self {
            documents: Documents::new(state_folder, documents_cli::formats()),
            person: std::env::var("USER")
                .or_else(|_| std::env::var("USERNAME"))
                .unwrap_or_else(|_| "Me".to_owned()),
            windows: Mutex::default(),
            next_window: AtomicU64::new(1),
        }
    }

    fn add(&self, file: PathBuf) -> String {
        let label = format!(
            "document-{}",
            self.next_window.fetch_add(1, Ordering::Relaxed)
        );
        self.lock()
            .insert(label.clone(), Shown { file, frames: None });
        label
    }

    fn file_of(&self, label: &str) -> Option<PathBuf> {
        self.lock().get(label).map(|shown| shown.file.clone())
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Shown>> {
        self.windows.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn main() -> tauri::Result<()> {
    let files: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, folder| {
                let files: Vec<PathBuf> = arguments
                    .iter()
                    .skip(1)
                    .map(|file| Path::new(&folder).join(file))
                    .collect();
                drop(launched(app, &files));
            },
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![open, send])
        .menu(menu)
        .on_menu_event(|app, event| chosen(app, &event))
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Destroyed) {
                window.state::<App>().lock().remove(window.label());
            }
        })
        .setup(move |app| {
            app.manage(App::new(app.path().app_data_dir()?.join("documents")));
            launched(app.handle(), &files)?;
            Ok(())
        })
        .build(tauri::generate_context!())?;
    app.run(|app, event| {
        if matches!(event, RunEvent::Exit) {
            tauri::async_runtime::block_on(app.state::<App>().documents.close());
        }
    });
    Ok(())
}

/// Opens each file of a launch in its own window, or asks for files when it names none. A second
/// launch of the app ends at once and gives its files to this one, so one server owns each file.
fn launched(app: &AppHandle, files: &[PathBuf]) -> tauri::Result<()> {
    if files.is_empty() {
        pick(app);
    }
    for file in files {
        show(app, file)?;
    }
    Ok(())
}

/// The Mac has its app menu, and an Edit menu so that its web view takes the edit keys. Linux and
/// Windows give a web view those keys by themselves.
fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let item = |id: &str, text: &str, keys: &str| {
        MenuItemBuilder::with_id(id, text)
            .accelerator(keys)
            .build(app)
    };
    let file = SubmenuBuilder::new(app, "File")
        .item(&item(OPEN, "Open…", "CmdOrCtrl+O")?)
        .item(&item(
            OPEN_AGAIN,
            "Open in Another Window",
            "CmdOrCtrl+Shift+N",
        )?)
        .separator()
        .item(&item(CLOSE, "Close Window", "CmdOrCtrl+W")?);
    #[cfg(target_os = "macos")]
    let menu = MenuBuilder::new(app)
        .item(
            &SubmenuBuilder::new(app, "Documents")
                .about(None)
                .separator()
                .services()
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?,
        )
        .item(&file.build()?)
        .item(
            &SubmenuBuilder::new(app, "Edit")
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?,
        );
    #[cfg(not(target_os = "macos"))]
    let menu = MenuBuilder::new(app).item(
        &file
            .separator()
            .item(&item(QUIT, "Quit", "CmdOrCtrl+Q")?)
            .build()?,
    );
    menu.build()
}

fn chosen(app: &AppHandle, event: &MenuEvent) {
    let focused = || {
        app.webview_windows()
            .into_values()
            .find(|window| window.is_focused().unwrap_or(false))
    };
    match event.id().as_ref() {
        OPEN => pick(app),
        OPEN_AGAIN => {
            let file = focused().and_then(|window| app.state::<App>().file_of(window.label()));
            if let Some(file) = file {
                drop(show(app, &file));
            }
        }
        CLOSE => {
            if let Some(window) = focused() {
                drop(window.close());
            }
        }
        QUIT => app.exit(0),
        _ => {}
    }
}

/// Asks for files and opens each in its own window. The app ends when the first ask gives none.
fn pick(app: &AppHandle) {
    let extensions = VIEWS.map(|(extension, _)| extension);
    let shows = app.clone();
    app.dialog()
        .file()
        .add_filter("Documents", &extensions)
        .pick_files(move |files| {
            let files = files.unwrap_or_default();
            if files.is_empty() && shows.webview_windows().is_empty() {
                shows.exit(0);
            }
            for file in files.into_iter().filter_map(|file| file.into_path().ok()) {
                drop(show(&shows, &file));
            }
        });
}

fn show(app: &AppHandle, file: &Path) -> tauri::Result<()> {
    let file = std::path::absolute(file)?;
    let title = file.file_name().map_or_else(
        || file.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let label = app.state::<App>().add(file);
    let opener = app.clone();
    WebviewWindowBuilder::new(app, label, WebviewUrl::default())
        .title(title)
        .inner_size(1100.0, 820.0)
        .min_inner_size(440.0, 360.0)
        .initialization_script(PAGE)
        .on_navigation(is_own)
        .on_new_window(move |url, _| {
            follow(&opener, &url);
            NewWindowResponse::Deny
        })
        .build()?;
    Ok(())
}

fn is_own(url: &Url) -> bool {
    url.scheme() == "tauri"
        || url.host_str() == Some("tauri.localhost")
        || matches!(url.as_str(), "about:blank" | "about:srcdoc")
}

/// A link that a view opens goes to the person's own browser or mail app.
fn follow(app: &AppHandle, url: &Url) {
    if matches!(url.scheme(), "http" | "https" | "mailto") {
        drop(app.opener().open_url(url.as_str(), None::<&str>));
    }
}

fn kind_of(file: &Path) -> Option<&'static str> {
    let extension = file.extension()?.to_str()?.to_ascii_lowercase();
    VIEWS
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, kind)| *kind)
}

/// Joins the window's file and sends its frames on `frames`. Gives the kind of view to show.
#[tauri::command]
async fn open(
    window: WebviewWindow,
    app: State<'_, App>,
    frames: Channel,
) -> Result<&'static str, String> {
    let label = window.label();
    let file = app.file_of(label).ok_or("This window has no file.")?;
    let kind = kind_of(&file).ok_or_else(|| OpenError::UnknownKind.to_string())?;
    let viewer = Viewer {
        id: label.to_owned(),
        name: app.person.clone(),
        can_edit: true,
    };
    let mut pipe = app
        .documents
        .open(&file, viewer)
        .await
        .map_err(|error| error.to_string())?;
    let (sender, mut outgoing) = mpsc::unbounded_channel::<Vec<u8>>();
    match app.lock().get_mut(label) {
        Some(shown) => shown.frames = Some(sender),
        None => return Err("The window closed.".to_owned()),
    }
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::select! {
                frame = outgoing.recv() => {
                    let Some(frame) = frame else { return };
                    if pipe.send(frame).await.is_err() {
                        return;
                    }
                }
                frame = pipe.receive() => {
                    let Some(frame) = frame else { return };
                    if frames.send(InvokeResponseBody::Raw(frame.to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    Ok(kind)
}

/// Takes one frame of the window's view, as bytes.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri gives a command its arguments by value"
)]
fn send(window: WebviewWindow, app: State<'_, App>, request: Request<'_>) -> Result<(), String> {
    let InvokeBody::Raw(frame) = request.body() else {
        return Err("A frame comes as bytes.".to_owned());
    };
    let frames = app
        .lock()
        .get(window.label())
        .and_then(|shown| shown.frames.clone())
        .ok_or("The file is not open.")?;
    frames
        .send(frame.clone())
        .map_err(|_| "The document closed.".to_owned())
}
