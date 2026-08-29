use std::path::Path;

use super::dto::{ClaudeProjectRef, MemoryRef, SessionRef};
use super::io::format_rfc3339;
use super::memory_dir::SelectedMemoryDirectory;
use super::path_safety::{self, PathSafetyError};

const MAX_PROJECT_ENTRIES: usize = 4_096;

fn bounded_sorted_entries(dir: &Path, label: &str) -> Result<Vec<std::fs::DirEntry>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("read {label}: {error}")),
    };
    let mut entries = entries
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read {label} entry: {error}"))?;
    if entries.len() > MAX_PROJECT_ENTRIES {
        return Err(format!(
            "{label} exceeds the {MAX_PROJECT_ENTRIES}-entry safety limit"
        ));
    }
    entries.sort_by_key(std::fs::DirEntry::file_name);
    Ok(entries)
}

fn reject_symlink(path: &Path, label: &str) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("{label} must not be a symlink"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {label}: {error}")),
    }
}

pub async fn list_claude_projects(home: &Path) -> Result<Vec<ClaudeProjectRef>, String> {
    let home = home.to_owned();
    let dir = home.join(".claude").join("projects");
    tokio::task::spawn_blocking(move || -> Result<Vec<ClaudeProjectRef>, String> {
        let mut out = Vec::new();
        for entry in bounded_sorted_entries(&dir, "Claude projects")? {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let Some(slug) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                continue;
            };
            if path_safety::validate_component("slug", &slug).is_err() {
                continue;
            }
            let project_dir = entry.path();
            let memory_count = count_with_extension(&project_dir.join("memory"), "md")?;
            let (session_count, cwd) = project_session_summary(&project_dir)?;
            let label = cwd
                .as_deref()
                .map_or_else(|| slug.clone(), |cwd| display_project_cwd(&home, cwd));
            out.push(ClaudeProjectRef {
                slug,
                label,
                memory_count,
                session_count,
            });
        }

        out.sort_by(|a, b| a.label.cmp(&b.label));
        Ok(out)
    })
    .await
    .map_err(|error| format!("list-projects task join: {error}"))?
}

fn project_session_summary(project_dir: &Path) -> Result<(u32, Option<String>), String> {
    let entries = bounded_sorted_entries(project_dir, "project sessions")?;
    let mut count = 0_u32;
    let mut cwd = None;
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
            continue;
        }
        count = count.saturating_add(1);
        if cwd.is_none() {
            cwd = read_session_cwd(&path);
        }
    }
    Ok((count, cwd))
}

fn read_session_cwd(path: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};

    let file = std::fs::File::open(path).ok()?;
    let reader = BufReader::new(file.take(256 * 1024));
    for line in reader.lines().take(32).map_while(Result::ok) {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if let Some(cwd) = value.get("cwd").and_then(serde_json::Value::as_str) {
            return Some(cwd.to_owned());
        }
    }
    None
}

fn display_project_cwd(home: &Path, cwd: &str) -> String {
    let cwd_path = Path::new(cwd);
    match cwd_path.strip_prefix(home) {
        Ok(relative) if relative.as_os_str().is_empty() => "~".to_owned(),
        Ok(relative) => format!("~/{}", relative.display()),
        Err(_) => cwd.to_owned(),
    }
}

pub async fn list_project_sessions(home: &Path, slug: &str) -> Result<Vec<SessionRef>, String> {
    path_safety::validate_component("slug", slug).map_err(|e| e.to_string())?;
    let projects_root = home.join(".claude").join("projects");
    let dir = match path_safety::join_under_root(&projects_root, slug, None) {
        Ok(p) => p,
        Err(PathSafetyError::NotFound) => return Ok(Vec::new()),
        Err(err) => return Err(format!("invalid slug: {err}")),
    };

    tokio::task::spawn_blocking(move || -> Result<Vec<SessionRef>, String> {
        let mut out = Vec::new();
        for entry in bounded_sorted_entries(&dir, "project sessions")? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                tracing::debug!(path = %path.display(), "project-sessions: skipping symlink");
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path_safety::validate_uuid("sessionId", stem).is_err() {
                continue;
            }
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(error) => {
                    tracing::debug!(path = %path.display(), %error, "project-sessions: metadata error");
                    continue;
                }
            };
            let modified_at = meta.modified().ok().and_then(format_rfc3339);
            let started_at = read_first_timestamp(&path);
            out.push(SessionRef {
                id: stem.to_owned(),
                size_bytes: meta.len(),
                modified_at,
                started_at,
            });
        }
        out.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        Ok(out)
    })
    .await
    .map_err(|error| format!("project-sessions task join: {error}"))?
}

pub async fn list_project_memories(home: &Path, slug: &str) -> Result<Vec<MemoryRef>, String> {
    path_safety::validate_component("slug", slug).map_err(|e| e.to_string())?;
    let projects_root = home.join(".claude").join("projects");
    let raw_project_dir = projects_root.join(slug);
    reject_symlink(&raw_project_dir, "project directory")?;
    let project_dir = match path_safety::join_under_root(&projects_root, slug, None) {
        Ok(p) => p,
        Err(PathSafetyError::NotFound) => return Ok(Vec::new()),
        Err(err) => return Err(format!("invalid slug: {err}")),
    };
    let dir = project_dir.join("memory");
    let metadata = match std::fs::symlink_metadata(&dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("inspect project memory directory: {error}")),
    };
    if metadata.file_type().is_symlink() {
        return Err("project memory directory must not be a symlink".to_owned());
    }
    let canonical_dir = path_safety::enforce_under_root(&project_dir, &dir)
        .map_err(|error| format!("invalid project memory directory: {error}"))?;
    tokio::task::spawn_blocking(move || -> Result<Vec<MemoryRef>, String> {
        let mut out = Vec::new();
        for entry in bounded_sorted_entries(&canonical_dir, "project memories")? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                tracing::debug!(path = %path.display(), "project-memories: skipping symlink");
                continue;
            }
            let Some(filename) = path.file_name().and_then(|n| n.to_str()).map(ToOwned::to_owned)
            else {
                continue;
            };
            let meta = match entry.metadata() {
                Ok(m) => m,
                Err(error) => {
                    tracing::debug!(path = %path.display(), %error, "project-memories: metadata error");
                    continue;
                }
            };
            let modified_at = meta.modified().ok().and_then(format_rfc3339);
            let (name, description, kind) = read_memory_frontmatter(&path);
            out.push(MemoryRef {
                filename,
                name,
                description,
                kind,
                size_bytes: meta.len(),
                modified_at,
            });
        }
        out.sort_by(|a, b| {
            let a_index = a.filename == "MEMORY.md";
            let b_index = b.filename == "MEMORY.md";
            match (a_index, b_index) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.filename.cmp(&b.filename),
            }
        });
        Ok(out)
    })
    .await
    .map_err(|error| format!("project-memories task join: {error}"))?
}

pub async fn read_project_memory(
    home: &Path,
    slug: &str,
    filename: &str,
) -> Result<String, String> {
    path_safety::validate_component("slug", slug).map_err(|e| e.to_string())?;
    path_safety::validate_component("filename", filename).map_err(|e| e.to_string())?;
    if !Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err("memory filename must end in .md".to_owned());
    }
    let projects_root = home.join(".claude").join("projects");
    let raw_project_dir = projects_root.join(slug);
    reject_symlink(&raw_project_dir, "project directory")?;
    let project_dir = match path_safety::join_under_root(&projects_root, slug, None) {
        Ok(p) => p,
        Err(PathSafetyError::NotFound) => return Err("project not found".to_owned()),
        Err(err) => return Err(format!("invalid slug: {err}")),
    };
    let filename = filename.to_owned();
    tokio::task::spawn_blocking(move || {
        SelectedMemoryDirectory::open(&project_dir)?.read_string(&filename)
    })
    .await
    .map_err(|error| format!("read project memory task join: {error}"))?
}

pub async fn copy_project_memory(
    home: &Path,
    source_slug: &str,
    filename: &str,
    target_slug: &str,
) -> Result<String, String> {
    path_safety::validate_component("source_slug", source_slug).map_err(|e| e.to_string())?;
    path_safety::validate_component("filename", filename).map_err(|e| e.to_string())?;
    path_safety::validate_component("target_slug", target_slug).map_err(|e| e.to_string())?;
    if !Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return Err("memory filename must end in .md".to_owned());
    }
    if source_slug == target_slug {
        return Err("source and target projects are the same".to_owned());
    }
    let projects_root = home.join(".claude").join("projects");
    let raw_source_project = projects_root.join(source_slug);
    reject_symlink(&raw_source_project, "source project directory")?;
    let source_project = match path_safety::join_under_root(&projects_root, source_slug, None) {
        Ok(p) => p,
        Err(PathSafetyError::NotFound) => return Err("source project not found".to_owned()),
        Err(err) => return Err(format!("invalid source slug: {err}")),
    };
    let source_project_for_read = source_project.clone();
    let source_filename = filename.to_owned();
    let content = tokio::task::spawn_blocking(move || {
        SelectedMemoryDirectory::open(&source_project_for_read)?.read_string(&source_filename)
    })
    .await
    .map_err(|error| format!("read source memory task join: {error}"))??;
    let raw_target_project = projects_root.join(target_slug);
    reject_symlink(&raw_target_project, "target project directory")?;
    let target_project = match path_safety::join_under_root(&projects_root, target_slug, None) {
        Ok(p) => p,
        Err(PathSafetyError::NotFound) => return Err("target project not found".to_owned()),
        Err(err) => return Err(format!("invalid target slug: {err}")),
    };
    let target_slug_owned = target_slug.to_owned();
    let filename_owned = filename.to_owned();
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let memory = SelectedMemoryDirectory::open_or_create(&target_project)?;
        memory
            .create_new(&filename_owned, &content)
            .map_err(|error| {
                if error.contains("File exists") || error.contains("exist") {
                    format!("{filename_owned} already exists in target project")
                } else {
                    error
                }
            })?;
        Ok(target_slug_owned)
    })
    .await
    .map_err(|error| format!("copy-memory task join: {error}"))?
}

fn count_with_extension(dir: &Path, ext: &str) -> Result<u32, String> {
    let count = bounded_sorted_entries(dir, "project memory files")?
        .into_iter()
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some(ext))
        .count();
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

fn read_memory_frontmatter(path: &Path) -> (Option<String>, Option<String>, Option<String>) {
    use std::io::{BufRead, BufReader};
    let Ok(file) = std::fs::File::open(path) else {
        return (None, None, None);
    };
    let mut reader = BufReader::new(file);
    let mut first = String::new();
    if reader.read_line(&mut first).is_err() || first.trim() != "---" {
        return (None, None, None);
    }
    let mut name = None;
    let mut description = None;
    let mut kind = None;
    for _ in 0..32 {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed == "---" {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("name:") {
            name = Some(value.trim().to_owned());
        } else if let Some(value) = trimmed.strip_prefix("description:") {
            description = Some(value.trim().to_owned());
        } else if let Some(value) = trimmed.strip_prefix("type:") {
            kind = Some(value.trim().to_owned());
        }
    }
    (name, description, kind)
}

fn read_first_timestamp(path: &Path) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file.take(64 * 1024));
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    value
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn project_entry_scan_accepts_limit_and_rejects_overflow() {
        let dir = tempdir().unwrap();
        for index in 0..MAX_PROJECT_ENTRIES {
            std::fs::write(dir.path().join(format!("{index:04}")), "").unwrap();
        }
        let entries = bounded_sorted_entries(dir.path(), "test entries").unwrap();
        assert_eq!(entries.len(), MAX_PROJECT_ENTRIES);
        assert_eq!(entries[0].file_name(), "0000");
        std::fs::write(dir.path().join("overflow"), "").unwrap();
        let error = bounded_sorted_entries(dir.path(), "test entries")
            .expect_err("over-limit directory must fail closed");
        assert!(error.contains("4096-entry safety limit"), "{error}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn read_and_copy_reject_same_root_memory_symlinks() -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let home = tempdir().map_err(|error| error.to_string())?;
        let projects = home.path().join(".claude").join("projects");
        let source = projects.join("source");
        let other = projects.join("other");
        let target = projects.join("target");
        std::fs::create_dir_all(&source).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(other.join("memory")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        std::fs::write(other.join("memory").join("fact.md"), "other fact")
            .map_err(|error| error.to_string())?;
        symlink(other.join("memory"), source.join("memory")).map_err(|error| error.to_string())?;

        assert!(
            read_project_memory(home.path(), "source", "fact.md")
                .await
                .expect_err("directory symlink must fail")
                .contains("must not be a symlink")
        );
        std::fs::remove_file(source.join("memory")).map_err(|error| error.to_string())?;
        std::fs::create_dir(source.join("memory")).map_err(|error| error.to_string())?;
        symlink(
            other.join("memory").join("fact.md"),
            source.join("memory").join("fact.md"),
        )
        .map_err(|error| error.to_string())?;
        assert!(
            read_project_memory(home.path(), "source", "fact.md")
                .await
                .expect_err("file symlink must fail")
                .contains("must be a regular file")
        );
        assert!(
            copy_project_memory(home.path(), "source", "fact.md", "target")
                .await
                .expect_err("copy source symlink must fail")
                .contains("must be a regular file")
        );
        assert!(!target.join("memory").join("fact.md").exists());
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn copy_memory_rejects_target_memory_symlink_escape() -> Result<(), String> {
        use std::os::unix::fs::symlink;

        let home = tempdir().map_err(|error| error.to_string())?;
        let projects = home.path().join(".claude").join("projects");
        let source = projects.join("source");
        let target = projects.join("target");
        let outside = home.path().join("outside");
        std::fs::create_dir_all(source.join("memory")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&outside).map_err(|error| error.to_string())?;
        std::fs::write(source.join("memory").join("fact.md"), "secret-free fact")
            .map_err(|error| error.to_string())?;
        symlink(&outside, target.join("memory")).map_err(|error| error.to_string())?;

        let error = copy_project_memory(home.path(), "source", "fact.md", "target")
            .await
            .expect_err("symlink escape must be rejected");
        assert!(error.contains("must not be a symlink"));
        assert!(!outside.join("fact.md").exists());
        Ok(())
    }

    #[tokio::test]
    async fn copy_memory_creates_contained_target_directory() -> Result<(), String> {
        let home = tempdir().map_err(|error| error.to_string())?;
        let projects = home.path().join(".claude").join("projects");
        let source = projects.join("source");
        let target = projects.join("target");
        std::fs::create_dir_all(source.join("memory")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
        std::fs::write(source.join("memory").join("fact.md"), "fact")
            .map_err(|error| error.to_string())?;

        assert_eq!(
            copy_project_memory(home.path(), "source", "fact.md", "target").await?,
            "target"
        );
        assert_eq!(
            std::fs::read_to_string(target.join("memory").join("fact.md"))
                .map_err(|error| error.to_string())?,
            "fact"
        );
        Ok(())
    }

    #[tokio::test]
    async fn project_labels_use_transcript_cwd() -> Result<(), String> {
        let home = tempdir().map_err(|error| error.to_string())?;
        let project_dir = home
            .path()
            .join(".claude")
            .join("projects")
            .join("-Users-john-Repos-Kodosi");
        std::fs::create_dir_all(&project_dir).map_err(|error| error.to_string())?;
        let cwd = home.path().join("Repos").join("Kodosi");
        std::fs::write(
            project_dir.join("session.jsonl"),
            format!(r#"{{"type":"user","cwd":"{}"}}"#, cwd.display()),
        )
        .map_err(|error| error.to_string())?;

        let projects = list_claude_projects(home.path()).await?;

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].label, "~/Repos/Kodosi");
        assert_eq!(projects[0].session_count, 1);
        Ok(())
    }
}
