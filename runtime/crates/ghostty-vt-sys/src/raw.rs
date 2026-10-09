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
    pub screen: Option<Screen>,
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
pub struct CheckpointLimits {
    pub max_json_bytes: usize,
    pub max_string_bytes: usize,
    pub max_continuation_bytes: usize,
    pub max_cells: usize,
}

impl Default for CheckpointLimits {
    fn default() -> Self {
        Self {
            max_json_bytes: GHOSTTY_CHECKPOINT_DEFAULT_MAX_JSON_BYTES as usize,
            max_string_bytes: GHOSTTY_CHECKPOINT_DEFAULT_MAX_STRING_BYTES as usize,
            max_continuation_bytes: GHOSTTY_CHECKPOINT_DEFAULT_MAX_CONTINUATION_BYTES as usize,
            max_cells: GHOSTTY_CHECKPOINT_DEFAULT_MAX_CELLS as usize,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointInfo {
    pub schema_version: u32,
    pub json_bytes: usize,
    pub continuation_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionProgress {
    Unsupported,
    Pending,
    Complete,
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
    dark: bool,
    size: GhosttySizeReportSize,
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
        dark: bool,
    ) -> Result<Self, Error> {
        let mut raw = ptr::null_mut();

        result(unsafe { ghostty_terminal_new(ptr::null(), ptr::from_mut(&mut raw), cols, rows) })?;
        if raw.is_null() {
            return Err(Error::InvalidValue);
        }
        let callbacks = Box::new(CallbackState {
            effects: Vec::new(),
            dark,
            size: GhosttySizeReportSize {
                rows,
                columns: cols,
                cell_width: 0,
                cell_height: 0,
            },
            poisoned: false,
            #[cfg(test)]
            panic_next: false,
        });
        let mut terminal = Self {
            raw,
            callbacks,
            _thread_owned: PhantomData,
        };
        terminal.configure(
            continuation_max_bytes,
            scrollback_max_bytes,
            scrollback_max_lines,
        )?;
        Ok(terminal)
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
            GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE,
            clipboard_callback as *const c_void,
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

    pub fn set_dark(&mut self, dark: bool) -> Result<(), Error> {
        self.ensure_healthy()?;
        self.callbacks.dark = dark;
        Ok(())
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
        let screen = options.screen.map(screen_to_raw);
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
            screen: screen.as_ref().map_or(ptr::null(), ptr::from_ref),
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

    pub fn encode_checkpoint(
        &mut self,
        limits: CheckpointLimits,
        max_history_rows: usize,
    ) -> Result<(Vec<u8>, CheckpointInfo), Error> {
        self.ensure_healthy()?;
        let raw_limits = checkpoint_limits(limits)?;
        let options = GhosttyCheckpointEncodeOptions {
            size: mem::size_of::<GhosttyCheckpointEncodeOptions>(),
            limits: raw_limits,
            max_history_rows,
        };
        let mut buffer = GhosttyBuffer {
            ptr: ptr::null_mut(),
            cap: 0,
            len: 0,
        };
        let mut info = checkpoint_info();

        let query = unsafe {
            ghostty_checkpoint_encode_buf(
                self.raw,
                ptr::from_ref(&options),
                ptr::from_mut(&mut buffer),
                ptr::from_mut(&mut info),
            )
        };
        if query != GhosttyResult_GHOSTTY_OUT_OF_SPACE {
            result(query)?;
        }
        let required = buffer.len;
        if required > limits.max_json_bytes || required > isize::MAX as usize {
            return Err(Error::LimitExceeded);
        }
        let mut bytes = vec![0_u8; required];
        buffer = GhosttyBuffer {
            ptr: bytes.as_mut_ptr(),
            cap: bytes.len(),
            len: 0,
        };
        info = checkpoint_info();

        result(unsafe {
            ghostty_checkpoint_encode_buf(
                self.raw,
                ptr::from_ref(&options),
                ptr::from_mut(&mut buffer),
                ptr::from_mut(&mut info),
            )
        })?;
        if buffer.len > bytes.len() {
            return Err(Error::OutOfSpace);
        }
        bytes.truncate(buffer.len);
        self.ensure_healthy()?;
        let info = checkpoint_info_from_raw(info)?;
        if info.schema_version != GHOSTTY_CHECKPOINT_SCHEMA_VERSION
            || info.json_bytes != bytes.len()
        {
            return Err(Error::InvalidValue);
        }
        Ok((bytes, info))
    }

    pub fn restore_checkpoint(
        &mut self,
        bytes: &[u8],
        limits: CheckpointLimits,
    ) -> Result<CheckpointInfo, Error> {
        self.ensure_healthy()?;
        if bytes.len() > limits.max_json_bytes || bytes.len() > isize::MAX as usize {
            return Err(Error::LimitExceeded);
        }
        let raw_limits = checkpoint_limits(limits)?;
        let options = GhosttyCheckpointRestoreOptions {
            size: mem::size_of::<GhosttyCheckpointRestoreOptions>(),
            limits: raw_limits,
        };
        let mut info = checkpoint_info();

        result(unsafe {
            ghostty_checkpoint_restore(
                self.raw,
                bytes.as_ptr(),
                bytes.len(),
                ptr::from_ref(&options),
                ptr::from_mut(&mut info),
            )
        })?;
        self.ensure_healthy()?;
        let info = checkpoint_info_from_raw(info)?;
        if info.schema_version != GHOSTTY_CHECKPOINT_SCHEMA_VERSION
            || info.json_bytes != bytes.len()
        {
            return Err(Error::InvalidValue);
        }
        Ok(info)
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

const fn checkpoint_limits(limits: CheckpointLimits) -> Result<GhosttyCheckpointLimits, Error> {
    if limits.max_json_bytes == 0
        || limits.max_string_bytes == 0
        || limits.max_continuation_bytes == 0
        || limits.max_cells == 0
    {
        return Err(Error::InvalidValue);
    }
    Ok(GhosttyCheckpointLimits {
        size: mem::size_of::<GhosttyCheckpointLimits>(),
        max_json_bytes: limits.max_json_bytes,
        max_string_bytes: limits.max_string_bytes,
        max_continuation_bytes: limits.max_continuation_bytes,
        max_cells: limits.max_cells,
    })
}

const fn checkpoint_info() -> GhosttyCheckpointInfo {
    GhosttyCheckpointInfo {
        size: mem::size_of::<GhosttyCheckpointInfo>(),
        schema_version: 0,
        json_bytes: 0,
        continuation_bytes: 0,
    }
}

const fn checkpoint_info_from_raw(info: GhosttyCheckpointInfo) -> Result<CheckpointInfo, Error> {
    if info.schema_version != GHOSTTY_CHECKPOINT_SCHEMA_VERSION {
        return Err(Error::NoValue);
    }
    Ok(CheckpointInfo {
        schema_version: info.schema_version,
        json_bytes: info.json_bytes,
        continuation_bytes: info.continuation_bytes,
    })
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

const fn screen_to_raw(screen: Screen) -> GhosttyTerminalScreen {
    match screen {
        Screen::Primary => GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_PRIMARY,
        Screen::Alternate => GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_ALTERNATE,
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

unsafe extern "C" fn clipboard_callback(
    _terminal: GhosttyTerminal,
    userdata: *mut c_void,
    write: *const GhosttyClipboardWrite,
) {
    if write.is_null() {
        callback(userdata, (), |state| state.poisoned = true);
        return;
    }

    let write = unsafe { &*write };
    if write.size < mem::size_of::<GhosttyClipboardWrite>() {
        callback(userdata, (), |state| state.poisoned = true);
        return;
    }
    let Some(reply) = write.reply else {
        callback(userdata, (), |state| state.poisoned = true);
        return;
    };
    let response = GhosttyClipboardWriteReply {
        size: mem::size_of::<GhosttyClipboardWriteReply>(),
        result: GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_DENIED,
        remember: false,
    };
    unsafe { reply(write, &raw const response) };
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

        unsafe {
            *out_scheme = if state.dark {
                GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_DARK
            } else {
                GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_LIGHT
            };
        }
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
        let Ok(device_type) = u16::try_from(GHOSTTY_DA_DEVICE_TYPE_VT220) else {
            state.poisoned = true;
            return false;
        };
        attributes.primary.conformance_level = conformance_level;
        let features = [selective_erase, ansi_color];
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
        Terminal::new(20, 4, 1024 * 1024, 8 * 1024 * 1024, 10_000, true).expect("terminal")
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
    fn malformed_clipboard_callback_poisons_terminal_before_writing() {
        for write in [
            None,
            Some(GhosttyClipboardWrite::default()),
            Some(GhosttyClipboardWrite {
                size: mem::size_of::<GhosttyClipboardWrite>(),
                ..GhosttyClipboardWrite::default()
            }),
        ] {
            let mut terminal = terminal();
            let userdata = ptr::from_mut(terminal.callbacks.as_mut()).cast::<c_void>();
            unsafe {
                clipboard_callback(
                    terminal.raw,
                    userdata,
                    write.as_ref().map_or(ptr::null(), ptr::from_ref),
                );
            }
            assert_eq!(terminal.state(), Err(Error::CallbackRejected));
        }
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
        assert_eq!(replies.first().copied(), Some(&b"\x1b]7501;?\x1b\\"[..]));
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
    fn kitty_clipboard_write_is_denied_without_a_writer_capability() {
        let mut terminal = terminal();
        let mut effects = terminal
            .write(b"\x1b]5522;type=write:id=linux\x1b\\")
            .expect("begin clipboard write");
        effects.extend(
            terminal
                .write(b"\x1b]5522;type=wdata:mime=dGV4dC9wbGFpbg==;S29kb3Np\x1b\\")
                .expect("write clipboard data"),
        );
        effects.extend(
            terminal
                .write(b"\x1b]5522;type=wdata\x1b\\")
                .expect("commit clipboard write"),
        );
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::PtyWrite(response)
                if response == b"\x1b]5522;type=write:status=EPERM:id=linux\x1b\\"
        )));
    }

    #[test]
    fn unrepresentable_borrowed_length_is_rejected_without_slice_construction() {
        let value = 1_u8;

        assert!(unsafe { borrowed_bytes(&raw const value, usize::MAX) }.is_none());
    }

    #[test]
    fn formatter_can_select_inactive_primary() {
        let mut terminal = terminal();
        terminal
            .write(b"primary\x1b[?1049halternate")
            .expect("write");
        let output = terminal
            .format(
                FormatOptions {
                    format: Format::Plain,
                    unwrap: false,
                    trim: true,
                    screen: Some(Screen::Primary),
                    palette: false,
                    modes: false,
                    scrolling_region: false,
                    tabstops: false,
                    pwd: false,
                    keyboard: false,
                    cursor: false,
                    style: false,
                    hyperlink: false,
                    protection: false,
                    kitty_keyboard: false,
                    charsets: false,
                },
                usize::MAX,
            )
            .expect("format");
        assert_eq!(output, b"primary");
        assert_eq!(
            terminal.state().expect("state").active_screen,
            Screen::Alternate
        );
    }

    #[test]
    fn focus_encoding_follows_terminal_mode() {
        let mut terminal = terminal();
        assert!(
            terminal
                .encode_focus(true)
                .expect("unreported focus")
                .is_empty()
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
        assert!(effects.iter().any(|effect| matches!(effect, Effect::PtyWrite(value) if value.starts_with(b"\x1b[?") && !value.windows(3).any(|window| window == b";52"))));
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

    #[test]
    fn checkpoint_two_call_encode_reports_metadata() {
        let mut terminal = terminal();
        terminal
            .write(b"12345678901234567890\x1b[31")
            .expect("write checkpoint source");

        let (bytes, info) = terminal
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("encode checkpoint");

        assert!(!bytes.is_empty());
        assert_eq!(info.schema_version, GHOSTTY_CHECKPOINT_SCHEMA_VERSION);
        assert_eq!(info.json_bytes, bytes.len());
        assert_eq!(info.continuation_bytes, 4);
    }

    #[test]
    fn checkpoint_caller_limit_is_checked_before_allocation() {
        let mut terminal = terminal();
        let (checkpoint, _) = terminal
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("baseline checkpoint");
        let limits = CheckpointLimits {
            max_json_bytes: checkpoint.len() - 1,
            ..CheckpointLimits::default()
        };

        assert_eq!(
            terminal.encode_checkpoint(limits, usize::MAX),
            Err(Error::LimitExceeded)
        );
    }

    #[test]
    fn checkpoint_restore_failure_is_transactional_and_retryable() {
        let mut source = terminal();
        source.write(b"source").expect("source write");
        let (checkpoint, _) = source
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("source checkpoint");

        let mut destination = terminal();
        destination
            .write(b"destination")
            .expect("destination write");
        let (before, _) = destination
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("before checkpoint");
        assert_eq!(
            destination.restore_checkpoint(b"{", CheckpointLimits::default()),
            Err(Error::InvalidValue)
        );
        let (after, _) = destination
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("after failed restore");
        assert_eq!(after, before);

        destination
            .restore_checkpoint(&checkpoint, CheckpointLimits::default())
            .expect("retry valid restore");
        let (restored, _) = destination
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("restored checkpoint");
        assert_eq!(restored, checkpoint);
    }

    #[test]
    fn checkpoint_unsupported_schema_maps_to_no_value_without_mutation() {
        let mut terminal = terminal();
        terminal.write(b"kept").expect("write");
        let (before, _) = terminal
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("before checkpoint");
        let mut unsupported = before.clone();
        let needle = b"\"schemaVersion\":2";
        let position = unsupported
            .windows(needle.len())
            .position(|window| window == needle)
            .expect("schema field");
        unsupported[position + needle.len() - 1] = b'3';

        assert_eq!(
            terminal.restore_checkpoint(&unsupported, CheckpointLimits::default()),
            Err(Error::NoValue)
        );
        let (after, _) = terminal
            .encode_checkpoint(CheckpointLimits::default(), usize::MAX)
            .expect("after unsupported restore");
        assert_eq!(after, before);
    }
}
