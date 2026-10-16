use serde::{Deserialize, Serialize};

pub const MAX_TERMINAL_ROWS: u16 = 512;
pub const MAX_TERMINAL_COLS: u16 = 1024;

pub const TERMINAL_CHECKPOINT_SCHEMA_VERSION: u32 = 2;

pub const TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES: usize = 8 * 1024 * 1024;

pub const TERMINAL_INPUT_MAX_BYTES: usize = 1024 * 1024;

#[expect(
    clippy::struct_field_names,
    reason = "the pixel suffix is the cross-language wire vocabulary and distinguishes geometry from terminal cells"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalPixelGeometry {
    width_pixels: u32,
    height_pixels: u32,
    cell_width_pixels: u32,
    cell_height_pixels: u32,
}

#[expect(
    clippy::struct_field_names,
    reason = "matches the geometry fields reported by this error"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "invalid terminal pixel geometry {width_pixels}x{height_pixels} with {cell_width_pixels}x{cell_height_pixels} cells"
)]
pub struct TerminalPixelGeometryError {
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub cell_width_pixels: u32,
    pub cell_height_pixels: u32,
}

impl TerminalPixelGeometry {
    pub fn new(
        width_pixels: u32,
        height_pixels: u32,
        cell_width_pixels: u32,
        cell_height_pixels: u32,
    ) -> Result<Self, TerminalPixelGeometryError> {
        if width_pixels == 0
            || height_pixels == 0
            || cell_width_pixels == 0
            || cell_height_pixels == 0
        {
            return Err(TerminalPixelGeometryError {
                width_pixels,
                height_pixels,
                cell_width_pixels,
                cell_height_pixels,
            });
        }
        Ok(Self {
            width_pixels,
            height_pixels,
            cell_width_pixels,
            cell_height_pixels,
        })
    }

    pub fn validate_for_size(self, size: TerminalSize) -> Result<Self, TerminalPixelGeometryError> {
        let expected_width = u32::from(size.cols()).checked_mul(self.cell_width_pixels);
        let expected_height = u32::from(size.rows()).checked_mul(self.cell_height_pixels);
        if expected_width != Some(self.width_pixels) || expected_height != Some(self.height_pixels)
        {
            return Err(TerminalPixelGeometryError {
                width_pixels: self.width_pixels,
                height_pixels: self.height_pixels,
                cell_width_pixels: self.cell_width_pixels,
                cell_height_pixels: self.cell_height_pixels,
            });
        }
        Ok(self)
    }

    pub const fn width_pixels(self) -> u32 {
        self.width_pixels
    }

    pub const fn height_pixels(self) -> u32 {
        self.height_pixels
    }

    pub const fn cell_width_pixels(self) -> u32 {
        self.cell_width_pixels
    }

    pub const fn cell_height_pixels(self) -> u32 {
        self.cell_height_pixels
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    rows: u16,
    cols: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid terminal size {rows}x{cols}")]
pub struct TerminalSizeError {
    pub rows: u16,
    pub cols: u16,
}

impl TerminalSize {
    pub fn new(rows: u16, cols: u16) -> Result<Self, TerminalSizeError> {
        if !(1..=MAX_TERMINAL_ROWS).contains(&rows) || !(1..=MAX_TERMINAL_COLS).contains(&cols) {
            return Err(TerminalSizeError { rows, cols });
        }
        Ok(Self { rows, cols })
    }

    pub fn rows(self) -> u16 {
        self.rows
    }

    pub fn cols(self) -> u16 {
        self.cols
    }
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            rows: 32,
            cols: 120,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalScreen {
    Primary,
    Alternate,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalMetadata {
    #[serde(default)]
    pub connected_users: Vec<String>,
    pub directory: Option<String>,
    pub title: Option<String>,
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<super::ProgramStatus>,
}

impl TerminalMetadata {
    pub fn is_valid(&self) -> bool {
        self.connected_users.len() <= crate::protocol::TERMINAL_VIEWERS_MOST
            && self
                .connected_users
                .iter()
                .all(|id| uuid::Uuid::parse_str(id).is_ok())
            && self.directory.as_deref().is_none_or(is_valid_folder)
            && self.title.as_ref().is_none_or(|value| value.len() <= 4096)
            && self
                .program
                .as_deref()
                .is_none_or(crate::agents::is_agent_name)
            && self
                .status
                .as_ref()
                .is_none_or(super::ProgramStatus::is_valid)
    }
}

/// Hosts report folders in their own form: `/…` on macOS and Linux, and a drive
/// (`C:\…`) or a share (`\\server\share`) on Windows. Viewers accept every form.
pub(super) fn is_valid_folder(value: &str) -> bool {
    value.len() <= 4096
        && !value.chars().any(char::is_control)
        && (value.starts_with('/')
            || (value.len() > 2 && value.starts_with(r"\\"))
            || matches!(value.as_bytes(), [drive, b':', b'\\' | b'/', ..] if drive.is_ascii_alphabetic()))
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub metadata: Option<TerminalMetadata>,
    rows: u16,
    cols: u16,
    pub active_screen: TerminalScreen,
    pub semantic_checkpoint: Vec<u8>,
    cursor_x: u16,
    cursor_y: u16,
    cursor_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TerminalCheckpointError {
    #[error(transparent)]
    InvalidSize(#[from] TerminalSizeError),
    #[error("terminal semantic checkpoint must not be empty")]
    EmptySemanticCheckpoint,
    #[error("terminal semantic checkpoint is {actual} bytes; maximum is {maximum}")]
    SemanticCheckpointTooLarge { actual: usize, maximum: usize },
    #[error("terminal checkpoint cursor {x},{y} is outside {cols}x{rows}")]
    InvalidCursor {
        x: u16,
        y: u16,
        rows: u16,
        cols: u16,
    },
}

impl Checkpoint {
    pub fn new(
        size: TerminalSize,
        active_screen: TerminalScreen,
        semantic_checkpoint: Vec<u8>,
        cursor_x: u16,
        cursor_y: u16,
        cursor_hidden: bool,
    ) -> Result<Self, TerminalCheckpointError> {
        validate_semantic_checkpoint(&semantic_checkpoint)?;
        if cursor_x >= size.cols() || cursor_y >= size.rows() {
            return Err(TerminalCheckpointError::InvalidCursor {
                x: cursor_x,
                y: cursor_y,
                rows: size.rows(),
                cols: size.cols(),
            });
        }
        Ok(Self {
            metadata: None,
            rows: size.rows(),
            cols: size.cols(),
            active_screen,
            semantic_checkpoint,
            cursor_x,
            cursor_y,
            cursor_hidden,
        })
    }

    pub fn size(&self) -> TerminalSize {
        TerminalSize {
            rows: self.rows,
            cols: self.cols,
        }
    }

    pub const fn rows(&self) -> u16 {
        self.rows
    }

    pub const fn cols(&self) -> u16 {
        self.cols
    }

    pub const fn cursor_x(&self) -> u16 {
        self.cursor_x
    }

    pub const fn cursor_y(&self) -> u16 {
        self.cursor_y
    }

    pub const fn cursor_hidden(&self) -> bool {
        self.cursor_hidden
    }
}

fn validate_semantic_checkpoint(bytes: &[u8]) -> Result<(), TerminalCheckpointError> {
    if bytes.is_empty() {
        return Err(TerminalCheckpointError::EmptySemanticCheckpoint);
    }
    if bytes.len() > TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES {
        return Err(TerminalCheckpointError::SemanticCheckpointTooLarge {
            actual: bytes.len(),
            maximum: TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
        });
    }
    Ok(())
}
