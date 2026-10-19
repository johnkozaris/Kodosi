use super::{DosiBody, DosiItem, DosiTone, conversation::clip};

const MAX_ITEMS: usize = 60;
const MAX_BYTES: usize = 12 * 1024;
const MAX_LINE: usize = 1200;

pub(super) fn catch_up(items: &[&DosiItem], whole: bool) -> Option<String> {
    let mut lines = Vec::new();
    let mut bytes = 0;
    let mut left_out = false;
    for item in items.iter().rev() {
        let Some(line) = line(item) else {
            continue;
        };
        if lines.len() == MAX_ITEMS || bytes + line.len() > MAX_BYTES {
            left_out = true;
            break;
        }
        bytes += line.len() + 1;
        lines.push(line);
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    let heading = if whole {
        "Earlier in this conversation with the person, from your other sessions and their other computers:"
    } else {
        "Since your last turn, on the person's other computers:"
    };
    let mut text = format!("({heading}\n");
    if left_out {
        text.push_str("[Older messages are left out.]\n");
    }
    text.push_str(&lines.join("\n"));
    text.push(')');
    Some(text)
}

fn line(item: &DosiItem) -> Option<String> {
    let line = match &item.body {
        DosiBody::Person { text } => format!("The person: {text}"),
        DosiBody::Text { text, .. } => format!("You: {text}"),
        DosiBody::Reasoning { .. } => return None,
        DosiBody::Tool { .. } => format!("You {}", item.body.deed()?),
        DosiBody::Question {
            question,
            options,
            answer,
            ..
        } => match answer {
            Some(answer) if !answer.is_empty() => format!(
                "You asked: {question} ({}). The person answered: {}",
                options.join(" / "),
                answer.join(", ")
            ),
            _ => format!("You asked: {question} ({}).", options.join(" / ")),
        },
        DosiBody::Note { text } => text.clone(),
        DosiBody::Notice { tone, text } => match tone {
            DosiTone::Stopped => "[The person stopped that turn.]".to_owned(),
            DosiTone::Failed | DosiTone::SignIn => {
                format!(
                    "[That turn failed: {}]",
                    text.as_deref().unwrap_or("no reason")
                )
            }
        },
    };
    Some(clip(&line, MAX_LINE))
}
