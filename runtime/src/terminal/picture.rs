use aws_lc_rs::digest::{SHA256, digest};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::{Error, Result, hex::hex};

pub const TERMINAL_PICTURE_MAX_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PictureFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl PictureFormat {
    fn of(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }

    #[cfg(feature = "host")]
    const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }
}

#[derive(Clone)]
pub struct Picture {
    pub bytes: Bytes,
    pub format: PictureFormat,
    pub digest: [u8; 32],
}

impl Picture {
    pub(crate) fn new(bytes: Bytes) -> Result<Self> {
        if bytes.len() > TERMINAL_PICTURE_MAX_BYTES {
            return Err(Error::Text(text!("Kodosi sends pictures up to 32 MB.")));
        }
        let format = PictureFormat::of(&bytes)
            .ok_or_else(|| Error::Text(text!("Kodosi pastes PNG, JPEG, GIF and WebP pictures.")))?;
        let mut sha256 = [0; 32];
        sha256.copy_from_slice(digest(&SHA256, &bytes).as_ref());
        Ok(Self {
            digest: sha256,
            bytes,
            format,
        })
    }

    pub(crate) async fn read(path: String) -> Result<Self> {
        tokio::task::spawn_blocking(move || {
            use std::io::Read as _;
            let file = std::fs::File::open(&path)?;
            let mut bytes = Vec::new();
            file.take(TERMINAL_PICTURE_MAX_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            Self::new(Bytes::from(bytes))
        })
        .await
        .map_err(|error| Error::Other(format!("The picture could not be read: {error}")))?
    }

    #[cfg(feature = "host")]
    pub(crate) fn name(&self) -> String {
        name(&self.digest, self.format)
    }

    pub(crate) fn sha256(&self) -> String {
        hex(&self.digest)
    }
}

#[cfg(feature = "host")]
pub(crate) fn name(digest: &[u8; 32], format: PictureFormat) -> String {
    format!("picture-{}.{}", hex(&digest[..8]), format.extension())
}

/// Uses the short space-free IPC directory on Unix so pasted picture paths rarely need quoting.
#[cfg(feature = "host")]
pub(crate) fn pictures_root(data_root: &std::path::Path) -> std::path::PathBuf {
    #[cfg(unix)]
    if let Ok(folder) = kodosi_os::ipc::private_dir(&crate::local_host::namespace(data_root)) {
        return folder.join("pictures");
    }
    data_root.join("pictures")
}

#[cfg(feature = "host")]
pub(crate) fn save(folder: &std::path::Path, picture: &Picture) -> Result<std::path::PathBuf> {
    kodosi_os::fs::create_private_dir(folder)?;
    let path = folder.join(picture.name());
    if !path.is_file() {
        kodosi_os::fs::write_private(&path, &picture.bytes)?;
    }
    Ok(path)
}

#[cfg(feature = "host")]
pub(crate) fn paste_text(path: &std::path::Path) -> String {
    let text = path.to_string_lossy();
    if text.contains(char::is_whitespace) {
        format!("\"{text}\"")
    } else {
        text.into_owned()
    }
}

#[cfg(all(test, feature = "host"))]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_known_by_its_first_bytes_and_named_by_its_content() {
        let png = Picture::new(Bytes::from_static(b"\x89PNG\r\n\x1a\nrest")).unwrap();
        assert_eq!(png.format, PictureFormat::Png);
        assert_eq!(png.name(), format!("picture-{}.png", hex(&png.digest[..8])));
        assert_eq!(
            png.name(),
            Picture::new(Bytes::from_static(b"\x89PNG\r\n\x1a\nrest"))
                .unwrap()
                .name()
        );
        let webp = Picture::new(Bytes::from_static(b"RIFF\0\0\0\0WEBPVP8 ")).unwrap();
        assert_eq!(webp.format, PictureFormat::Webp);
        assert!(Picture::new(Bytes::from_static(b"%PDF-1.7")).is_err());
        assert!(Picture::new(Bytes::from(vec![0xff; TERMINAL_PICTURE_MAX_BYTES + 1])).is_err());
    }
}
