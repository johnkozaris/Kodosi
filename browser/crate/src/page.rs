use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use base64::Engine as _;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    Error,
    devtools::{self, Devtools},
    engine::Engine,
    link::ToEngine,
};

/// The address of a host page's own files, which never reach the network.
const ORIGIN: &str = "https://kodosi.invalid/";
const BINDING: &str = "kodosiHost";
const MESSAGES: usize = 256;
const MOST_SIDE: u32 = 8192;
const RESIZE_LIMIT: Duration = Duration::from_secs(5);

pub struct HostPage {
    engine: Arc<Engine>,
    page: i64,
    target: String,
    session: String,
    messages: mpsc::Receiver<String>,
    width: u32,
    height: u32,
}

impl HostPage {
    pub(crate) async fn open(
        engine: Arc<Engine>,
        folder: PathBuf,
        width: u32,
        height: u32,
        script: &str,
    ) -> Result<Self, Error> {
        let opened = engine.open_page(width, height).await?;
        let (said, messages) = mpsc::channel(MESSAGES);
        tokio::spawn(serve(
            engine.devtools().clone(),
            opened.session.clone(),
            folder,
            opened.events,
            said,
        ));
        let page = Self {
            engine,
            page: opened.page,
            target: opened.target,
            session: opened.session,
            messages,
            width,
            height,
        };
        let pattern = json!({ "patterns": [{ "urlPattern": format!("{ORIGIN}*") }] });
        page.call("Fetch.enable", pattern).await?;
        page.call("Runtime.enable", json!({})).await?;
        page.call("Runtime.addBinding", json!({ "name": BINDING }))
            .await?;
        page.call(
            "Page.addScriptToEvaluateOnNewDocument",
            json!({ "source": script }),
        )
        .await?;
        page.call(
            "Page.navigate",
            json!({ "url": format!("{ORIGIN}index.html") }),
        )
        .await?;
        Ok(page)
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.engine
            .devtools()
            .call(method, params, Some(&self.session))
            .await
    }

    pub async fn message(&mut self) -> Option<String> {
        self.messages.recv().await
    }

    /// A script that waits for the page's files answers through the binding instead, since the
    /// page gets no files during the call.
    pub async fn run(&self, script: &str) -> Result<Value, Error> {
        let answer = self
            .call(
                "Runtime.evaluate",
                json!({ "expression": script, "returnByValue": true }),
            )
            .await?;
        if let Some(problem) = answer.get("exceptionDetails") {
            let text = problem["exception"]["description"]
                .as_str()
                .or_else(|| problem["text"].as_str())
                .unwrap_or("the script failed");
            return Err(Error::Engine(text.to_owned()));
        }
        Ok(answer["result"]["value"].clone())
    }

    /// Makes the view at least this large, since a picture reaches no farther than the view.
    /// Says whether the view grew, which can move what the page shows.
    pub async fn hold(&mut self, width: f64, height: f64) -> Result<bool, Error> {
        let wanted = (side(width)?.max(self.width), side(height)?.max(self.height));
        if wanted == (self.width, self.height) {
            return Ok(false);
        }
        (self.width, self.height) = wanted;
        self.engine.command(ToEngine::ResizePage {
            page: self.page,
            width: self.width,
            height: self.height,
        });
        let script = format!(
            "new Promise((done) => {{
               const check = () => innerWidth >= {} && innerHeight >= {} && done(true);
               addEventListener('resize', check);
               check();
             }})",
            self.width, self.height
        );
        let resized = self.call(
            "Runtime.evaluate",
            json!({ "expression": script, "awaitPromise": true }),
        );
        match tokio::time::timeout(RESIZE_LIMIT, resized).await {
            Ok(Ok(_)) => Ok(true),
            Ok(Err(problem)) => Err(problem),
            Err(_) => Err(Error::Engine("the page kept its size".to_owned())),
        }
    }

    /// In CSS pixels from the top of the document. The part must fit in the view.
    pub async fn picture(&self, x: f64, y: f64, width: f64, height: f64) -> Result<Vec<u8>, Error> {
        if width > f64::from(self.width) || height > f64::from(self.height) {
            return Err(Error::Engine(
                "the picture is larger than the page".to_owned(),
            ));
        }
        let answer = self
            .call(
                "Page.captureScreenshot",
                json!({
                    "format": "png",
                    "clip": { "x": x, "y": y, "width": width, "height": height, "scale": 1 },
                    "captureBeyondViewport": true,
                }),
            )
            .await?;
        let data = answer["data"].as_str().ok_or(Error::Malformed)?;
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|_| Error::Malformed)
    }

    pub async fn close(self) {
        let engine = Arc::clone(&self.engine);
        drop(self);
        engine.stop_if_idle().await;
    }
}

impl Drop for HostPage {
    fn drop(&mut self) {
        self.engine.forget_page(self.page, Some(&self.target));
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the side is a whole number from 0 to MOST_SIDE"
)]
fn side(length: f64) -> Result<u32, Error> {
    let side = length.ceil();
    if (0.0..=f64::from(MOST_SIDE)).contains(&side) {
        Ok(side as u32)
    } else {
        Err(Error::Engine(
            "the picture is larger than a page can be".to_owned(),
        ))
    }
}

async fn serve(
    devtools: Devtools,
    session: String,
    folder: PathBuf,
    mut events: mpsc::UnboundedReceiver<devtools::Event>,
    said: mpsc::Sender<String>,
) {
    while let Some(event) = events.recv().await {
        match event.method.as_str() {
            "Fetch.requestPaused" => {
                let answer = answer_of(&folder, &event.params).await;
                if let Err(problem) = devtools
                    .call("Fetch.fulfillRequest", answer, Some(&session))
                    .await
                {
                    tracing::debug!(%problem, "a host page did not get its file");
                }
            }
            "Runtime.bindingCalled" if event.params["name"] == BINDING => {
                if let Some(payload) = event.params["payload"].as_str()
                    && said.send(payload.to_owned()).await.is_err()
                {
                    return;
                }
            }
            _ => {}
        }
    }
}

async fn answer_of(folder: &Path, params: &Value) -> Value {
    let request = params["requestId"].clone();
    let address = params["request"]["url"].as_str().unwrap_or_default();
    let file = file_of(folder, address);
    let body = match &file {
        Some(path) => tokio::fs::read(path).await.ok(),
        None => None,
    };
    match (file, body) {
        (Some(path), Some(body)) => json!({
            "requestId": request,
            "responseCode": 200,
            "responseHeaders": [{ "name": "Content-Type", "value": kind_of(&path) }],
            "body": base64::engine::general_purpose::STANDARD.encode(body),
        }),
        _ => json!({ "requestId": request, "responseCode": 404 }),
    }
}

/// The file that an address of a host page names, inside its folder and nowhere else.
fn file_of(folder: &Path, address: &str) -> Option<PathBuf> {
    let path = address.strip_prefix(ORIGIN)?;
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let relative = Path::new(path);
    if relative
        .components()
        .any(|part| !matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(folder.join(relative))
}

fn kind_of(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_gets_files_of_its_folder_only() {
        let folder = Path::new("/views");
        assert_eq!(
            file_of(folder, "https://kodosi.invalid/assets/docx.js?v=1"),
            Some(PathBuf::from("/views/assets/docx.js"))
        );
        assert_eq!(file_of(folder, "https://kodosi.invalid/../secret"), None);
        assert_eq!(file_of(folder, "https://kodosi.invalid//etc/passwd"), None);
        assert_eq!(file_of(folder, "https://example.com/index.html"), None);
    }

    #[test]
    fn modules_go_out_as_scripts() {
        assert_eq!(
            kind_of(Path::new("index-a1.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            kind_of(Path::new("worker.mjs")),
            "text/javascript; charset=utf-8"
        );
    }
}
