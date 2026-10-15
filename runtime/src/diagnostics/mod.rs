use std::{
    borrow::Cow,
    fmt::Write as _,
    fs::File,
    io::Write as _,
    path::{Path, PathBuf},
    sync::Mutex,
};

use time::format_description::well_known::Rfc3339;
use tracing::{
    Event, Level, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};

mod redact;

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_RECORD_BYTES: usize = 32 * 1024;
const CLIENT: &str = "kodosi_client";

#[derive(Debug, Clone, Copy)]
pub enum Role {
    App,
    Host,
}

impl Role {
    const fn file(self) -> &'static str {
        match self {
            Self::App => "app.log",
            Self::Host => "host.log",
        }
    }
}

pub fn install(data_root: &Path, role: Role) {
    let home = kodosi_os::fs::home_dir().ok();
    drop(tracing::subscriber::set_global_default(Log::new(
        data_root,
        role,
        home.as_deref(),
    )));
}

pub fn client(level: Level, message: &[u8]) {
    let message = String::from_utf8_lossy(&message[..message.len().min(MAX_RECORD_BYTES + 1)]);
    match level {
        Level::ERROR => tracing::error!(target: CLIENT, "{message}"),
        Level::WARN => tracing::warn!(target: CLIENT, "{message}"),
        _ => tracing::info!(target: CLIENT, "{message}"),
    }
}

struct Log {
    path: PathBuf,
    home: Vec<String>,
    file: Mutex<Option<File>>,
}

impl Log {
    fn new(data_root: &Path, role: Role, home: Option<&Path>) -> Self {
        Self {
            path: data_root.join(role.file()),
            home: redact::home_forms(home),
            file: Mutex::new(None),
        }
    }

    fn line(&self, level: Level, target: &str, body: &str) -> String {
        let time = time::OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default();
        let mut end = body.len().min(MAX_RECORD_BYTES);
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        let mut safe = redact::redact(&body[..end], &self.home);
        if end < body.len() {
            safe.push_str(" [TRUNCATED]");
        }
        format!("{time} {level} {target}{}\n", escape_controls(&safe))
    }

    fn write(&self, line: &str) {
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        if file
            .as_ref()
            .and_then(|file| file.metadata().ok())
            .is_some_and(|metadata| metadata.len() >= MAX_BYTES)
        {
            *file = None;
            drop(std::fs::rename(
                &self.path,
                self.path.with_extension("log.1"),
            ));
        }
        if file.is_none() {
            *file = kodosi_os::fs::append_private(&self.path).ok();
        }
        if let Some(file) = file.as_mut() {
            drop(file.write_all(line.as_bytes()));
        }
    }
}

fn escape_controls(text: &str) -> Cow<'_, str> {
    if !text
        .chars()
        .any(|symbol| symbol.is_control() && symbol != '\t')
    {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for symbol in text.chars() {
        match symbol {
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push('\t'),
            symbol if symbol.is_control() => {
                let _written = write!(escaped, "\\u{{{:x}}}", u32::from(symbol));
            }
            symbol => escaped.push(symbol),
        }
    }
    Cow::Owned(escaped)
}

struct Body(String);

impl Visit for Body {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let _written = write!(self.0, " {value:?}");
        } else {
            let _written = write!(self.0, " {}={value:?}", field.name());
        }
    }
}

impl Subscriber for Log {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        *metadata.level() <= Level::INFO && metadata.target().starts_with("kodosi")
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let metadata = event.metadata();
        let mut body = Body(String::new());
        event.record(&mut body);
        self.write(&self.line(*metadata.level(), metadata.target(), &body.0));
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(root: &Path, role: Role) -> String {
        std::fs::read_to_string(root.join(role.file())).unwrap()
    }

    #[test]
    fn only_important_runtime_events_reach_the_log() {
        let root = tempfile::tempdir().unwrap();
        tracing::subscriber::with_default(Log::new(root.path(), Role::Host, None), || {
            tracing::debug!("local client disconnected");
            tracing::warn!(error = "link lost", "remote access interrupted");
        });
        let written = read(root.path(), Role::Host);
        assert_eq!(written.lines().count(), 1);
        assert!(written.contains("WARN kodosi_runtime::diagnostics"));
        assert!(written.ends_with("remote access interrupted error=\"link lost\"\n"));
    }

    #[test]
    fn client_records_are_redacted_bounded_and_one_line_each() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        tracing::subscriber::with_default(Log::new(root.path(), Role::App, Some(&home)), || {
            client(
                Level::WARN,
                format!(
                    "kodosi.view: token=secret-value\nWARN forged {}/notes",
                    home.display()
                )
                .as_bytes(),
            );
            let mut large = b"pem=-----BEGIN PRIVATE KEY-----\n".to_vec();
            large.resize(1024 * 1024, b's');
            client(Level::ERROR, &large);
        });
        let written = read(root.path(), Role::App);
        let lines: Vec<_> = written.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains(
            "WARN kodosi_client kodosi.view: token=[REDACTED]\\nWARN forged [HOME]/notes"
        ));
        assert!(lines[1].contains("ERROR kodosi_client pem=[REDACTED]"));
        assert!(lines[1].ends_with(" [TRUNCATED]"));
        assert!(!lines[1].contains(&"s".repeat(64)));
        assert!(lines[1].len() < 1024);
    }

    #[test]
    fn a_full_log_is_replaced_by_one_older_copy() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("app.log");
        std::fs::write(&path, vec![b'x'; usize::try_from(MAX_BYTES).unwrap()]).unwrap();
        let log = Log::new(root.path(), Role::App, None);
        log.write("first\n");
        log.write("second\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
        assert!(root.path().join("app.log.1").exists());
    }
}
