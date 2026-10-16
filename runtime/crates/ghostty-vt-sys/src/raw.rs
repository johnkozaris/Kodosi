#![allow(
    clippy::struct_excessive_bools,
    clippy::wildcard_imports,
    non_upper_case_globals
)]

use std::{
    ffi::c_void,
    marker::PhantomData,
    mem::{self, MaybeUninit},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    rc::Rc,
    slice,
};

use crate::*;

const SNAPSHOT_ENVELOPE: &[u8] = b"GHOSTSNP\x01\x00";
const SNAPSHOT_RECORD_HEADER_BYTES: usize = 10;
const SNAPSHOT_TERMINAL_TAG: u16 = 1;
const SNAPSHOT_READY_TAG: u16 = 5;
const SNAPSHOT_TERMINAL_HEADER_BYTES: usize = 103;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Primary,
    Alternate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    PtyWrite(Vec<u8>),
    Bell,
    Title(Vec<u8>),
    Pwd(Vec<u8>),
    DesktopNotification {
        title: Vec<u8>,
        body: Vec<u8>,
    },
    ProgramStatus(ProgramStatus),
    Progress {
        state: ProgressState,
        progress: Option<u8>,
    },
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressState {
    Remove,
    Set,
    Error,
    Indeterminate,
    Pause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusState {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramStatusKind {
    Permission,
    Question,
    Auth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatus {
    pub state: ProgramStatusState,
    pub kind: Option<ProgramStatusKind>,
    pub progress: Option<u8>,
    pub id: Vec<u8>,
    pub app: Vec<u8>,
    pub title: Vec<u8>,
    pub message: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Plain,
    Vt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    pub format: Format,
    pub unwrap: bool,
    pub trim: bool,
    pub palette: bool,
    pub modes: bool,
    pub scrolling_region: bool,
    pub tabstops: bool,
    pub pwd: bool,
    pub keyboard: bool,
    pub cursor: bool,
    pub style: bool,
    pub hyperlink: bool,
    pub protection: bool,
    pub kitty_keyboard: bool,
    pub charsets: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalState {
    pub rows: u16,
    pub cols: u16,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_visible: bool,
    pub active_screen: Screen,
    pub vt_processing_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionProgress {
    Unsupported,
    Pending,
    Complete,
}

/// What a snapshot declares in its first record. The decoder allocates by
/// these values before it checks the rest of the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotHeader {
    pub cols: u16,
    pub rows: u16,
    pub scrollback_max_bytes: Option<u64>,
    pub scrollback_max_lines: Option<u64>,
}

impl SnapshotHeader {
    /// Follows the documented TERMINAL record of snapshot format version 1.
    pub fn read(snapshot: &[u8]) -> Result<Self, Error> {
        let record = snapshot
            .strip_prefix(SNAPSHOT_ENVELOPE)
            .ok_or(Error::InvalidValue)?;
        let (Some(header), Some(fields)) = (
            record.get(..SNAPSHOT_RECORD_HEADER_BYTES),
            record.get(
                SNAPSHOT_RECORD_HEADER_BYTES
                    ..SNAPSHOT_RECORD_HEADER_BYTES + SNAPSHOT_TERMINAL_HEADER_BYTES,
            ),
        ) else {
            return Err(Error::InvalidValue);
        };
        let payload = u32::from_le_bytes([header[2], header[3], header[4], header[5]]) as usize;
        if u16::from_le_bytes([header[0], header[1]]) != SNAPSHOT_TERMINAL_TAG
            || payload < SNAPSHOT_TERMINAL_HEADER_BYTES
        {
            return Err(Error::InvalidValue);
        }
        let limit = |at: usize| {
            let mut value = [0; 8];
            value.copy_from_slice(&fields[at..at + 8]);
            Some(u64::from_le_bytes(value)).filter(|&value| value != u64::MAX)
        };
        Ok(Self {
            cols: u16::from_le_bytes([fields[0], fields[1]]),
            rows: u16::from_le_bytes([fields[2], fields[3]]),
            scrollback_max_bytes: limit(87),
            scrollback_max_lines: limit(95),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
    InvalidValue,
    OutOfSpace,
    NoValue,
    Io,
    LimitExceeded,
    CallbackRejected,
    ProcessingFailed,
    UnexpectedResult(i32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "libghostty-vt error: {self:?}")
    }
}

impl std::error::Error for Error {}

struct CallbackState {
    effects: Vec<Effect>,
    size: GhosttySizeReportSize,
    version: &'static str,
    poisoned: bool,
    #[cfg(test)]
    panic_next: bool,
}

pub struct Terminal {
    raw: GhosttyTerminal,
    callbacks: Box<CallbackState>,
    _thread_owned: PhantomData<Rc<()>>,
}

impl Terminal {
    pub fn new(
        cols: u16,
        rows: u16,
        continuation_max_bytes: usize,
        scrollback_max_bytes: usize,
        scrollback_max_lines: usize,
    ) -> Result<Self, Error> {
        let mut raw = ptr::null_mut();

        result(unsafe { ghostty_terminal_new(ptr::null(), ptr::from_mut(&mut raw), cols, rows) })?;
        let mut terminal = Self::adopt(raw, cols, rows)?;
        terminal.configure(
            continuation_max_bytes,
            scrollback_max_bytes,
            scrollback_max_lines,
        )?;
        Ok(terminal)
    }

    /// Restores a terminal from a Ghostty snapshot. A snapshot that ends at READY
    /// restores the screens without the history after them. The decoder keeps
    /// history up to the limits the snapshot declares, so a snapshot that
    /// declares more than these limits is refused before it is decoded.
    pub fn from_snapshot(
        snapshot: &[u8],
        continuation_max_bytes: usize,
        scrollback_max_bytes: usize,
        scrollback_max_lines: usize,
    ) -> Result<Self, Error> {
        let header = SnapshotHeader::read(snapshot)?;
        let within = |declared: Option<u64>, limit: usize| {
            declared
                .is_some_and(|declared| u64::try_from(limit).is_ok_and(|limit| declared <= limit))
        };
        if !within(header.scrollback_max_bytes, scrollback_max_bytes)
            || !within(header.scrollback_max_lines, scrollback_max_lines)
        {
            return Err(Error::LimitExceeded);
        }
        let mut decoder = ptr::null_mut();

        result(unsafe {
            ghostty_snapshot_decoder_new_buf(
                ptr::null(),
                ptr::from_mut(&mut decoder),
                snapshot.as_ptr(),
                snapshot.len(),
            )
        })?;
        if decoder.is_null() {
            return Err(Error::InvalidValue);
        }
        let decoder = DecoderGuard(decoder);
        decoder.set(
            GhosttySnapshotDecoderOption_GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES,
            &continuation_max_bytes,
        )?;
        decoder.set(
            GhosttySnapshotDecoderOption_GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION,
            &true,
        )?;
        let mut raw = ptr::null_mut();
        result(unsafe { ghostty_snapshot_decoder_ready(decoder.0, ptr::from_mut(&mut raw)) })?;
        let mut terminal = Self::adopt(raw, 0, 0)?;
        let history = decoder.restore_history(snapshot.len());
        drop(decoder);
        history?;
        let state = terminal.state()?;
        terminal.callbacks.size.rows = state.rows;
        terminal.callbacks.size.columns = state.cols;
        terminal.configure(
            continuation_max_bytes,
            scrollback_max_bytes,
            scrollback_max_lines,
        )?;
        Ok(terminal)
    }

    fn adopt(raw: GhosttyTerminal, cols: u16, rows: u16) -> Result<Self, Error> {
        if raw.is_null() {
            return Err(Error::InvalidValue);
        }
        Ok(Self {
            raw,
            callbacks: Box::new(CallbackState {
                effects: Vec::new(),
                size: GhosttySizeReportSize {
                    rows,
                    columns: cols,
                    cell_width: 0,
                    cell_height: 0,
                },
                version: "",
                poisoned: false,
                #[cfg(test)]
                panic_next: false,
            }),
            _thread_owned: PhantomData,
        })
    }

    fn configure(
        &mut self,
        continuation_max_bytes: usize,
        scrollback_max_bytes: usize,
        scrollback_max_lines: usize,
    ) -> Result<(), Error> {
        let userdata = ptr::from_mut(self.callbacks.as_mut()).cast::<c_void>();
        self.set_raw(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_USERDATA,
            userdata.cast_const(),
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_WRITE_PTY,
            write_pty_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_BELL,
            bell_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TITLE_CHANGED,
            title_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PWD_CHANGED,
            pwd_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION,
            desktop_notification_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PROGRAM_STATUS,
            program_status_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PROGRESS_REPORT,
            progress_report_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_RESET,
            reset_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SIZE,
            size_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_SCHEME,
            color_scheme_callback as *const c_void,
        )?;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DEVICE_ATTRIBUTES,
            device_attributes_callback as *const c_void,
        )?;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES,
            &continuation_max_bytes,
        )?;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES,
            &scrollback_max_bytes,
        )?;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES,
            &scrollback_max_lines,
        )?;
        let graphics_disabled = 0_u64;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT,
            &graphics_disabled,
        )?;
        let glyph_protocol_disabled = false;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_GLYPH_PROTOCOL,
            &glyph_protocol_disabled,
        )?;
        let title_reports_disabled = false;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TITLE_REPORT,
            &title_reports_disabled,
        )?;
        let terminfo = b"xterm-ghostty";
        let terminfo = GhosttyString {
            ptr: terminfo.as_ptr(),
            len: terminfo.len(),
        };
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TERMINFO_NAME,
            &terminfo,
        )
    }

    fn set_callback(
        &mut self,
        option: GhosttyTerminalOption,
        callback: *const c_void,
    ) -> Result<(), Error> {
        self.set_raw(option, callback)
    }

    fn set_value<T>(&mut self, option: GhosttyTerminalOption, value: &T) -> Result<(), Error> {
        self.set_raw(option, ptr::from_ref(value).cast::<c_void>())
    }

    fn set_raw(
        &mut self,
        option: GhosttyTerminalOption,
        value: *const c_void,
    ) -> Result<(), Error> {
        result(unsafe { ghostty_terminal_set(self.raw, option, value) })
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<Vec<Effect>, Error> {
        self.ensure_healthy()?;

        unsafe { ghostty_terminal_vt_write(self.raw, bytes.as_ptr(), bytes.len()) };
        self.drain_effects()
    }

    pub fn resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> Result<Vec<Effect>, Error> {
        self.ensure_healthy()?;

        result(unsafe {
            ghostty_terminal_resize(self.raw, cols, rows, cell_width_px, cell_height_px)
        })?;
        self.callbacks.size = GhosttySizeReportSize {
            rows,
            columns: cols,
            cell_width: cell_width_px,
            cell_height: cell_height_px,
        };
        self.drain_effects()
    }

    /// Matches `ConPTY`, which measures grapheme clusters (mode 2027) and keeps no
    /// scrollback, so a taller screen gains blank rows instead of history.
    pub fn use_console_profile(&mut self) -> Result<(), Error> {
        self.ensure_healthy()?;
        let graphemes = GhosttyTerminalModeConfig {
            mode: 2027,
            value: true,
        };
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_MODE_DEFAULT,
            &graphemes,
        )?;
        self.set_value(GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_MODE, &graphemes)?;
        self.keep_scrollback_on_resize()
    }

    /// A snapshot holds the console modes but not this resize rule.
    pub fn report_version(&mut self, version: &'static str) -> Result<(), Error> {
        self.ensure_healthy()?;
        self.callbacks.version = version;
        self.set_callback(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_XTVERSION,
            xtversion_callback as *const c_void,
        )
    }

    pub fn keep_scrollback_on_resize(&mut self) -> Result<(), Error> {
        self.ensure_healthy()?;
        let pull_scrollback = false;
        self.set_value(
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_RESIZE_PULL_SCROLLBACK,
            &pull_scrollback,
        )
    }

    pub fn encode_focus(&mut self, focused: bool) -> Result<Vec<u8>, Error> {
        self.ensure_healthy()?;
        if !self.mode(1004, false)? {
            return Ok(Vec::new());
        }
        let event = if focused {
            GhosttyFocusEvent_GHOSTTY_FOCUS_GAINED
        } else {
            GhosttyFocusEvent_GHOSTTY_FOCUS_LOST
        };
        encode_buffer(|buffer, capacity, written| unsafe {
            ghostty_focus_encode(event, buffer.cast::<std::ffi::c_char>(), capacity, written)
        })
    }

    pub fn format(
        &mut self,
        options: FormatOptions,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, Error> {
        self.ensure_healthy()?;
        let raw_options = GhosttyFormatterTerminalOptions {
            size: mem::size_of::<GhosttyFormatterTerminalOptions>(),
            emit: match options.format {
                Format::Plain => GhosttyFormatterFormat_GHOSTTY_FORMATTER_FORMAT_PLAIN,
                Format::Vt => GhosttyFormatterFormat_GHOSTTY_FORMATTER_FORMAT_VT,
            },
            unwrap: options.unwrap,
            trim: options.trim,
            extra: GhosttyFormatterTerminalExtra {
                size: mem::size_of::<GhosttyFormatterTerminalExtra>(),
                palette: options.palette,
                modes: options.modes,
                scrolling_region: options.scrolling_region,
                tabstops: options.tabstops,
                pwd: options.pwd,
                keyboard: options.keyboard,
                screen: GhosttyFormatterScreenExtra {
                    size: mem::size_of::<GhosttyFormatterScreenExtra>(),
                    cursor: options.cursor,
                    style: options.style,
                    hyperlink: options.hyperlink,
                    protection: options.protection,
                    kitty_keyboard: options.kitty_keyboard,
                    charsets: options.charsets,
                },
            },
            selection: ptr::null(),
        };
        let mut formatter = ptr::null_mut();

        result(unsafe {
            ghostty_formatter_terminal_new(
                ptr::null(),
                ptr::from_mut(&mut formatter),
                self.raw,
                raw_options,
            )
        })?;
        if formatter.is_null() {
            return Err(Error::InvalidValue);
        }
        let guard = FormatterGuard(formatter);
        let mut required = 0;

        let query = unsafe {
            ghostty_formatter_format_buf(guard.0, ptr::null_mut(), 0, ptr::from_mut(&mut required))
        };
        if query != GhosttyResult_GHOSTTY_OUT_OF_SPACE {
            result(query)?;
        }
        if required > maximum_bytes || required > isize::MAX as usize {
            return Err(Error::LimitExceeded);
        }
        let mut bytes = vec![0_u8; required];
        let mut written = 0;

        result(unsafe {
            ghostty_formatter_format_buf(
                guard.0,
                bytes.as_mut_ptr(),
                bytes.len(),
                ptr::from_mut(&mut written),
            )
        })?;
        if written > bytes.len() {
            return Err(Error::OutOfSpace);
        }
        bytes.truncate(written);
        self.ensure_healthy()?;
        Ok(bytes)
    }

    pub fn state(&mut self) -> Result<TerminalState, Error> {
        self.ensure_healthy()?;
        let mut cols = 0_u16;
        let mut rows = 0_u16;
        let mut cursor_x = 0_u16;
        let mut cursor_y = 0_u16;
        let mut active_screen: GhosttyTerminalScreen = 0;
        let mut cursor_visible = false;
        let mut vt_processing_error = false;
        let keys = [
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLS,
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_ROWS,
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_X,
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_Y,
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN,
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE,
            GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR,
        ];
        let mut values = [
            ptr::from_mut(&mut cols).cast::<c_void>(),
            ptr::from_mut(&mut rows).cast::<c_void>(),
            ptr::from_mut(&mut cursor_x).cast::<c_void>(),
            ptr::from_mut(&mut cursor_y).cast::<c_void>(),
            ptr::from_mut(&mut active_screen).cast::<c_void>(),
            ptr::from_mut(&mut cursor_visible).cast::<c_void>(),
            ptr::from_mut(&mut vt_processing_error).cast::<c_void>(),
        ];
        let mut written = 0;

        result(unsafe {
            ghostty_terminal_get_multi(
                self.raw,
                keys.len(),
                keys.as_ptr(),
                values.as_mut_ptr(),
                ptr::from_mut(&mut written),
            )
        })?;
        if written != keys.len() {
            return Err(Error::InvalidValue);
        }
        Ok(TerminalState {
            rows,
            cols,
            cursor_x,
            cursor_y,
            cursor_visible,
            active_screen: screen_from_raw(active_screen)?,
            vt_processing_error,
        })
    }

    pub fn mode(&mut self, value: u16, ansi: bool) -> Result<bool, Error> {
        self.ensure_healthy()?;
        let mut config = GhosttyTerminalModeConfig {
            mode: value & 0x7fff | (u16::from(ansi) << 15),
            value: false,
        };

        result(unsafe {
            ghostty_terminal_get(
                self.raw,
                GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MODE,
                ptr::from_mut(&mut config).cast::<c_void>(),
            )
        })?;
        Ok(config.value)
    }

    /// Encodes a Ghostty snapshot. Without history it ends at READY: the screens,
    /// their cursors and modes, and the unfinished input.
    pub fn encode_snapshot(
        &mut self,
        history: bool,
        maximum_bytes: usize,
    ) -> Result<Vec<u8>, Error> {
        self.ensure_healthy()?;
        let mut sink = SnapshotSink {
            bytes: Vec::new(),
            maximum_bytes,
            history,
            scanned: 0,
            ready: None,
            exceeded: false,
            unknown_format: false,
        };
        let writer = GhosttyWriter {
            write: Some(write_snapshot),
            userdata: ptr::from_mut(&mut sink).cast::<c_void>(),
        };

        let encoded = unsafe { ghostty_snapshot_encode(self.raw, writer) };
        if sink.exceeded {
            return Err(Error::LimitExceeded);
        }
        if sink.unknown_format {
            return Err(Error::InvalidValue);
        }
        match sink.ready {
            Some(end) if !history => sink.bytes.truncate(end),
            _ => {
                result(encoded)?;
                if !history {
                    return Err(Error::InvalidValue);
                }
            }
        }
        self.ensure_healthy()?;
        Ok(sink.bytes)
    }

    pub fn compression_activity(&mut self) -> Result<u64, Error> {
        self.ensure_healthy()?;
        let mut activity = 0;

        result(unsafe {
            ghostty_terminal_compression_activity(self.raw, ptr::from_mut(&mut activity))
        })?;
        Ok(activity)
    }

    pub fn compress_incremental(&mut self) -> Result<CompressionProgress, Error> {
        self.ensure_healthy()?;
        let mut progress = 0;

        result(unsafe {
            ghostty_terminal_compress(
                self.raw,
                GhosttyTerminalCompressionMode_GHOSTTY_TERMINAL_COMPRESSION_MODE_INCREMENTAL,
                ptr::from_mut(&mut progress),
            )
        })?;
        match progress {
            GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_UNSUPPORTED => {
                Ok(CompressionProgress::Unsupported)
            }
            GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_PENDING => {
                Ok(CompressionProgress::Pending)
            }
            GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_COMPLETE => {
                Ok(CompressionProgress::Complete)
            }
            _ => Err(Error::InvalidValue),
        }
    }

    #[cfg(test)]
    fn panic_next_callback(&mut self) {
        self.callbacks.panic_next = true;
    }

    fn drain_effects(&mut self) -> Result<Vec<Effect>, Error> {
        self.ensure_healthy()?;
        Ok(mem::take(&mut self.callbacks.effects))
    }

    fn ensure_healthy(&self) -> Result<(), Error> {
        if self.callbacks.poisoned {
            Err(Error::CallbackRejected)
        } else {
            Ok(())
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        unsafe {
            ghostty_terminal_free(self.raw);
        }
    }
}

struct FormatterGuard(GhosttyFormatter);

impl Drop for FormatterGuard {
    fn drop(&mut self) {
        unsafe { ghostty_formatter_free(self.0) };
    }
}

struct DecoderGuard(GhosttySnapshotDecoder);

impl DecoderGuard {
    fn set<T>(&self, option: GhosttySnapshotDecoderOption, value: &T) -> Result<(), Error> {
        result(unsafe {
            ghostty_snapshot_decoder_set(self.0, option, ptr::from_ref(value).cast::<c_void>())
        })
    }

    fn source_offset(&self) -> Result<usize, Error> {
        let mut offset = 0_usize;

        result(unsafe {
            ghostty_snapshot_decoder_get(
                self.0,
                GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET,
                ptr::from_mut(&mut offset).cast::<c_void>(),
            )
        })?;
        Ok(offset)
    }

    fn restore_history(&self, snapshot_len: usize) -> Result<(), Error> {
        if self.source_offset()? == snapshot_len {
            return Ok(());
        }
        loop {
            let next = unsafe { ghostty_snapshot_decoder_next(self.0) };
            if next == GhosttyResult_GHOSTTY_NO_VALUE {
                break;
            }
            result(next)?;
        }
        if self.source_offset()? == snapshot_len {
            Ok(())
        } else {
            Err(Error::InvalidValue)
        }
    }
}

impl Drop for DecoderGuard {
    fn drop(&mut self) {
        unsafe { ghostty_snapshot_decoder_free(self.0) };
    }
}

struct SnapshotSink {
    bytes: Vec<u8>,
    maximum_bytes: usize,
    history: bool,
    scanned: usize,
    ready: Option<usize>,
    exceeded: bool,
    unknown_format: bool,
}

impl SnapshotSink {
    fn accept(&mut self, data: &[u8]) -> bool {
        if data.len() > self.maximum_bytes - self.bytes.len() {
            self.exceeded = true;
            return false;
        }
        self.bytes.extend_from_slice(data);
        if self.history {
            return true;
        }
        self.find_ready();
        self.ready.is_none() && !self.unknown_format
    }

    /// Follows the documented record framing of snapshot format version 1.
    fn find_ready(&mut self) {
        if self.scanned == 0 {
            let Some(envelope) = self.bytes.get(..SNAPSHOT_ENVELOPE.len()) else {
                return;
            };
            if envelope != SNAPSHOT_ENVELOPE {
                self.unknown_format = true;
                return;
            }
            self.scanned = SNAPSHOT_ENVELOPE.len();
        }
        while let Some(header) = self
            .bytes
            .get(self.scanned..self.scanned + SNAPSHOT_RECORD_HEADER_BYTES)
        {
            let tag = u16::from_le_bytes([header[0], header[1]]);
            let payload = u32::from_le_bytes([header[2], header[3], header[4], header[5]]) as usize;
            let end = self.scanned + SNAPSHOT_RECORD_HEADER_BYTES + payload;
            if end > self.bytes.len() {
                return;
            }
            self.scanned = end;
            if tag == SNAPSHOT_READY_TAG {
                self.ready = Some(end);
                return;
            }
        }
    }
}

unsafe extern "C" fn write_snapshot(userdata: *mut c_void, data: *const u8, len: usize) -> bool {
    if userdata.is_null() || data.is_null() || len > isize::MAX as usize {
        return false;
    }
    let sink = unsafe { &mut *userdata.cast::<SnapshotSink>() };

    sink.accept(unsafe { slice::from_raw_parts(data, len) })
}

fn encode_buffer(
    mut encode: impl FnMut(*mut u8, usize, *mut usize) -> GhosttyResult,
) -> Result<Vec<u8>, Error> {
    let mut required = 0;
    let query = encode(ptr::null_mut(), 0, ptr::from_mut(&mut required));
    if query != GhosttyResult_GHOSTTY_OUT_OF_SPACE {
        result(query)?;
        if required == 0 {
            return Ok(Vec::new());
        }
    }
    let mut output = vec![0_u8; required];
    let mut written = 0;
    result(encode(
        output.as_mut_ptr(),
        output.len(),
        ptr::from_mut(&mut written),
    ))?;
    if written > output.len() {
        return Err(Error::OutOfSpace);
    }
    output.truncate(written);
    Ok(output)
}

const fn result(value: GhosttyResult) -> Result<(), Error> {
    match value {
        GhosttyResult_GHOSTTY_SUCCESS => Ok(()),
        GhosttyResult_GHOSTTY_OUT_OF_MEMORY => Err(Error::OutOfMemory),
        GhosttyResult_GHOSTTY_INVALID_VALUE => Err(Error::InvalidValue),
        GhosttyResult_GHOSTTY_OUT_OF_SPACE => Err(Error::OutOfSpace),
        GhosttyResult_GHOSTTY_NO_VALUE => Err(Error::NoValue),
        GhosttyResult_GHOSTTY_IO_ERROR => Err(Error::Io),
        GhosttyResult_GHOSTTY_LIMIT_EXCEEDED => Err(Error::LimitExceeded),
        other => Err(Error::UnexpectedResult(other)),
    }
}

const fn screen_from_raw(screen: GhosttyTerminalScreen) -> Result<Screen, Error> {
    match screen {
        GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_PRIMARY => Ok(Screen::Primary),
        GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_ALTERNATE => Ok(Screen::Alternate),
        _ => Err(Error::InvalidValue),
    }
}

fn callback<R: Copy>(
    userdata: *mut c_void,
    failure: R,
    operation: impl FnOnce(&mut CallbackState) -> R,
) -> R {
    if userdata.is_null() {
        return failure;
    }
    let completed = catch_unwind(AssertUnwindSafe(|| {
        let state = unsafe { &mut *userdata.cast::<CallbackState>() };
        #[cfg(test)]
        assert!(!mem::take(&mut state.panic_next), "injected callback panic");
        operation(state)
    }));
    completed.unwrap_or_else(|_| {
        unsafe { (*userdata.cast::<CallbackState>()).poisoned = true };
        failure
    })
}

unsafe fn borrowed_bytes(ptr: *const u8, len: usize) -> Option<Vec<u8>> {
    if len == 0 {
        return Some(Vec::new());
    }
    if ptr.is_null() || len > isize::MAX as usize {
        return None;
    }

    Some(unsafe { slice::from_raw_parts(ptr, len) }.to_vec())
}

unsafe fn query_string(terminal: GhosttyTerminal, key: GhosttyTerminalData) -> Option<Vec<u8>> {
    let mut string = MaybeUninit::<GhosttyString>::uninit();

    if unsafe { ghostty_terminal_get(terminal, key, string.as_mut_ptr().cast::<c_void>()) }
        != GhosttyResult_GHOSTTY_SUCCESS
    {
        return None;
    }

    let string = unsafe { string.assume_init() };

    unsafe { borrowed_bytes(string.ptr, string.len) }
}

unsafe extern "C" fn write_pty_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    data: *const u8,
    len: usize,
) {
    callback(userdata, (), |state| {
        if let Some(bytes) = unsafe { borrowed_bytes(data, len) } {
            state.effects.push(Effect::PtyWrite(bytes));
        } else {
            state.poisoned = true;
        }
    });
}

unsafe extern "C" fn bell_callback(_terminal: GhosttyTerminal, userdata: *mut c_void) {
    callback(userdata, (), |state| state.effects.push(Effect::Bell));
}

unsafe extern "C" fn title_callback(terminal: GhosttyTerminal, userdata: *mut c_void) {
    callback(userdata, (), |state| {
        let title =
            unsafe { query_string(terminal, GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_TITLE) };
        if let Some(title) = title {
            state.effects.push(Effect::Title(title));
        }
    });
}

unsafe extern "C" fn pwd_callback(terminal: GhosttyTerminal, userdata: *mut c_void) {
    callback(userdata, (), |state| {
        let pwd = unsafe { query_string(terminal, GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_PWD) };
        if let Some(pwd) = pwd {
            state.effects.push(Effect::Pwd(pwd));
        }
    });
}

unsafe extern "C" fn desktop_notification_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    notification: *const GhosttyTerminalDesktopNotification,
) {
    callback(userdata, (), |state| {
        if notification.is_null() {
            state.poisoned = true;
            return;
        }

        let notification = unsafe { &*notification };
        if notification.size < mem::size_of::<GhosttyTerminalDesktopNotification>() {
            state.poisoned = true;
            return;
        }

        let Some(title) =
            (unsafe { borrowed_bytes(notification.title.ptr, notification.title.len) })
        else {
            state.poisoned = true;
            return;
        };

        let Some(body) = (unsafe { borrowed_bytes(notification.body.ptr, notification.body.len) })
        else {
            state.poisoned = true;
            return;
        };
        state
            .effects
            .push(Effect::DesktopNotification { title, body });
    });
}

unsafe extern "C" fn program_status_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    report: *const GhosttyTerminalProgramStatus,
) {
    callback(userdata, (), |state| {
        if report.is_null() {
            state.poisoned = true;
            return;
        }

        let report = unsafe { &*report };
        if report.size < mem::size_of::<GhosttyTerminalProgramStatus>() {
            state.poisoned = true;
            return;
        }

        let status = match report.state {
            GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_IDLE => ProgramStatusState::Idle,
            GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_WORKING => {
                ProgramStatusState::Working
            }
            GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_DONE => ProgramStatusState::Done,
            GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_BLOCKED => {
                ProgramStatusState::Blocked
            }
            GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_ERROR => {
                ProgramStatusState::Error
            }
            GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_CLEAR => {
                ProgramStatusState::Clear
            }
            _ => return,
        };
        let kind = match report.kind {
            GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_PERMISSION => {
                Some(ProgramStatusKind::Permission)
            }
            GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_QUESTION => {
                Some(ProgramStatusKind::Question)
            }
            GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_AUTH => {
                Some(ProgramStatusKind::Auth)
            }
            _ => None,
        };
        let text = |string: &GhosttyString| unsafe { borrowed_bytes(string.ptr, string.len) };
        let (Some(id), Some(app), Some(title), Some(message)) = (
            text(&report.id),
            text(&report.app),
            text(&report.title),
            text(&report.message),
        ) else {
            state.poisoned = true;
            return;
        };
        state.effects.push(Effect::ProgramStatus(ProgramStatus {
            state: status,
            kind,
            progress: u8::try_from(report.progress).ok(),
            id,
            app,
            title,
            message,
        }));
    });
}

unsafe extern "C" fn progress_report_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    report: *const GhosttyTerminalProgressReport,
) {
    callback(userdata, (), |state| {
        if report.is_null() {
            state.poisoned = true;
            return;
        }

        let report = unsafe { &*report };
        if report.size < mem::size_of::<GhosttyTerminalProgressReport>() {
            state.poisoned = true;
            return;
        }

        let progress = match report.state {
            GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_REMOVE => {
                ProgressState::Remove
            }
            GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_SET => ProgressState::Set,
            GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_ERROR => {
                ProgressState::Error
            }
            GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_INDETERMINATE => {
                ProgressState::Indeterminate
            }
            GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_PAUSE => {
                ProgressState::Pause
            }
            _ => return,
        };
        state.effects.push(Effect::Progress {
            state: progress,
            progress: u8::try_from(report.progress)
                .ok()
                .filter(|progress| *progress <= 100),
        });
    });
}

unsafe extern "C" fn xtversion_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
) -> GhosttyString {
    callback(
        userdata,
        GhosttyString {
            ptr: ptr::null(),
            len: 0,
        },
        |state| GhosttyString {
            ptr: state.version.as_ptr(),
            len: state.version.len(),
        },
    )
}

unsafe extern "C" fn reset_callback(_terminal: GhosttyTerminal, userdata: *mut c_void) {
    callback(userdata, (), |state| state.effects.push(Effect::Reset));
}

unsafe extern "C" fn size_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    out_size: *mut GhosttySizeReportSize,
) -> bool {
    callback(userdata, false, |state| {
        if out_size.is_null() {
            state.poisoned = true;
            return false;
        }

        unsafe { *out_size = state.size };
        true
    })
}

unsafe extern "C" fn color_scheme_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    out_scheme: *mut GhosttyColorScheme,
) -> bool {
    callback(userdata, false, |state| {
        if out_scheme.is_null() {
            state.poisoned = true;
            return false;
        }

        unsafe { *out_scheme = GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_DARK };
        true
    })
}

unsafe extern "C" fn device_attributes_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    out_attributes: *mut GhosttyDeviceAttributes,
) -> bool {
    callback(userdata, false, |state| {
        if out_attributes.is_null() {
            state.poisoned = true;
            return false;
        }
        let mut attributes = GhosttyDeviceAttributes::default();
        let Ok(conformance_level) = u16::try_from(GHOSTTY_DA_CONFORMANCE_VT220) else {
            state.poisoned = true;
            return false;
        };
        let Ok(selective_erase) = u16::try_from(GHOSTTY_DA_FEATURE_SELECTIVE_ERASE) else {
            state.poisoned = true;
            return false;
        };
        let Ok(ansi_color) = u16::try_from(GHOSTTY_DA_FEATURE_ANSI_COLOR) else {
            state.poisoned = true;
            return false;
        };
        let Ok(clipboard) = u16::try_from(GHOSTTY_DA_FEATURE_CLIPBOARD) else {
            state.poisoned = true;
            return false;
        };
        let Ok(device_type) = u16::try_from(GHOSTTY_DA_DEVICE_TYPE_VT220) else {
            state.poisoned = true;
            return false;
        };
        attributes.primary.conformance_level = conformance_level;
        let features = [selective_erase, ansi_color, clipboard];
        attributes.primary.features[..features.len()].copy_from_slice(&features);
        attributes.primary.num_features = features.len();
        attributes.secondary.device_type = device_type;
        attributes.tertiary.unit_id = 0;

        unsafe { *out_attributes = attributes };
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal() -> Terminal {
        Terminal::new(20, 4, 1024 * 1024, 8 * 1024 * 1024, 10_000).expect("terminal")
    }

    #[test]
    fn callback_panic_is_contained_and_reported_by_write() {
        let mut terminal = terminal();
        terminal.panic_next_callback();

        assert_eq!(terminal.write(b"\x07"), Err(Error::CallbackRejected));
        assert_eq!(terminal.state(), Err(Error::CallbackRejected));
    }

    #[test]
    fn callback_panic_returns_safe_value_and_poisons_terminal() {
        let mut terminal = terminal();
        terminal.panic_next_callback();
        let userdata = ptr::from_mut(terminal.callbacks.as_mut()).cast::<c_void>();
        let mut scheme = GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_LIGHT;

        let accepted = unsafe { color_scheme_callback(terminal.raw, userdata, &raw mut scheme) };

        assert!(!accepted);
        assert_eq!(
            scheme, GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_LIGHT,
            "a failed callback must not claim or write an output value"
        );
        assert_eq!(terminal.state(), Err(Error::CallbackRejected));
    }

    #[test]
    fn a_program_status_query_gets_its_answer_before_the_device_attributes() {
        let mut terminal = terminal();
        let effects = terminal.write(b"\x1b]7501;?\x1b\\\x1b[c").expect("write");
        let replies = effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::PtyWrite(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            replies.first().copied(),
            Some(
                &b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\"[..]
            )
        );
        assert!(
            replies
                .get(1)
                .is_some_and(|reply| reply.starts_with(b"\x1b[?"))
        );
    }

    #[test]
    fn program_status_reports_arrive_in_order_with_their_values() {
        let mut terminal = terminal();
        let effects = terminal
            .write(
                b"\x1b]7501;state=blocked:kind=permission:progress=42:id=tasks/review:app=claude-code:title=UmV2aWV3:msg=QWxsb3c/\x07\
                  \x1b]7501;state=working:kind=auth:progress=101\x07\
                  \x1b]7501;state=dance\x07\
                  \x1b]7501;state=idle:msg=bm8KbmV3bGluZQ==\x07\
                  \x1b]7501;state=clear:id=tasks\x07",
            )
            .expect("write");
        assert_eq!(
            effects,
            vec![
                Effect::ProgramStatus(ProgramStatus {
                    state: ProgramStatusState::Blocked,
                    kind: Some(ProgramStatusKind::Permission),
                    progress: Some(42),
                    id: b"tasks/review".to_vec(),
                    app: b"claude-code".to_vec(),
                    title: b"Review".to_vec(),
                    message: b"Allow?".to_vec(),
                }),
                Effect::ProgramStatus(ProgramStatus {
                    state: ProgramStatusState::Working,
                    kind: None,
                    progress: None,
                    id: Vec::new(),
                    app: Vec::new(),
                    title: Vec::new(),
                    message: Vec::new(),
                }),
                Effect::ProgramStatus(ProgramStatus {
                    state: ProgramStatusState::Clear,
                    kind: None,
                    progress: None,
                    id: b"tasks".to_vec(),
                    app: Vec::new(),
                    title: Vec::new(),
                    message: Vec::new(),
                }),
            ]
        );
    }

    #[test]
    fn a_full_reset_clears_every_program_status_record_and_then_reports_the_reset() {
        let mut terminal = terminal();
        terminal
            .write(b"\x1b]7501;state=working\x07")
            .expect("write");
        let effects = terminal.write(b"\x1bc").expect("reset");
        let cleared = effects.iter().position(|effect| {
            matches!(
                effect,
                Effect::ProgramStatus(ProgramStatus {
                    state: ProgramStatusState::Clear,
                    id,
                    ..
                }) if id.is_empty()
            )
        });
        let reset = effects
            .iter()
            .position(|effect| matches!(effect, Effect::Reset));
        assert!(cleared.is_some() && cleared < reset);
    }

    #[test]
    fn progress_reports_arrive_with_their_state_and_value() {
        let mut terminal = terminal();
        let effects = terminal
            .write(
                b"\x1b]9;4;1;40\x07\x1b]9;4;3\x07\x1b]9;4;2;70\x07\x1b]9;4;4;10\x07\x1b]9;4;0\x07",
            )
            .expect("write");
        assert_eq!(
            effects,
            vec![
                Effect::Progress {
                    state: ProgressState::Set,
                    progress: Some(40)
                },
                Effect::Progress {
                    state: ProgressState::Indeterminate,
                    progress: None
                },
                Effect::Progress {
                    state: ProgressState::Error,
                    progress: Some(70)
                },
                Effect::Progress {
                    state: ProgressState::Pause,
                    progress: Some(10)
                },
                Effect::Progress {
                    state: ProgressState::Remove,
                    progress: None
                },
            ]
        );
    }

    #[test]
    fn ordered_effects_are_copied_before_return() {
        let mut terminal = terminal();
        let effects = terminal
            .write(b"\x07\x1b]9;hello\x07\x1b]52;c;aGk=\x07")
            .expect("write");
        assert!(matches!(effects.first(), Some(Effect::Bell)));
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::DesktopNotification { body, .. } if body == b"hello")
        ));
    }

    #[test]
    fn a_kitty_clipboard_write_is_refused_before_it_keeps_data() {
        let mut terminal = terminal();
        let begin = terminal
            .write(b"\x1b]5522;type=write:id=linux\x1b\\")
            .expect("begin clipboard write");
        assert_eq!(
            begin,
            vec![Effect::PtyWrite(
                b"\x1b]5522;type=write:status=ENOSYS:id=linux\x1b\\".to_vec()
            )]
        );
        for packet in [
            &b"\x1b]5522;type=wdata:mime=dGV4dC9wbGFpbg==;S29kb3Np\x1b\\"[..],
            b"\x1b]5522;type=wdata\x1b\\",
        ] {
            assert_eq!(terminal.write(packet).expect("later packet"), []);
        }
    }

    #[test]
    fn unrepresentable_borrowed_length_is_rejected_without_slice_construction() {
        let value = 1_u8;

        assert!(unsafe { borrowed_bytes(&raw const value, usize::MAX) }.is_none());
    }

    #[test]
    fn focus_encoding_follows_terminal_mode() {
        let mut terminal = terminal();
        assert_eq!(
            terminal.encode_focus(true).expect("unreported focus"),
            [] as [u8; 0]
        );
        terminal.write(b"\x1b[?1004h").expect("focus reporting");
        assert_eq!(terminal.encode_focus(true).expect("focus"), b"\x1b[I");
        assert_eq!(terminal.encode_focus(false).expect("blur"), b"\x1b[O");
    }

    #[test]
    fn title_pwd_and_query_effects_are_owned_bytes() {
        let mut terminal = terminal();
        let effects = terminal
            .write(b"\x1b]2;agent\x07\x1b]7;file://host/tmp/project\x07\x1b[c")
            .expect("effects");
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::Title(value) if value == b"agent"))
        );
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::Pwd(value) if value == b"file://host/tmp/project")
        ));
        assert!(effects.iter().any(
            |effect| matches!(effect, Effect::PtyWrite(value) if value == b"\x1b[?62;6;22;52c")
        ));
    }

    #[test]
    fn a_version_query_gets_the_reported_version() {
        let mut terminal = terminal();
        assert_eq!(
            terminal.write(b"\x1b[>q").expect("default version"),
            vec![Effect::PtyWrite(b"\x1bP>|libghostty\x1b\\".to_vec())]
        );

        terminal.report_version("Kodosi 1.2.3").expect("version");
        assert_eq!(
            terminal.write(b"\x1b[>q").expect("reported version"),
            vec![Effect::PtyWrite(b"\x1bP>|Kodosi 1.2.3\x1b\\".to_vec())]
        );
    }

    #[test]
    fn title_reports_and_clipboard_reads_never_reflect_into_pty_input() {
        let mut terminal = terminal();
        let effects = terminal
            .write(b"\x1b]2;printf injected\\n\x07\x1b[21t\x1b]52;c;?\x07")
            .expect("queries");

        assert_eq!(
            effects,
            vec![Effect::Title(b"printf injected\\n".to_vec())],
            "title setting remains observable, but CSI 21t and OSC 52 reads stay silent"
        );
    }

    fn snapshot(terminal: &mut Terminal, history: bool) -> Vec<u8> {
        terminal
            .encode_snapshot(history, usize::MAX)
            .expect("snapshot")
    }

    fn restore(snapshot: &[u8]) -> Result<Terminal, Error> {
        Terminal::from_snapshot(snapshot, 1024 * 1024, 8 * 1024 * 1024, 10_000)
    }

    fn with_history() -> Terminal {
        let mut terminal = terminal();
        for line in 0..400 {
            terminal
                .write(format!("history line {line}\r\n").as_bytes())
                .expect("history");
        }
        terminal
    }

    #[test]
    fn a_snapshot_keeps_unfinished_input() {
        let mut source = terminal();
        source.write(b"before\x1b]2;half").expect("source");
        let mut restored = restore(&snapshot(&mut source, true)).expect("restore");

        let effects = restored.write(b" title\x07after").expect("finish title");
        source.write(b" title\x07after").expect("source finish");

        assert!(effects.contains(&Effect::Title(b"half title".to_vec())));
        assert_eq!(snapshot(&mut restored, true), snapshot(&mut source, true));
    }

    #[test]
    fn a_snapshot_over_the_limit_is_refused() {
        let mut terminal = with_history();
        let full = snapshot(&mut terminal, true);

        assert_eq!(
            terminal.encode_snapshot(true, full.len() - 1),
            Err(Error::LimitExceeded)
        );
        assert_eq!(terminal.encode_snapshot(true, full.len()), Ok(full));
    }

    #[test]
    fn a_snapshot_tells_its_size_and_scrollback_before_it_is_decoded() {
        let visible = snapshot(&mut with_history(), false);

        assert_eq!(
            SnapshotHeader::read(&visible),
            Ok(SnapshotHeader {
                cols: 20,
                rows: 4,
                scrollback_max_bytes: Some(8 * 1024 * 1024),
                scrollback_max_lines: Some(10_000),
            })
        );
        assert_eq!(
            SnapshotHeader::read(&visible[..SNAPSHOT_ENVELOPE.len() + 50]),
            Err(Error::InvalidValue)
        );
        assert_eq!(SnapshotHeader::read(b"GHOSTSNP"), Err(Error::InvalidValue));
    }

    #[test]
    fn a_snapshot_that_keeps_more_history_than_allowed_is_refused_before_decoding() {
        let full = snapshot(&mut with_history(), true);
        let mut unbounded =
            Terminal::new(20, 4, 1024 * 1024, usize::MAX, usize::MAX).expect("terminal");
        unbounded.write(b"history").expect("history");
        let unbounded = snapshot(&mut unbounded, true);

        for (snapshot, bytes, lines) in [
            (&full, 8 * 1024 * 1024 - 1, 10_000),
            (&full, 8 * 1024 * 1024, 9_999),
            (&unbounded, usize::MAX - 1, usize::MAX - 1),
        ] {
            assert_eq!(
                Terminal::from_snapshot(snapshot, 1024 * 1024, bytes, lines).err(),
                Some(Error::LimitExceeded)
            );
        }
        assert!(restore(&full).is_ok());
    }

    #[test]
    fn a_visible_snapshot_ends_at_ready_and_restores_alone() {
        let mut source = with_history();
        let full = snapshot(&mut source, true);
        let visible = snapshot(&mut source, false);

        assert!(visible.len() < full.len() && full.starts_with(&visible));
        let mut restored = restore(&visible).expect("restore visible");
        assert_eq!(snapshot(&mut restored, false), visible);
        assert_eq!(restored.state(), source.state());
        assert_eq!(
            source.encode_snapshot(false, visible.len() - 1),
            Err(Error::LimitExceeded)
        );
    }

    #[test]
    fn a_glyph_command_that_a_snapshot_cuts_stays_off() {
        let mut source = terminal();
        source
            .write(b"\x1b_25a1;r;cp=e0a0;AAAA")
            .expect("start glyph command");
        let mut restored = restore(&snapshot(&mut source, true)).expect("restore");

        let effects = restored
            .write(b"AAAAAAAAAA==\x1b\\")
            .expect("end glyph command");

        assert_eq!(effects, Vec::new());
    }

    #[test]
    fn a_damaged_cut_or_extended_snapshot_is_refused() {
        let mut source = with_history();
        let full = snapshot(&mut source, true);
        let visible = snapshot(&mut source, false);
        let mut damaged = full.clone();
        damaged[visible.len() / 2] ^= 0x40;
        let mut extended = full.clone();
        extended.push(0);

        for input in [
            &[][..],
            &damaged[..],
            &extended[..],
            &full[..visible.len() - 1],
            &full[..=visible.len()],
            &full[..full.len() - 1],
        ] {
            assert_eq!(restore(input).err(), Some(Error::InvalidValue));
        }
    }
}
