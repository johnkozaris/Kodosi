use std::{
    collections::BTreeSet,
    io::{self, Write as _},
    time::Duration,
};

use serde_json::{Value, json};

use super::{Context, DosiAction, print_json, print_text, render, request};
use crate::{
    Error, Result,
    dosi::{DosiBody, DosiItem, DosiTone, DosiToolStatus},
    local_host,
};

pub(super) const SKILL: &str = include_str!("../../skills/kodosi-dosi/SKILL.md");

pub(super) async fn run(
    action: DosiAction,
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let command = match action {
        DosiAction::Skill => return print_text(SKILL),
        DosiAction::Say { text, timeout } => {
            return say(client, context, &text, timeout, json_output).await;
        }
        DosiAction::Show => return show(client, context, json_output).await,
        DosiAction::Stop => json!({"type":"dosi.stop"}),
        DosiAction::Ask {
            question,
            options,
            multiple,
            terminal,
        } => {
            let token = std::env::var(crate::dosi::TOKEN_VARIABLE).map_err(|_| {
                Error::Invalid(
                    "Only Dosi in the Kodosi app asks with buttons. Ask in your answer.".to_owned(),
                )
            })?;
            json!({"type":"dosi.ask","token":token,"question":question,"options":options,"multiple":multiple,"terminal":terminal})
        }
    };
    let event = request(client, context, command, json_output).await?;
    if json_output {
        print_json(&event)
    } else {
        print_text(&render::result(&event, context))
    }
}

async fn say(
    client: &mut local_host::Client,
    context: &mut Context,
    text: &str,
    seconds: u64,
    json_output: bool,
) -> Result<()> {
    let text = text.trim();
    request(
        client,
        context,
        json!({"type":"dosi.send","text":text}),
        false,
    )
    .await?;
    let mut shown = BTreeSet::new();
    tokio::time::timeout(Duration::from_secs(seconds), async {
        loop {
            let frame = client.next().await?;
            let Some(event) = frame.get("event") else {
                continue;
            };
            if !context.observe(event) {
                continue;
            }
            match event["type"].as_str() {
                Some("dosi.item") => {
                    let Ok(item) = serde_json::from_value::<DosiItem>(event["item"].clone()) else {
                        continue;
                    };
                    let mine =
                        matches!(&item.body, DosiBody::Person { text: said } if said == text);
                    if mine || !settled(&item.body) || !shown.insert(item.id.clone()) {
                        continue;
                    }
                    if json_output {
                        print_json(event)?;
                    } else if let Some(line) = line(&item.body) {
                        writeln!(io::stdout().lock(), "{line}")?;
                    }
                }
                Some("dosi.snapshot") if answered(&event["dosi"], text) => return Ok(()),
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| {
        Error::Other("Dosi goes on working in Kodosi; this command stopped waiting.".into())
    })?
}

async fn show(
    client: &mut local_host::Client,
    context: &mut Context,
    json_output: bool,
) -> Result<()> {
    let connecting = |context: &Context| {
        context
            .latest
            .get("dosi.snapshot")
            .is_none_or(|event| event["dosi"]["state"] == "connecting")
    };
    if connecting(context) {
        drop(
            tokio::time::timeout(Duration::from_secs(10), async {
                while connecting(context) {
                    let frame = client.next().await?;
                    if let Some(event) = frame.get("event") {
                        context.observe(event);
                    }
                }
                Ok::<_, Error>(())
            })
            .await,
        );
    }
    let snapshot = context
        .latest
        .get("dosi.snapshot")
        .map(|event| event["dosi"].clone())
        .ok_or_else(|| Error::Other("This host runs no Dosi.".into()))?;
    if json_output {
        return print_json(&snapshot);
    }
    let items: Vec<DosiItem> = serde_json::from_value(snapshot["items"].clone())?;
    let lines = items
        .iter()
        .filter_map(|item| line(&item.body))
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return print_text("Dosi's conversation is empty.");
    }
    print_text(&lines.join("\n"))
}

fn answered(snapshot: &Value, text: &str) -> bool {
    matches!(snapshot["state"].as_str(), Some("idle" | "unavailable"))
        && !snapshot["queued"]
            .as_array()
            .is_some_and(|queued| queued.iter().any(|waiting| waiting["text"] == text))
}

const fn settled(body: &DosiBody) -> bool {
    !matches!(
        body,
        DosiBody::Text { live: true, .. }
            | DosiBody::Reasoning { live: true, .. }
            | DosiBody::Tool {
                status: DosiToolStatus::Running,
                ..
            }
    )
}

fn line(body: &DosiBody) -> Option<String> {
    Some(match body {
        DosiBody::Person { text } => format!("You: {text}"),
        DosiBody::Text { text, .. } => format!("Dosi: {text}"),
        DosiBody::Note { text } => text.clone(),
        DosiBody::Tool { status, .. } => {
            let failed = if *status == DosiToolStatus::Failed {
                " (failed)"
            } else {
                ""
            };
            format!("  · {}{failed}", body.deed()?)
        }
        DosiBody::Question {
            question,
            options,
            answer,
            ..
        } => match answer {
            Some(answer) if !answer.is_empty() => format!(
                "Dosi asks: {question} [{}] → {}",
                options.join(" / "),
                answer.join(", ")
            ),
            _ => format!("Dosi asks: {question} [{}]", options.join(" / ")),
        },
        DosiBody::Notice { tone, text } => match (tone, text) {
            (DosiTone::Stopped, _) => "(Stopped)".to_owned(),
            (DosiTone::SignIn, Some(reason)) => format!("(Sign in to the agent: {reason})"),
            (_, Some(reason)) => format!("(Failed: {reason})"),
            (_, None) => "(Failed)".to_owned(),
        },
        DosiBody::Reasoning { .. } => return None,
    })
}
