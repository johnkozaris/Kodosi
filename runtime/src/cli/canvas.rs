use std::path::{Path, PathBuf};

use clap::Subcommand;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Action, Context, output, print_json, print_text, request};
use crate::{Error, Result, local_host};

const NO_TERMINAL: &str =
    "This shell is not in a running Kodosi terminal on this computer, so it has no canvas.";

#[derive(Subcommand)]
pub(super) enum CanvasAction {
    /// List items shown on this terminal's canvas.
    List,
    /// Remove items from this terminal's canvas by name, or pass --all.
    Remove {
        /// File name or path of each item to remove, or "browser" or "screen".
        names: Vec<String>,
        /// Remove all files from the canvas.
        #[arg(long, conflicts_with = "names")]
        all: bool,
    },
}

/// Verify the terminal still exists: a child process can outlive its terminal while keeping
/// `KODOSI_SESSION_ID` in its environment.
fn terminal(context: &Context) -> Result<Uuid> {
    super::this_terminal()
        .and_then(|id| Uuid::parse_str(&id).ok())
        .filter(|id| context.session(*id).is_ok())
        .ok_or_else(|| Error::Invalid(NO_TERMINAL.to_owned()))
}

pub(super) async fn without_terminal(action: &Action, data_root: &Path) -> Result<Option<u8>> {
    match action {
        Action::Doc { arguments } => {
            if let Err(usage) = crate::documents::parse(arguments.clone()) {
                usage.print()?;
                return Ok(Some(u8::try_from(usage.exit_code()).unwrap_or(2)));
            }
        }
        Action::Browser { arguments } if crate::canvas::answers_alone(arguments) => {
            let (text, code) = crate::canvas::answer_alone(data_root, arguments)
                .await
                .map_err(|problem| Error::Other(problem.to_string()))?;
            print_text(&text)?;
            return Ok(Some(u8::try_from(code).unwrap_or(1)));
        }
        Action::Browser { .. } | Action::Show { .. } | Action::Canvas { .. } => {}
        _ => return Ok(None),
    }
    if super::this_terminal().is_none() {
        return Err(Error::Invalid(NO_TERMINAL.to_owned()));
    }
    Ok(None)
}

/// `kodosi show window <words>` shows the window whose app or title holds the words.
const WINDOW: &str = "window";

pub(super) async fn show(
    things: Vec<PathBuf>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<u8> {
    let session = terminal(context)?;
    let mut paths = Vec::new();
    let mut address = None;
    let mut browser = false;
    let mut screen = None;
    let mut things = things.into_iter();
    while let Some(thing) = things.next() {
        if thing.as_os_str() == crate::canvas::BROWSER_PIPE {
            browser = true;
            continue;
        }
        if thing.as_os_str() == crate::canvas::SCREEN_PIPE {
            screen = Some(json!({"kind":"screen"}));
            continue;
        }
        if thing.as_os_str() == WINDOW {
            let words = things
                .by_ref()
                .map(|word| word.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(" ");
            if words.is_empty() {
                return Err(Error::Invalid(
                    "Name the window by words of its app or title.".to_owned(),
                ));
            }
            screen = Some(json!({"kind":"window","words":words}));
            continue;
        }
        match thing
            .to_str()
            .filter(|word| crate::canvas::is_address(word))
        {
            Some(_) if address.is_some() => {
                return Err(Error::Invalid("Show one address at a time.".to_owned()));
            }
            Some(word) => address = Some(word.to_owned()),
            None => {
                paths.push(super::path(Some(thing.clone()))?.ok_or_else(|| {
                    Error::Invalid(format!("There is no file {}.", thing.display()))
                })?);
            }
        }
    }
    let folder = super::path(Some(PathBuf::from(".")))?
        .ok_or_else(|| Error::Invalid("This folder is not available.".to_owned()))?;
    let event = request(
        client,
        context,
        json!({"type":"canvas.show","sessionId":session,"paths":paths,"address":address,"browser":browser,"screen":screen,"folder":folder}),
        json_output,
    )
    .await?;
    output(&event, json_output, answer_text(&event))?;
    Ok(event["answer"]["code"]
        .as_i64()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

pub(super) async fn browse(
    arguments: Vec<String>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<u8> {
    let session = terminal(context)?;
    let folder = super::path(Some(PathBuf::from(".")))?
        .ok_or_else(|| Error::Invalid("This folder is not available.".to_owned()))?;
    let event = request(
        client,
        context,
        json!({"type":"canvas.browser","sessionId":session,"folder":folder,"arguments":arguments}),
        json_output,
    )
    .await?;
    output(&event, json_output, answer_text(&event))?;
    Ok(event["answer"]["code"]
        .as_i64()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

pub(super) async fn document(
    arguments: Vec<String>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<u8> {
    let session = terminal(context)?;
    let folder = super::path(Some(PathBuf::from(".")))?
        .ok_or_else(|| Error::Invalid("This folder is not available.".to_owned()))?;
    let event = request(
        client,
        context,
        json!({"type":"canvas.document","sessionId":session,"folder":folder,"arguments":arguments}),
        json_output,
    )
    .await?;
    output(&event, json_output, answer_text(&event))?;
    Ok(event["answer"]["code"]
        .as_i64()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

pub(super) async fn run(
    action: Option<CanvasAction>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let session = terminal(context)?;
    let canvas = context.session(session)?["canvas"].clone();
    match action.unwrap_or(CanvasAction::List) {
        CanvasAction::List => output(&canvas, json_output, &listed(&canvas)),
        CanvasAction::Remove { names, all } => {
            if names.is_empty() && !all {
                return Err(Error::Invalid(
                    "Name the items to take off, or use --all.".to_owned(),
                ));
            }
            let keys = names
                .iter()
                .map(|name| key(&canvas, name))
                .collect::<Result<Vec<_>>>()?;
            let event = request(
                client,
                context,
                json!({"type":"canvas.request","sessionId":session,"request":{"type":"remove","keys":keys}}),
                json_output,
            )
            .await?;
            if json_output {
                print_json(&event)
            } else if names.is_empty() {
                print_text("The canvas no longer shows its files.")
            } else {
                print_text(&format!("The canvas no longer shows {}.", names.join(", ")))
            }
        }
    }
}

fn answer_text(event: &Value) -> &str {
    event["answer"]["text"].as_str().unwrap_or_default()
}

fn key(canvas: &Value, name: &str) -> Result<String> {
    if name == crate::canvas::BROWSER_PIPE || name == crate::canvas::SCREEN_PIPE {
        return Ok(name.to_owned());
    }
    let items = canvas["items"].as_array().map_or(&[][..], Vec::as_slice);
    let matching = items
        .iter()
        .filter(|item| item["name"] == name || item["key"] == name)
        .filter_map(|item| item["key"].as_str())
        .collect::<Vec<_>>();
    match matching[..] {
        [key] => Ok(key.to_owned()),
        [] => Err(Error::Invalid(format!(
            "The canvas shows no {name}. Run kodosi canvas to list what it shows."
        ))),
        _ => Err(Error::Invalid(format!(
            "The canvas shows more than one {name}. Name it by its path."
        ))),
    }
}

fn listed(canvas: &Value) -> String {
    let mut lines = canvas["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let missing = if item["missing"] == true {
                " (missing)"
            } else {
                ""
            };
            format!(
                "{}  {}{missing}\n  {}",
                item["name"].as_str().unwrap_or_default(),
                item["kind"].as_str().unwrap_or_default(),
                item["key"].as_str().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let browser = &canvas["browser"];
    if browser.is_object() {
        let state = browser["state"].as_str().unwrap_or_default();
        let tab = browser["tabs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tab| tab["active"] == true)
            .and_then(|tab| tab["url"].as_str());
        lines.push(tab.map_or_else(
            || format!("browser  {state}"),
            |tab| format!("browser  {state}  {tab}"),
        ));
    }
    let screen = &canvas["screen"];
    if screen.is_object() {
        lines.push(if screen["shared"] == true {
            "screen  this computer's screen, for its owner's devices".to_owned()
        } else {
            "screen  this computer's screen, which its owner's devices see once the person at it turns sharing on in Kodosi's settings".to_owned()
        });
    }
    if lines.is_empty() {
        "The canvas of this terminal shows nothing.".to_owned()
    } else {
        lines.join("\n")
    }
}
