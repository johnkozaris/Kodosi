use std::process::Stdio;

use crate::{AppError, Result};

#[cfg(target_os = "macos")]
#[path = "browser_macos.rs"]
mod imp;
#[cfg(not(target_os = "macos"))]
#[path = "browser_unix.rs"]
mod imp;

fn browser_url(value: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(value).map_err(|error| AppError::InvalidUrl {
        value: value.to_owned(),
        reason: error.to_string(),
    })?;
    if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
        return Err(AppError::InvalidUrl {
            value: value.to_owned(),
            reason: "browser URL must be absolute HTTP(S) with a host".to_owned(),
        });
    }
    Ok(url)
}

pub(crate) fn open_url(url: &str) -> Result<()> {
    let url = browser_url(url)?;
    let mut command = imp::browser_command(url.as_str())?;
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());

    let mut child = command.spawn().map_err(|error| AppError::Unsupported {
        reason: format!("failed to launch browser: {error}"),
    })?;
    drop(tokio::spawn(async move {
        match child.wait().await {
            Ok(status) if status.success() => {}
            Ok(status) => {
                tracing::warn!(%status, "browser launcher exited unsuccessfully");
            }
            Err(error) => {
                tracing::warn!(%error, "browser launcher could not be reaped");
            }
        }
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::browser_url;

    #[test]
    fn accepts_http_and_https_browser_urls() {
        let https = browser_url("https://idp.example/device?user_code=A1-B2").unwrap();
        assert_eq!(https.as_str(), "https://idp.example/device?user_code=A1-B2");
        let localhost = browser_url("http://localhost:8787/callback").unwrap();
        assert_eq!(localhost.host_str(), Some("localhost"));
    }

    #[test]
    fn rejects_non_http_browser_urls() {
        for value in [
            "file:///tmp/device",
            "javascript:alert(1)",
            "/relative/device",
            "https:///",
        ] {
            assert!(browser_url(value).is_err(), "accepted {value}");
        }
    }
}
