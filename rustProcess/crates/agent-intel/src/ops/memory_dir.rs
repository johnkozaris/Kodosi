use std::{io::Read, path::Path};

use super::io::MAX_AGENT_INTEL_FILE_BYTES;

#[cfg(unix)]
fn map_directory_open_error(label: &str, error: rustix::io::Errno) -> String {
    if matches!(error, rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR) {
        format!("{label} must not be a symlink or non-directory")
    } else {
        format!("open {label}: {error}")
    }
}

pub(crate) struct SelectedMemoryDirectory {
    #[cfg(unix)]
    fd: std::os::fd::OwnedFd,
    #[cfg(not(unix))]
    path: std::path::PathBuf,
}

impl SelectedMemoryDirectory {
    pub(crate) fn open(project_dir: &Path) -> Result<Self, String> {
        Self::open_inner(project_dir, false)
    }

    pub(crate) fn open_or_create(project_dir: &Path) -> Result<Self, String> {
        Self::open_inner(project_dir, true)
    }

    #[cfg(unix)]
    fn open_inner(project_dir: &Path, create: bool) -> Result<Self, String> {
        use rustix::fs::{Mode, OFlags};

        let project = rustix::fs::open(
            project_dir,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| map_directory_open_error("selected project directory", error))?;
        if create {
            match rustix::fs::mkdirat(&project, "memory", Mode::from_raw_mode(0o700)) {
                Ok(()) | Err(rustix::io::Errno::EXIST) => {}
                Err(error) => return Err(format!("create selected memory directory: {error}")),
            }
        }
        let fd = rustix::fs::openat(
            &project,
            "memory",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| map_directory_open_error("selected memory directory", error))?;
        Ok(Self { fd })
    }

    #[cfg(not(unix))]
    fn open_inner(project_dir: &Path, create: bool) -> Result<Self, String> {
        let path = project_dir.join("memory");
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("selected memory path must be a real directory".to_owned());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                std::fs::create_dir(&path)
                    .map_err(|error| format!("create selected memory directory: {error}"))?;
            }
            Err(error) => return Err(format!("open selected memory directory: {error}")),
        }
        let canonical_project = project_dir
            .canonicalize()
            .map_err(|error| format!("resolve selected project: {error}"))?;
        let canonical_memory = path
            .canonicalize()
            .map_err(|error| format!("resolve selected memory directory: {error}"))?;
        if !canonical_memory.starts_with(&canonical_project) {
            return Err("selected memory directory escapes its project".to_owned());
        }
        Ok(Self {
            path: canonical_memory,
        })
    }

    pub(crate) fn read_string(&self, filename: &str) -> Result<String, String> {
        let file = self.open_regular(filename)?;
        let mut bytes = Vec::new();
        file.take(MAX_AGENT_INTEL_FILE_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| format!("read selected memory file: {error}"))?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_AGENT_INTEL_FILE_BYTES {
            return Err(format!(
                "file too large (> {MAX_AGENT_INTEL_FILE_BYTES} bytes)"
            ));
        }
        String::from_utf8(bytes).map_err(|error| format!("file is not valid UTF-8: {error}"))
    }

    #[cfg(unix)]
    fn open_regular(&self, filename: &str) -> Result<std::fs::File, String> {
        use rustix::fs::{Mode, OFlags};

        let fd = rustix::fs::openat(
            &self.fd,
            filename,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|error| {
            if matches!(error, rustix::io::Errno::LOOP | rustix::io::Errno::ISDIR) {
                "selected memory path must be a regular file".to_owned()
            } else {
                format!("open selected memory file: {error}")
            }
        })?;
        let stat = rustix::fs::fstat(&fd)
            .map_err(|error| format!("inspect selected memory file: {error}"))?;
        if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile {
            return Err("selected memory path must be a regular file".to_owned());
        }
        Ok(std::fs::File::from(fd))
    }

    #[cfg(not(unix))]
    fn open_regular(&self, filename: &str) -> Result<std::fs::File, String> {
        let path = self.path.join(filename);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect selected memory file: {error}"))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("selected memory path must be a regular file".to_owned());
        }
        std::fs::File::open(&path).map_err(|error| format!("open selected memory file: {error}"))
    }

    pub(crate) fn create_new(&self, filename: &str, content: &str) -> Result<(), String> {
        use std::io::Write;

        let mut file = self.create_new_file(filename)?;
        file.write_all(content.as_bytes())
            .map_err(|error| format!("write selected memory file: {error}"))
    }

    #[cfg(unix)]
    fn create_new_file(&self, filename: &str) -> Result<std::fs::File, String> {
        use rustix::fs::{Mode, OFlags};

        let fd = rustix::fs::openat(
            &self.fd,
            filename,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|error| format!("create selected memory file: {error}"))?;
        Ok(std::fs::File::from(fd))
    }

    #[cfg(not(unix))]
    fn create_new_file(&self, filename: &str) -> Result<std::fs::File, String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join(filename))
            .map_err(|error| format!("create selected memory file: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::SelectedMemoryDirectory;

    #[cfg(unix)]
    #[test]
    fn held_directory_never_follows_replaced_memory_path() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let memory = project.join("memory");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&memory).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(memory.join("fact.md"), "inside").unwrap();
        std::fs::write(outside.join("fact.md"), "outside").unwrap();
        let selected = SelectedMemoryDirectory::open(&project).unwrap();
        let displaced = project.join("memory-old");
        std::fs::rename(&memory, &displaced).unwrap();
        symlink(&outside, &memory).unwrap();

        assert_eq!(selected.read_string("fact.md").unwrap(), "inside");
        selected.create_new("created.md", "created").unwrap();
        assert_eq!(
            std::fs::read_to_string(displaced.join("created.md")).unwrap(),
            "created"
        );
        assert!(!outside.join("created.md").exists());
        assert_eq!(
            std::fs::read_to_string(outside.join("fact.md")).unwrap(),
            "outside"
        );
    }

    #[cfg(unix)]
    #[test]
    fn regular_file_open_rejects_symlink_and_directory() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let memory = project.join("memory");
        std::fs::create_dir_all(memory.join("directory.md")).unwrap();
        let outside = temp.path().join("outside.md");
        std::fs::write(&outside, "outside").unwrap();
        symlink(&outside, memory.join("link.md")).unwrap();
        let selected = SelectedMemoryDirectory::open(&project).unwrap();

        assert!(selected.read_string("link.md").is_err());
        assert!(selected.read_string("directory.md").is_err());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "outside");
    }
}
