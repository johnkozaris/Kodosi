use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use clap::Parser as _;
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

use super::{
    BROWSER_PIPE, Canvas, CanvasAnswer, CanvasBrowser, CanvasRequest, PipeEnds,
    attach::{Part, Uploads},
    browser::{self, Browser, Places, Terminal},
    engine::BrowserHost,
    files::{Files, Showing, Stamp, key_of},
    render::{self, RenderArguments},
};
use crate::documents::{DocumentHost, path_of};
use crate::text::Text;

const JOBS: usize = 64;
const CHECK: Duration = Duration::from_secs(1);

type Reply = oneshot::Sender<Result<CanvasAnswer, Text>>;

/// What all canvases of this computer share.
#[derive(Clone)]
pub(crate) struct CanvasServices {
    pub(crate) documents: DocumentHost,
    pub(crate) browsers: BrowserHost,
}

impl CanvasServices {
    pub(crate) fn new(data_root: &Path) -> Self {
        Self {
            documents: DocumentHost::new(data_root),
            browsers: BrowserHost::new(data_root),
        }
    }

    pub(crate) async fn close(&self) {
        self.documents.close().await;
        self.browsers.close().await;
    }
}

enum Job {
    Show {
        paths: Vec<PathBuf>,
        terminal: String,
        reply: Reply,
    },
    Browse {
        folder: PathBuf,
        arguments: Vec<String>,
        terminal: String,
        agent: Option<String>,
        reply: Reply,
    },
    Request {
        request: CanvasRequest,
        reply: Reply,
    },
    Render {
        file: PathBuf,
        out: Option<PathBuf>,
        viewer: documents::Viewer,
        documents: DocumentHost,
        reply: Reply,
    },
    Watch {
        viewer: shared_browser::Viewer,
        reply: oneshot::Sender<Result<PipeEnds, Text>>,
    },
    Downloaded {
        path: PathBuf,
    },
    Restore {
        paths: Vec<PathBuf>,
        address: Option<String>,
        from: Uuid,
    },
    End {
        remove: bool,
        reply: oneshot::Sender<()>,
    },
}

/// The canvas of one terminal: the files that its agent shows and its shared browser.
#[derive(Clone)]
pub(crate) struct CanvasHandle {
    jobs: mpsc::Sender<Job>,
    state: watch::Receiver<Canvas>,
    folder: watch::Sender<PathBuf>,
    shared: watch::Sender<bool>,
    documents: DocumentHost,
}

impl CanvasHandle {
    pub(crate) fn spawn(id: Uuid, data_root: &Path, services: CanvasServices) -> Self {
        let (jobs, receiver) = mpsc::channel(JOBS);
        let (published, state) = watch::channel(Canvas::default());
        let (folder, current_folder) = watch::channel(PathBuf::new());
        let (shared, sharing) = watch::channel(false);
        let actor = Actor {
            id,
            data_root: data_root.to_owned(),
            jobs: receiver,
            published,
            stamp: Stamp::default(),
            files: Files::default(),
            uploads: Uploads::default(),
            folder: current_folder,
            own_jobs: jobs.downgrade(),
            sharing,
            browsers: services.browsers,
            browser: None,
        };
        tokio::spawn(actor.run());
        Self {
            jobs,
            state,
            folder,
            shared,
            documents: services.documents,
        }
    }

    /// A shared terminal does not use a person's kept sign-ins, so its browser moves to a
    /// profile of its own.
    pub(crate) fn set_shared(&self, shared: bool) {
        self.shared.send_if_modified(|current| {
            let changed = *current != shared;
            *current = shared;
            changed
        });
    }

    /// The folder of the terminal, where the files that people add go.
    pub(crate) fn moved_to(&self, folder: PathBuf) {
        self.folder.send_replace(folder);
    }

    pub(crate) fn state(&self) -> watch::Receiver<Canvas> {
        self.state.clone()
    }

    /// What the canvas shows now; none when it is empty.
    pub(crate) fn current(&self) -> Option<Canvas> {
        Some(self.state.borrow().clone()).filter(|canvas| !canvas.is_empty())
    }

    pub(crate) fn paths(&self) -> Vec<PathBuf> {
        self.state
            .borrow()
            .items
            .iter()
            .map(|item| PathBuf::from(&item.key))
            .collect()
    }

    pub(crate) async fn show(
        &self,
        paths: Vec<PathBuf>,
        terminal: String,
    ) -> Result<CanvasAnswer, Text> {
        self.ask(|reply| Job::Show {
            paths,
            terminal,
            reply,
        })
        .await
    }

    pub(crate) async fn browse(
        &self,
        folder: PathBuf,
        arguments: Vec<String>,
        terminal: String,
        agent: Option<String>,
    ) -> Result<CanvasAnswer, Text> {
        self.ask(|reply| Job::Browse {
            folder,
            arguments,
            terminal,
            agent,
            reply,
        })
        .await
    }

    /// Joins the live document of an item of this canvas. `view` names this one view, and
    /// `person` is the same in each view of one person.
    pub(crate) async fn document(
        &self,
        key: &str,
        view: String,
        person: String,
        name: String,
    ) -> Result<PipeEnds, Text> {
        if key == BROWSER_PIPE {
            let (reply, answer) = oneshot::channel();
            let viewer = shared_browser::Viewer {
                id: view,
                person,
                name,
            };
            self.jobs
                .try_send(Job::Watch { viewer, reply })
                .map_err(|_| busy())?;
            return answer
                .await
                .unwrap_or_else(|_| Err(text!("The terminal of this canvas closed.")));
        }
        let shown = self
            .state
            .borrow()
            .items
            .iter()
            .any(|item| item.key == key && item.view.is_some());
        if !shown {
            return Err(text!("This file is not on the canvas."));
        }
        let viewer = documents::Viewer {
            id: person,
            name,
            can_edit: true,
        };
        self.documents.open(&path_of(key), viewer).await
    }

    /// Runs an agent's `kodosi doc` command.
    pub(crate) async fn document_command(
        &self,
        agent: Option<String>,
        terminal: String,
        folder: PathBuf,
        arguments: Vec<String>,
    ) -> CanvasAnswer {
        let name = agent.unwrap_or_else(|| "Agent".to_owned());
        let viewer = documents::Viewer {
            id: format!("agent-{terminal}-{name}"),
            name,
            can_edit: true,
        };
        if arguments.first().is_some_and(|word| word == "render") {
            return self.render(viewer, &folder, arguments).await;
        }
        self.documents.run(&viewer, &folder, arguments).await
    }

    /// Draws each page of a file in this terminal's browser, as people see it, and saves the
    /// pictures. The agent looks and changes nothing.
    async fn render(
        &self,
        viewer: documents::Viewer,
        folder: &Path,
        arguments: Vec<String>,
    ) -> CanvasAnswer {
        let parsed = match RenderArguments::try_parse_from(arguments) {
            Ok(parsed) => parsed,
            Err(error) => {
                return CanvasAnswer::Text {
                    text: error.render().to_string(),
                    code: i32::from(error.use_stderr()) * 2,
                };
            }
        };
        let file = folder.join(parsed.file);
        let out = parsed.out.map(|out| folder.join(out));
        let viewer = documents::Viewer {
            can_edit: false,
            ..viewer
        };
        let documents = self.documents.clone();
        self.ask(|reply| Job::Render {
            file,
            out,
            viewer,
            documents,
            reply,
        })
        .await
        .unwrap_or_else(|problem| CanvasAnswer::Text {
            text: format!("{problem}\n"),
            code: 1,
        })
    }

    pub(crate) async fn request(&self, request: CanvasRequest) -> Result<CanvasAnswer, Text> {
        self.ask(|reply| Job::Request { request, reply }).await
    }

    /// Brings back the files and the browser of the terminal that this one replaces.
    /// The web address of the page that the browser shows.
    pub(crate) fn address(&self) -> Option<String> {
        self.state
            .borrow()
            .browser
            .as_ref()?
            .tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.url.clone())
            .filter(|address| address.starts_with("http"))
    }

    pub(crate) fn restore(&self, paths: Vec<PathBuf>, address: Option<String>, from: Uuid) {
        drop(self.jobs.try_send(Job::Restore {
            paths,
            address,
            from,
        }));
    }

    /// Closes the browser. `remove` also deletes its profile, because the terminal does not
    /// come back.
    pub(crate) async fn end(&self, remove: bool) {
        let (reply, done) = oneshot::channel();
        if self.jobs.send(Job::End { remove, reply }).await.is_ok() {
            drop(done.await);
        }
    }

    async fn ask(&self, job: impl FnOnce(Reply) -> Job) -> Result<CanvasAnswer, Text> {
        let (reply, answer) = oneshot::channel();
        self.jobs.try_send(job(reply)).map_err(|_| busy())?;
        answer
            .await
            .unwrap_or_else(|_| Err(text!("The terminal of this canvas closed.")))
    }
}

struct Actor {
    id: Uuid,
    data_root: PathBuf,
    jobs: mpsc::Receiver<Job>,
    published: watch::Sender<Canvas>,
    stamp: Stamp,
    files: Files,
    uploads: Uploads,
    folder: watch::Receiver<PathBuf>,
    own_jobs: mpsc::WeakSender<Job>,
    sharing: watch::Receiver<bool>,
    browsers: BrowserHost,
    browser: Option<(Arc<Browser>, watch::Receiver<CanvasBrowser>)>,
}

impl Actor {
    async fn run(mut self) {
        let mut check = tokio::time::interval(CHECK);
        check.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                job = self.jobs.recv() => match job {
                    Some(Job::End { remove, reply }) => {
                        self.end(remove).await;
                        let _ = reply.send(());
                        return;
                    }
                    Some(job) => self.handle(job).await,
                    None => {
                        self.end(false).await;
                        return;
                    }
                },
                changed = self.sharing.changed() => {
                    if changed.is_ok() && let Some(browser) = self.existing() {
                        tokio::spawn(async move { browser.sleep().await });
                    }
                }
                _ = check.tick(), if !self.files.is_empty() => {
                    if self.files.check(&self.stamp).await {
                        self.publish();
                    }
                }
                changed = async {
                    match &mut self.browser {
                        Some((_, state)) => state.changed().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if changed.is_err() {
                        self.browser = None;
                    }
                    self.publish();
                }
            }
        }
    }

    fn publish(&self) {
        let canvas = Canvas {
            sequence: self.stamp.now(),
            items: self.files.items(),
            browser: self
                .browser
                .as_ref()
                .map(|(_, state)| state.borrow().clone()),
        };
        self.published.send_if_modified(|current| {
            if *current == canvas {
                false
            } else {
                *current = canvas;
                true
            }
        });
    }

    async fn handle(&mut self, job: Job) {
        match job {
            Job::Show {
                paths,
                terminal,
                reply,
            } => {
                let answer = self
                    .files
                    .show(paths, &self.stamp)
                    .await
                    .map(|showing| shown(&showing, &terminal));
                self.publish();
                drop(reply.send(answer));
            }
            Job::Browse {
                folder,
                arguments,
                terminal,
                agent,
                reply,
            } => {
                if let Some(missing) = browser::missing().filter(|_| self.browser.is_none()) {
                    drop(reply.send(Err(missing)));
                    return;
                }
                let browser = self.browser();
                self.publish();
                tokio::spawn(async move {
                    drop(reply.send(browser.run(folder, arguments, &terminal, agent).await));
                });
            }
            Job::Request { request, reply } => self.request(request, reply).await,
            Job::Render {
                file,
                out,
                viewer,
                documents,
                reply,
            } => {
                let host = self.browsers.clone();
                let out = out.unwrap_or_else(|| pictures(&self.data_root, self.id));
                let profile = if *self.sharing.borrow() {
                    self.id.to_string()
                } else {
                    browser::KEPT_PROFILE.to_owned()
                };
                tokio::spawn(async move {
                    let drawn = render::render(host, profile, documents, viewer, file, out).await;
                    drop(reply.send(drawn));
                });
            }
            Job::Watch { viewer, reply } => {
                let browser = self.browser();
                self.publish();
                tokio::spawn(async move { drop(reply.send(browser.watch(viewer).await)) });
            }
            Job::Downloaded { path } => {
                if let Err(problem) = self.files.show(vec![path], &self.stamp).await {
                    tracing::debug!(%problem, "the canvas does not show this download");
                }
                self.publish();
            }
            Job::Restore {
                paths,
                address,
                from,
            } => {
                self.files.restore(paths, &self.stamp);
                let earlier = browser::own_profile(&self.data_root, from);
                let profile = browser::own_profile(&self.data_root, self.id);
                drop(
                    tokio::task::spawn_blocking(move || {
                        if earlier.is_dir() && !profile.exists() {
                            drop(std::fs::rename(&earlier, &profile));
                        }
                    })
                    .await,
                );
                if address.is_some() {
                    self.browser().reopen_at(address);
                }
                self.publish();
            }
            Job::End { .. } => {}
        }
    }

    async fn request(&mut self, request: CanvasRequest, reply: Reply) {
        match request {
            CanvasRequest::Read {
                key,
                revision,
                offset,
            } => {
                let answer = self.files.read(&key, revision, offset).await;
                drop(reply.send(answer));
            }
            CanvasRequest::Remove { keys } => {
                let browser = keys.iter().any(|key| key == BROWSER_PIPE);
                if browser && let Some((browser, _)) = self.browser.take() {
                    tokio::spawn(async move { browser.remove().await });
                }
                let files = keys
                    .into_iter()
                    .filter(|key| key != BROWSER_PIPE)
                    .collect::<Vec<_>>();
                if !(browser && files.is_empty()) {
                    self.files.remove(&files);
                }
                self.publish();
                drop(reply.send(Ok(CanvasAnswer::Done)));
            }
            CanvasRequest::Attach {
                upload,
                name,
                offset,
                data,
                last,
                page,
            } => {
                let Ok(upload) = Uuid::parse_str(&upload) else {
                    drop(reply.send(Err(Text::plain("This upload has no number."))));
                    return;
                };
                let part = Part {
                    upload,
                    name: &name,
                    offset,
                    data: &data,
                    last,
                    page,
                };
                let answer = self.attach(part).await;
                drop(reply.send(answer));
            }
            CanvasRequest::Act { action } => {
                let browser = self.browser();
                self.publish();
                tokio::spawn(async move { drop(reply.send(browser.act(action).await)) });
            }
        }
    }

    async fn attach(&mut self, part: Part<'_>) -> Result<CanvasAnswer, Text> {
        let folder = self.folder.borrow().clone();
        let page = part.page;
        let Some(path) = self.uploads.add(&folder, part).await? else {
            return Ok(CanvasAnswer::Done);
        };
        let key = key_of(&path);
        if page {
            return Ok(CanvasAnswer::Attached { key });
        }
        let showing = self.files.show(vec![path], &self.stamp).await;
        self.publish();
        showing.map(|_| CanvasAnswer::Attached { key })
    }

    fn existing(&self) -> Option<Arc<Browser>> {
        self.browser
            .as_ref()
            .map(|(browser, _)| Arc::clone(browser))
    }

    fn browser(&mut self) -> Arc<Browser> {
        if let Some(browser) = self.existing() {
            return browser;
        }
        let browser = Browser::start(
            Terminal {
                id: self.id,
                folder: self.folder.clone(),
                shared: self.sharing.clone(),
                show: {
                    let jobs = self.own_jobs.clone();
                    Box::new(move |path| {
                        if let Some(jobs) = jobs.upgrade() {
                            drop(jobs.try_send(Job::Downloaded { path }));
                        }
                    })
                },
            },
            Places::of(&self.data_root),
            self.browsers.clone(),
            self.stamp.clone(),
        );
        let state = browser.state();
        self.browser = Some((Arc::clone(&browser), state));
        browser
    }

    async fn end(&mut self, remove: bool) {
        self.uploads.clear().await;
        if remove {
            drop(tokio::fs::remove_dir_all(pictures(&self.data_root, self.id)).await);
        }
        if let Some((browser, _)) = self.browser.take() {
            if remove {
                browser.remove().await;
            } else {
                browser.sleep().await;
            }
        } else if remove {
            let profile = browser::own_profile(&self.data_root, self.id);
            drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(profile)).await);
        }
    }
}

/// The private folder of the pictures that `kodosi doc render` draws for a terminal.
fn pictures(data_root: &Path, id: Uuid) -> PathBuf {
    data_root.join("render").join(id.to_string())
}

fn shown(showing: &Showing, terminal: &str) -> CanvasAnswer {
    let names = showing
        .shown
        .iter()
        .map(|shown| shown.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let mut lines = vec![format!(
        "On the canvas of {terminal}: {names}. The people in this terminal open the canvas to see it."
    )];
    lines.extend(showing.shown.iter().filter_map(|shown| {
        shown.loads.as_ref().map(|address| {
            format!(
                "{} loads {address} from the network, which the canvas does not do. Put what the page needs into the page.",
                shown.name
            )
        })
    }));
    if !showing.dropped.is_empty() {
        lines.push(format!(
            "The canvas keeps {} items, so it no longer shows {}.",
            super::MOST_ITEMS,
            showing.dropped.join(", ")
        ));
    }
    CanvasAnswer::Text {
        text: lines.join("\n"),
        code: 0,
    }
}

/// Deletes the browser profiles of the terminals that do not come back, and the pictures of
/// earlier renders.
pub(crate) fn keep_profiles(data_root: &Path, returning: &HashSet<Uuid>) {
    for profile in browser::left_profiles(data_root, returning) {
        drop(std::fs::remove_dir_all(profile));
    }
    drop(std::fs::remove_dir_all(data_root.join("render")));
}

/// Closes the browsers that an earlier host left, for example after a crash.
pub(crate) async fn close_left(data_root: &Path) {
    browser::close_left(data_root).await;
}

fn busy() -> Text {
    tracing::warn!("the queue of a canvas is full, so it refuses a step");
    text!("The canvas is busy. Try again in a moment.")
}
