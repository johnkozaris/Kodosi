use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

use super::{
    BrowserAct, Canvas, CanvasAnswer, CanvasBrowser, CanvasRequest,
    browser::{self, Browser, Places},
    files::{Files, Showing, Stamp},
};

const JOBS: usize = 64;
const CHECK: Duration = Duration::from_secs(1);
const BROWSER: &str = "browser";

type Reply = oneshot::Sender<Result<CanvasAnswer, String>>;

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
    Restore {
        paths: Vec<PathBuf>,
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
}

impl CanvasHandle {
    pub(crate) fn spawn(id: Uuid, data_root: &Path) -> Self {
        let (jobs, receiver) = mpsc::channel(JOBS);
        let (published, state) = watch::channel(Canvas::default());
        let actor = Actor {
            id,
            data_root: data_root.to_owned(),
            jobs: receiver,
            published,
            stamp: Stamp::default(),
            files: Files::default(),
            browser: None,
        };
        tokio::spawn(actor.run());
        Self { jobs, state }
    }

    pub(crate) fn state(&self) -> watch::Receiver<Canvas> {
        self.state.clone()
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
    ) -> Result<CanvasAnswer, String> {
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
    ) -> Result<CanvasAnswer, String> {
        self.ask(|reply| Job::Browse {
            folder,
            arguments,
            terminal,
            agent,
            reply,
        })
        .await
    }

    pub(crate) async fn request(&self, request: CanvasRequest) -> Result<CanvasAnswer, String> {
        self.ask(|reply| Job::Request { request, reply }).await
    }

    /// Brings back the files and the browser of the terminal that this one replaces.
    pub(crate) fn restore(&self, paths: Vec<PathBuf>, from: Uuid) {
        drop(self.jobs.try_send(Job::Restore { paths, from }));
    }

    /// Closes the browser. `remove` also deletes its profile, because the terminal does not
    /// come back.
    pub(crate) async fn end(&self, remove: bool) {
        let (reply, done) = oneshot::channel();
        if self.jobs.send(Job::End { remove, reply }).await.is_ok() {
            drop(done.await);
        }
    }

    async fn ask(&self, job: impl FnOnce(Reply) -> Job) -> Result<CanvasAnswer, String> {
        let (reply, answer) = oneshot::channel();
        self.jobs
            .try_send(job(reply))
            .map_err(|_| "The canvas is busy. Try again in a moment.".to_owned())?;
        answer
            .await
            .unwrap_or_else(|_| Err("The terminal of this canvas closed.".to_owned()))
    }
}

struct Actor {
    id: Uuid,
    data_root: PathBuf,
    jobs: mpsc::Receiver<Job>,
    published: watch::Sender<Canvas>,
    stamp: Stamp,
    files: Files,
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
                    drop(reply.send(Err(missing.to_owned())));
                    return;
                }
                let browser = self.browser();
                self.publish();
                tokio::spawn(async move {
                    drop(reply.send(browser.run(folder, arguments, &terminal, agent).await));
                });
            }
            Job::Request { request, reply } => self.request(request, reply).await,
            Job::Restore { paths, from } => {
                self.files.restore(paths, &self.stamp);
                let earlier = Places::of(&self.data_root, from).profile;
                let profile = Places::of(&self.data_root, self.id).profile;
                let moved = tokio::task::spawn_blocking(move || {
                    earlier.is_dir()
                        && !profile.exists()
                        && std::fs::rename(&earlier, &profile).is_ok()
                })
                .await
                .unwrap_or(false);
                if moved {
                    self.browser();
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
                let browser = keys.iter().any(|key| key == BROWSER);
                if browser && let Some((browser, _)) = self.browser.take() {
                    tokio::spawn(async move { browser.remove().await });
                }
                let files = keys
                    .into_iter()
                    .filter(|key| key != BROWSER)
                    .collect::<Vec<_>>();
                if !(browser && files.is_empty()) {
                    self.files.remove(&files);
                }
                self.publish();
                drop(reply.send(Ok(CanvasAnswer::Done)));
            }
            CanvasRequest::Picture { after } => {
                let Some(browser) = self.existing() else {
                    drop(reply.send(Err(NO_BROWSER.to_owned())));
                    return;
                };
                tokio::spawn(async move { drop(reply.send(browser.picture(after).await)) });
            }
            CanvasRequest::Input {
                events,
                width,
                height,
            } => {
                let Some(browser) = self.existing() else {
                    drop(reply.send(Err(NO_BROWSER.to_owned())));
                    return;
                };
                tokio::spawn(async move {
                    drop(reply.send(browser.input(events, width, height).await));
                });
            }
            CanvasRequest::Act { action } => {
                let Some(browser) = self.existing() else {
                    drop(reply.send(Err(NO_BROWSER.to_owned())));
                    return;
                };
                let waking = matches!(action, BrowserAct::Wake { .. });
                tokio::spawn(async move { drop(reply.send(browser.act(action).await)) });
                if waking {
                    self.publish();
                }
            }
        }
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
            self.id,
            Places::of(&self.data_root, self.id),
            self.stamp.clone(),
        );
        let state = browser.state();
        self.browser = Some((Arc::clone(&browser), state));
        browser
    }

    async fn end(&mut self, remove: bool) {
        if let Some((browser, _)) = self.browser.take() {
            if remove {
                browser.remove().await;
            } else {
                browser.sleep().await;
            }
        } else if remove {
            let profile = Places::of(&self.data_root, self.id).profile;
            drop(tokio::task::spawn_blocking(move || std::fs::remove_dir_all(profile)).await);
        }
    }
}

const NO_BROWSER: &str = "This canvas has no browser. The agent opens one with kodosi browser.";

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

/// Deletes the browser profiles of the terminals that do not come back.
pub(crate) fn keep_profiles(data_root: &Path, returning: &HashSet<Uuid>) {
    for profile in browser::left_profiles(data_root, returning) {
        drop(std::fs::remove_dir_all(profile));
    }
}

/// Closes the browsers that an earlier host left, for example after a crash.
pub(crate) async fn close_left(data_root: &Path) {
    browser::close_left(data_root).await;
}
