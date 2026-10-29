use std::path::PathBuf;

use clap::Subcommand;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Context, output, print_json, print_text, request};
use crate::{Error, Result, local_host};

const IN_A_TERMINAL: &str =
    "The canvas belongs to a terminal. Run this in a Kodosi terminal on this computer.";

#[derive(Subcommand)]
pub(super) enum CanvasAction {
    /// List what the canvas of this terminal shows.
    List,
    /// Take items off the canvas of this terminal: name them, or use --all.
    Remove {
        /// An item's file name or path, or browser for the shared browser.
        names: Vec<String>,
        /// Take all files off.
        #[arg(long, conflicts_with = "names")]
        all: bool,
    },
}

fn terminal() -> Result<Uuid> {
    super::this_terminal()
        .and_then(|id| Uuid::parse_str(&id).ok())
        .ok_or_else(|| Error::Invalid(IN_A_TERMINAL.to_owned()))
}

pub(super) async fn show(
    files: Vec<PathBuf>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let session = terminal()?;
    let paths = files
        .into_iter()
        .map(|file| {
            super::path(Some(file.clone()))?
                .ok_or_else(|| Error::Invalid(format!("There is no file {}.", file.display())))
        })
        .collect::<Result<Vec<_>>>()?;
    let event = request(
        client,
        context,
        json!({"type":"canvas.show","sessionId":session,"paths":paths}),
        json_output,
    )
    .await?;
    output(&event, json_output, answer_text(&event))
}

/// Runs an agent-browser command and gives its exit code.
pub(super) async fn browse(
    arguments: Vec<String>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<u8> {
    let session = terminal()?;
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

pub(super) async fn run(
    action: Option<CanvasAction>,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let session = terminal()?;
    let canvas = context
        .session(session)
        .map_or(Value::Null, |entry| entry["canvas"].clone());
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
            } else {
                print_text("The canvas no longer shows them.")
            }
        }
    }
}

fn answer_text(event: &Value) -> &str {
    event["answer"]["text"].as_str().unwrap_or_default()
}

fn key(canvas: &Value, name: &str) -> Result<String> {
    if name == "browser" {
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
    if let Some(browser) = canvas["browser"].as_object() {
        let tab = browser["tabs"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|tab| tab["active"] == true)
            .and_then(|tab| tab["url"].as_str())
            .unwrap_or_default();
        lines.push(format!(
            "browser  {}  {tab}",
            browser["state"].as_str().unwrap_or_default()
        ));
    }
    if lines.is_empty() {
        "The canvas of this terminal shows nothing.".to_owned()
    } else {
        lines.join("\n")
    }
}
