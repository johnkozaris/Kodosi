use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;

pub const MAX_TERMINAL_ROWS: u16 = 512;
pub const MAX_TERMINAL_COLS: u16 = 1024;

pub const TERMINAL_CHECKPOINT_SCHEMA_VERSION: u32 = 2;

pub const TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES: usize = 8 * 1024 * 1024;

pub const TERMINAL_SEMANTIC_CHECKPOINT_MAX_BASE64_BYTES: usize =
    base64_encoded_len(TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES);

pub const TERMINAL_CHECKPOINT_JSON_OVERHEAD_MAX_BYTES: usize = 143;

pub const TERMINAL_CHECKPOINT_PROVEN_MAX_SERIALIZED_BYTES: usize = checked_add(
    TERMINAL_SEMANTIC_CHECKPOINT_MAX_BASE64_BYTES,
    TERMINAL_CHECKPOINT_JSON_OVERHEAD_MAX_BYTES,
);

pub const TERMINAL_CHECKPOINT_MAX_SERIALIZED_BYTES: usize = 12 * 1024 * 1024;

pub const TERMINAL_PRESENTATION_SCHEMA_VERSION: u32 = 2;

pub const TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES: usize = 8 * 1024 * 1024;

pub const TERMINAL_PRESENTATION_ENCRYPTED_FRAME_OVERHEAD_BYTES: usize = 1 + 4 + 8 + 8 + 16;
pub const TERMINAL_PRESENTATION_ENCRYPTED_FRAME_MAX_BYTES: usize = checked_add(
    TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES,
    TERMINAL_PRESENTATION_ENCRYPTED_FRAME_OVERHEAD_BYTES,
);

pub const TERMINAL_PRESENTATION_CONTROL_OVERHEAD_BYTES: usize = 64 * 1024;
pub const TERMINAL_PRESENTATION_CONTROL_MAX_BYTES: usize = checked_add(
    TERMINAL_PRESENTATION_MAX_SERIALIZED_BYTES,
    TERMINAL_PRESENTATION_CONTROL_OVERHEAD_BYTES,
);

pub const TERMINAL_LOCAL_CHECKPOINT_FRAME_OVERHEAD_BYTES: usize = 64;
pub const TERMINAL_LOCAL_CHECKPOINT_FRAME_MAX_BYTES: usize = checked_add(
    TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
    TERMINAL_LOCAL_CHECKPOINT_FRAME_OVERHEAD_BYTES,
);

const _: () = assert!(
    TERMINAL_CHECKPOINT_PROVEN_MAX_SERIALIZED_BYTES <= TERMINAL_CHECKPOINT_MAX_SERIALIZED_BYTES
);

const fn checked_add(left: usize, right: usize) -> usize {
    match left.checked_add(right) {
        Some(sum) => sum,
        None => panic!("terminal protocol byte cap overflow"),
    }
}

const fn checked_mul(left: usize, right: usize) -> usize {
    match left.checked_mul(right) {
        Some(product) => product,
        None => panic!("terminal protocol byte cap overflow"),
    }
}

const fn base64_encoded_len(raw_len: usize) -> usize {
    checked_mul(checked_add(raw_len, 2) / 3, 4)
}

#[expect(
    clippy::struct_field_names,
    reason = "the pixel suffix is the cross-language wire vocabulary and distinguishes geometry from terminal cells"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPixelGeometry {
    width_pixels: u32,
    height_pixels: u32,
    cell_width_pixels: u32,
    cell_height_pixels: u32,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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

impl<'de> Deserialize<'de> for TerminalSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            rows: u16,
            cols: u16,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.rows, wire.cols).map_err(serde::de::Error::custom)
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision(u64);

impl Revision {
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalScreen {
    Primary,
    Alternate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCheckpointV2 {
    schema_version: u32,
    rows: u16,
    cols: u16,
    pub active_screen: TerminalScreen,
    #[serde(with = "base64_bytes")]
    pub semantic_checkpoint: Vec<u8>,
    cursor_x: u16,
    cursor_y: u16,
    cursor_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TerminalCheckpointError {
    #[error("unsupported terminal checkpoint schema version {0}")]
    UnsupportedSchema(u32),
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

impl TerminalCheckpointV2 {
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
            schema_version: TERMINAL_CHECKPOINT_SCHEMA_VERSION,
            rows: size.rows(),
            cols: size.cols(),
            active_screen,
            semantic_checkpoint,
            cursor_x,
            cursor_y,
            cursor_hidden,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
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

impl<'de> Deserialize<'de> for TerminalCheckpointV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            rows: u16,
            cols: u16,
            active_screen: TerminalScreen,
            #[serde(with = "base64_bytes")]
            semantic_checkpoint: Vec<u8>,
            cursor_x: u16,
            cursor_y: u16,
            cursor_hidden: bool,
        }

        let wire = Wire::deserialize(deserializer)?;
        if wire.schema_version != TERMINAL_CHECKPOINT_SCHEMA_VERSION {
            return Err(serde::de::Error::custom(
                TerminalCheckpointError::UnsupportedSchema(wire.schema_version),
            ));
        }
        let size = TerminalSize::new(wire.rows, wire.cols).map_err(serde::de::Error::custom)?;
        Self::new(
            size,
            wire.active_screen,
            wire.semantic_checkpoint,
            wire.cursor_x,
            wire.cursor_y,
            wire.cursor_hidden,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPresentationV2 {
    schema_version: u32,
    rows: u16,
    cols: u16,
    pub active_screen: TerminalScreen,
    pub plain_lines: Vec<String>,
    cursor_x: u16,
    cursor_y: u16,
    cursor_hidden: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TerminalPresentationError {
    #[error("unsupported terminal presentation schema version {0}")]
    UnsupportedSchema(u32),
    #[error(transparent)]
    InvalidSize(#[from] TerminalSizeError),
    #[error("terminal presentation has {actual} lines for a {rows}-row grid")]
    InvalidLineCount { actual: usize, rows: u16 },
    #[error("terminal presentation cursor {x},{y} is outside {cols}x{rows}")]
    InvalidCursor {
        x: u16,
        y: u16,
        rows: u16,
        cols: u16,
    },
}

impl TerminalPresentationV2 {
    pub fn new(
        size: TerminalSize,
        active_screen: TerminalScreen,
        plain_lines: Vec<String>,
        cursor_x: u16,
        cursor_y: u16,
        cursor_hidden: bool,
    ) -> Result<Self, TerminalPresentationError> {
        if plain_lines.len() != usize::from(size.rows()) {
            return Err(TerminalPresentationError::InvalidLineCount {
                actual: plain_lines.len(),
                rows: size.rows(),
            });
        }
        if cursor_x >= size.cols() || cursor_y >= size.rows() {
            return Err(TerminalPresentationError::InvalidCursor {
                x: cursor_x,
                y: cursor_y,
                rows: size.rows(),
                cols: size.cols(),
            });
        }
        Ok(Self {
            schema_version: TERMINAL_PRESENTATION_SCHEMA_VERSION,
            rows: size.rows(),
            cols: size.cols(),
            active_screen,
            plain_lines,
            cursor_x,
            cursor_y,
            cursor_hidden,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
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

const TERMINAL_PRESENTATION_LINE_MAX_BYTES: usize = 256 * 1024;

struct BoundedPlainLines(Vec<String>);

impl<'de> Deserialize<'de> for BoundedPlainLines {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LinesVisitor;

        impl<'de> serde::de::Visitor<'de> for LinesVisitor {
            type Value = BoundedPlainLines;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(
                    formatter,
                    "at most {MAX_TERMINAL_ROWS} terminal lines of at most {TERMINAL_PRESENTATION_LINE_MAX_BYTES} UTF-8 bytes"
                )
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut lines = Vec::with_capacity(
                    sequence
                        .size_hint()
                        .unwrap_or_default()
                        .min(usize::from(MAX_TERMINAL_ROWS)),
                );
                while let Some(line) = sequence.next_element::<String>()? {
                    if lines.len() >= usize::from(MAX_TERMINAL_ROWS) {
                        return Err(serde::de::Error::custom(
                            "terminal presentation has too many lines",
                        ));
                    }
                    if line.len() > TERMINAL_PRESENTATION_LINE_MAX_BYTES {
                        return Err(serde::de::Error::custom(
                            "terminal presentation line exceeds byte limit",
                        ));
                    }
                    lines.push(line);
                }
                Ok(BoundedPlainLines(lines))
            }
        }

        deserializer.deserialize_seq(LinesVisitor)
    }
}

impl<'de> Deserialize<'de> for TerminalPresentationV2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            rows: u16,
            cols: u16,
            active_screen: TerminalScreen,
            plain_lines: BoundedPlainLines,
            cursor_x: u16,
            cursor_y: u16,
            cursor_hidden: bool,
        }

        let wire = Wire::deserialize(deserializer)?;
        if wire.schema_version != TERMINAL_PRESENTATION_SCHEMA_VERSION {
            return Err(serde::de::Error::custom(
                TerminalPresentationError::UnsupportedSchema(wire.schema_version),
            ));
        }
        let size = TerminalSize::new(wire.rows, wire.cols).map_err(serde::de::Error::custom)?;
        Self::new(
            size,
            wire.active_screen,
            wire.plain_lines.0,
            wire.cursor_x,
            wire.cursor_y,
            wire.cursor_hidden,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalPresentationFrame {
    pub revision: Revision,
    pub presentation: TerminalPresentationV2,
    pub captured_at: OffsetDateTime,
}

impl TerminalPresentationFrame {
    pub fn new(revision: Revision, presentation: TerminalPresentationV2) -> Self {
        Self {
            revision,
            presentation,
            captured_at: OffsetDateTime::now_utc(),
        }
    }
}

mod base64_bytes {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub(super) fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(encoded).map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use super::*;

    fn valid_json() -> serde_json::Value {
        serde_json::json!({
            "schemaVersion": 2,
            "rows": 24,
            "cols": 80,
            "activeScreen": "primary",
            "semanticCheckpoint": "Y2hlY2twb2ludA==",
            "cursorX": 79,
            "cursorY": 23,
            "cursorHidden": false
        })
    }

    #[test]
    fn pixel_geometry_must_match_the_terminal_grid_exactly() {
        let size = TerminalSize::new(24, 80).expect("size");
        assert!(
            TerminalPixelGeometry::new(800, 480, 10, 20)
                .and_then(|geometry| geometry.validate_for_size(size))
                .is_ok()
        );
        for geometry in [(801, 480, 10, 20), (800, 481, 10, 20)] {
            assert!(
                TerminalPixelGeometry::new(geometry.0, geometry.1, geometry.2, geometry.3)
                    .and_then(|value| value.validate_for_size(size))
                    .is_err()
            );
        }
    }

    #[test]
    fn terminal_size_enforces_shared_boundaries_and_deserialization() {
        assert!(TerminalSize::new(1, 1).is_ok());
        assert!(TerminalSize::new(MAX_TERMINAL_ROWS, MAX_TERMINAL_COLS).is_ok());
        for (rows, cols) in [
            (0, 1),
            (1, 0),
            (MAX_TERMINAL_ROWS + 1, MAX_TERMINAL_COLS),
            (MAX_TERMINAL_ROWS, MAX_TERMINAL_COLS + 1),
            (u16::MAX, u16::MAX),
        ] {
            assert!(TerminalSize::new(rows, cols).is_err(), "{rows}x{cols}");
        }
        assert!(
            serde_json::from_value::<TerminalSize>(serde_json::json!({
                "rows": MAX_TERMINAL_ROWS + 1,
                "cols": MAX_TERMINAL_COLS,
            }))
            .is_err()
        );
    }

    #[test]
    fn semantic_checkpoint_has_exact_base64_wire_fields() {
        let snapshot = TerminalCheckpointV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Alternate,
            b"checkpoint".to_vec(),
            7,
            3,
            true,
        )
        .expect("snapshot");
        let value = serde_json::to_value(&snapshot).expect("serialize");
        assert_eq!(
            value
                .as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>(),
            [
                "activeScreen",
                "cols",
                "cursorHidden",
                "cursorX",
                "cursorY",
                "rows",
                "schemaVersion",
                "semanticCheckpoint",
            ]
        );
        assert_eq!(value["schemaVersion"], 2);
        assert_eq!(value["semanticCheckpoint"], "Y2hlY2twb2ludA==");
        assert_eq!(value["activeScreen"], "alternate");
        assert_eq!(
            serde_json::from_value::<TerminalCheckpointV2>(value).expect("deserialize"),
            snapshot
        );
    }

    #[test]
    fn semantic_checkpoint_rejects_invalid_schema_size_cursor_and_checkpoint() {
        for (field, value) in [
            ("schemaVersion", serde_json::json!(1)),
            ("rows", serde_json::json!(0)),
            ("cols", serde_json::json!(0)),
            ("cursorX", serde_json::json!(80)),
            ("cursorY", serde_json::json!(24)),
            ("semanticCheckpoint", serde_json::json!("")),
        ] {
            let mut invalid = valid_json();
            invalid[field] = value;
            assert!(
                serde_json::from_value::<TerminalCheckpointV2>(invalid).is_err(),
                "{field} must be validated"
            );
        }
    }

    #[test]
    fn checkpoint_schema_and_raw_bound_match_proven_serialized_maximum() {
        let maximum = vec![0x5a; TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES];
        let maximum_size = TerminalSize::new(MAX_TERMINAL_ROWS, MAX_TERMINAL_COLS).expect("size");
        let snapshot = TerminalCheckpointV2::new(
            maximum_size,
            TerminalScreen::Alternate,
            maximum,
            maximum_size.cols() - 1,
            maximum_size.rows() - 1,
            false,
        )
        .expect("maximum checkpoint accepted");
        let serialized = serde_json::to_vec(&snapshot).expect("serialize");
        assert_eq!(
            serialized.len(),
            TERMINAL_CHECKPOINT_PROVEN_MAX_SERIALIZED_BYTES
        );
        assert_eq!(
            serialized.len() - TERMINAL_SEMANTIC_CHECKPOINT_MAX_BASE64_BYTES,
            TERMINAL_CHECKPOINT_JSON_OVERHEAD_MAX_BYTES
        );
        let value: serde_json::Value = serde_json::from_slice(&serialized).expect("JSON");
        assert_eq!(
            value.as_object().expect("checkpoint object").keys().count(),
            8,
            "a new serialized field must be incorporated into the closed-schema size proof"
        );
    }

    #[test]
    fn oversized_raw_semantic_checkpoint_is_rejected_before_construction() {
        let oversized_len = TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES + 1;
        let error = TerminalCheckpointV2::new(
            TerminalSize::new(1, 1).expect("size"),
            TerminalScreen::Primary,
            vec![0x5a; oversized_len],
            0,
            0,
            false,
        )
        .expect_err("oversized constructor input rejected");
        assert_eq!(
            error,
            TerminalCheckpointError::SemanticCheckpointTooLarge {
                actual: oversized_len,
                maximum: TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES,
            }
        );
    }

    #[test]
    fn semantic_checkpoint_deserializer_enforces_raw_bound() {
        let oversized = vec![0x5a; TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES + 1];
        let mut value = valid_json();
        value["semanticCheckpoint"] = serde_json::json!(STANDARD.encode(oversized));
        assert!(serde_json::from_value::<TerminalCheckpointV2>(value).is_err());
    }

    #[test]
    fn semantic_checkpoint_rejects_missing_unknown_and_removed_projection_fields() {
        for field in [
            "schemaVersion",
            "rows",
            "cols",
            "activeScreen",
            "semanticCheckpoint",
            "cursorX",
            "cursorY",
            "cursorHidden",
        ] {
            let mut missing = valid_json();
            missing.as_object_mut().expect("object").remove(field);
            assert!(
                serde_json::from_value::<TerminalCheckpointV2>(missing).is_err(),
                "{field} must be required"
            );
        }

        for removed_or_unknown in [
            "parserContinuation",
            "freshSurfaceVt",
            "rawCliVt",
            "plainLines",
            "legacyTerminalState",
        ] {
            let mut value = valid_json();
            value[removed_or_unknown] = serde_json::json!("");
            assert!(
                serde_json::from_value::<TerminalCheckpointV2>(value).is_err(),
                "{removed_or_unknown} must be rejected"
            );
        }
    }

    #[test]
    fn presentation_deserializer_bounds_lines_before_object_validation() {
        let too_many = serde_json::json!({
            "schemaVersion": TERMINAL_PRESENTATION_SCHEMA_VERSION,
            "rows": 1,
            "cols": 1,
            "activeScreen": "primary",
            "plainLines": vec![""; usize::from(MAX_TERMINAL_ROWS) + 1],
            "cursorX": 0,
            "cursorY": 0,
            "cursorHidden": false,
        });
        assert!(serde_json::from_value::<TerminalPresentationV2>(too_many).is_err());

        let oversized_line = serde_json::json!({
            "schemaVersion": TERMINAL_PRESENTATION_SCHEMA_VERSION,
            "rows": 1,
            "cols": 1,
            "activeScreen": "primary",
            "plainLines": ["x".repeat(TERMINAL_PRESENTATION_LINE_MAX_BYTES + 1)],
            "cursorX": 0,
            "cursorY": 0,
            "cursorHidden": false,
        });
        assert!(serde_json::from_value::<TerminalPresentationV2>(oversized_line).is_err());
    }

    #[test]
    fn presentation_rejects_line_count_and_accepts_multiscalar_cells() {
        let size = TerminalSize::new(2, 3).expect("size");
        assert!(matches!(
            TerminalPresentationV2::new(
                size,
                TerminalScreen::Primary,
                vec![String::new()],
                0,
                0,
                false,
            ),
            Err(TerminalPresentationError::InvalidLineCount { .. })
        ));

        for line in ["e\u{301}", "👩‍💻"] {
            let presentation = TerminalPresentationV2::new(
                size,
                TerminalScreen::Primary,
                vec![line.to_owned(), String::new()],
                0,
                0,
                false,
            )
            .expect("Ghostty-formatted grapheme must not be measured by scalar count");
            let encoded = serde_json::to_vec(&presentation).expect("serialize");
            let decoded: TerminalPresentationV2 =
                serde_json::from_slice(&encoded).expect("deserialize");
            assert_eq!(decoded.plain_lines, presentation.plain_lines);
        }
    }

    #[test]
    fn plain_presentation_rejects_semantic_and_vt_projection_fields() {
        let presentation = TerminalPresentationV2::new(
            TerminalSize::new(24, 80).expect("size"),
            TerminalScreen::Primary,
            vec!["line".to_owned(); 24],
            0,
            0,
            false,
        )
        .expect("presentation");
        let value = serde_json::to_value(&presentation).expect("serialize");
        assert_eq!(
            value
                .as_object()
                .expect("object")
                .keys()
                .collect::<Vec<_>>(),
            [
                "activeScreen",
                "cols",
                "cursorHidden",
                "cursorX",
                "cursorY",
                "plainLines",
                "rows",
                "schemaVersion",
            ]
        );
        for forbidden in [
            "semanticCheckpoint",
            "freshSurfaceVt",
            "rawCliVt",
            "parserContinuation",
        ] {
            let mut with_forbidden = value.clone();
            with_forbidden[forbidden] = serde_json::json!("");
            assert!(
                serde_json::from_value::<TerminalPresentationV2>(with_forbidden).is_err(),
                "{forbidden} must be rejected"
            );
        }
    }

    #[test]
    fn checkpoint_cap_includes_base64_and_fixed_json_without_overflow() {
        assert_eq!(TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES, 8_388_608);
        assert_eq!(TERMINAL_SEMANTIC_CHECKPOINT_MAX_BASE64_BYTES, 11_184_812);
        assert_eq!(TERMINAL_CHECKPOINT_JSON_OVERHEAD_MAX_BYTES, 143);
        assert_eq!(TERMINAL_CHECKPOINT_PROVEN_MAX_SERIALIZED_BYTES, 11_184_955);
        assert_eq!(TERMINAL_CHECKPOINT_MAX_SERIALIZED_BYTES, 12_582_912);

        assert_eq!(base64_encoded_len(0), 0);
        assert_eq!(base64_encoded_len(1), 4);
        assert_eq!(base64_encoded_len(2), 4);
        assert_eq!(base64_encoded_len(3), 4);
        assert_eq!(base64_encoded_len(4), 8);
    }
}
