use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::{Error, Result, protocol::FolderList};

const SHOWN: usize = 500;
const READ: usize = 20_000;
const RECENT: usize = 8;
const WAIT: Duration = Duration::from_secs(5);
const FILE: &str = "folders.json";

pub(crate) struct Recent {
    file: PathBuf,
    folders: Vec<String>,
}

impl Recent {
    pub(crate) fn load(data_root: &Path) -> Self {
        let file = data_root.join(FILE);
        let folders = std::fs::read(&file)
            .ok()
            .filter(|bytes| bytes.len() <= 64 * 1024)
            .and_then(|bytes| serde_json::from_slice::<Vec<String>>(&bytes).ok())
            .unwrap_or_default();
        Self { file, folders }
    }

    pub(crate) fn remember(&mut self, folder: &str) {
        if self.folders.first().is_some_and(|first| first == folder) {
            return;
        }
        self.folders.retain(|known| known != folder);
        self.folders.insert(0, folder.to_owned());
        self.folders.truncate(RECENT);
        let saved = serde_json::to_vec(&self.folders)
            .map_err(Error::from)
            .and_then(|bytes| Ok(kodosi_os::fs::write_private(&self.file, &bytes)?));
        if let Err(error) = saved {
            tracing::warn!(%error, "could not save the recent folders");
        }
    }

    pub(crate) fn with(&self, running: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut recent = Vec::new();
        for folder in running.into_iter().chain(self.folders.iter().cloned()) {
            if !recent.contains(&folder) {
                recent.push(folder);
            }
        }
        recent.truncate(RECENT);
        recent
    }
}

pub(crate) async fn list(
    home: PathBuf,
    path: Option<String>,
    recent: Vec<String>,
) -> Result<FolderList> {
    let folder = path.map_or_else(|| home.clone(), PathBuf::from);
    if !folder.is_absolute() {
        return Err(Error::Invalid(
            "Choose a folder by its full path on that computer.".to_owned(),
        ));
    }
    let listing = move || {
        let home = kodosi_os::fs::canonicalize(&home).unwrap_or(home);
        read(&folder).map(|(folder, folders)| (home, folder, folders))
    };
    let (home, folder, folders) = tokio::time::timeout(WAIT, tokio::task::spawn_blocking(listing))
        .await
        .map_err(|_| Error::Invalid("The folder did not open in time.".to_owned()))?
        .map_err(std::io::Error::from)??;
    Ok(FolderList {
        parent: folder.parent().map(text).transpose()?,
        path: text(&folder)?,
        home: text(&home)?,
        folders,
        recent,
    })
}

fn read(folder: &Path) -> Result<(PathBuf, Vec<String>)> {
    let folder = kodosi_os::fs::canonicalize(folder).map_err(|error| unreadable(&error))?;
    let mut folders = Vec::new();
    let entries = std::fs::read_dir(&folder).map_err(|error| unreadable(&error))?;
    for entry in entries.take(READ) {
        let Ok(entry) = entry else { continue };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let directory = entry.file_type().is_ok_and(|kind| {
            kind.is_dir()
                || kind.is_symlink() && entry.path().metadata().is_ok_and(|meta| meta.is_dir())
        });
        if directory && !name.starts_with('.') && !hidden(&entry) {
            folders.push(name);
        }
    }
    folders.sort_by_cached_key(|name| name.to_lowercase());
    folders.truncate(SHOWN);
    Ok((folder, folders))
}

#[cfg(target_os = "macos")]
fn hidden(entry: &std::fs::DirEntry) -> bool {
    use std::os::macos::fs::MetadataExt;
    const UF_HIDDEN: u32 = 0x8000;
    entry
        .metadata()
        .is_ok_and(|meta| meta.st_flags() & UF_HIDDEN != 0)
}

#[cfg(windows)]
fn hidden(entry: &std::fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    entry
        .metadata()
        .is_ok_and(|meta| meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
}

#[cfg(not(any(target_os = "macos", windows)))]
const fn hidden(_: &std::fs::DirEntry) -> bool {
    false
}

fn unreadable(error: &std::io::Error) -> Error {
    Error::Invalid(match error.kind() {
        std::io::ErrorKind::NotFound => "This folder is not there.".to_owned(),
        std::io::ErrorKind::PermissionDenied => "Kodosi cannot open this folder.".to_owned(),
        _ => format!("Kodosi cannot open this folder: {error}"),
    })
}

fn text(path: &Path) -> Result<String> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        Error::Invalid("The folder name is not text that Kodosi can show.".to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_folder_lists_its_visible_folders_by_name_and_names_its_parent() {
        let root = tempfile::tempdir().unwrap();
        for name in ["beta", "Alpha", ".hidden"] {
            std::fs::create_dir(root.path().join(name)).unwrap();
        }
        std::fs::write(root.path().join("notes.txt"), b"x").unwrap();
        let home = kodosi_os::fs::canonicalize(root.path()).unwrap();
        let listed = list(root.path().to_path_buf(), None, vec!["/work".to_owned()])
            .await
            .unwrap();
        assert_eq!(listed.folders, ["Alpha", "beta"]);
        assert_eq!(
            (listed.path.as_str(), listed.home.as_str()),
            (home.to_str().unwrap(), home.to_str().unwrap())
        );
        assert_eq!(
            listed.parent.as_deref(),
            home.parent().and_then(Path::to_str)
        );
        assert_eq!(listed.recent, ["/work"]);
        let inner = list(
            home.clone(),
            Some(home.join("beta").to_string_lossy().into_owned()),
            vec![],
        )
        .await
        .unwrap();
        assert_eq!(inner.folders, Vec::<String>::new());
        assert!(
            list(
                home.clone(),
                Some(home.join("gone").to_string_lossy().into_owned()),
                vec![]
            )
            .await
            .is_err()
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_folder_that_finder_hides_stays_hidden() {
        let root = tempfile::tempdir().unwrap();
        for name in ["Library", "Projects"] {
            std::fs::create_dir(root.path().join(name)).unwrap();
        }
        let hide = std::process::Command::new("chflags")
            .arg("hidden")
            .arg(root.path().join("Library"))
            .status()
            .unwrap();
        assert!(hide.success());
        let home = kodosi_os::fs::canonicalize(root.path()).unwrap();
        assert_eq!(
            list(home, None, vec![]).await.unwrap().folders,
            ["Projects"]
        );
    }

    #[test]
    fn recent_folders_keep_the_newest_first_once_and_survive_a_restart() {
        let root = tempfile::tempdir().unwrap();
        let mut recent = Recent::load(root.path());
        for folder in ["/a", "/b", "/a"] {
            recent.remember(folder);
        }
        assert_eq!(
            Recent::load(root.path()).with(["/c".to_owned(), "/b".to_owned()]),
            ["/c", "/b", "/a"]
        );
    }
}
