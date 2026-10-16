use serde::{Deserialize, Serialize};

pub const MAX_TERMINAL_ROWS: u16 = 512;
pub const MAX_TERMINAL_COLS: u16 = 1024;

pub const TERMINAL_CHECKPOINT_SCHEMA_VERSION: u32 = 5;

/// The most bytes of a snapshot without its images.
///
/// When the full history is larger, a view gets the visible part.
pub const TERMINAL_SNAPSHOT_TEXT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// The most bytes of Kitty image data that each screen of a terminal stores.
pub const TERMINAL_IMAGE_STORAGE_MAX_BYTES: u64 = ghostty_vt::IMAGE_STORAGE_MAX_BYTES;

/// The most bytes of a snapshot.
///
/// It holds its text and, for each of the two screens, the stored images and
/// an unfinished image transmission of at most the storage limit each, with
/// the records of at most 4,096 images and 16,384 placements.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the image storage limit is 64 MiB"
)]
pub const TERMINAL_SNAPSHOT_MAX_BYTES: usize = TERMINAL_SNAPSHOT_TEXT_MAX_BYTES
    + 4 * TERMINAL_IMAGE_STORAGE_MAX_BYTES as usize
    + 4 * 1024 * 1024;

/// The most bytes of image pixels in a snapshot for remote views.
///
/// An image beyond it stays blank in those views until the program sends it
/// again.
pub const TERMINAL_REMOTE_IMAGE_DATA_MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The bounds of a checkpoint that holds at most `image_data_bytes` of pixels.
#[cfg(any(feature = "host", test))]
pub(crate) const fn snapshot_limits(image_data_bytes: u64) -> ghostty_vt::SnapshotLimits {
    ghostty_vt::SnapshotLimits {
        text_bytes: TERMINAL_SNAPSHOT_TEXT_MAX_BYTES,
        total_bytes: TERMINAL_SNAPSHOT_MAX_BYTES,
        image_data_bytes,
    }
}

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

/// The pixel size of one cell of a terminal on its host.
///
/// Each view keeps it in its own terminal state, so a Kitty image covers the
/// same cells in every view, and scales it to its own cells when it draws.
/// Zero means that no view reported a pixel size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CellPixels {
    pub width: u32,
    pub height: u32,
}

impl CellPixels {
    /// The cells of a terminal of this pixel size.
    pub fn within(width_pixels: u32, height_pixels: u32, size: TerminalSize) -> Self {
        Self {
            width: width_pixels / u32::from(size.cols),
            height: height_pixels / u32::from(size.rows),
        }
    }

    pub const fn of(geometry: TerminalPixelGeometry) -> Self {
        Self {
            width: geometry.cell_width_pixels,
            height: geometry.cell_height_pixels,
        }
    }

    /// The size of a terminal of these cells in pixels, where a PTY can hold it.
    pub fn pty_pixels(self, size: TerminalSize) -> (Option<u16>, Option<u16>) {
        let pixels = |cells: u16, cell: u32| {
            u32::from(cells)
                .checked_mul(cell)
                .and_then(|pixels| u16::try_from(pixels).ok())
                .filter(|&pixels| pixels > 0)
        };
        (
            pixels(size.cols, self.width),
            pixels(size.rows, self.height),
        )
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<super::TerminalContext>,
}

impl TerminalMetadata {
    pub fn is_valid(&self) -> bool {
        self.connected_users.len() <= 64
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
            && self
                .context
                .as_ref()
                .is_none_or(super::TerminalContext::is_valid)
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
    pub metadata: Option<Box<TerminalMetadata>>,
    size: TerminalSize,
    cell: CellPixels,
    /// A Ghostty snapshot; one that ends at READY has no history.
    snapshot: Vec<u8>,
    /// The host terminal is a console: a resize does not pull rows from history.
    /// The snapshot does not hold this rule.
    pub console: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TerminalCheckpointError {
    #[error(transparent)]
    InvalidSize(#[from] TerminalSizeError),
    #[error("terminal snapshot is {actual} bytes; maximum is {maximum}")]
    SnapshotTooLarge { actual: usize, maximum: usize },
    #[error("terminal snapshot does not start with a terminal record")]
    UnreadableSnapshot,
}

impl Checkpoint {
    /// Takes the size from the snapshot, so a snapshot of a terminal over the
    /// size limits is refused before anything decodes it.
    pub fn new(snapshot: Vec<u8>, console: bool) -> Result<Self, TerminalCheckpointError> {
        if snapshot.len() > TERMINAL_SNAPSHOT_MAX_BYTES {
            return Err(TerminalCheckpointError::SnapshotTooLarge {
                actual: snapshot.len(),
                maximum: TERMINAL_SNAPSHOT_MAX_BYTES,
            });
        }
        let header = ghostty_vt::SnapshotHeader::read(&snapshot)
            .map_err(|_| TerminalCheckpointError::UnreadableSnapshot)?;
        let size = TerminalSize::new(header.rows, header.cols)?;
        Ok(Self {
            metadata: None,
            size,
            cell: CellPixels::within(header.width_px, header.height_px, size),
            snapshot,
            console,
        })
    }

    pub const fn size(&self) -> TerminalSize {
        self.size
    }

    /// The cell size of the host terminal when the snapshot was taken.
    pub const fn cell(&self) -> CellPixels {
        self.cell
    }

    pub const fn rows(&self) -> u16 {
        self.size.rows
    }

    pub const fn cols(&self) -> u16 {
        self.size.cols
    }

    pub fn snapshot(&self) -> &[u8] {
        &self.snapshot
    }
}
