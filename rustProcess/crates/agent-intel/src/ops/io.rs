use std::path::Path;

use tokio::fs;
use tokio::io::AsyncReadExt;

pub const MAX_AGENT_INTEL_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub async fn read_bounded_string(path: &Path) -> Result<String, String> {
    read_bounded_string_with_cap(path, MAX_AGENT_INTEL_FILE_BYTES).await
}

pub async fn read_bounded_string_with_cap(path: &Path, cap: u64) -> Result<String, String> {
    let file = match fs::File::open(path).await {
        Ok(f) => f,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err("file not found".to_owned());
        }
        Err(error) => {
            tracing::debug!(path = %path.display(), %error, "open failed");
            return Err(format!("failed to open file: {error}"));
        }
    };
    let mut reader = file.take(cap + 1);
    let mut buffer = Vec::with_capacity(usize::try_from(cap.min(1 << 20)).unwrap_or(1 << 20));
    if let Err(error) = reader.read_to_end(&mut buffer).await {
        return Err(format!("failed to read file: {error}"));
    }
    if buffer.len() as u64 > cap {
        tracing::warn!(
            path = %path.display(),
            cap,
            "agent-intel file exceeds per-read size cap; refusing to load"
        );
        return Err(format!("file too large (> {cap} bytes)"));
    }
    String::from_utf8(buffer).map_err(|error| format!("file is not valid UTF-8: {error}"))
}

pub fn format_rfc3339(t: std::time::SystemTime) -> Option<String> {
    let dt: time::OffsetDateTime = t.into();
    dt.format(&time::format_description::well_known::Rfc3339)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn bounded_errors_on_oversize() -> Result<(), String> {
        let dir = tempdir().map_err(|e| e.to_string())?;
        let path = dir.path().join("big.txt");
        let mut f = fs::File::create(&path).await.map_err(|e| e.to_string())?;
        f.write_all(&[b'a'; 200]).await.map_err(|e| e.to_string())?;
        assert!(read_bounded_string_with_cap(&path, 100).await.is_err());
        Ok(())
    }
}
