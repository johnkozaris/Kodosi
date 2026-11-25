use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::Duration,
};

use documents_cli::Command;
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

use super::{
    BROWSER_PIPE, Canvas, CanvasAnswer, CanvasBrowser, CanvasOpen, CanvasRequest, PipeEnds,
    SCREEN_PIPE, ScreenShown,
    attach::{Part, Uploads},
    browser::{self, Browser, Places, Terminal},
    engine::BrowserHost,
    files::{self, Files, Showing, Stamp, key_of},
    render,
    screen::ScreenHost,
};
use crate::documents::{DocumentAction, DocumentHost, RenderArguments};
use crate::text::Text;

const JOBS: usize = 64;
const CHECK: Duration = Duration::from_secs(1);

type Reply = oneshot::Sender<Result<CanvasAnswer, Text>>;

#[derive(Clone)]
pub(crate) struct CanvasServices {
    pub(crate) documents: DocumentHost,
    pub(crate) browsers: BrowserHost,
    pub(crate) agent_browser: PathBuf,
    pub(crate) screen: ScreenHost,
}

impl CanvasServices {
    pub(crate) fn new(data_root: &Path, isolated: bool) -> Self {
        Self {
            documents: DocumentHost::new(data_root),
            browsers: BrowserHost::new(data_root, isolated),
            agent_browser: browser::agent_browser(),
            screen: ScreenHost::new(data_root),
        }
    }

    pub(crate) async fn close(&self) {
        self.documents.close().await;
        self.browsers.close().await;
        self.screen.saved().await;
    }
}

enum Job {
    Show {
        show: Show,
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
    LastTabClosed {
        browser: Weak<Browser>,
    },
    Edited {
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

pub(crate) struct Show {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) address: Option<String>,
    pub(crate) browser_as_it_is: bool,
    pub(crate) screen: Option<ScreenShown>,
    pub(crate) folder: PathBuf,
    pub(crate) terminal: String,
    pub(crate) agent: Option<String>,
}

#[derive(Clone)]
pub(crate) struct CanvasHandle {
    jobs: mpsc::Sender<Job>,
    state: watch::Receiver<Canvas>,
    page: watch::Receiver<Option<String>>,
    folder: watch::Sender<PathBuf>,
    shared: watch::Sender<bool>,
    documents: DocumentHost,
    screen: ScreenHost,
}

impl CanvasHandle {
    pub(crate) fn spawn(id: Uuid, data_root: &Path, services: CanvasServices) -> Self {
        let (jobs, receiver) = mpsc::channel(JOBS);
        let (published, state) = watch::channel(Canvas::default());
        let (last_page, page) = watch::channel(None);
        let (folder, current_folder) = watch::channel(PathBuf::new());
        let (shared, sharing) = watch::channel(false);
        let actor = Actor {
            id,
            data_root: data_root.to_owned(),
            jobs: receiver,
            published,
            page: last_page,
            stamp: Stamp::default(),
            files: Files::default(),
            uploads: Uploads::default(),
            folder: current_folder,
            own_jobs: jobs.downgrade(),
            sharing,
            documents: services.documents.clone(),
            browsers: services.browsers,
            agent_browser: services.agent_browser,
            browser: None,
            screen: services.screen.clone(),
            allowed: services.screen.allowed(),
            screen_shown: false,
            open: None,
            change: None,
        };
        tokio::spawn(actor.run());
        Self {
            jobs,
            state,
            page,
            folder,
            shared,
            documents: services.documents,
            screen: services.screen,
        }
    }

    /// Shared terminals switch to an isolated browser profile so they do not expose the user's
    /// saved sign-ins.
    pub(crate) fn set_shared(&self, shared: bool) {
        self.shared.send_if_modified(|current| {
            let changed = *current != shared;
            *current = shared;
            changed
        });
    }

    pub(crate) fn moved_to(&self, folder: PathBuf) {
        self.folder.send_replace(folder);
    }

    pub(crate) fn state(&self) -> watch::Receiver<Canvas> {
        self.state.clone()
    }

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

    pub(crate) async fn show(&self, show: Show) -> Result<CanvasAnswer, Text> {
        self.ask(|reply| Job::Show { show, reply }).await
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

    pub(crate) async fn document(
        &self,
        key: &str,
        view: String,
        person: String,
        name: String,
        local: bool,
    ) -> Result<PipeEnds, Text> {
        if key == SCREEN_PIPE {
            if !self.state.borrow().screen {
                return Err(text!("This computer's screen is not on the canvas."));
            }
            let viewer = shared_browser::Viewer {
                id: view,
                person,
                name,
                local,
            };
            return self.screen.watch(viewer).await;
        }
        if key == BROWSER_PIPE {
            let (reply, answer) = oneshot::channel();
            let viewer = shared_browser::Viewer {
                id: view,
                person,
                name,
                local,
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
        self.documents.open(Path::new(key), viewer).await
    }

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
        let command = match crate::documents::parse(arguments) {
            Ok(DocumentAction::Edit(command)) => command.rooted(&folder),
            Ok(DocumentAction::Render(parsed)) => {
                return self.render(viewer, &folder, parsed).await;
            }
            Err(usage) => {
                return CanvasAnswer::Text {
                    text: usage.render().to_string(),
                    code: usage.exit_code(),
                };
            }
        };
        let edited = match &command {
            Command::Apply(apply) => Some(apply.file.clone()),
            _ => None,
        };
        let answer = self.documents.run(&viewer, command).await;
        if let (Some(path), CanvasAnswer::Text { code: 0, .. }) = (edited, &answer) {
            drop(self.jobs.try_send(Job::Edited { path }));
        }
        answer
    }

    async fn render(
        &self,
        viewer: documents::Viewer,
        folder: &Path,
        parsed: RenderArguments,
    ) -> CanvasAnswer {
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

    pub(crate) fn address(&self) -> Option<String> {
        self.page.borrow().clone()
    }

    pub(crate) fn restore(&self, paths: Vec<PathBuf>, address: Option<String>, from: Uuid) {
        drop(self.jobs.try_send(Job::Restore {
            paths,
            address,
            from,
        }));
    }

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
    page: watch::Sender<Option<String>>,
    stamp: Stamp,
    files: Files,
    uploads: Uploads,
    folder: watch::Receiver<PathBuf>,
    own_jobs: mpsc::WeakSender<Job>,
    sharing: watch::Receiver<bool>,
    documents: DocumentHost,
    browsers: BrowserHost,
    agent_browser: PathBuf,
    browser: Option<(Arc<Browser>, watch::Receiver<CanvasBrowser>)>,
    screen: ScreenHost,
    allowed: watch::Receiver<bool>,
    screen_shown: bool,
    open: Option<CanvasOpen>,
    change: Option<CanvasOpen>,
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
                        tokio::spawn(async move { browser.change_profile().await });
                    }
                }
                changed = self.allowed.changed() => {
                    if changed.is_ok() && !*self.allowed.borrow_and_update() {
                        self.screen_shown = false;
                        self.publish();
                    }
                }
                _ = check.tick(), if !self.files.is_empty() => {
                    let checked = self.files.check(&self.stamp).await;
                    if let Some(key) = checked.changed.into_iter().next() {
                        self.change = Some(self.mark(key));
                    }
                    if checked.any {
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
        self.page.send_replace(
            self.browser
                .as_ref()
                .and_then(|(browser, _)| browser.page()),
        );
        let shows = |mark: &&CanvasOpen| match mark.key.as_str() {
            BROWSER_PIPE => self.browser.is_some(),
            SCREEN_PIPE => self.screen_shown,
            key => self.files.contains(key),
        };
        let canvas = Canvas {
            sequence: self.stamp.now(),
            items: self.files.items(),
            browser: self
                .browser
                .as_ref()
                .map(|(_, state)| state.borrow().clone()),
            screen: self.screen_shown,
            open: self.open.as_ref().filter(shows).cloned(),
            change: self.change.as_ref().filter(shows).cloned(),
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
            Job::Show { show, reply } => self.show(show, reply).await,
            Job::Browse {
                folder,
                arguments,
                terminal,
                agent,
                reply,
            } => {
                let starts = self.browser.is_none();
                if let Some(missing) = browser::missing(&self.agent_browser).filter(|_| starts) {
                    drop(reply.send(Err(missing)));
                    return;
                }
                let browser = self.browser();
                self.change = Some(self.mark(BROWSER_PIPE.to_owned()));
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
            // Views reconnect their pipe after disconnects (including closing the last tab), so
            // only attach to an already-open browser instead of starting a new one.
            Job::Watch { viewer, reply } => match self.existing() {
                Some(browser) => {
                    tokio::spawn(async move { drop(reply.send(browser.watch(viewer).await)) });
                }
                None => drop(reply.send(Err(text!("The shared browser is closed.")))),
            },
            Job::Downloaded { path } => {
                match files::examine_all(vec![path]).await {
                    Ok(files) => drop(self.files.show(files, &self.stamp)),
                    Err(problem) => {
                        tracing::debug!(%problem, "the canvas does not show this download");
                    }
                }
                self.publish();
            }
            Job::LastTabClosed { browser } => self.last_tab_closed(&browser),
            Job::Edited { path } => {
                let key = key_of(&kodosi_os::fs::canonicalize(&path).unwrap_or(path));
                if self.files.contains(&key) {
                    self.change = Some(self.mark(key));
                    self.publish();
                }
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

    async fn show(&mut self, show: Show, reply: Reply) {
        let Show {
            paths,
            address,
            browser_as_it_is,
            screen,
            folder,
            terminal,
            agent,
        } = show;
        if (address.is_some() || browser_as_it_is)
            && self.browser.is_none()
            && let Some(missing) = browser::missing(&self.agent_browser)
        {
            drop(reply.send(Err(missing)));
            return;
        }
        let screen_line = match self.show_screen(screen, &terminal).await {
            Ok(line) => line,
            Err(problem) => {
                drop(reply.send(Err(problem)));
                return;
            }
        };
        let can_browse = self.browser.is_some() || browser::missing(&self.agent_browser).is_none();
        let examined = match files::examine_all(paths).await {
            Ok(examined) => examined,
            Err(problem) => {
                drop(reply.send(Err(problem)));
                return;
            }
        };
        let (pages, kept): (Vec<_>, Vec<_>) = examined
            .into_iter()
            .partition(|file| can_browse && file.loads.is_some() && file.address().is_some());
        let showing = self.files.show(kept, &self.stamp);
        let mut lines = shown(&showing, &terminal);
        let screen = screen_line.is_some();
        lines.extend(screen_line);
        let mut addresses = Vec::from_iter(address);
        for page in pages {
            lines.push(format!(
                "{} loads {} from the network, so it opens in the shared browser.",
                page.name(),
                page.loads.as_deref().unwrap_or_default()
            ));
            addresses.extend(page.address());
        }
        if browser_as_it_is && addresses.is_empty() {
            let at = self
                .browser()
                .page()
                .map_or_default(|address| format!(" at {address}"));
            lines.insert(
                0,
                format!(
                    "The canvas of {terminal} shows its browser{at}, and it opens for the people who look at this terminal."
                ),
            );
        }
        let browser = (!addresses.is_empty()).then(|| self.browser());
        let key = if browser.is_some() || browser_as_it_is {
            Some(BROWSER_PIPE.to_owned())
        } else if screen {
            Some(SCREEN_PIPE.to_owned())
        } else {
            showing.shown.first().map(|shown| shown.key.clone())
        };
        if let Some(key) = key {
            self.open = Some(self.mark(key));
        }
        self.publish();
        let Some(browser) = browser else {
            drop(reply.send(Ok(CanvasAnswer::Text {
                text: lines.join("\n"),
                code: 0,
            })));
            return;
        };
        tokio::spawn(async move {
            let opened = open_addresses(&browser, addresses, &folder, &terminal, agent, lines);
            drop(reply.send(opened.await));
        });
    }

    async fn show_screen(
        &mut self,
        screen: Option<ScreenShown>,
        terminal: &str,
    ) -> Result<Option<String>, Text> {
        let Some(screen) = screen else {
            return Ok(None);
        };
        let line = shown_screen(&screen, terminal);
        self.screen.show(screen).await?;
        self.screen_shown = true;
        Ok(Some(line))
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
                let screen = keys.iter().any(|key| key == SCREEN_PIPE);
                if screen {
                    self.screen_shown = false;
                }
                let files = keys
                    .into_iter()
                    .filter(|key| key != BROWSER_PIPE && key != SCREEN_PIPE)
                    .collect::<Vec<_>>();
                if !((browser || screen) && files.is_empty()) {
                    self.files.remove(&files);
                }
                self.publish();
                drop(reply.send(Ok(CanvasAnswer::Done)));
            }
            CanvasRequest::ShowScreen => {
                let answer = self.screen.check().map(|()| CanvasAnswer::Done);
                if answer.is_ok() {
                    self.screen_shown = true;
                    self.publish();
                }
                drop(reply.send(answer));
            }
            CanvasRequest::Trash { key } => {
                let answer = self.trash(key).await;
                drop(reply.send(answer));
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

    /// Remove the item and publish state before closing the document so client views detach first.
    async fn trash(&mut self, key: String) -> Result<CanvasAnswer, Text> {
        let (path, name) = self
            .files
            .take(&key)
            .ok_or_else(|| text!("This item is no longer on the canvas."))?;
        self.publish();
        self.documents.close_for_removal(&path).await;
        let moved = tokio::task::spawn_blocking(move || kodosi_os::fs::move_to_trash(&path))
            .await
            .unwrap_or_else(|stopped| Err(std::io::Error::other(stopped)));
        if let Err(problem) = moved {
            tracing::warn!(%problem, "a file of the canvas did not go to the trash");
            return Err(text!(
                "{name} could not be moved to the trash, so it stays in its folder.",
                name = &name
            ));
        }
        Ok(CanvasAnswer::Done)
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
        let showing = files::examine_all(vec![path])
            .await
            .map(|files| self.files.show(files, &self.stamp));
        self.publish();
        showing.map(|_| CanvasAnswer::Attached { key })
    }

    fn last_tab_closed(&mut self, closed: &Weak<Browser>) {
        let current = self
            .browser
            .as_ref()
            .is_some_and(|(open, _)| Weak::as_ptr(closed) == Arc::as_ptr(open));
        if current && let Some((browser, _)) = self.browser.take() {
            tokio::spawn(async move {
                browser.end().await;
                browser.forget_pages().await;
            });
            self.publish();
        }
    }

    fn existing(&self) -> Option<Arc<Browser>> {
        self.browser
            .as_ref()
            .map(|(browser, _)| Arc::clone(browser))
    }

    fn mark(&self, key: String) -> CanvasOpen {
        let at = [&self.open, &self.change]
            .into_iter()
            .flatten()
            .map(|mark| mark.at + 1)
            .fold(crate::identity::now_ms(), u64::max);
        CanvasOpen { key, at }
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
                last_tab_closed: {
                    let jobs = self.own_jobs.clone();
                    Box::new(move |browser| {
                        if let Some(jobs) = jobs.upgrade() {
                            drop(jobs.try_send(Job::LastTabClosed { browser }));
                        }
                    })
                },
            },
            Places::of(&self.data_root, self.agent_browser.clone()),
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
                browser.end().await;
            }
        } else if remove {
            let profile = browser::own_profile(&self.data_root, self.id);
            drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(profile)).await);
        }
    }
}

fn pictures(data_root: &Path, id: Uuid) -> PathBuf {
    data_root.join("render").join(id.to_string())
}

/// Opens the first address in the shared browser's tab and the others in new tabs, and answers
/// with the lines so far and what each run said.
async fn open_addresses(
    browser: &Arc<Browser>,
    addresses: Vec<String>,
    folder: &Path,
    terminal: &str,
    agent: Option<String>,
    mut lines: Vec<String>,
) -> Result<CanvasAnswer, Text> {
    let mut code = 0;
    for (index, address) in addresses.into_iter().enumerate() {
        let arguments = if index == 0 {
            vec!["open".to_owned(), address]
        } else {
            vec!["tab".to_owned(), "new".to_owned(), address]
        };
        if let CanvasAnswer::Text { text, code: run } = browser
            .run(folder.to_owned(), arguments, terminal, agent.clone())
            .await?
        {
            lines.push(text.trim_end().to_owned());
            code = code.max(run);
        }
    }
    lines.retain(|line| !line.is_empty());
    Ok(CanvasAnswer::Text {
        text: lines.join("\n"),
        code,
    })
}

fn shown_screen(screen: &ScreenShown, terminal: &str) -> String {
    match screen {
        ScreenShown::Screen => {
            format!("The canvas of {terminal} shows this computer's screen to its owner's devices.")
        }
        ScreenShown::Window { words } => format!(
            "The canvas of {terminal} shows the window on this computer whose app or title holds \u{201c}{words}\u{201d}, or the whole screen when none does, to its owner's devices."
        ),
    }
}

fn shown(showing: &Showing, terminal: &str) -> Vec<String> {
    let mut lines = Vec::new();
    if !showing.shown.is_empty() {
        let names = showing
            .shown
            .iter()
            .map(|shown| shown.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "The canvas of {terminal} shows {names}, and it opens for the people who look at this terminal."
        ));
    }
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
    lines
}

/// Deletes orphaned browser profiles and render directories on a background thread (profiles can
/// contain thousands of files). Safe to run concurrently with startup because callers invoke it
/// before terminals start and new terminals use fresh UUIDs.
pub(crate) fn keep_profiles(data_root: &Path, returning: &HashSet<Uuid>) {
    let mut left = browser::left_profiles(data_root, returning);
    left.extend(
        std::fs::read_dir(data_root.join("render"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path()),
    );
    if left.is_empty() {
        return;
    }
    drop(
        std::thread::Builder::new()
            .name("kodosi-profiles".into())
            .spawn(move || {
                for folder in left {
                    drop(std::fs::remove_dir_all(folder));
                }
            }),
    );
}

pub(crate) async fn close_left(data_root: &Path) {
    browser::close_left(data_root).await;
}

fn busy() -> Text {
    tracing::warn!("the queue of a canvas is full, so it refuses a step");
    text!("The canvas is busy. Try again in a moment.")
}
