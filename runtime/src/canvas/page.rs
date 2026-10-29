use pulldown_cmark::{Options, Parser, html};

const TEMPLATE: &str = include_str!("../../skills/kodosi-canvas/assets/page.html");

/// Makes a Markdown or text file into a page with the look of the canvas template.
pub(super) fn page(name: &str, source: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(source);
    let markdown = std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
        });
    let body = if markdown {
        let mut body = String::with_capacity(text.len() * 2);
        html::push_html(&mut body, Parser::new_ext(&text, Options::all()));
        body
    } else {
        format!("<pre><code>{}</code></pre>", escape(&text))
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title>{}</head><body><main>{body}</main></body></html>",
        escape(name),
        style()
    )
    .into_bytes()
}

fn style() -> &'static str {
    TEMPLATE
        .find("<style>")
        .zip(TEMPLATE.find("</style>"))
        .map_or("", |(start, end)| &TEMPLATE[start..end + "</style>".len()])
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// The first address that a page loads from the network, which the canvas does not do. A link
/// that a person follows is no load.
pub(super) fn network_reference(source: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(source);
    let lower = text.to_ascii_lowercase();
    let attribute = [
        "src=\"http",
        "src='http",
        "src=http",
        "url(http",
        "url(\"http",
        "url('http",
        "@import \"http",
        "@import 'http",
    ]
    .iter()
    .filter_map(|needle| lower.find(needle))
    .min();
    let mut link = None;
    let mut from = 0;
    while let Some(start) = lower[from..].find("<link").map(|at| from + at) {
        let end = lower[start..]
            .find('>')
            .map_or(lower.len(), |at| start + at);
        let tag = &lower[start..end];
        link = tag
            .find("href=\"http")
            .or_else(|| tag.find("href='http"))
            .map(|at| start + at);
        if link.is_some() {
            break;
        }
        from = end;
    }
    let at = attribute.into_iter().chain(link).min()?;
    let start = at + lower[at..].find("http")?;
    Some(
        text[start..]
            .chars()
            .take_while(|character| !matches!(character, '"' | '\'' | ')' | ' ' | '>'))
            .take(120)
            .collect(),
    )
}
