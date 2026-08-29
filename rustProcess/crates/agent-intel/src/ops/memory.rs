use std::path::{Path, PathBuf};

use crate::memory::MemoryFileInfo;

use super::memory_dir::SelectedMemoryDirectory;
use super::path_safety::{self, PathSafetyError};

pub async fn list_memory(home: &Path, cwd: &str) -> Result<Vec<MemoryFileInfo>, String> {
    let cwd = canonical_cwd_string(cwd)?;
    let memory_dir = memory_dir(home, &cwd);
    let projects_root = home.join(".claude").join("projects");
    reject_symlinked_memory_path(&memory_dir)?;
    let canonical_memory_dir = match path_safety::enforce_under_root(&projects_root, &memory_dir) {
        Ok(p) => p,
        Err(PathSafetyError::NotFound) => return Ok(Vec::new()),
        Err(err) => return Err(format!("invalid memory path: {err}")),
    };
    tokio::task::spawn_blocking(move || crate::memory::list_memory_files(&canonical_memory_dir))
        .await
        .map_err(|e| format!("task join error: {e}"))
}

pub async fn read_memory(home: &Path, cwd: &str, filename: &str) -> Result<String, String> {
    let cwd = canonical_cwd_string(cwd)?;
    path_safety::validate_component("filename", filename)
        .map_err(|e| format!("invalid memory filename: {e}"))?;
    let memory_dir = memory_dir(home, &cwd);
    let projects_root = home.join(".claude").join("projects");
    reject_symlinked_memory_path(&memory_dir)?;
    let canonical_memory_dir = match path_safety::enforce_under_root(&projects_root, &memory_dir) {
        Ok(path) => path,
        Err(PathSafetyError::NotFound) => return Err("file not found".to_owned()),
        Err(error) => return Err(format!("invalid memory path: {error}")),
    };
    let project_dir = canonical_memory_dir
        .parent()
        .ok_or_else(|| "invalid memory path".to_owned())?
        .to_owned();
    let filename = filename.to_owned();
    tokio::task::spawn_blocking(move || {
        SelectedMemoryDirectory::open(&project_dir)?.read_string(&filename)
    })
    .await
    .map_err(|error| format!("read memory task join: {error}"))?
}

fn memory_dir(home: &Path, canonical_cwd: &str) -> PathBuf {
    home.join(".claude")
        .join("projects")
        .join(crate::claude::ClaudeCodeProvider::encode_project_path(
            canonical_cwd,
        ))
        .join("memory")
}

fn reject_symlinked_memory_path(memory_dir: &Path) -> Result<(), String> {
    let project_dir = memory_dir
        .parent()
        .ok_or_else(|| "invalid memory path".to_owned())?;
    for path in [project_dir, memory_dir] {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("inspect memory path: {error}")),
        };
        if metadata.file_type().is_symlink() {
            return Err("invalid memory path: symlinked directories are not allowed".to_owned());
        }
    }
    Ok(())
}

fn canonical_cwd_string(cwd: &str) -> Result<String, String> {
    path_safety::canonicalize_user_dir("cwd", cwd)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|err| format!("invalid cwd: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn read_memory_rejects_selected_file_symlink() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().unwrap();
        let cwd = home.path().join("work");
        std::fs::create_dir(&cwd).unwrap();
        let canonical_cwd = canonical_cwd_string(&cwd.to_string_lossy()).unwrap();
        let memory_dir = memory_dir(home.path(), &canonical_cwd);
        std::fs::create_dir_all(&memory_dir).unwrap();
        let outside = home.path().join("outside.md");
        std::fs::write(&outside, "outside").unwrap();
        symlink(&outside, memory_dir.join("fact.md")).unwrap();

        let error = read_memory(home.path(), &cwd.to_string_lossy(), "fact.md")
            .await
            .expect_err("selected file symlink must fail");
        assert!(error.contains("regular file"), "{error}");
    }
}
