use std::fmt;

use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum ClipboardError {
    #[error("system clipboard unavailable")]
    BackendUnavailable,
    #[error("failed to initialize system clipboard backend")]
    InitFailed {
        #[source]
        source: arboard::Error,
    },
    #[error("failed to write system clipboard")]
    WriteFailed {
        #[source]
        source: arboard::Error,
    },
    #[cfg(test)]
    #[error("{reason}")]
    RecordingFailure { reason: String },
}

pub(crate) struct SystemClipboardBridge {
    backend: ClipboardBackend,
}

enum ClipboardBackend {
    Uninitialized,
    Available(arboard::Clipboard),
    Unavailable,
    #[cfg(test)]
    Recording {
        writes: Vec<String>,
        failure: Option<String>,
    },
}

impl SystemClipboardBridge {
    pub(crate) fn new() -> Self {
        Self {
            backend: ClipboardBackend::Uninitialized,
        }
    }

    pub(crate) fn is_available(&self) -> bool {
        #[cfg(test)]
        if matches!(&self.backend, ClipboardBackend::Recording { .. }) {
            return true;
        }

        match &self.backend {
            ClipboardBackend::Available(_) => true,
            ClipboardBackend::Uninitialized => true,
            ClipboardBackend::Unavailable => false,
            #[cfg(test)]
            ClipboardBackend::Recording { .. } => true,
        }
    }

    pub(crate) fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        #[cfg(test)]
        if let ClipboardBackend::Recording { writes, failure } = &mut self.backend {
            if let Some(reason) = failure.as_ref() {
                return Err(ClipboardError::RecordingFailure {
                    reason: reason.clone(),
                });
            }
            writes.push(text.to_owned());
            return Ok(());
        }

        let write_result = {
            let clipboard = self.ensure_backend()?;
            clipboard.set_text(text.to_owned())
        };
        match write_result {
            Ok(()) => Ok(()),
            Err(error) => Err(ClipboardError::WriteFailed { source: error }),
        }
    }

    fn ensure_backend(&mut self) -> Result<&mut arboard::Clipboard, ClipboardError> {
        if matches!(&self.backend, ClipboardBackend::Uninitialized) {
            let init =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(arboard::Clipboard::new));
            match init {
                Ok(Ok(clipboard)) => {
                    self.backend = ClipboardBackend::Available(clipboard);
                }
                Ok(Err(error)) => {
                    tracing::info!(%error, "system clipboard initialization failed; OSC 52 will retry");
                    if matches!(&error, arboard::Error::ClipboardNotSupported) {
                        self.backend = ClipboardBackend::Unavailable;
                    }
                    return Err(ClipboardError::InitFailed { source: error });
                }
                Err(_) => {
                    tracing::warn!(
                        "system clipboard backend panicked during init; OSC 52 support disabled"
                    );
                    self.backend = ClipboardBackend::Unavailable;
                    return Err(ClipboardError::BackendUnavailable);
                }
            }
        }

        match &mut self.backend {
            ClipboardBackend::Available(clipboard) => Ok(clipboard),
            ClipboardBackend::Unavailable => Err(ClipboardError::BackendUnavailable),
            ClipboardBackend::Uninitialized => Err(ClipboardError::BackendUnavailable),
            #[cfg(test)]
            ClipboardBackend::Recording { .. } => Err(ClipboardError::BackendUnavailable),
        }
    }

    fn backend_state(&self) -> &'static str {
        match &self.backend {
            ClipboardBackend::Available(_) => "available",
            ClipboardBackend::Uninitialized => "uninitialized",
            ClipboardBackend::Unavailable => "unavailable",
            #[cfg(test)]
            ClipboardBackend::Recording { .. } => "recording",
        }
    }

    #[cfg(test)]
    pub(crate) fn recording() -> Self {
        Self {
            backend: ClipboardBackend::Recording {
                writes: Vec::new(),
                failure: None,
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn failing(reason: &str) -> Self {
        Self {
            backend: ClipboardBackend::Recording {
                writes: Vec::new(),
                failure: Some(reason.to_owned()),
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn recorded_texts(&self) -> Vec<String> {
        match &self.backend {
            ClipboardBackend::Recording { writes, .. } => writes.clone(),
            _ => Vec::new(),
        }
    }
}

impl fmt::Debug for SystemClipboardBridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemClipboardBridge")
            .field("state", &self.backend_state())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{ClipboardError, SystemClipboardBridge};

    #[test]
    fn recording_failure_preserves_display_reason() {
        let mut clipboard = SystemClipboardBridge::failing("clipboard busy");

        let error = clipboard
            .set_text("copied text")
            .expect_err("recording backend should inject configured failure");

        std::assert_matches!(error, ClipboardError::RecordingFailure { .. });
        assert_eq!(error.to_string(), "clipboard busy");
    }

    #[test]
    fn backend_unavailable_has_stable_display_reason() {
        assert_eq!(
            ClipboardError::BackendUnavailable.to_string(),
            "system clipboard unavailable"
        );
    }
}
