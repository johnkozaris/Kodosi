use std::path::{Path, PathBuf};

#[must_use]
pub fn resolve(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command.contains('/') || command_path.is_absolute() {
        return executable_candidate(command_path);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .find_map(|directory| executable_candidate(&directory.join(command)))
    })
}

fn executable_candidate(path: &Path) -> Option<PathBuf> {
    is_executable(path).then(|| path.to_path_buf())
}

fn is_executable(path: &Path) -> bool {
    path.is_file()
        && rustix::fs::accessat(
            rustix::fs::CWD,
            path,
            rustix::fs::Access::EXEC_OK,
            rustix::fs::AtFlags::EACCESS,
        )
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn resolution_skips_non_executable_path_candidates() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let blocked = first.path().join("mcp-tool");
        let executable = second.path().join("mcp-tool");
        std::fs::write(&blocked, b"blocked").unwrap();
        std::fs::write(&executable, b"executable").unwrap();
        std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();

        assert_eq!(
            std::env::split_paths(&path)
                .find_map(|directory| executable_candidate(&directory.join("mcp-tool"))),
            Some(executable)
        );
        assert_eq!(executable_candidate(&blocked), None);
    }

    #[test]
    fn resolution_uses_effective_owner_permissions_not_any_execute_bit() {
        if rustix::process::geteuid().is_root() {
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let executable_for_others_only = dir.path().join("other-exec");
        std::fs::write(&executable_for_others_only, b"blocked").unwrap();
        std::fs::set_permissions(
            &executable_for_others_only,
            std::fs::Permissions::from_mode(0o001),
        )
        .unwrap();

        assert_eq!(
            executable_candidate(&executable_for_others_only),
            None,
            "the owner class applies to the current effective identity even when an unrelated execute bit is set"
        );
    }
}
