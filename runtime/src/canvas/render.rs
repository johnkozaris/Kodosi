use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::Engine as _;
use clap::Parser;
use documents::Viewer;
use serde::Deserialize;
use serde_json::json;
use shared_browser::HostPage;

use super::{CanvasAnswer, DocumentView, PipeEnds, engine::BrowserHost, view_of};
use crate::documents::DocumentHost;
use crate::text::Text;

const RENDER_LIMIT: Duration = Duration::from_mins(2);
const MOST_PICTURES: usize = 300;

#[derive(Parser)]
#[command(name = "kodosi doc render")]
pub(super) struct RenderArguments {
    /// A document, sheet, presentation, PDF or text file.
    pub file: PathBuf,
    /// The folder of the pictures. Without it, they go to a private folder of the terminal.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

/// A place in the page, in CSS pixels from the top left of the document.
#[derive(Deserialize)]
struct Area {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

trait Page: Send {
    fn run(&mut self, script: &str) -> impl Future<Output = Result<(), Text>> + Send;
    fn message(&mut self) -> impl Future<Output = Option<String>> + Send;
    fn hold(&mut self, width: f64, height: f64) -> impl Future<Output = Result<bool, Text>> + Send;
    fn picture(&self, area: &Area) -> impl Future<Output = Result<Vec<u8>, Text>> + Send;
}

impl Page for HostPage {
    async fn run(&mut self, script: &str) -> Result<(), Text> {
        Self::run(self, script)
            .await
            .map(drop)
            .map_err(|problem| Text::plain(problem.to_string()))
    }

    async fn message(&mut self) -> Option<String> {
        Self::message(self).await
    }

    async fn hold(&mut self, width: f64, height: f64) -> Result<bool, Text> {
        Self::hold(self, width, height)
            .await
            .map_err(|problem| Text::plain(problem.to_string()))
    }

    async fn picture(&self, area: &Area) -> Result<Vec<u8>, Text> {
        Self::picture(self, area.x, area.y, area.width, area.height)
            .await
            .map_err(|problem| Text::plain(problem.to_string()))
    }
}

const fn page_size(view: DocumentView) -> (u32, u32) {
    match view {
        DocumentView::Docx | DocumentView::Pdf | DocumentView::Text => (1100, 900),
        DocumentView::Xlsx => (1400, 900),
        DocumentView::Pptx => (1440, 900),
    }
}

fn views() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    let folder = program.parent()?;
    [
        folder.join("DocumentViews"),
        folder.parent()?.join("Resources").join("DocumentViews"),
    ]
    .into_iter()
    .find(|views| views.join("index.html").is_file())
}

fn script(view: &str) -> String {
    format!(
        r#"(() => {{
  let views;
  Object.defineProperty(window, "documentView", {{
    configurable: true,
    get: () => views,
    set(value) {{
      views = value;
      const say = (word, text) => kodosiHost(text === undefined ? word : `${{word}} ${{text}}`);
      const failed = (error) => say("error", String(error?.message ?? error));
      const host = value.hosted(
        {{ kind: {view}, theme: {{ scheme: "light" }}, render: true, ready: () => say("ready") }},
        (frame) => say("frame", frame),
      );
      window.kodosiRender = {{
        deliver: host.deliver,
        pictures: () => host.pictures().then((count) => say("pictures", count), failed),
        picture: (index) =>
          host.picture(index).then((area) => say("picture", JSON.stringify(area)), failed),
      }};
      host.attach();
    }},
  }});
}})();"#,
        view = json!(view),
    )
}

fn picture_names(file: &Path, count: usize) -> Vec<String> {
    let stem = file
        .file_stem()
        .map_or_else(|| "page".into(), |stem| stem.to_string_lossy());
    (1..=count)
        .map(|number| format!("{stem}-{number}.png"))
        .collect()
}

pub(super) async fn render(
    host: BrowserHost,
    profile: String,
    documents: DocumentHost,
    viewer: Viewer,
    file: PathBuf,
    out: PathBuf,
) -> Result<CanvasAnswer, Text> {
    let view = view_of(&file).ok_or_else(|| {
        Text::plain(
            "kodosi doc render draws .docx, .xlsx, .pptx, .pdf, .txt, .json, .yaml, .toml and .xml files.",
        )
    })?;
    let views =
        views().ok_or_else(|| Text::plain("This Kodosi has no document views to draw with."))?;
    let browsers = host.browsers(|_| {}).await?;
    let mut page = browsers
        .host_page(&profile, views, page_size(view), &script(view.name()))
        .await
        .map_err(|problem| Text::plain(problem.to_string()))?;
    let drawn = tokio::time::timeout(
        RENDER_LIMIT,
        draw(&mut page, &documents, viewer, &file, &out),
    )
    .await;
    page.close().await;
    let saved = drawn.map_err(|_| Text::plain("The pictures took too long."))??;
    let mut text = String::new();
    for path in &saved {
        let _ = writeln!(text, "{}", path.display());
    }
    Ok(CanvasAnswer::Text { text, code: 0 })
}

async fn draw(
    page: &mut HostPage,
    documents: &DocumentHost,
    viewer: Viewer,
    file: &Path,
    out: &Path,
) -> Result<Vec<PathBuf>, Text> {
    let mut bridge = Bridge {
        page,
        pipe: documents.open(file, viewer).await?,
    };
    bridge.answer("ready").await?;
    bridge.save(file, out).await
}

struct Bridge<'a, P> {
    page: &'a mut P,
    pipe: PipeEnds,
}

enum Step {
    Said(Option<String>),
    Sent(Option<Vec<u8>>),
}

impl<P: Page> Bridge<'_, P> {
    /// A view may show one page at a time, as a presentation shows one slide, so each picture
    /// follows the view's answer for its page.
    async fn save(&mut self, file: &Path, out: &Path) -> Result<Vec<PathBuf>, Text> {
        let mut count = self.count().await?;
        let (mut width, mut height) = (0.0_f64, 0.0_f64);
        for index in 0..count {
            let area = self.area(index).await?;
            width = width.max(area.width);
            height = height.max(area.height);
        }
        if self.page.hold(width, height).await? {
            count = self.count().await?;
        }
        tokio::fs::create_dir_all(out)
            .await
            .map_err(|problem| Text::plain(problem.to_string()))?;
        let mut saved = Vec::new();
        for (index, name) in picture_names(file, count).into_iter().enumerate() {
            let area = self.area(index).await?;
            let picture = self.page.picture(&area).await?;
            let path = out.join(name);
            tokio::fs::write(&path, picture)
                .await
                .map_err(|problem| Text::plain(problem.to_string()))?;
            saved.push(path);
        }
        Ok(saved)
    }

    async fn count(&mut self) -> Result<usize, Text> {
        self.ask("kodosiRender.pictures()").await?;
        let count: usize = self
            .answer("pictures")
            .await?
            .parse()
            .map_err(|_| Text::plain("The view gave no pictures."))?;
        Ok(count.min(MOST_PICTURES))
    }

    async fn area(&mut self, index: usize) -> Result<Area, Text> {
        self.ask(&format!("kodosiRender.picture({index})")).await?;
        serde_json::from_str(&self.answer("picture").await?)
            .map_err(|_| Text::plain("The view gave no place for a picture."))
    }

    async fn ask(&mut self, script: &str) -> Result<(), Text> {
        self.page.run(script).await
    }

    async fn answer(&mut self, word: &str) -> Result<String, Text> {
        loop {
            let step = tokio::select! {
                message = self.page.message() => Step::Said(message),
                frame = self.pipe.receiver.recv() => Step::Sent(frame),
            };
            match step {
                Step::Said(message) => {
                    let message = message.ok_or_else(|| Text::plain("The browser stopped."))?;
                    let (head, rest) = message.split_once(' ').unwrap_or((&message, ""));
                    match head {
                        "frame" => {
                            let frame = base64::engine::general_purpose::STANDARD
                                .decode(rest)
                                .map_err(|_| Text::plain("The view sent a broken frame."))?;
                            self.pipe
                                .sender
                                .send(frame)
                                .await
                                .map_err(|_| Text::plain("The document closed."))?;
                        }
                        "error" => return Err(Text::plain(rest.to_owned())),
                        _ if head == word => return Ok(rest.to_owned()),
                        _ => {}
                    }
                }
                Step::Sent(frame) => {
                    let frame = frame.ok_or_else(|| Text::plain("The document closed."))?;
                    let data = base64::engine::general_purpose::STANDARD.encode(frame);
                    self.ask(&format!("kodosiRender.deliver({})", json!(data)))
                        .await?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_of_file_gets_its_view() {
        assert_eq!(
            view_of(Path::new("plan.DOCX")).map(DocumentView::name),
            Some("docx")
        );
        assert_eq!(
            view_of(Path::new("a/b.pdf")).map(DocumentView::name),
            Some("pdf")
        );
        assert_eq!(
            view_of(Path::new("config.YML")).map(DocumentView::name),
            Some("text")
        );
        assert_eq!(view_of(Path::new("notes.md")), None);
    }

    #[test]
    fn pictures_take_the_name_of_their_file() {
        assert_eq!(
            picture_names(Path::new("/work/plan.docx"), 2),
            ["plan-1.png", "plan-2.png"]
        );
    }

    #[test]
    fn the_script_names_the_view_as_text() {
        assert!(script("xlsx").contains(r#"kind: "xlsx""#));
    }

    #[derive(Default)]
    struct Slides {
        shown: u8,
        said: std::collections::VecDeque<String>,
    }

    impl Page for Slides {
        fn run(&mut self, script: &str) -> impl Future<Output = Result<(), Text>> + Send {
            if script == "kodosiRender.pictures()" {
                self.said.push_back("pictures 3".to_owned());
            } else if let Some(index) = script
                .strip_prefix("kodosiRender.picture(")
                .and_then(|rest| rest.strip_suffix(')'))
            {
                self.shown = index.parse().unwrap();
                let area = r#"{"x":0,"y":0,"width":160,"height":90}"#;
                self.said.push_back(format!("picture {area}"));
            }
            std::future::ready(Ok(()))
        }

        fn message(&mut self) -> impl Future<Output = Option<String>> + Send {
            std::future::ready(self.said.pop_front())
        }

        fn hold(
            &mut self,
            _width: f64,
            _height: f64,
        ) -> impl Future<Output = Result<bool, Text>> + Send {
            std::future::ready(Ok(false))
        }

        fn picture(&self, _area: &Area) -> impl Future<Output = Result<Vec<u8>, Text>> + Send {
            std::future::ready(Ok(vec![self.shown]))
        }
    }

    #[tokio::test]
    async fn each_picture_shows_its_own_slide() {
        let out = tempfile::tempdir().unwrap();
        let (sender, _sent) = tokio::sync::mpsc::channel(1);
        let (_document, receiver) = tokio::sync::mpsc::channel(1);
        let mut slides = Slides::default();
        let mut bridge = Bridge {
            page: &mut slides,
            pipe: PipeEnds { sender, receiver },
        };
        let saved = bridge
            .save(Path::new("deck.pptx"), out.path())
            .await
            .unwrap();
        let pictures: Vec<Vec<u8>> = saved
            .iter()
            .map(|path| std::fs::read(path).unwrap())
            .collect();
        assert_eq!(pictures, [[0], [1], [2]]);
    }
}
