use std::io::Read;
use std::path::Path;

const MAX_MEMORY_FILES: usize = 4096;
const MAX_MEMORY_FRONTMATTER_BYTES: u64 = 64 * 1024;

pub fn list_memory_files(memory_dir: &Path) -> Vec<MemoryFileInfo> {
    let Ok(entries) = std::fs::read_dir(memory_dir) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for entry in entries.flatten().take(MAX_MEMORY_FILES) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();

        let memory_type = read_frontmatter_type(&path);

        files.push(MemoryFileInfo {
            filename,
            memory_type,
            path: Some(path.to_string_lossy().into_owned()),
        });
    }

    files.sort_by(|a, b| {
        let a_is_index = a.filename == "MEMORY.md";
        let b_is_index = b.filename == "MEMORY.md";
        b_is_index
            .cmp(&a_is_index)
            .then(a.filename.cmp(&b.filename))
    });

    files
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFileInfo {
    pub filename: String,
    pub memory_type: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

fn read_frontmatter_type(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut content = String::new();
    file.take(MAX_MEMORY_FRONTMATTER_BYTES.saturating_add(1))
        .read_to_string(&mut content)
        .ok()?;
    if u64::try_from(content.len()).unwrap_or(u64::MAX) > MAX_MEMORY_FRONTMATTER_BYTES {
        return None;
    }
    extract_frontmatter_type(&content)
}

fn extract_frontmatter_type(content: &str) -> Option<String> {
    if !content.starts_with("---") {
        return None;
    }
    let end = content[3..].find("---")?;
    let frontmatter = &content[3..3 + end];
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("type:") {
            return Some(value.trim().to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_skips_symlinks_and_bounds_frontmatter_reads() {
        let dir = tempfile::tempdir().unwrap();
        let oversized = dir.path().join("large.md");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len(MAX_MEMORY_FRONTMATTER_BYTES + 1).unwrap();
        std::fs::write(dir.path().join("small.md"), "---\ntype: project\n---\nbody").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("small.md"), dir.path().join("link.md"))
            .unwrap();

        let files = list_memory_files(dir.path());

        assert_eq!(
            files
                .iter()
                .find(|file| file.filename == "small.md")
                .unwrap()
                .memory_type
                .as_deref(),
            Some("project")
        );
        assert!(
            files
                .iter()
                .find(|file| file.filename == "large.md")
                .unwrap()
                .memory_type
                .is_none()
        );
        #[cfg(unix)]
        assert!(!files.iter().any(|file| file.filename == "link.md"));
    }

    #[test]
    fn extracts_frontmatter_type() {
        let content = "---\nname: test\ntype: feedback\n---\nContent here";
        assert_eq!(
            extract_frontmatter_type(content),
            Some("feedback".to_owned())
        );
    }
}
