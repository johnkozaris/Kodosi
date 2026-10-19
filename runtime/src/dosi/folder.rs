use std::path::{Path, PathBuf};

pub(crate) const DEFAULT_INSTRUCTIONS: &str =
    include_str!("../../skills/kodosi-dosi/INSTRUCTIONS.md");
pub(crate) const SKILL: &str = include_str!("../../skills/kodosi-dosi/SKILL.md");

pub(super) fn path(home: &Path) -> PathBuf {
    home.join(".kodosi").join("dosi")
}

fn reference() -> &'static str {
    SKILL
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(SKILL, |(_, body)| body)
        .trim_start()
}

pub(super) fn agents_file(instructions: &str) -> String {
    format!("{}\n\n{}", instructions.trim_end(), reference())
}

pub(super) fn prepare(folder: &Path, instructions: &str) -> std::io::Result<()> {
    if let Some(parent) = folder.parent() {
        kodosi_os::fs::create_private_dir(parent)?;
    }
    kodosi_os::fs::create_private_dir(folder)?;
    for place in ["notes", "skills", "tools"] {
        kodosi_os::fs::create_private_dir(&folder.join(place))?;
    }
    let watching = folder.join("watching.md");
    if !watching.exists() {
        kodosi_os::fs::write_private(&watching, b"")?;
    }
    write_if_changed(&folder.join("AGENTS.md"), &agents_file(instructions))?;
    write_if_changed(&folder.join("CLAUDE.md"), "@AGENTS.md\n")
}

fn write_if_changed(path: &Path, text: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|current| current == text) {
        return Ok(());
    }
    kodosi_os::fs::write_private(path, text.as_bytes())
}
