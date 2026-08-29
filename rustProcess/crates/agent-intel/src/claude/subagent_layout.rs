use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

const MAX_SESSION_ID_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSessionId(String);

impl TranscriptSessionId {
    pub fn parse(value: &str) -> Result<Self, InvalidTranscriptSessionId> {
        let path = Path::new(value);
        let mut components = path.components();
        let valid_component =
            matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
        if value.is_empty()
            || value.len() > MAX_SESSION_ID_BYTES
            || !valid_component
            || value == "."
            || value == ".."
            || value.contains('/')
            || value.contains('\\')
        {
            return Err(InvalidTranscriptSessionId);
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidTranscriptSessionId;

impl fmt::Display for InvalidTranscriptSessionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("transcript session id must be one bounded path component")
    }
}

impl std::error::Error for InvalidTranscriptSessionId {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubagentTranscript {
    pub agent_id: String,
    pub path: PathBuf,

    pub workflow_run_id: Option<String>,
}

pub fn find_subagent_transcripts(
    projects_dir: &Path,
    project_slug: &str,
    session_id: &TranscriptSessionId,
) -> Vec<SubagentTranscript> {
    let Ok(projects_root) = std::fs::canonicalize(projects_dir) else {
        return Vec::new();
    };
    let Ok(project_directory) = std::fs::canonicalize(projects_dir.join(project_slug)) else {
        return Vec::new();
    };
    if !project_directory.starts_with(&projects_root) {
        return Vec::new();
    }
    let Ok(session_root) = std::fs::canonicalize(project_directory.join(session_id.as_str()))
    else {
        return Vec::new();
    };
    if !session_root.starts_with(&project_directory) {
        return Vec::new();
    }
    let Ok(subagents_root) = std::fs::canonicalize(session_root.join("subagents")) else {
        return Vec::new();
    };
    if !subagents_root.starts_with(&session_root) {
        return Vec::new();
    }

    let mut out = Vec::new();
    collect_flat(&subagents_root, &subagents_root, None, &mut out);
    let workflows = subagents_root.join("workflows");
    if let Ok(entries) = std::fs::read_dir(&workflows) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let path = entry.path();
            let Ok(canonical) = std::fs::canonicalize(&path) else {
                continue;
            };
            if !canonical.starts_with(&subagents_root) {
                continue;
            }
            let run_id = entry.file_name().to_string_lossy().into_owned();
            collect_flat(&canonical, &subagents_root, Some(&run_id), &mut out);
        }
    }
    out.sort_by(|left, right| {
        left.agent_id
            .cmp(&right.agent_id)
            .then_with(|| left.workflow_run_id.cmp(&right.workflow_run_id))
            .then_with(|| left.path.cmp(&right.path))
    });
    out
}

fn collect_flat(
    dir: &Path,
    subagents_root: &Path,
    workflow_run_id: Option<&str>,
    out: &mut Vec<SubagentTranscript>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let Ok(canonical) = std::fs::canonicalize(&path) else {
            continue;
        };
        if !canonical.starts_with(subagents_root) {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(stripped) = name.strip_prefix("agent-") else {
            continue;
        };
        let Some(agent_id) = stripped.strip_suffix(".jsonl") else {
            continue;
        };
        if agent_id.is_empty() {
            continue;
        }
        out.push(SubagentTranscript {
            agent_id: agent_id.to_owned(),
            path,
            workflow_run_id: workflow_run_id.map(str::to_owned),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, b"{}\n").expect("write");
    }

    fn session(value: &str) -> TranscriptSessionId {
        TranscriptSessionId::parse(value).expect("valid session id")
    }

    #[test]
    fn finds_flat_and_nested_layouts_in_stable_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let projects = temp.path();
        let slug = "my-project";
        let sid = session("session-abc");
        let subagents = projects.join(slug).join(sid.as_str()).join("subagents");
        touch(&subagents.join("workflows/run-2/agent-z.jsonl"));
        touch(&subagents.join("agent-b.jsonl"));
        touch(&subagents.join("workflows/run-1/agent-a.jsonl"));
        touch(&subagents.join("agent-a.jsonl"));
        touch(&subagents.join("not-an-agent.jsonl"));
        touch(&subagents.join("agent-no-suffix"));
        touch(&subagents.join("workflows/run-1/random.txt"));

        let found = find_subagent_transcripts(projects, slug, &sid);
        let identities = found
            .iter()
            .map(|entry| (entry.agent_id.as_str(), entry.workflow_run_id.as_deref()))
            .collect::<Vec<_>>();
        assert_eq!(
            identities,
            vec![
                ("a", None),
                ("a", Some("run-1")),
                ("b", None),
                ("z", Some("run-2"))
            ]
        );
    }

    #[test]
    fn accepts_unicode_and_rejects_non_component_session_ids() {
        assert!(TranscriptSessionId::parse("セッション-α").is_ok());
        for invalid in ["", ".", "..", "../escape", "a/b", r"a\b", "/absolute"] {
            assert!(
                TranscriptSessionId::parse(invalid).is_err(),
                "{invalid:?} must be rejected"
            );
        }
        assert!(TranscriptSessionId::parse(&"x".repeat(MAX_SESSION_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn returns_empty_when_no_subagent_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let out = find_subagent_transcripts(temp.path(), "no-such", &session("no-such"));
        assert!(out.is_empty());
    }

    #[test]
    fn empty_agent_id_after_prefix_is_skipped() {
        let temp = tempfile::tempdir().expect("tempdir");
        let projects = temp.path();
        let sid = session("x");
        let subagents = projects.join("s").join(sid.as_str()).join("subagents");
        touch(&subagents.join("agent-.jsonl"));
        let out = find_subagent_transcripts(projects, "s", &sid);
        assert!(out.is_empty(), "empty agent id must not be returned");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_session_and_transcript_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let projects = temp.path().join("projects");
        let project = projects.join("slug");
        let outside = temp.path().join("outside");
        touch(&outside.join("subagents/agent-escaped.jsonl"));
        std::fs::create_dir_all(&project).expect("project");
        symlink(&outside, project.join("escaped-session")).expect("session symlink");
        assert!(
            find_subagent_transcripts(&projects, "slug", &session("escaped-session")).is_empty()
        );

        let safe = project.join("safe/subagents");
        std::fs::create_dir_all(&safe).expect("safe subagents");
        symlink(
            outside.join("subagents/agent-escaped.jsonl"),
            safe.join("agent-link.jsonl"),
        )
        .expect("file symlink");
        assert!(find_subagent_transcripts(&projects, "slug", &session("safe")).is_empty());
    }
}
