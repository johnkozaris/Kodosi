use std::path::PathBuf;

#[must_use]
pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map_or_else(
        || PathBuf::from("/dev/null/kodosi-home-unavailable"),
        PathBuf::from,
    )
}

#[must_use]
pub fn claude_home() -> PathBuf {
    home_dir().join(".claude")
}

#[must_use]
pub fn copilot_home() -> PathBuf {
    home_dir().join(".copilot")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_home_is_under_home() {
        let home = home_dir();
        let claude = claude_home();
        assert!(
            claude.starts_with(&home),
            "{} should start with {}",
            claude.display(),
            home.display()
        );
        assert_eq!(claude.file_name().and_then(|n| n.to_str()), Some(".claude"));
    }

    #[test]
    fn copilot_home_is_under_home() {
        assert_eq!(
            copilot_home().file_name().and_then(|n| n.to_str()),
            Some(".copilot")
        );
    }
}
