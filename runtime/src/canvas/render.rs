//! `kodosi doc render`: the terminal's browser draws a live document with the views that people
//! see, and the host saves one picture of each page.

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

use super::{CanvasAnswer, PipeEnds, engine::BrowserHost};
use crate::documents::DocumentHost;

/// How long the pictures of one file may take, once the browser runs.
const RENDER_LIMIT: Duration = Duration::from_mins(2);
const MOST_PICTURES: usize = 300;

#[derive(Parser)]
#[command(name = "kodosi doc render")]
pub(super) struct RenderArguments {
    /// A Word, Excel, `PowerPoint` or PDF file.
    pub file: PathBuf,
    /// The folder of the pictures. Without it, they go to a folder of temporary files.
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

/// The view that draws a file, and the size of the page that it draws in.
fn view_of(file: &Path) -> Option<(&'static str, (u32, u32))> {
    let extension = file.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "docx" => Some(("word", (1100, 900))),
        "xlsx" => Some(("excel", (1400, 900))),
        "pptx" => Some(("powerpoint", (1440, 900))),
        "pdf" => Some(("pdf", (1100, 900))),
        _ => None,
    }
}

/// The views that the app ships: next to the kodosi program, or in the resources of the Mac
/// app.
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

/// Runs before the page's own scripts: when the views arrive, they mount for pictures, and they
/// talk with the host through its binding.
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

/// The file names of the pictures of a file.
fn picture_names(file: &Path, count: usize) -> Vec<String> {
    let stem = file
        .file_stem()
        .map_or_else(|| "page".into(), |stem| stem.to_string_lossy());
    (1..=count)
        .map(|number| format!("{stem}-{number}.png"))
        .collect()
}

/// Draws each page of a file and saves its picture in `out`.
pub(super) async fn render(
    host: BrowserHost,
    profile: String,
    documents: DocumentHost,
    viewer: Viewer,
    file: PathBuf,
    out: PathBuf,
) -> Result<CanvasAnswer, String> {
    let (view, size) =
        view_of(&file).ok_or("kodosi doc render draws Word, Excel, PowerPoint and PDF files.")?;
    let views = views().ok_or("This Kodosi has no document views to draw with.")?;
    let browsers = host.browsers(|_| {}).await?;
    let mut page = browsers
        .host_page(&profile, views, size, &script(view))
        .await
        .map_err(|problem| problem.to_string())?;
    let drawn = tokio::time::timeout(
        RENDER_LIMIT,
        draw(&mut page, &documents, viewer, &file, &out),
    )
    .await;
    page.close().await;
    let saved = drawn.map_err(|_| "The pictures took too long.".to_owned())??;
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
) -> Result<Vec<PathBuf>, String> {
    let mut bridge = Bridge {
        page,
        pipe: documents.open(file, viewer).await?,
    };
    bridge.answer("ready").await?;
    bridge.ask("kodosiRender.pictures()").await?;
    let count: usize = bridge
        .answer("pictures")
        .await?
        .parse()
        .map_err(|_| "The view gave no pictures.".to_owned())?;
    tokio::fs::create_dir_all(out)
        .await
        .map_err(|problem| problem.to_string())?;
    let mut saved = Vec::new();
    for (index, name) in picture_names(file, count.min(MOST_PICTURES))
        .into_iter()
        .enumerate()
    {
        bridge
            .ask(&format!("kodosiRender.picture({index})"))
            .await?;
        let area: Area = serde_json::from_str(&bridge.answer("picture").await?)
            .map_err(|_| "The view gave no place for a picture.".to_owned())?;
        let picture = bridge
            .page
            .picture(area.x, area.y, area.width, area.height)
            .await
            .map_err(|problem| problem.to_string())?;
        let path = out.join(name);
        tokio::fs::write(&path, picture)
            .await
            .map_err(|problem| problem.to_string())?;
        saved.push(path);
    }
    Ok(saved)
}

/// The page of the views on one side and the document on the other.
struct Bridge<'a> {
    page: &'a mut HostPage,
    pipe: PipeEnds,
}

enum Step {
    Said(Option<String>),
    Sent(Option<Vec<u8>>),
}

impl Bridge<'_> {
    async fn ask(&self, script: &str) -> Result<(), String> {
        self.page
            .run(script)
            .await
            .map(drop)
            .map_err(|problem| problem.to_string())
    }

    /// Carries frames both ways until the page says a message that starts with `word`.
    async fn answer(&mut self, word: &str) -> Result<String, String> {
        loop {
            let step = tokio::select! {
                message = self.page.message() => Step::Said(message),
                frame = self.pipe.receiver.recv() => Step::Sent(frame),
            };
            match step {
                Step::Said(message) => {
                    let message = message.ok_or("The browser stopped.")?;
                    let (head, rest) = message.split_once(' ').unwrap_or((&message, ""));
                    match head {
                        "frame" => {
                            let frame = base64::engine::general_purpose::STANDARD
                                .decode(rest)
                                .map_err(|_| "The view sent a broken frame.".to_owned())?;
                            self.pipe
                                .sender
                                .send(frame)
                                .await
                                .map_err(|_| "The document closed.".to_owned())?;
                        }
                        "error" => return Err(rest.to_owned()),
                        _ if head == word => return Ok(rest.to_owned()),
                        _ => {}
                    }
                }
                Step::Sent(frame) => {
                    let frame = frame.ok_or("The document closed.")?;
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
            view_of(Path::new("plan.DOCX")).map(|(view, _)| view),
            Some("word")
        );
        assert_eq!(
            view_of(Path::new("a/b.pdf")).map(|(view, _)| view),
            Some("pdf")
        );
        assert_eq!(view_of(Path::new("notes.txt")), None);
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
        assert!(script("excel").contains(r#"kind: "excel""#));
    }
}
