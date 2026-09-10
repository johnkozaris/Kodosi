use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AgentTarget {
    Claude,
    VsCode,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgentFile {
    pub path: PathBuf,
    pub target: AgentTarget,

    pub name: String,

    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disallowed_tools: Vec<String>,

    pub frontmatter_raw: String,

    pub body_raw: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

impl CustomAgentFile {
    pub fn parse(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|error| format!("metadata {}: {error}", path.display()))?;
        if !metadata.is_file() {
            return Err(format!("{} is not a regular file", path.display()));
        }
        if metadata.len() > MAX_CUSTOM_AGENT_FILE_BYTES {
            return Ok(Self::degraded(
                path.to_owned(),
                format!("custom agent file exceeds {MAX_CUSTOM_AGENT_FILE_BYTES} byte limit"),
            ));
        }
        let file = std::fs::File::open(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let mut contents = String::new();
        file.take(MAX_CUSTOM_AGENT_FILE_BYTES.saturating_add(1))
            .read_to_string(&mut contents)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_CUSTOM_AGENT_FILE_BYTES {
            return Ok(Self::degraded(
                path.to_owned(),
                format!("custom agent file exceeds {MAX_CUSTOM_AGENT_FILE_BYTES} byte limit"),
            ));
        }
        Ok(Self::from_contents(path.to_owned(), &contents))
    }

    pub fn from_contents(path: PathBuf, contents: &str) -> Self {
        let target = target_for_path(&path);
        if u64::try_from(contents.len()).unwrap_or(u64::MAX) > MAX_CUSTOM_AGENT_FILE_BYTES {
            return Self::degraded(
                path,
                format!("custom agent file exceeds {MAX_CUSTOM_AGENT_FILE_BYTES} byte limit"),
            );
        }
        let (frontmatter, body) = match split_frontmatter_bounded(contents) {
            Ok(parts) => parts,
            Err(error) => return Self::degraded(path, error),
        };
        let name = scan_scalar(&frontmatter, "name").unwrap_or_default();
        let description = scan_scalar(&frontmatter, "description").unwrap_or_default();
        let model = scan_scalar(&frontmatter, "model");
        let tools = scan_sequence(&frontmatter, "tools");
        let disallowed_tools = scan_sequence(&frontmatter, "disallowedTools");
        let mut errors = Vec::new();
        if name.is_empty() {
            errors.push("missing required frontmatter field: name".to_owned());
        }
        if description.is_empty() {
            errors.push("missing required frontmatter field: description".to_owned());
        }
        Self {
            path,
            target,
            name,
            description,
            model,
            tools,
            disallowed_tools,
            frontmatter_raw: frontmatter,
            body_raw: body,
            errors,
        }
    }

    fn degraded(path: PathBuf, error: String) -> Self {
        let target = target_for_path(&path);
        let name = fallback_agent_name(&path);
        Self {
            path,
            target,
            name,
            description: String::new(),
            model: None,
            tools: Vec::new(),
            disallowed_tools: Vec::new(),
            frontmatter_raw: String::new(),
            body_raw: String::new(),
            errors: vec![error],
        }
    }
}

const MAX_CUSTOM_AGENT_FILE_BYTES: u64 = 1024 * 1024;
const MAX_CUSTOM_AGENT_FRONTMATTER_BYTES: usize = 64 * 1024;
pub const MAX_CUSTOM_AGENT_FILES: usize = 4096;

#[derive(Debug, Clone, Copy)]
pub enum AgentFileConvention {
    ClaudeMarkdown,
    CopilotAgentMarkdown,
}

pub fn find_custom_agent_files(root: &Path, convention: AgentFileConvention) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .take(MAX_CUSTOM_AGENT_FILES)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return None;
            }
            let name = path.file_name()?.to_str()?;
            let name_lower = name.to_ascii_lowercase();
            let is_agent_md = name_lower.ends_with(".agent.md");
            let is_md = Path::new(name)
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
            let accepted = match convention {
                AgentFileConvention::ClaudeMarkdown => is_md,
                AgentFileConvention::CopilotAgentMarkdown => is_agent_md,
            };
            accepted.then_some(path)
        })
        .collect();
    out.sort();
    out
}

fn target_for_path(path: &Path) -> AgentTarget {
    let filename = path
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if filename.ends_with(".agent.md") {
        return AgentTarget::VsCode;
    }

    let has_claude_agents = path
        .components()
        .collect::<Vec<_>>()
        .windows(2)
        .any(|pair| {
            pair[0].as_os_str().eq_ignore_ascii_case(".claude")
                && pair[1].as_os_str().eq_ignore_ascii_case("agents")
        });
    if has_claude_agents {
        return AgentTarget::Claude;
    }
    AgentTarget::Unknown
}

fn fallback_agent_name(path: &Path) -> String {
    path.file_name()
        .map(|name| {
            name.to_string_lossy()
                .trim_end_matches(".agent.md")
                .trim_end_matches(".md")
                .to_owned()
        })
        .unwrap_or_default()
}

#[cfg(test)]
fn split_frontmatter(contents: &str) -> (String, String) {
    split_frontmatter_bounded(contents).unwrap_or_else(|_| (String::new(), contents.to_owned()))
}

fn split_frontmatter_bounded(contents: &str) -> Result<(String, String), String> {
    let trimmed = contents.trim_start_matches('\u{feff}');
    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return Ok((String::new(), String::new()));
    };
    if first.trim() != "---" {
        return Ok((String::new(), contents.to_owned()));
    }
    let mut frontmatter = String::new();
    let mut found_close = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            found_close = true;
            break;
        }
        if frontmatter
            .len()
            .saturating_add(line.len())
            .saturating_add(1)
            > MAX_CUSTOM_AGENT_FRONTMATTER_BYTES
        {
            return Err(format!(
                "custom agent frontmatter exceeds {MAX_CUSTOM_AGENT_FRONTMATTER_BYTES} byte limit"
            ));
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    if !found_close {
        return Ok((String::new(), contents.to_owned()));
    }
    let body: String = lines.collect::<Vec<_>>().join("\n");
    Ok((frontmatter, body))
}

fn scan_sequence(frontmatter: &str, key: &str) -> Vec<String> {
    let needle = format!("{key}:");
    let mut lines = frontmatter.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(&needle) else {
            continue;
        };
        let value = rest.trim();
        if let Some(inner) = value.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            return inner
                .split(',')
                .map(str::trim)
                .map(|s| s.trim_matches('"').trim_matches('\'').to_owned())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if !value.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for next in lines.by_ref() {
            let stripped = next.trim_start();
            if stripped.is_empty() {
                continue;
            }
            let Some(item) = stripped.strip_prefix("- ") else {
                break;
            };
            let cleaned = item.trim().trim_matches('"').trim_matches('\'').to_owned();
            if !cleaned.is_empty() {
                out.push(cleaned);
            }
        }
        return out;
    }
    Vec::new()
}

fn scan_scalar(frontmatter: &str, key: &str) -> Option<String> {
    let needle = format!("{key}:");
    for raw_line in frontmatter.lines() {
        let line = raw_line.trim_start();
        let Some(rest) = line.strip_prefix(&needle) else {
            continue;
        };
        let value = rest.trim();
        if value.is_empty()
            || value.starts_with('-')
            || value.starts_with('|')
            || value.starts_with('>')
        {
            continue;
        }
        let cleaned = value.trim_matches('"').trim_matches('\'').trim().to_owned();
        if cleaned.is_empty() {
            continue;
        }
        return Some(cleaned);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_claude_agent_frontmatter() {
        let path = PathBuf::from("/home/u/.claude/agents/researcher.md");
        let contents = "---\nname: researcher\ndescription: Reads codebases\nmodel: sonnet\n---\n# Researcher\n\nDoes research.";
        let parsed = CustomAgentFile::from_contents(path, contents);
        assert_eq!(parsed.target, AgentTarget::Claude);
        assert_eq!(parsed.name, "researcher");
        assert_eq!(parsed.description, "Reads codebases");
        assert_eq!(parsed.model.as_deref(), Some("sonnet"));
        assert!(parsed.body_raw.contains("Does research"));
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn vscode_target_detected_from_dot_agent_md_suffix() {
        let path = PathBuf::from("/tmp/my-agent.agent.md");
        let contents = "---\nname: x\ndescription: y\n---\nbody";
        let parsed = CustomAgentFile::from_contents(path, contents);
        assert_eq!(parsed.target, AgentTarget::VsCode);
    }

    #[test]
    fn missing_required_fields_recorded_as_errors() {
        let path = PathBuf::from("/tmp/x.agent.md");
        let parsed = CustomAgentFile::from_contents(path, "---\nmodel: opus\n---\n");
        assert_eq!(parsed.errors.len(), 2);
        assert!(parsed.errors.iter().any(|e| e.contains("name")));
        assert!(parsed.errors.iter().any(|e| e.contains("description")));
    }

    #[test]
    fn unquoted_quoted_and_whitespace_scalars_all_parse() {
        let frontmatter = "name: bare\ndescription: \"quoted desc\"\nmodel: '  sonnet  '\n";
        assert_eq!(scan_scalar(frontmatter, "name").as_deref(), Some("bare"));
        assert_eq!(
            scan_scalar(frontmatter, "description").as_deref(),
            Some("quoted desc")
        );
        assert_eq!(scan_scalar(frontmatter, "model").as_deref(), Some("sonnet"));
    }

    #[test]
    fn sequence_form_is_skipped_by_scalar_scanner() {
        let frontmatter = "tools:\n  - Read\n  - Edit\n";
        assert!(scan_scalar(frontmatter, "tools").is_none());
    }

    #[test]
    fn block_sequence_extracted() {
        let frontmatter = "name: x\ntools:\n  - Read\n  - Edit\n  - Bash\nmodel: opus\n";
        assert_eq!(
            scan_sequence(frontmatter, "tools"),
            vec!["Read".to_string(), "Edit".to_string(), "Bash".to_string()]
        );
    }

    #[test]
    fn inline_flow_sequence_extracted() {
        let frontmatter = r#"tools: [Read, "Edit", 'Bash']"#;
        assert_eq!(
            scan_sequence(frontmatter, "tools"),
            vec!["Read".to_string(), "Edit".to_string(), "Bash".to_string()]
        );
    }

    #[test]
    fn missing_sequence_returns_empty() {
        let frontmatter = "name: x\ndescription: y\n";
        assert!(scan_sequence(frontmatter, "tools").is_empty());
    }

    #[test]
    fn frontmatter_with_tools_round_trips_through_parse() {
        let path = PathBuf::from("/tmp/x.agent.md");
        let contents = "---\nname: x\ndescription: y\ntools:\n  - Read\n  - Bash\n---\nbody";
        let parsed = CustomAgentFile::from_contents(path, contents);
        assert_eq!(parsed.tools, vec!["Read".to_string(), "Bash".to_string()]);
    }

    #[test]
    fn disallowed_tools_are_parsed_by_the_same_canonical_scanner() {
        let path = PathBuf::from("/home/u/.claude/agents/safe.md");
        let contents =
            "---\nname: safe\ndescription: Safe agent\ndisallowedTools: [Bash, Write]\n---\n";
        let parsed = CustomAgentFile::from_contents(path, contents);
        assert_eq!(parsed.disallowed_tools, vec!["Bash", "Write"]);
    }

    #[test]
    fn frontmatter_split_handles_missing_close_marker() {
        let (fm, body) = split_frontmatter("---\nname: x\nbody-without-close");
        assert!(fm.is_empty());
        assert!(body.contains("body-without-close"));
    }

    #[test]
    fn file_without_frontmatter_returns_body_only() {
        let (fm, body) = split_frontmatter("# Title\nNo frontmatter here.");
        assert!(fm.is_empty());
        assert!(body.contains("Title"));
    }

    #[test]
    fn finds_agent_files_in_directory() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agents_dir = temp.path().join("agents");
        std::fs::create_dir(&agents_dir).expect("mkdir");
        std::fs::write(
            agents_dir.join("a.md"),
            "---\nname: a\ndescription: d\n---\n",
        )
        .unwrap();
        std::fs::write(
            agents_dir.join("b.agent.md"),
            "---\nname: b\ndescription: d\n---\n",
        )
        .unwrap();
        std::fs::write(agents_dir.join("readme.txt"), "not an agent").unwrap();
        let found = find_custom_agent_files(&agents_dir, AgentFileConvention::ClaudeMarkdown);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn copilot_convention_excludes_ordinary_markdown_case_insensitively() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agents_dir = temp.path().join("agents");
        std::fs::create_dir(&agents_dir).expect("mkdir");
        std::fs::write(agents_dir.join("README.md"), "notes").unwrap();
        std::fs::write(agents_dir.join("Reviewer.AGENT.MD"), "agent").unwrap();
        let found = find_custom_agent_files(&agents_dir, AgentFileConvention::CopilotAgentMarkdown);
        assert_eq!(found.len(), 1);
        assert_eq!(
            target_for_path(&found[0]),
            AgentTarget::VsCode,
            "case-insensitive suffix remains a Copilot target"
        );
    }

    #[test]
    fn oversized_agent_file_returns_visible_degradation_without_reading_body() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("oversized.agent.md");
        let file = std::fs::File::create(&path).expect("create");
        file.set_len(MAX_CUSTOM_AGENT_FILE_BYTES + 1)
            .expect("set length");

        let parsed = CustomAgentFile::parse(&path).expect("oversize is a visible row");
        assert_eq!(parsed.name, "oversized");
        assert!(parsed.body_raw.is_empty());
        assert!(parsed.errors[0].contains("exceeds"));
    }

    #[test]
    fn oversized_frontmatter_returns_visible_degradation() {
        let path = PathBuf::from("/repo/.github/agents/large.agent.md");
        let contents = format!(
            "---\n{}\n---\nbody",
            "x".repeat(MAX_CUSTOM_AGENT_FRONTMATTER_BYTES + 1)
        );
        let parsed = CustomAgentFile::from_contents(path, &contents);
        assert_eq!(parsed.name, "large");
        assert!(parsed.errors[0].contains("frontmatter exceeds"));
        assert!(parsed.frontmatter_raw.is_empty());
    }
}
