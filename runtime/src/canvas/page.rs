use pulldown_cmark::{Options, Parser, html};

/// GitHub Flavored Markdown options. Smart punctuation, math, sub/superscript, and wikilinks are
/// excluded because they mangle CLI flags, prices, paths, and shell tests (`[[ ... ]]`).
const MARKDOWN: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_GFM)
    .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
    .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);

pub(super) fn page(name: &str, source: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(source);
    let extension = std::path::Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let body = match extension.as_deref() {
        Some("md" | "markdown") => {
            let mut body = String::with_capacity(text.len() * 2);
            html::push_html(&mut body, Parser::new_ext(&text, MARKDOWN));
            body
        }
        Some("diff" | "patch") => diff(&text),
        _ => format!("<pre><code>{}</code></pre>", escape(&text)),
    };
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{}</title>{}</head><body><main>{body}</main></body></html>",
        escape(name),
        style()
    )
    .into_bytes()
}

/// Marks added and removed lines with the page's own colors.
fn diff(text: &str) -> String {
    let mut body = String::with_capacity(text.len() * 2);
    body.push_str("<pre><code>");
    for line in text.lines() {
        let class = if line.starts_with("diff --git ") {
            Some("eyebrow")
        } else if ["--- ", "+++ ", "index ", "new file", "deleted file", "@@"]
            .iter()
            .any(|start| line.starts_with(start))
        {
            Some("note")
        } else if line.starts_with('+') {
            Some("good")
        } else if line.starts_with('-') {
            Some("bad")
        } else {
            None
        };
        match class {
            Some(class) => {
                body.push_str("<span class=\"");
                body.push_str(class);
                body.push_str("\">");
                body.push_str(&escape(line));
                body.push_str("</span>");
            }
            None => body.push_str(&escape(line)),
        }
        body.push('\n');
    }
    body.push_str("</code></pre>");
    body
}

fn style() -> &'static str {
    let template = crate::agents::PAGE;
    template
        .find("<style>")
        .zip(template.find("</style>"))
        .map_or("", |(start, end)| &template[start..end + "</style>".len()])
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
