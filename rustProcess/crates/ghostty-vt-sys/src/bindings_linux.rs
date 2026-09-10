pub const GHOSTTY_CHECKPOINT_SCHEMA_VERSION: u32 = 2;
pub const GHOSTTY_CHECKPOINT_DEFAULT_MAX_JSON_BYTES: u32 = 8388608;
pub const GHOSTTY_CHECKPOINT_DEFAULT_MAX_STRING_BYTES: u32 = 8388608;
pub const GHOSTTY_CHECKPOINT_DEFAULT_MAX_CONTINUATION_BYTES: u32 = 1048576;
pub const GHOSTTY_CHECKPOINT_DEFAULT_MAX_CELLS: u32 = 16777216;
pub const GHOSTTY_DA_CONFORMANCE_VT220: u32 = 62;
pub const GHOSTTY_DA_FEATURE_SELECTIVE_ERASE: u32 = 6;
pub const GHOSTTY_DA_FEATURE_ANSI_COLOR: u32 = 22;
pub const GHOSTTY_DA_FEATURE_CLIPBOARD: u32 = 52;
pub const GHOSTTY_DA_DEVICE_TYPE_VT220: u32 = 1;
pub const GhosttyResult_GHOSTTY_SUCCESS: GhosttyResult = 0;
pub const GhosttyResult_GHOSTTY_OUT_OF_MEMORY: GhosttyResult = -1;
pub const GhosttyResult_GHOSTTY_INVALID_VALUE: GhosttyResult = -2;
pub const GhosttyResult_GHOSTTY_OUT_OF_SPACE: GhosttyResult = -3;
pub const GhosttyResult_GHOSTTY_NO_VALUE: GhosttyResult = -4;
pub const GhosttyResult_GHOSTTY_IO_ERROR: GhosttyResult = -5;
pub const GhosttyResult_GHOSTTY_LIMIT_EXCEEDED: GhosttyResult = -6;
pub const GhosttyResult_GHOSTTY_REJECTED: GhosttyResult = -7;
pub const GhosttyResult_GHOSTTY_RESULT_MAX_VALUE: GhosttyResult = 2147483647;
pub type GhosttyResult = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyTerminalImpl {
    _unused: [u8; 0],
}
pub type GhosttyTerminal = *mut GhosttyTerminalImpl;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyFormatterImpl {
    _unused: [u8; 0],
}
pub type GhosttyFormatter = *mut GhosttyFormatterImpl;
pub const GhosttyFormatterFormat_GHOSTTY_FORMATTER_FORMAT_PLAIN: GhosttyFormatterFormat = 0;
pub const GhosttyFormatterFormat_GHOSTTY_FORMATTER_FORMAT_VT: GhosttyFormatterFormat = 1;
pub const GhosttyFormatterFormat_GHOSTTY_FORMATTER_FORMAT_HTML: GhosttyFormatterFormat = 2;
pub const GhosttyFormatterFormat_GHOSTTY_FORMATTER_FORMAT_MAX_VALUE: GhosttyFormatterFormat =
    2147483647;
pub type GhosttyFormatterFormat = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyString {
    pub ptr: *const u8,
    pub len: usize,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyString"][::std::mem::size_of::<GhosttyString>() - 16usize];
    ["Alignment of GhosttyString"][::std::mem::align_of::<GhosttyString>() - 8usize];
    ["Offset of field: GhosttyString::ptr"][::std::mem::offset_of!(GhosttyString, ptr) - 0usize];
    ["Offset of field: GhosttyString::len"][::std::mem::offset_of!(GhosttyString, len) - 8usize];
};
impl Default for GhosttyString {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyBuffer {
    pub ptr: *mut u8,
    pub cap: usize,
    pub len: usize,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyBuffer"][::std::mem::size_of::<GhosttyBuffer>() - 24usize];
    ["Alignment of GhosttyBuffer"][::std::mem::align_of::<GhosttyBuffer>() - 8usize];
    ["Offset of field: GhosttyBuffer::ptr"][::std::mem::offset_of!(GhosttyBuffer, ptr) - 0usize];
    ["Offset of field: GhosttyBuffer::cap"][::std::mem::offset_of!(GhosttyBuffer, cap) - 8usize];
    ["Offset of field: GhosttyBuffer::len"][::std::mem::offset_of!(GhosttyBuffer, len) - 16usize];
};
impl Default for GhosttyBuffer {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyAllocatorVtable {
    pub alloc: ::std::option::Option<
        unsafe extern "C" fn(
            ctx: *mut ::std::os::raw::c_void,
            len: usize,
            alignment: u8,
            ret_addr: usize,
        ) -> *mut ::std::os::raw::c_void,
    >,
    pub resize: ::std::option::Option<
        unsafe extern "C" fn(
            ctx: *mut ::std::os::raw::c_void,
            memory: *mut ::std::os::raw::c_void,
            memory_len: usize,
            alignment: u8,
            new_len: usize,
            ret_addr: usize,
        ) -> bool,
    >,
    pub remap: ::std::option::Option<
        unsafe extern "C" fn(
            ctx: *mut ::std::os::raw::c_void,
            memory: *mut ::std::os::raw::c_void,
            memory_len: usize,
            alignment: u8,
            new_len: usize,
            ret_addr: usize,
        ) -> *mut ::std::os::raw::c_void,
    >,
    pub free: ::std::option::Option<
        unsafe extern "C" fn(
            ctx: *mut ::std::os::raw::c_void,
            memory: *mut ::std::os::raw::c_void,
            memory_len: usize,
            alignment: u8,
            ret_addr: usize,
        ),
    >,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyAllocatorVtable"][::std::mem::size_of::<GhosttyAllocatorVtable>() - 32usize];
    ["Alignment of GhosttyAllocatorVtable"]
        [::std::mem::align_of::<GhosttyAllocatorVtable>() - 8usize];
    ["Offset of field: GhosttyAllocatorVtable::alloc"]
        [::std::mem::offset_of!(GhosttyAllocatorVtable, alloc) - 0usize];
    ["Offset of field: GhosttyAllocatorVtable::resize"]
        [::std::mem::offset_of!(GhosttyAllocatorVtable, resize) - 8usize];
    ["Offset of field: GhosttyAllocatorVtable::remap"]
        [::std::mem::offset_of!(GhosttyAllocatorVtable, remap) - 16usize];
    ["Offset of field: GhosttyAllocatorVtable::free"]
        [::std::mem::offset_of!(GhosttyAllocatorVtable, free) - 24usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyAllocator {
    pub ctx: *mut ::std::os::raw::c_void,
    pub vtable: *const GhosttyAllocatorVtable,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyAllocator"][::std::mem::size_of::<GhosttyAllocator>() - 16usize];
    ["Alignment of GhosttyAllocator"][::std::mem::align_of::<GhosttyAllocator>() - 8usize];
    ["Offset of field: GhosttyAllocator::ctx"]
        [::std::mem::offset_of!(GhosttyAllocator, ctx) - 0usize];
    ["Offset of field: GhosttyAllocator::vtable"]
        [::std::mem::offset_of!(GhosttyAllocator, vtable) - 8usize];
};
impl Default for GhosttyAllocator {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyCheckpointLimits {
    pub size: usize,
    pub max_json_bytes: usize,
    pub max_string_bytes: usize,
    pub max_continuation_bytes: usize,
    pub max_cells: usize,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyCheckpointLimits"][::std::mem::size_of::<GhosttyCheckpointLimits>() - 40usize];
    ["Alignment of GhosttyCheckpointLimits"]
        [::std::mem::align_of::<GhosttyCheckpointLimits>() - 8usize];
    ["Offset of field: GhosttyCheckpointLimits::size"]
        [::std::mem::offset_of!(GhosttyCheckpointLimits, size) - 0usize];
    ["Offset of field: GhosttyCheckpointLimits::max_json_bytes"]
        [::std::mem::offset_of!(GhosttyCheckpointLimits, max_json_bytes) - 8usize];
    ["Offset of field: GhosttyCheckpointLimits::max_string_bytes"]
        [::std::mem::offset_of!(GhosttyCheckpointLimits, max_string_bytes) - 16usize];
    ["Offset of field: GhosttyCheckpointLimits::max_continuation_bytes"]
        [::std::mem::offset_of!(GhosttyCheckpointLimits, max_continuation_bytes) - 24usize];
    ["Offset of field: GhosttyCheckpointLimits::max_cells"]
        [::std::mem::offset_of!(GhosttyCheckpointLimits, max_cells) - 32usize];
};
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyCheckpointEncodeOptions {
    pub size: usize,
    pub limits: GhosttyCheckpointLimits,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyCheckpointEncodeOptions"]
        [::std::mem::size_of::<GhosttyCheckpointEncodeOptions>() - 48usize];
    ["Alignment of GhosttyCheckpointEncodeOptions"]
        [::std::mem::align_of::<GhosttyCheckpointEncodeOptions>() - 8usize];
    ["Offset of field: GhosttyCheckpointEncodeOptions::size"]
        [::std::mem::offset_of!(GhosttyCheckpointEncodeOptions, size) - 0usize];
    ["Offset of field: GhosttyCheckpointEncodeOptions::limits"]
        [::std::mem::offset_of!(GhosttyCheckpointEncodeOptions, limits) - 8usize];
};
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyCheckpointRestoreOptions {
    pub size: usize,
    pub limits: GhosttyCheckpointLimits,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyCheckpointRestoreOptions"]
        [::std::mem::size_of::<GhosttyCheckpointRestoreOptions>() - 48usize];
    ["Alignment of GhosttyCheckpointRestoreOptions"]
        [::std::mem::align_of::<GhosttyCheckpointRestoreOptions>() - 8usize];
    ["Offset of field: GhosttyCheckpointRestoreOptions::size"]
        [::std::mem::offset_of!(GhosttyCheckpointRestoreOptions, size) - 0usize];
    ["Offset of field: GhosttyCheckpointRestoreOptions::limits"]
        [::std::mem::offset_of!(GhosttyCheckpointRestoreOptions, limits) - 8usize];
};
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyCheckpointInfo {
    pub size: usize,
    pub schema_version: u32,
    pub json_bytes: usize,
    pub continuation_bytes: usize,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyCheckpointInfo"][::std::mem::size_of::<GhosttyCheckpointInfo>() - 32usize];
    ["Alignment of GhosttyCheckpointInfo"]
        [::std::mem::align_of::<GhosttyCheckpointInfo>() - 8usize];
    ["Offset of field: GhosttyCheckpointInfo::size"]
        [::std::mem::offset_of!(GhosttyCheckpointInfo, size) - 0usize];
    ["Offset of field: GhosttyCheckpointInfo::schema_version"]
        [::std::mem::offset_of!(GhosttyCheckpointInfo, schema_version) - 8usize];
    ["Offset of field: GhosttyCheckpointInfo::json_bytes"]
        [::std::mem::offset_of!(GhosttyCheckpointInfo, json_bytes) - 16usize];
    ["Offset of field: GhosttyCheckpointInfo::continuation_bytes"]
        [::std::mem::offset_of!(GhosttyCheckpointInfo, continuation_bytes) - 24usize];
};
unsafe extern "C" {
    pub fn ghostty_checkpoint_schema() -> u32;
}
unsafe extern "C" {
    pub fn ghostty_checkpoint_encode_buf(
        terminal: GhosttyTerminal,
        options: *const GhosttyCheckpointEncodeOptions,
        buffer: *mut GhosttyBuffer,
        info: *mut GhosttyCheckpointInfo,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_checkpoint_restore(
        terminal: GhosttyTerminal,
        json: *const u8,
        json_len: usize,
        options: *const GhosttyCheckpointRestoreOptions,
        info: *mut GhosttyCheckpointInfo,
    ) -> GhosttyResult;
}
pub const GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_LIGHT: GhosttyColorScheme = 0;
pub const GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_DARK: GhosttyColorScheme = 1;
pub const GhosttyColorScheme_GHOSTTY_COLOR_SCHEME_MAX_VALUE: GhosttyColorScheme = 2147483647;
pub type GhosttyColorScheme = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyDeviceAttributesPrimary {
    pub conformance_level: u16,
    pub features: [u16; 64usize],
    pub num_features: usize,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyDeviceAttributesPrimary"]
        [::std::mem::size_of::<GhosttyDeviceAttributesPrimary>() - 144usize];
    ["Alignment of GhosttyDeviceAttributesPrimary"]
        [::std::mem::align_of::<GhosttyDeviceAttributesPrimary>() - 8usize];
    ["Offset of field: GhosttyDeviceAttributesPrimary::conformance_level"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesPrimary, conformance_level) - 0usize];
    ["Offset of field: GhosttyDeviceAttributesPrimary::features"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesPrimary, features) - 2usize];
    ["Offset of field: GhosttyDeviceAttributesPrimary::num_features"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesPrimary, num_features) - 136usize];
};
impl Default for GhosttyDeviceAttributesPrimary {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyDeviceAttributesSecondary {
    pub device_type: u16,
    pub firmware_version: u16,
    pub rom_cartridge: u16,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyDeviceAttributesSecondary"]
        [::std::mem::size_of::<GhosttyDeviceAttributesSecondary>() - 6usize];
    ["Alignment of GhosttyDeviceAttributesSecondary"]
        [::std::mem::align_of::<GhosttyDeviceAttributesSecondary>() - 2usize];
    ["Offset of field: GhosttyDeviceAttributesSecondary::device_type"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesSecondary, device_type) - 0usize];
    ["Offset of field: GhosttyDeviceAttributesSecondary::firmware_version"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesSecondary, firmware_version) - 2usize];
    ["Offset of field: GhosttyDeviceAttributesSecondary::rom_cartridge"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesSecondary, rom_cartridge) - 4usize];
};
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyDeviceAttributesTertiary {
    pub unit_id: u32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyDeviceAttributesTertiary"]
        [::std::mem::size_of::<GhosttyDeviceAttributesTertiary>() - 4usize];
    ["Alignment of GhosttyDeviceAttributesTertiary"]
        [::std::mem::align_of::<GhosttyDeviceAttributesTertiary>() - 4usize];
    ["Offset of field: GhosttyDeviceAttributesTertiary::unit_id"]
        [::std::mem::offset_of!(GhosttyDeviceAttributesTertiary, unit_id) - 0usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyDeviceAttributes {
    pub primary: GhosttyDeviceAttributesPrimary,
    pub secondary: GhosttyDeviceAttributesSecondary,
    pub tertiary: GhosttyDeviceAttributesTertiary,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyDeviceAttributes"]
        [::std::mem::size_of::<GhosttyDeviceAttributes>() - 160usize];
    ["Alignment of GhosttyDeviceAttributes"]
        [::std::mem::align_of::<GhosttyDeviceAttributes>() - 8usize];
    ["Offset of field: GhosttyDeviceAttributes::primary"]
        [::std::mem::offset_of!(GhosttyDeviceAttributes, primary) - 0usize];
    ["Offset of field: GhosttyDeviceAttributes::secondary"]
        [::std::mem::offset_of!(GhosttyDeviceAttributes, secondary) - 144usize];
    ["Offset of field: GhosttyDeviceAttributes::tertiary"]
        [::std::mem::offset_of!(GhosttyDeviceAttributes, tertiary) - 152usize];
};
impl Default for GhosttyDeviceAttributes {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
pub const GhosttyFocusEvent_GHOSTTY_FOCUS_GAINED: GhosttyFocusEvent = 0;
pub const GhosttyFocusEvent_GHOSTTY_FOCUS_LOST: GhosttyFocusEvent = 1;
pub const GhosttyFocusEvent_GHOSTTY_FOCUS_MAX_VALUE: GhosttyFocusEvent = 2147483647;
pub type GhosttyFocusEvent = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn ghostty_focus_encode(
        event: GhosttyFocusEvent,
        buf: *mut ::std::os::raw::c_char,
        buf_len: usize,
        out_written: *mut usize,
    ) -> GhosttyResult;
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyGridRef {
    pub size: usize,
    pub node: *mut ::std::os::raw::c_void,
    pub x: u16,
    pub y: u16,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyGridRef"][::std::mem::size_of::<GhosttyGridRef>() - 24usize];
    ["Alignment of GhosttyGridRef"][::std::mem::align_of::<GhosttyGridRef>() - 8usize];
    ["Offset of field: GhosttyGridRef::size"]
        [::std::mem::offset_of!(GhosttyGridRef, size) - 0usize];
    ["Offset of field: GhosttyGridRef::node"]
        [::std::mem::offset_of!(GhosttyGridRef, node) - 8usize];
    ["Offset of field: GhosttyGridRef::x"][::std::mem::offset_of!(GhosttyGridRef, x) - 16usize];
    ["Offset of field: GhosttyGridRef::y"][::std::mem::offset_of!(GhosttyGridRef, y) - 18usize];
};
impl Default for GhosttyGridRef {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttySelection {
    pub size: usize,
    pub start: GhosttyGridRef,
    pub end: GhosttyGridRef,
    pub rectangle: bool,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttySelection"][::std::mem::size_of::<GhosttySelection>() - 64usize];
    ["Alignment of GhosttySelection"][::std::mem::align_of::<GhosttySelection>() - 8usize];
    ["Offset of field: GhosttySelection::size"]
        [::std::mem::offset_of!(GhosttySelection, size) - 0usize];
    ["Offset of field: GhosttySelection::start"]
        [::std::mem::offset_of!(GhosttySelection, start) - 8usize];
    ["Offset of field: GhosttySelection::end"]
        [::std::mem::offset_of!(GhosttySelection, end) - 32usize];
    ["Offset of field: GhosttySelection::rectangle"]
        [::std::mem::offset_of!(GhosttySelection, rectangle) - 56usize];
};
impl Default for GhosttySelection {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
pub type GhosttyMode = u16;
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttySizeReportSize {
    pub rows: u16,
    pub columns: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttySizeReportSize"][::std::mem::size_of::<GhosttySizeReportSize>() - 12usize];
    ["Alignment of GhosttySizeReportSize"]
        [::std::mem::align_of::<GhosttySizeReportSize>() - 4usize];
    ["Offset of field: GhosttySizeReportSize::rows"]
        [::std::mem::offset_of!(GhosttySizeReportSize, rows) - 0usize];
    ["Offset of field: GhosttySizeReportSize::columns"]
        [::std::mem::offset_of!(GhosttySizeReportSize, columns) - 2usize];
    ["Offset of field: GhosttySizeReportSize::cell_width"]
        [::std::mem::offset_of!(GhosttySizeReportSize, cell_width) - 4usize];
    ["Offset of field: GhosttySizeReportSize::cell_height"]
        [::std::mem::offset_of!(GhosttySizeReportSize, cell_height) - 8usize];
};
pub const GhosttyTerminalCompressionMode_GHOSTTY_TERMINAL_COMPRESSION_MODE_INCREMENTAL:
    GhosttyTerminalCompressionMode = 0;
pub const GhosttyTerminalCompressionMode_GHOSTTY_TERMINAL_COMPRESSION_MODE_FULL:
    GhosttyTerminalCompressionMode = 1;
pub const GhosttyTerminalCompressionMode_GHOSTTY_TERMINAL_COMPRESSION_MODE_MAX_VALUE:
    GhosttyTerminalCompressionMode = 2147483647;
pub type GhosttyTerminalCompressionMode = ::std::os::raw::c_int;
pub const GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_UNSUPPORTED:
    GhosttyTerminalCompressionResult = 0;
pub const GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_PENDING:
    GhosttyTerminalCompressionResult = 1;
pub const GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_COMPLETE:
    GhosttyTerminalCompressionResult = 2;
pub const GhosttyTerminalCompressionResult_GHOSTTY_TERMINAL_COMPRESSION_RESULT_MAX_VALUE:
    GhosttyTerminalCompressionResult = 2147483647;
pub type GhosttyTerminalCompressionResult = ::std::os::raw::c_int;
pub const GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_PRIMARY: GhosttyTerminalScreen = 0;
pub const GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_ALTERNATE: GhosttyTerminalScreen = 1;
pub const GhosttyTerminalScreen_GHOSTTY_TERMINAL_SCREEN_MAX_VALUE: GhosttyTerminalScreen =
    2147483647;
pub type GhosttyTerminalScreen = ::std::os::raw::c_int;
pub const GhosttyClipboardLocation_GHOSTTY_CLIPBOARD_LOCATION_STANDARD: GhosttyClipboardLocation =
    0;
pub const GhosttyClipboardLocation_GHOSTTY_CLIPBOARD_LOCATION_SELECTION: GhosttyClipboardLocation =
    1;
pub const GhosttyClipboardLocation_GHOSTTY_CLIPBOARD_LOCATION_PRIMARY: GhosttyClipboardLocation = 2;
pub const GhosttyClipboardLocation_GHOSTTY_CLIPBOARD_LOCATION_MAX_VALUE: GhosttyClipboardLocation =
    2147483647;
pub type GhosttyClipboardLocation = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyClipboardContent {
    pub mime: GhosttyString,
    pub data: GhosttyString,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyClipboardContent"][::std::mem::size_of::<GhosttyClipboardContent>() - 32usize];
    ["Alignment of GhosttyClipboardContent"]
        [::std::mem::align_of::<GhosttyClipboardContent>() - 8usize];
    ["Offset of field: GhosttyClipboardContent::mime"]
        [::std::mem::offset_of!(GhosttyClipboardContent, mime) - 0usize];
    ["Offset of field: GhosttyClipboardContent::data"]
        [::std::mem::offset_of!(GhosttyClipboardContent, data) - 16usize];
};
impl Default for GhosttyClipboardContent {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_SUCCESS:
    GhosttyClipboardWriteResult = 0;
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_DENIED:
    GhosttyClipboardWriteResult = 1;
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_UNSUPPORTED:
    GhosttyClipboardWriteResult = 2;
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_BUSY:
    GhosttyClipboardWriteResult = 3;
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_INVALID_DATA:
    GhosttyClipboardWriteResult = 4;
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_IO_ERROR:
    GhosttyClipboardWriteResult = 5;
pub const GhosttyClipboardWriteResult_GHOSTTY_CLIPBOARD_WRITE_RESULT_MAX_VALUE:
    GhosttyClipboardWriteResult = 2147483647;
pub type GhosttyClipboardWriteResult = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyClipboardWriteReply {
    pub size: usize,
    pub result: GhosttyClipboardWriteResult,
    pub remember: bool,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyClipboardWriteReply"]
        [::std::mem::size_of::<GhosttyClipboardWriteReply>() - 16usize];
    ["Alignment of GhosttyClipboardWriteReply"]
        [::std::mem::align_of::<GhosttyClipboardWriteReply>() - 8usize];
    ["Offset of field: GhosttyClipboardWriteReply::size"]
        [::std::mem::offset_of!(GhosttyClipboardWriteReply, size) - 0usize];
    ["Offset of field: GhosttyClipboardWriteReply::result"]
        [::std::mem::offset_of!(GhosttyClipboardWriteReply, result) - 8usize];
    ["Offset of field: GhosttyClipboardWriteReply::remember"]
        [::std::mem::offset_of!(GhosttyClipboardWriteReply, remember) - 12usize];
};
impl Default for GhosttyClipboardWriteReply {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
pub type GhosttyClipboardWriteReplyFn = ::std::option::Option<
    unsafe extern "C" fn(
        write: *const GhosttyClipboardWrite,
        reply: *const GhosttyClipboardWriteReply,
    ),
>;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyClipboardWrite {
    pub size: usize,
    pub location: GhosttyClipboardLocation,
    pub contents: *const GhosttyClipboardContent,
    pub contents_len: usize,
    pub name: GhosttyString,
    pub granted: bool,
    pub can_remember: bool,
    pub ctx: *const ::std::os::raw::c_void,
    pub reply: GhosttyClipboardWriteReplyFn,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyClipboardWrite"][::std::mem::size_of::<GhosttyClipboardWrite>() - 72usize];
    ["Alignment of GhosttyClipboardWrite"]
        [::std::mem::align_of::<GhosttyClipboardWrite>() - 8usize];
    ["Offset of field: GhosttyClipboardWrite::size"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, size) - 0usize];
    ["Offset of field: GhosttyClipboardWrite::location"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, location) - 8usize];
    ["Offset of field: GhosttyClipboardWrite::contents"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, contents) - 16usize];
    ["Offset of field: GhosttyClipboardWrite::contents_len"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, contents_len) - 24usize];
    ["Offset of field: GhosttyClipboardWrite::name"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, name) - 32usize];
    ["Offset of field: GhosttyClipboardWrite::granted"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, granted) - 48usize];
    ["Offset of field: GhosttyClipboardWrite::can_remember"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, can_remember) - 49usize];
    ["Offset of field: GhosttyClipboardWrite::ctx"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, ctx) - 56usize];
    ["Offset of field: GhosttyClipboardWrite::reply"]
        [::std::mem::offset_of!(GhosttyClipboardWrite, reply) - 64usize];
};
impl Default for GhosttyClipboardWrite {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyTerminalDesktopNotification {
    pub size: usize,
    pub title: GhosttyString,
    pub body: GhosttyString,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyTerminalDesktopNotification"]
        [::std::mem::size_of::<GhosttyTerminalDesktopNotification>() - 40usize];
    ["Alignment of GhosttyTerminalDesktopNotification"]
        [::std::mem::align_of::<GhosttyTerminalDesktopNotification>() - 8usize];
    ["Offset of field: GhosttyTerminalDesktopNotification::size"]
        [::std::mem::offset_of!(GhosttyTerminalDesktopNotification, size) - 0usize];
    ["Offset of field: GhosttyTerminalDesktopNotification::title"]
        [::std::mem::offset_of!(GhosttyTerminalDesktopNotification, title) - 8usize];
    ["Offset of field: GhosttyTerminalDesktopNotification::body"]
        [::std::mem::offset_of!(GhosttyTerminalDesktopNotification, body) - 24usize];
};
impl Default for GhosttyTerminalDesktopNotification {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
pub const GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_REMOVE:
    GhosttyTerminalProgressState = 0;
pub const GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_SET:
    GhosttyTerminalProgressState = 1;
pub const GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_ERROR:
    GhosttyTerminalProgressState = 2;
pub const GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_INDETERMINATE:
    GhosttyTerminalProgressState = 3;
pub const GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_PAUSE:
    GhosttyTerminalProgressState = 4;
pub const GhosttyTerminalProgressState_GHOSTTY_TERMINAL_PROGRESS_STATE_MAX_VALUE:
    GhosttyTerminalProgressState = 2147483647;
pub type GhosttyTerminalProgressState = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyTerminalProgressReport {
    pub size: usize,
    pub state: GhosttyTerminalProgressState,
    pub progress: i8,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyTerminalProgressReport"]
        [::std::mem::size_of::<GhosttyTerminalProgressReport>() - 16usize];
    ["Alignment of GhosttyTerminalProgressReport"]
        [::std::mem::align_of::<GhosttyTerminalProgressReport>() - 8usize];
    ["Offset of field: GhosttyTerminalProgressReport::size"]
        [::std::mem::offset_of!(GhosttyTerminalProgressReport, size) - 0usize];
    ["Offset of field: GhosttyTerminalProgressReport::state"]
        [::std::mem::offset_of!(GhosttyTerminalProgressReport, state) - 8usize];
    ["Offset of field: GhosttyTerminalProgressReport::progress"]
        [::std::mem::offset_of!(GhosttyTerminalProgressReport, progress) - 12usize];
};
impl Default for GhosttyTerminalProgressReport {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyTerminalModeConfig {
    pub mode: GhosttyMode,
    pub value: bool,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyTerminalModeConfig"]
        [::std::mem::size_of::<GhosttyTerminalModeConfig>() - 4usize];
    ["Alignment of GhosttyTerminalModeConfig"]
        [::std::mem::align_of::<GhosttyTerminalModeConfig>() - 2usize];
    ["Offset of field: GhosttyTerminalModeConfig::mode"]
        [::std::mem::offset_of!(GhosttyTerminalModeConfig, mode) - 0usize];
    ["Offset of field: GhosttyTerminalModeConfig::value"]
        [::std::mem::offset_of!(GhosttyTerminalModeConfig, value) - 2usize];
};
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_USERDATA: GhosttyTerminalOption = 0;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_WRITE_PTY: GhosttyTerminalOption = 1;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_BELL: GhosttyTerminalOption = 2;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_ENQUIRY: GhosttyTerminalOption = 3;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_XTVERSION: GhosttyTerminalOption = 4;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TITLE_CHANGED: GhosttyTerminalOption = 5;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SIZE: GhosttyTerminalOption = 6;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_SCHEME: GhosttyTerminalOption = 7;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DEVICE_ATTRIBUTES: GhosttyTerminalOption = 8;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TITLE: GhosttyTerminalOption = 9;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PWD: GhosttyTerminalOption = 10;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_FOREGROUND: GhosttyTerminalOption = 11;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_BACKGROUND: GhosttyTerminalOption = 12;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_CURSOR: GhosttyTerminalOption = 13;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_COLOR_PALETTE: GhosttyTerminalOption = 14;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_STORAGE_LIMIT:
    GhosttyTerminalOption = 15;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_FILE:
    GhosttyTerminalOption = 16;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_TEMP_FILE:
    GhosttyTerminalOption = 17;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_KITTY_IMAGE_MEDIUM_SHARED_MEM:
    GhosttyTerminalOption = 18;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_APC_MAX_BYTES: GhosttyTerminalOption = 19;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_APC_MAX_BYTES_KITTY: GhosttyTerminalOption =
    20;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SELECTION: GhosttyTerminalOption = 21;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_STYLE: GhosttyTerminalOption =
    22;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DEFAULT_CURSOR_BLINK: GhosttyTerminalOption =
    23;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_GLYPH_PROTOCOL: GhosttyTerminalOption = 24;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PWD_CHANGED: GhosttyTerminalOption = 25;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE: GhosttyTerminalOption = 26;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_BYTES: GhosttyTerminalOption =
    27;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SCROLLBACK_MAX_LINES: GhosttyTerminalOption =
    28;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_DESKTOP_NOTIFICATION: GhosttyTerminalOption =
    29;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PROGRESS_REPORT: GhosttyTerminalOption = 30;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_CONTINUATION_MAX_BYTES: GhosttyTerminalOption =
    31;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TITLE_REPORT: GhosttyTerminalOption = 32;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_MODE_DEFAULT: GhosttyTerminalOption = 33;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_MODE: GhosttyTerminalOption = 34;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_UNKNOWN_SEQUENCE: GhosttyTerminalOption = 35;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_UNKNOWN_MAX_BYTES: GhosttyTerminalOption = 36;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_TERMINFO_NAME: GhosttyTerminalOption = 37;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_CLIPBOARD_READ: GhosttyTerminalOption = 38;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_CLIPBOARD_WRITE_MAX_BYTES:
    GhosttyTerminalOption = 39;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_MAX_VALUE: GhosttyTerminalOption = 2147483647;
pub type GhosttyTerminalOption = ::std::os::raw::c_int;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_INVALID: GhosttyTerminalData = 0;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLS: GhosttyTerminalData = 1;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_ROWS: GhosttyTerminalData = 2;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_X: GhosttyTerminalData = 3;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_Y: GhosttyTerminalData = 4;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_PENDING_WRAP: GhosttyTerminalData = 5;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_ACTIVE_SCREEN: GhosttyTerminalData = 6;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_VISIBLE: GhosttyTerminalData = 7;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_KEYBOARD_FLAGS: GhosttyTerminalData = 8;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SCROLLBAR: GhosttyTerminalData = 9;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_STYLE: GhosttyTerminalData = 10;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MOUSE_TRACKING: GhosttyTerminalData = 11;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_TITLE: GhosttyTerminalData = 12;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_PWD: GhosttyTerminalData = 13;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_TOTAL_ROWS: GhosttyTerminalData = 14;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SCROLLBACK_ROWS: GhosttyTerminalData = 15;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_WIDTH_PX: GhosttyTerminalData = 16;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_HEIGHT_PX: GhosttyTerminalData = 17;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND: GhosttyTerminalData = 18;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND: GhosttyTerminalData = 19;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_CURSOR: GhosttyTerminalData = 20;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_PALETTE: GhosttyTerminalData = 21;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_FOREGROUND_DEFAULT: GhosttyTerminalData =
    22;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_BACKGROUND_DEFAULT: GhosttyTerminalData =
    23;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_CURSOR_DEFAULT: GhosttyTerminalData = 24;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_COLOR_PALETTE_DEFAULT: GhosttyTerminalData = 25;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT: GhosttyTerminalData =
    26;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_FILE: GhosttyTerminalData =
    27;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_TEMP_FILE:
    GhosttyTerminalData = 28;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_IMAGE_MEDIUM_SHARED_MEM:
    GhosttyTerminalData = 29;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_KITTY_GRAPHICS: GhosttyTerminalData = 30;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SELECTION: GhosttyTerminalData = 31;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_VIEWPORT_ACTIVE: GhosttyTerminalData = 32;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR: GhosttyTerminalData = 33;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SCROLLBACK_MAX_BYTES: GhosttyTerminalData = 34;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_SCROLLBACK_MAX_LINES: GhosttyTerminalData = 35;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CONTINUATION_MAX_BYTES: GhosttyTerminalData =
    36;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MODE: GhosttyTerminalData = 37;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_VT_GROUND: GhosttyTerminalData = 38;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CURSOR_AT_PROMPT: GhosttyTerminalData = 39;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_CLIPBOARD_WRITE_MAX_BYTES: GhosttyTerminalData =
    40;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MAX_VALUE: GhosttyTerminalData = 2147483647;
pub type GhosttyTerminalData = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn ghostty_terminal_new(
        allocator: *const GhosttyAllocator,
        terminal: *mut GhosttyTerminal,
        cols: u16,
        rows: u16,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_terminal_free(terminal: GhosttyTerminal);
}
unsafe extern "C" {
    pub fn ghostty_terminal_resize(
        terminal: GhosttyTerminal,
        cols: u16,
        rows: u16,
        cell_width_px: u32,
        cell_height_px: u32,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_terminal_set(
        terminal: GhosttyTerminal,
        option: GhosttyTerminalOption,
        value: *const ::std::os::raw::c_void,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);
}
unsafe extern "C" {
    pub fn ghostty_terminal_compression_activity(
        terminal: GhosttyTerminal,
        out_activity: *mut u64,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_terminal_compress(
        terminal: GhosttyTerminal,
        mode: GhosttyTerminalCompressionMode,
        out_result: *mut GhosttyTerminalCompressionResult,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_terminal_get(
        terminal: GhosttyTerminal,
        data: GhosttyTerminalData,
        out: *mut ::std::os::raw::c_void,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_terminal_get_multi(
        terminal: GhosttyTerminal,
        count: usize,
        keys: *const GhosttyTerminalData,
        values: *mut *mut ::std::os::raw::c_void,
        out_written: *mut usize,
    ) -> GhosttyResult;
}
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyFormatterScreenExtra {
    pub size: usize,
    pub cursor: bool,
    pub style: bool,
    pub hyperlink: bool,
    pub protection: bool,
    pub kitty_keyboard: bool,
    pub charsets: bool,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyFormatterScreenExtra"]
        [::std::mem::size_of::<GhosttyFormatterScreenExtra>() - 16usize];
    ["Alignment of GhosttyFormatterScreenExtra"]
        [::std::mem::align_of::<GhosttyFormatterScreenExtra>() - 8usize];
    ["Offset of field: GhosttyFormatterScreenExtra::size"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, size) - 0usize];
    ["Offset of field: GhosttyFormatterScreenExtra::cursor"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, cursor) - 8usize];
    ["Offset of field: GhosttyFormatterScreenExtra::style"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, style) - 9usize];
    ["Offset of field: GhosttyFormatterScreenExtra::hyperlink"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, hyperlink) - 10usize];
    ["Offset of field: GhosttyFormatterScreenExtra::protection"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, protection) - 11usize];
    ["Offset of field: GhosttyFormatterScreenExtra::kitty_keyboard"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, kitty_keyboard) - 12usize];
    ["Offset of field: GhosttyFormatterScreenExtra::charsets"]
        [::std::mem::offset_of!(GhosttyFormatterScreenExtra, charsets) - 13usize];
};
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyFormatterTerminalExtra {
    pub size: usize,
    pub palette: bool,
    pub modes: bool,
    pub scrolling_region: bool,
    pub tabstops: bool,
    pub pwd: bool,
    pub keyboard: bool,
    pub screen: GhosttyFormatterScreenExtra,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyFormatterTerminalExtra"]
        [::std::mem::size_of::<GhosttyFormatterTerminalExtra>() - 32usize];
    ["Alignment of GhosttyFormatterTerminalExtra"]
        [::std::mem::align_of::<GhosttyFormatterTerminalExtra>() - 8usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::size"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, size) - 0usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::palette"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, palette) - 8usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::modes"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, modes) - 9usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::scrolling_region"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, scrolling_region) - 10usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::tabstops"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, tabstops) - 11usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::pwd"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, pwd) - 12usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::keyboard"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, keyboard) - 13usize];
    ["Offset of field: GhosttyFormatterTerminalExtra::screen"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalExtra, screen) - 16usize];
};
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyFormatterTerminalOptions {
    pub size: usize,
    pub emit: GhosttyFormatterFormat,
    pub unwrap: bool,
    pub trim: bool,
    pub extra: GhosttyFormatterTerminalExtra,
    pub selection: *const GhosttySelection,
    pub screen: *const GhosttyTerminalScreen,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyFormatterTerminalOptions"]
        [::std::mem::size_of::<GhosttyFormatterTerminalOptions>() - 64usize];
    ["Alignment of GhosttyFormatterTerminalOptions"]
        [::std::mem::align_of::<GhosttyFormatterTerminalOptions>() - 8usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::size"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, size) - 0usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::emit"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, emit) - 8usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::unwrap"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, unwrap) - 12usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::trim"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, trim) - 13usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::extra"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, extra) - 16usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::selection"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, selection) - 48usize];
    ["Offset of field: GhosttyFormatterTerminalOptions::screen"]
        [::std::mem::offset_of!(GhosttyFormatterTerminalOptions, screen) - 56usize];
};
impl Default for GhosttyFormatterTerminalOptions {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
unsafe extern "C" {
    pub fn ghostty_formatter_terminal_new(
        allocator: *const GhosttyAllocator,
        formatter: *mut GhosttyFormatter,
        terminal: GhosttyTerminal,
        options: GhosttyFormatterTerminalOptions,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_formatter_format_buf(
        formatter: GhosttyFormatter,
        buf: *mut u8,
        buf_len: usize,
        out_written: *mut usize,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_formatter_free(formatter: GhosttyFormatter);
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyKeyEventImpl {
    _unused: [u8; 0],
}
pub type GhosttyKeyEvent = *mut GhosttyKeyEventImpl;
pub const GhosttyKeyAction_GHOSTTY_KEY_ACTION_RELEASE: GhosttyKeyAction = 0;
pub const GhosttyKeyAction_GHOSTTY_KEY_ACTION_PRESS: GhosttyKeyAction = 1;
pub const GhosttyKeyAction_GHOSTTY_KEY_ACTION_REPEAT: GhosttyKeyAction = 2;
pub const GhosttyKeyAction_GHOSTTY_KEY_ACTION_MAX_VALUE: GhosttyKeyAction = 2147483647;
pub type GhosttyKeyAction = ::std::os::raw::c_int;
pub type GhosttyMods = u16;
pub const GhosttyKey_GHOSTTY_KEY_UNIDENTIFIED: GhosttyKey = 0;
pub const GhosttyKey_GHOSTTY_KEY_BACKQUOTE: GhosttyKey = 1;
pub const GhosttyKey_GHOSTTY_KEY_BACKSLASH: GhosttyKey = 2;
pub const GhosttyKey_GHOSTTY_KEY_BRACKET_LEFT: GhosttyKey = 3;
pub const GhosttyKey_GHOSTTY_KEY_BRACKET_RIGHT: GhosttyKey = 4;
pub const GhosttyKey_GHOSTTY_KEY_COMMA: GhosttyKey = 5;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_0: GhosttyKey = 6;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_1: GhosttyKey = 7;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_2: GhosttyKey = 8;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_3: GhosttyKey = 9;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_4: GhosttyKey = 10;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_5: GhosttyKey = 11;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_6: GhosttyKey = 12;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_7: GhosttyKey = 13;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_8: GhosttyKey = 14;
pub const GhosttyKey_GHOSTTY_KEY_DIGIT_9: GhosttyKey = 15;
pub const GhosttyKey_GHOSTTY_KEY_EQUAL: GhosttyKey = 16;
pub const GhosttyKey_GHOSTTY_KEY_INTL_BACKSLASH: GhosttyKey = 17;
pub const GhosttyKey_GHOSTTY_KEY_INTL_RO: GhosttyKey = 18;
pub const GhosttyKey_GHOSTTY_KEY_INTL_YEN: GhosttyKey = 19;
pub const GhosttyKey_GHOSTTY_KEY_A: GhosttyKey = 20;
pub const GhosttyKey_GHOSTTY_KEY_B: GhosttyKey = 21;
pub const GhosttyKey_GHOSTTY_KEY_C: GhosttyKey = 22;
pub const GhosttyKey_GHOSTTY_KEY_D: GhosttyKey = 23;
pub const GhosttyKey_GHOSTTY_KEY_E: GhosttyKey = 24;
pub const GhosttyKey_GHOSTTY_KEY_F: GhosttyKey = 25;
pub const GhosttyKey_GHOSTTY_KEY_G: GhosttyKey = 26;
pub const GhosttyKey_GHOSTTY_KEY_H: GhosttyKey = 27;
pub const GhosttyKey_GHOSTTY_KEY_I: GhosttyKey = 28;
pub const GhosttyKey_GHOSTTY_KEY_J: GhosttyKey = 29;
pub const GhosttyKey_GHOSTTY_KEY_K: GhosttyKey = 30;
pub const GhosttyKey_GHOSTTY_KEY_L: GhosttyKey = 31;
pub const GhosttyKey_GHOSTTY_KEY_M: GhosttyKey = 32;
pub const GhosttyKey_GHOSTTY_KEY_N: GhosttyKey = 33;
pub const GhosttyKey_GHOSTTY_KEY_O: GhosttyKey = 34;
pub const GhosttyKey_GHOSTTY_KEY_P: GhosttyKey = 35;
pub const GhosttyKey_GHOSTTY_KEY_Q: GhosttyKey = 36;
pub const GhosttyKey_GHOSTTY_KEY_R: GhosttyKey = 37;
pub const GhosttyKey_GHOSTTY_KEY_S: GhosttyKey = 38;
pub const GhosttyKey_GHOSTTY_KEY_T: GhosttyKey = 39;
pub const GhosttyKey_GHOSTTY_KEY_U: GhosttyKey = 40;
pub const GhosttyKey_GHOSTTY_KEY_V: GhosttyKey = 41;
pub const GhosttyKey_GHOSTTY_KEY_W: GhosttyKey = 42;
pub const GhosttyKey_GHOSTTY_KEY_X: GhosttyKey = 43;
pub const GhosttyKey_GHOSTTY_KEY_Y: GhosttyKey = 44;
pub const GhosttyKey_GHOSTTY_KEY_Z: GhosttyKey = 45;
pub const GhosttyKey_GHOSTTY_KEY_MINUS: GhosttyKey = 46;
pub const GhosttyKey_GHOSTTY_KEY_PERIOD: GhosttyKey = 47;
pub const GhosttyKey_GHOSTTY_KEY_QUOTE: GhosttyKey = 48;
pub const GhosttyKey_GHOSTTY_KEY_SEMICOLON: GhosttyKey = 49;
pub const GhosttyKey_GHOSTTY_KEY_SLASH: GhosttyKey = 50;
pub const GhosttyKey_GHOSTTY_KEY_ALT_LEFT: GhosttyKey = 51;
pub const GhosttyKey_GHOSTTY_KEY_ALT_RIGHT: GhosttyKey = 52;
pub const GhosttyKey_GHOSTTY_KEY_BACKSPACE: GhosttyKey = 53;
pub const GhosttyKey_GHOSTTY_KEY_CAPS_LOCK: GhosttyKey = 54;
pub const GhosttyKey_GHOSTTY_KEY_CONTEXT_MENU: GhosttyKey = 55;
pub const GhosttyKey_GHOSTTY_KEY_CONTROL_LEFT: GhosttyKey = 56;
pub const GhosttyKey_GHOSTTY_KEY_CONTROL_RIGHT: GhosttyKey = 57;
pub const GhosttyKey_GHOSTTY_KEY_ENTER: GhosttyKey = 58;
pub const GhosttyKey_GHOSTTY_KEY_META_LEFT: GhosttyKey = 59;
pub const GhosttyKey_GHOSTTY_KEY_META_RIGHT: GhosttyKey = 60;
pub const GhosttyKey_GHOSTTY_KEY_SHIFT_LEFT: GhosttyKey = 61;
pub const GhosttyKey_GHOSTTY_KEY_SHIFT_RIGHT: GhosttyKey = 62;
pub const GhosttyKey_GHOSTTY_KEY_SPACE: GhosttyKey = 63;
pub const GhosttyKey_GHOSTTY_KEY_TAB: GhosttyKey = 64;
pub const GhosttyKey_GHOSTTY_KEY_CONVERT: GhosttyKey = 65;
pub const GhosttyKey_GHOSTTY_KEY_KANA_MODE: GhosttyKey = 66;
pub const GhosttyKey_GHOSTTY_KEY_NON_CONVERT: GhosttyKey = 67;
pub const GhosttyKey_GHOSTTY_KEY_DELETE: GhosttyKey = 68;
pub const GhosttyKey_GHOSTTY_KEY_END: GhosttyKey = 69;
pub const GhosttyKey_GHOSTTY_KEY_HELP: GhosttyKey = 70;
pub const GhosttyKey_GHOSTTY_KEY_HOME: GhosttyKey = 71;
pub const GhosttyKey_GHOSTTY_KEY_INSERT: GhosttyKey = 72;
pub const GhosttyKey_GHOSTTY_KEY_PAGE_DOWN: GhosttyKey = 73;
pub const GhosttyKey_GHOSTTY_KEY_PAGE_UP: GhosttyKey = 74;
pub const GhosttyKey_GHOSTTY_KEY_ARROW_DOWN: GhosttyKey = 75;
pub const GhosttyKey_GHOSTTY_KEY_ARROW_LEFT: GhosttyKey = 76;
pub const GhosttyKey_GHOSTTY_KEY_ARROW_RIGHT: GhosttyKey = 77;
pub const GhosttyKey_GHOSTTY_KEY_ARROW_UP: GhosttyKey = 78;
pub const GhosttyKey_GHOSTTY_KEY_NUM_LOCK: GhosttyKey = 79;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_0: GhosttyKey = 80;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_1: GhosttyKey = 81;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_2: GhosttyKey = 82;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_3: GhosttyKey = 83;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_4: GhosttyKey = 84;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_5: GhosttyKey = 85;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_6: GhosttyKey = 86;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_7: GhosttyKey = 87;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_8: GhosttyKey = 88;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_9: GhosttyKey = 89;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_ADD: GhosttyKey = 90;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_BACKSPACE: GhosttyKey = 91;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_CLEAR: GhosttyKey = 92;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_CLEAR_ENTRY: GhosttyKey = 93;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_COMMA: GhosttyKey = 94;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_DECIMAL: GhosttyKey = 95;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_DIVIDE: GhosttyKey = 96;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_ENTER: GhosttyKey = 97;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_EQUAL: GhosttyKey = 98;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_MEMORY_ADD: GhosttyKey = 99;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_MEMORY_CLEAR: GhosttyKey = 100;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_MEMORY_RECALL: GhosttyKey = 101;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_MEMORY_STORE: GhosttyKey = 102;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_MEMORY_SUBTRACT: GhosttyKey = 103;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_MULTIPLY: GhosttyKey = 104;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_PAREN_LEFT: GhosttyKey = 105;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_PAREN_RIGHT: GhosttyKey = 106;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_SUBTRACT: GhosttyKey = 107;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_SEPARATOR: GhosttyKey = 108;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_UP: GhosttyKey = 109;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_DOWN: GhosttyKey = 110;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_RIGHT: GhosttyKey = 111;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_LEFT: GhosttyKey = 112;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_BEGIN: GhosttyKey = 113;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_HOME: GhosttyKey = 114;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_END: GhosttyKey = 115;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_INSERT: GhosttyKey = 116;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_DELETE: GhosttyKey = 117;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_PAGE_UP: GhosttyKey = 118;
pub const GhosttyKey_GHOSTTY_KEY_NUMPAD_PAGE_DOWN: GhosttyKey = 119;
pub const GhosttyKey_GHOSTTY_KEY_ESCAPE: GhosttyKey = 120;
pub const GhosttyKey_GHOSTTY_KEY_F1: GhosttyKey = 121;
pub const GhosttyKey_GHOSTTY_KEY_F2: GhosttyKey = 122;
pub const GhosttyKey_GHOSTTY_KEY_F3: GhosttyKey = 123;
pub const GhosttyKey_GHOSTTY_KEY_F4: GhosttyKey = 124;
pub const GhosttyKey_GHOSTTY_KEY_F5: GhosttyKey = 125;
pub const GhosttyKey_GHOSTTY_KEY_F6: GhosttyKey = 126;
pub const GhosttyKey_GHOSTTY_KEY_F7: GhosttyKey = 127;
pub const GhosttyKey_GHOSTTY_KEY_F8: GhosttyKey = 128;
pub const GhosttyKey_GHOSTTY_KEY_F9: GhosttyKey = 129;
pub const GhosttyKey_GHOSTTY_KEY_F10: GhosttyKey = 130;
pub const GhosttyKey_GHOSTTY_KEY_F11: GhosttyKey = 131;
pub const GhosttyKey_GHOSTTY_KEY_F12: GhosttyKey = 132;
pub const GhosttyKey_GHOSTTY_KEY_F13: GhosttyKey = 133;
pub const GhosttyKey_GHOSTTY_KEY_F14: GhosttyKey = 134;
pub const GhosttyKey_GHOSTTY_KEY_F15: GhosttyKey = 135;
pub const GhosttyKey_GHOSTTY_KEY_F16: GhosttyKey = 136;
pub const GhosttyKey_GHOSTTY_KEY_F17: GhosttyKey = 137;
pub const GhosttyKey_GHOSTTY_KEY_F18: GhosttyKey = 138;
pub const GhosttyKey_GHOSTTY_KEY_F19: GhosttyKey = 139;
pub const GhosttyKey_GHOSTTY_KEY_F20: GhosttyKey = 140;
pub const GhosttyKey_GHOSTTY_KEY_F21: GhosttyKey = 141;
pub const GhosttyKey_GHOSTTY_KEY_F22: GhosttyKey = 142;
pub const GhosttyKey_GHOSTTY_KEY_F23: GhosttyKey = 143;
pub const GhosttyKey_GHOSTTY_KEY_F24: GhosttyKey = 144;
pub const GhosttyKey_GHOSTTY_KEY_F25: GhosttyKey = 145;
pub const GhosttyKey_GHOSTTY_KEY_FN: GhosttyKey = 146;
pub const GhosttyKey_GHOSTTY_KEY_FN_LOCK: GhosttyKey = 147;
pub const GhosttyKey_GHOSTTY_KEY_PRINT_SCREEN: GhosttyKey = 148;
pub const GhosttyKey_GHOSTTY_KEY_SCROLL_LOCK: GhosttyKey = 149;
pub const GhosttyKey_GHOSTTY_KEY_PAUSE: GhosttyKey = 150;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_BACK: GhosttyKey = 151;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_FAVORITES: GhosttyKey = 152;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_FORWARD: GhosttyKey = 153;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_HOME: GhosttyKey = 154;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_REFRESH: GhosttyKey = 155;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_SEARCH: GhosttyKey = 156;
pub const GhosttyKey_GHOSTTY_KEY_BROWSER_STOP: GhosttyKey = 157;
pub const GhosttyKey_GHOSTTY_KEY_EJECT: GhosttyKey = 158;
pub const GhosttyKey_GHOSTTY_KEY_LAUNCH_APP_1: GhosttyKey = 159;
pub const GhosttyKey_GHOSTTY_KEY_LAUNCH_APP_2: GhosttyKey = 160;
pub const GhosttyKey_GHOSTTY_KEY_LAUNCH_MAIL: GhosttyKey = 161;
pub const GhosttyKey_GHOSTTY_KEY_MEDIA_PLAY_PAUSE: GhosttyKey = 162;
pub const GhosttyKey_GHOSTTY_KEY_MEDIA_SELECT: GhosttyKey = 163;
pub const GhosttyKey_GHOSTTY_KEY_MEDIA_STOP: GhosttyKey = 164;
pub const GhosttyKey_GHOSTTY_KEY_MEDIA_TRACK_NEXT: GhosttyKey = 165;
pub const GhosttyKey_GHOSTTY_KEY_MEDIA_TRACK_PREVIOUS: GhosttyKey = 166;
pub const GhosttyKey_GHOSTTY_KEY_POWER: GhosttyKey = 167;
pub const GhosttyKey_GHOSTTY_KEY_SLEEP: GhosttyKey = 168;
pub const GhosttyKey_GHOSTTY_KEY_AUDIO_VOLUME_DOWN: GhosttyKey = 169;
pub const GhosttyKey_GHOSTTY_KEY_AUDIO_VOLUME_MUTE: GhosttyKey = 170;
pub const GhosttyKey_GHOSTTY_KEY_AUDIO_VOLUME_UP: GhosttyKey = 171;
pub const GhosttyKey_GHOSTTY_KEY_WAKE_UP: GhosttyKey = 172;
pub const GhosttyKey_GHOSTTY_KEY_COPY: GhosttyKey = 173;
pub const GhosttyKey_GHOSTTY_KEY_CUT: GhosttyKey = 174;
pub const GhosttyKey_GHOSTTY_KEY_PASTE: GhosttyKey = 175;
pub const GhosttyKey_GHOSTTY_KEY_MAX_VALUE: GhosttyKey = 2147483647;
pub type GhosttyKey = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn ghostty_key_event_new(
        allocator: *const GhosttyAllocator,
        event: *mut GhosttyKeyEvent,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_key_event_free(event: GhosttyKeyEvent);
}
unsafe extern "C" {
    pub fn ghostty_key_event_set_action(event: GhosttyKeyEvent, action: GhosttyKeyAction);
}
unsafe extern "C" {
    pub fn ghostty_key_event_set_key(event: GhosttyKeyEvent, key: GhosttyKey);
}
unsafe extern "C" {
    pub fn ghostty_key_event_set_mods(event: GhosttyKeyEvent, mods: GhosttyMods);
}
unsafe extern "C" {
    pub fn ghostty_key_event_set_utf8(
        event: GhosttyKeyEvent,
        utf8: *const ::std::os::raw::c_char,
        len: usize,
    );
}
unsafe extern "C" {
    pub fn ghostty_key_event_set_unshifted_codepoint(event: GhosttyKeyEvent, codepoint: u32);
}
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyKeyEncoderImpl {
    _unused: [u8; 0],
}
pub type GhosttyKeyEncoder = *mut GhosttyKeyEncoderImpl;
unsafe extern "C" {
    pub fn ghostty_key_encoder_new(
        allocator: *const GhosttyAllocator,
        encoder: *mut GhosttyKeyEncoder,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_key_encoder_free(encoder: GhosttyKeyEncoder);
}
unsafe extern "C" {
    pub fn ghostty_key_encoder_setopt_from_terminal(
        encoder: GhosttyKeyEncoder,
        terminal: GhosttyTerminal,
    );
}
unsafe extern "C" {
    pub fn ghostty_key_encoder_encode(
        encoder: GhosttyKeyEncoder,
        event: GhosttyKeyEvent,
        out_buf: *mut ::std::os::raw::c_char,
        out_buf_size: usize,
        out_len: *mut usize,
    ) -> GhosttyResult;
}
