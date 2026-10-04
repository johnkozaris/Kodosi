use std::{
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::Write as _,
    os::unix::fs::OpenOptionsExt as _,
    path::{Path, PathBuf},
    sync::Mutex,
};

use time::format_description::well_known::Rfc3339;
use tracing::{
    Event, Level, Metadata, Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};

const MAX_BYTES: u64 = 1024 * 1024;

pub fn install(data_root: &Path) {
    drop(tracing::subscriber::set_global_default(Log::new(data_root)));
}

struct Log {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl Log {
    fn new(data_root: &Path) -> Self {
        Self {
            path: data_root.join("diagnostics.log"),
            file: Mutex::new(None),
        }
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
            *file = OpenOptions::new()
                .create(true)
                .append(true)
                .mode(0o600)
                .open(&self.path)
                .ok();
        }
        if let Some(file) = file.as_mut() {
            drop(file.write_all(line.as_bytes()));
        }
    }
}

struct Line(String);

impl Visit for Line {
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
        let time = time::OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default();
        let metadata = event.metadata();
        let mut line = Line(format!("{time} {} {}", metadata.level(), metadata.target()));
        event.record(&mut line);
        line.0.push('\n');
        self.write(&line.0);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_important_runtime_events_reach_the_diagnostics_file() {
        let root = tempfile::tempdir().unwrap();
        tracing::subscriber::with_default(Log::new(root.path()), || {
            tracing::debug!("local client disconnected");
            tracing::warn!(error = "link lost", "remote access interrupted");
        });
        let written = std::fs::read_to_string(root.path().join("diagnostics.log")).unwrap();
        assert_eq!(written.lines().count(), 1);
        assert!(written.contains("WARN kodosi_runtime::diagnostics"));
        assert!(written.ends_with("remote access interrupted error=\"link lost\"\n"));
    }

    #[test]
    fn a_full_diagnostics_file_is_replaced_by_one_older_copy() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("diagnostics.log");
        std::fs::write(&path, vec![b'x'; usize::try_from(MAX_BYTES).unwrap()]).unwrap();
        let log = Log::new(root.path());
        log.write("first\n");
        log.write("second\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second\n");
        assert!(root.path().join("diagnostics.log.1").exists());
    }
}
