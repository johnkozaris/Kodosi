use std::path::PathBuf;

use shared_browser::link::Address;

/// What the host gives the engine on its command line. The token is in a file of the profile,
/// since other people on the computer can read a command line.
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

    /// Reads the host's address and removes the file, so the token is on disk only until the
    /// engine starts.
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
    fn the_options_come_from_the_kodosi_switches_and_others_go_to_chromium() {
        let options = Options::parse(
            [
                "engine",
                "--kodosi-profile=/p",
                "--use-mock-keychain",
                "--kodosi-link=/p/link.json",
                "--kodosi-debug-port=9333",
            ]
            .map(String::from),
        )
        .unwrap();
        assert_eq!(
            options,
            Options {
                profile: PathBuf::from("/p"),
                link: PathBuf::from("/p/link.json"),
                debug_port: 9333,
            }
        );
        assert!(Options::parse(["engine".to_owned()]).is_err());
    }

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
