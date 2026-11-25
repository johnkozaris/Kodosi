use std::path::PathBuf;

use shared_browser::link::Address;

/// The host auth token is passed via the `link` file rather than a CLI flag because process
/// arguments are readable by other local users.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Options {
    pub profile: PathBuf,
    pub link: PathBuf,
    pub debug_port: u16,
}

impl Options {
    pub(crate) fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut profile = None;
        let mut link = None;
        let mut debug_port = None;
        for argument in arguments {
            if let Some(value) = argument.strip_prefix("--kodosi-profile=") {
                profile = Some(PathBuf::from(value));
            } else if let Some(value) = argument.strip_prefix("--kodosi-link=") {
                link = Some(PathBuf::from(value));
            } else if let Some(value) = argument.strip_prefix("--kodosi-debug-port=") {
                debug_port = value.parse().ok();
            }
        }
        Ok(Self {
            profile: profile.ok_or("no --kodosi-profile")?,
            link: link.ok_or("no --kodosi-link")?,
            debug_port: debug_port.ok_or("no --kodosi-debug-port")?,
        })
    }

    /// Reads the host address and immediately unlinks the file to limit how long the token
    /// stays on disk.
    pub(crate) fn take_address(&self) -> Result<Address, String> {
        let text = std::fs::read_to_string(&self.link)
            .map_err(|problem| format!("no link file: {problem}"))?;
        drop(std::fs::remove_file(&self.link));
        serde_json::from_str(&text).map_err(|problem| format!("a broken link file: {problem}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_link_file_is_read_once() {
        let folder = std::env::temp_dir().join(format!("kodosi-link-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("link.json");
        std::fs::write(&file, r#"{"port":4000,"token":"abc"}"#).unwrap();
        let options = Options {
            profile: folder.clone(),
            link: file.clone(),
            debug_port: 1,
        };
        assert_eq!(
            options.take_address().unwrap(),
            Address {
                port: 4000,
                token: "abc".into()
            }
        );
        assert!(!file.exists());
        assert!(options.take_address().is_err());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
