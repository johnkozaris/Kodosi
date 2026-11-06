use std::{
    collections::HashMap,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::Engine as _;
use tokio::{
    fs::{self, File, OpenOptions},
    io::AsyncWriteExt as _,
    time::Instant,
};
use uuid::Uuid;

use super::{MOST_FILE_BYTES, kind_of};
use crate::text::Text;

const MOST_UPLOADS: usize = 4;
const MOST_NUMBER: u32 = 999;
const LIFE: Duration = Duration::from_mins(10);

struct Upload {
    file: File,
    part: PathBuf,
    written: u64,
    started: Instant,
}

/// One part of a file that a person adds to a canvas.
pub(super) struct Part<'a> {
    pub(super) upload: Uuid,
    pub(super) name: &'a str,
    pub(super) offset: u64,
    pub(super) data: &'a str,
    pub(super) last: bool,
    pub(super) page: bool,
}

/// The files that people are adding. A file grows beside its final place and takes a free name
/// when it is whole, so an agent never sees a part of it.
#[derive(Default)]
pub(super) struct Uploads {
    open: HashMap<Uuid, Upload>,
}

impl Uploads {
    /// Takes the next part. The last part gives the path of the whole file.
    pub(super) async fn add(
        &mut self,
        folder: &Path,
        part: Part<'_>,
    ) -> Result<Option<PathBuf>, Text> {
        self.forget_old().await;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(part.data)
            .map_err(|_| text!("This part of the file is not readable."))?;
        if part.offset == 0 {
            self.start(folder, &part).await?;
        }
        let Some(upload) = self.open.get_mut(&part.upload) else {
            return Err(text!("This upload ended. Add the file again."));
        };
        let written = upload.written + bytes.len() as u64;
        if upload.written != part.offset {
            self.abandon(part.upload).await;
            return Err(text!("A part of the file got lost. Add the file again."));
        }
        if written > MOST_FILE_BYTES {
            self.abandon(part.upload).await;
            return Err(text!(
                "The canvas takes files up to {most} MB.",
                most = MOST_FILE_BYTES / 1024 / 1024
            ));
        }
        let saved = upload.file.write_all(&bytes).await;
        upload.written = written;
        if saved.is_err() {
            self.abandon(part.upload).await;
            return Err(text!("{name} could not be saved.", name = part.name));
        }
        if !part.last {
            return Ok(None);
        }
        let Some(mut upload) = self.open.remove(&part.upload) else {
            return Err(text!("This upload ended. Add the file again."));
        };
        let flushed = upload.file.flush().await;
        drop(upload.file);
        let placed = match flushed {
            Ok(()) => place(&upload.part, folder, part.name).await,
            Err(error) => Err(error),
        };
        if placed.is_err() {
            drop(fs::remove_file(&upload.part).await);
        }
        placed
            .map(Some)
            .map_err(|_| text!("{name} could not be saved.", name = part.name))
    }

    pub(super) async fn clear(&mut self) {
        for (_, upload) in self.open.drain() {
            drop(upload.file);
            drop(fs::remove_file(upload.part).await);
        }
    }

    async fn start(&mut self, folder: &Path, part: &Part<'_>) -> Result<(), Text> {
        if !part.page && kind_of(Path::new(part.name)).is_none() {
            return Err(text!(
                "The canvas shows pages (HTML, Markdown, text), pictures, documents, sheets, presentations and PDFs."
            ));
        }
        if self.open.len() >= MOST_UPLOADS {
            return Err(text!("Other files are on their way. Try again in a moment."));
        }
        let path = folder.join(format!(".{}.part", part.upload));
        let file = File::create(&path)
            .await
            .map_err(|_| text!("{name} could not be saved.", name = part.name))?;
        self.open.insert(
            part.upload,
            Upload {
                file,
                part: path,
                written: 0,
                started: Instant::now(),
            },
        );
        Ok(())
    }

    /// A person who leaves in the middle of an upload does not hold its place.
    async fn forget_old(&mut self) {
        let old: Vec<Uuid> = self
            .open
            .iter()
            .filter(|(_, upload)| upload.started.elapsed() > LIFE)
            .map(|(id, _)| *id)
            .collect();
        for id in old {
            self.abandon(id).await;
        }
    }

    async fn abandon(&mut self, id: Uuid) {
        if let Some(upload) = self.open.remove(&id) {
            drop(upload.file);
            drop(fs::remove_file(upload.part).await);
        }
    }
}

/// A new empty file claims the name, so two people who add one name at once keep both files.
async fn place(part: &Path, folder: &Path, name: &str) -> std::io::Result<PathBuf> {
    for number in 1..=MOST_NUMBER {
        let path = folder.join(numbered(name, number));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .await
        {
            Ok(claimed) => {
                drop(claimed);
                fs::rename(part, &path).await?;
                return Ok(path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(ErrorKind::AlreadyExists.into())
}

fn numbered(name: &str, number: u32) -> String {
    match name.rsplit_once('.') {
        _ if number == 1 => name.to_owned(),
        Some((stem, extension)) if !stem.is_empty() => format!("{stem} {number}.{extension}"),
        _ => format!("{name} {number}"),
    }
}
