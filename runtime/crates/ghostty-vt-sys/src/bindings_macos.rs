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
pub struct GhosttySnapshotDecoderImpl {
    _unused: [u8; 0],
}
pub type GhosttySnapshotDecoder = *mut GhosttySnapshotDecoderImpl;
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
pub type GhosttyWriterFn = ::std::option::Option<
    unsafe extern "C" fn(
        userdata: *mut ::std::os::raw::c_void,
        data: *const u8,
        len: usize,
    ) -> bool,
>;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyWriter {
    pub write: GhosttyWriterFn,
    pub userdata: *mut ::std::os::raw::c_void,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyWriter"][::std::mem::size_of::<GhosttyWriter>() - 16usize];
    ["Alignment of GhosttyWriter"][::std::mem::align_of::<GhosttyWriter>() - 8usize];
    ["Offset of field: GhosttyWriter::write"]
        [::std::mem::offset_of!(GhosttyWriter, write) - 0usize];
    ["Offset of field: GhosttyWriter::userdata"]
        [::std::mem::offset_of!(GhosttyWriter, userdata) - 8usize];
};
impl Default for GhosttyWriter {
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
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttyPointCoordinate {
    pub x: u16,
    pub y: u32,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyPointCoordinate"][::std::mem::size_of::<GhosttyPointCoordinate>() - 8usize];
    ["Alignment of GhosttyPointCoordinate"]
        [::std::mem::align_of::<GhosttyPointCoordinate>() - 4usize];
    ["Offset of field: GhosttyPointCoordinate::x"]
        [::std::mem::offset_of!(GhosttyPointCoordinate, x) - 0usize];
    ["Offset of field: GhosttyPointCoordinate::y"]
        [::std::mem::offset_of!(GhosttyPointCoordinate, y) - 4usize];
};
pub const GhosttyPointTag_GHOSTTY_POINT_TAG_ACTIVE: GhosttyPointTag = 0;
pub const GhosttyPointTag_GHOSTTY_POINT_TAG_VIEWPORT: GhosttyPointTag = 1;
pub const GhosttyPointTag_GHOSTTY_POINT_TAG_SCREEN: GhosttyPointTag = 2;
pub const GhosttyPointTag_GHOSTTY_POINT_TAG_HISTORY: GhosttyPointTag = 3;
pub const GhosttyPointTag_GHOSTTY_POINT_TAG_MAX_VALUE: GhosttyPointTag = 2147483647;
pub type GhosttyPointTag = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Copy, Clone)]
pub union GhosttyPointValue {
    pub coordinate: GhosttyPointCoordinate,
    pub _padding: [u64; 2usize],
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyPointValue"][::std::mem::size_of::<GhosttyPointValue>() - 16usize];
    ["Alignment of GhosttyPointValue"][::std::mem::align_of::<GhosttyPointValue>() - 8usize];
    ["Offset of field: GhosttyPointValue::coordinate"]
        [::std::mem::offset_of!(GhosttyPointValue, coordinate) - 0usize];
    ["Offset of field: GhosttyPointValue::_padding"]
        [::std::mem::offset_of!(GhosttyPointValue, _padding) - 0usize];
};
impl Default for GhosttyPointValue {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
#[repr(C)]
#[derive(Copy, Clone)]
pub struct GhosttyPoint {
    pub tag: GhosttyPointTag,
    pub value: GhosttyPointValue,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyPoint"][::std::mem::size_of::<GhosttyPoint>() - 24usize];
    ["Alignment of GhosttyPoint"][::std::mem::align_of::<GhosttyPoint>() - 8usize];
    ["Offset of field: GhosttyPoint::tag"][::std::mem::offset_of!(GhosttyPoint, tag) - 0usize];
    ["Offset of field: GhosttyPoint::value"][::std::mem::offset_of!(GhosttyPoint, value) - 8usize];
};
impl Default for GhosttyPoint {
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
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_IDLE: GhosttyProgramStatusState =
    0;
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_WORKING:
    GhosttyProgramStatusState = 1;
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_DONE: GhosttyProgramStatusState =
    2;
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_BLOCKED:
    GhosttyProgramStatusState = 3;
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_ERROR: GhosttyProgramStatusState =
    4;
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_CLEAR: GhosttyProgramStatusState =
    5;
pub const GhosttyProgramStatusState_GHOSTTY_PROGRAM_STATUS_STATE_MAX_VALUE:
    GhosttyProgramStatusState = 2147483647;
pub type GhosttyProgramStatusState = ::std::os::raw::c_int;
pub const GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_NONE: GhosttyProgramStatusKind = 0;
pub const GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_PERMISSION:
    GhosttyProgramStatusKind = 1;
pub const GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_QUESTION: GhosttyProgramStatusKind =
    2;
pub const GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_AUTH: GhosttyProgramStatusKind = 3;
pub const GhosttyProgramStatusKind_GHOSTTY_PROGRAM_STATUS_KIND_MAX_VALUE: GhosttyProgramStatusKind =
    2147483647;
pub type GhosttyProgramStatusKind = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyTerminalProgramStatus {
    pub size: usize,
    pub state: GhosttyProgramStatusState,
    pub kind: GhosttyProgramStatusKind,
    pub progress: i8,
    pub id: GhosttyString,
    pub app: GhosttyString,
    pub title: GhosttyString,
    pub message: GhosttyString,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyTerminalProgramStatus"]
        [::std::mem::size_of::<GhosttyTerminalProgramStatus>() - 88usize];
    ["Alignment of GhosttyTerminalProgramStatus"]
        [::std::mem::align_of::<GhosttyTerminalProgramStatus>() - 8usize];
    ["Offset of field: GhosttyTerminalProgramStatus::size"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, size) - 0usize];
    ["Offset of field: GhosttyTerminalProgramStatus::state"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, state) - 8usize];
    ["Offset of field: GhosttyTerminalProgramStatus::kind"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, kind) - 12usize];
    ["Offset of field: GhosttyTerminalProgramStatus::progress"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, progress) - 16usize];
    ["Offset of field: GhosttyTerminalProgramStatus::id"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, id) - 24usize];
    ["Offset of field: GhosttyTerminalProgramStatus::app"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, app) - 40usize];
    ["Offset of field: GhosttyTerminalProgramStatus::title"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, title) - 56usize];
    ["Offset of field: GhosttyTerminalProgramStatus::message"]
        [::std::mem::offset_of!(GhosttyTerminalProgramStatus, message) - 72usize];
};
impl Default for GhosttyTerminalProgramStatus {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}
pub const GhosttyHierarchicalContextAction_GHOSTTY_HIERARCHICAL_CONTEXT_ACTION_START:
    GhosttyHierarchicalContextAction = 0;
pub const GhosttyHierarchicalContextAction_GHOSTTY_HIERARCHICAL_CONTEXT_ACTION_END:
    GhosttyHierarchicalContextAction = 1;
pub const GhosttyHierarchicalContextAction_GHOSTTY_HIERARCHICAL_CONTEXT_ACTION_MAX_VALUE:
    GhosttyHierarchicalContextAction = 2147483647;
pub type GhosttyHierarchicalContextAction = ::std::os::raw::c_int;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct GhosttyTerminalHierarchicalContext {
    pub size: usize,
    pub action: GhosttyHierarchicalContextAction,
    pub id: GhosttyString,
    pub fields: GhosttyString,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyTerminalHierarchicalContext"]
        [::std::mem::size_of::<GhosttyTerminalHierarchicalContext>() - 48usize];
    ["Alignment of GhosttyTerminalHierarchicalContext"]
        [::std::mem::align_of::<GhosttyTerminalHierarchicalContext>() - 8usize];
    ["Offset of field: GhosttyTerminalHierarchicalContext::size"]
        [::std::mem::offset_of!(GhosttyTerminalHierarchicalContext, size) - 0usize];
    ["Offset of field: GhosttyTerminalHierarchicalContext::action"]
        [::std::mem::offset_of!(GhosttyTerminalHierarchicalContext, action) - 8usize];
    ["Offset of field: GhosttyTerminalHierarchicalContext::id"]
        [::std::mem::offset_of!(GhosttyTerminalHierarchicalContext, id) - 16usize];
    ["Offset of field: GhosttyTerminalHierarchicalContext::fields"]
        [::std::mem::offset_of!(GhosttyTerminalHierarchicalContext, fields) - 32usize];
};
impl Default for GhosttyTerminalHierarchicalContext {
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
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_RESIZE_PULL_SCROLLBACK: GhosttyTerminalOption =
    40;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_RENDER_HOLD: GhosttyTerminalOption = 41;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_SEMANTIC_PROMPT: GhosttyTerminalOption = 42;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_RESET: GhosttyTerminalOption = 43;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_XT_CHECKSUM_REPORT: GhosttyTerminalOption = 44;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_XT_CHECKSUM_EXTENSION: GhosttyTerminalOption =
    45;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_PROGRAM_STATUS: GhosttyTerminalOption = 46;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_RESIZE_WINDOW: GhosttyTerminalOption = 47;
pub const GhosttyTerminalOption_GHOSTTY_TERMINAL_OPT_HIERARCHICAL_CONTEXT: GhosttyTerminalOption =
    48;
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
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MOUSE_SHAPE: GhosttyTerminalData = 41;
pub const GhosttyTerminalData_GHOSTTY_TERMINAL_DATA_MEMORY_USAGE: GhosttyTerminalData = 42;
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
unsafe extern "C" {
    pub fn ghostty_terminal_grid_ref(
        terminal: GhosttyTerminal,
        point: GhosttyPoint,
        out_ref: *mut GhosttyGridRef,
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
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttyFormatterTerminalOptions"]
        [::std::mem::size_of::<GhosttyFormatterTerminalOptions>() - 56usize];
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
pub const GhosttySnapshotDecoderOption_GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_CONTINUATION_BYTES:
    GhosttySnapshotDecoderOption = 0;
pub const GhosttySnapshotDecoderOption_GHOSTTY_SNAPSHOT_DECODER_OPT_RETAIN_CONTINUATION:
    GhosttySnapshotDecoderOption = 1;
pub const GhosttySnapshotDecoderOption_GHOSTTY_SNAPSHOT_DECODER_OPT_COMPRESS_HISTORY:
    GhosttySnapshotDecoderOption = 2;
pub const GhosttySnapshotDecoderOption_GHOSTTY_SNAPSHOT_DECODER_OPT_MAX_VALUE:
    GhosttySnapshotDecoderOption = 2147483647;
pub type GhosttySnapshotDecoderOption = ::std::os::raw::c_int;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_INVALID:
    GhosttySnapshotDecoderData = 0;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_MAX_CONTINUATION_BYTES:
    GhosttySnapshotDecoderData = 1;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_SOURCE_OFFSET:
    GhosttySnapshotDecoderData = 2;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_HISTORY_ROWS_PRIMARY:
    GhosttySnapshotDecoderData = 3;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_HISTORY_ROWS_ALTERNATE:
    GhosttySnapshotDecoderData = 4;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_SCREEN:
    GhosttySnapshotDecoderData = 5;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_ROWS:
    GhosttySnapshotDecoderData = 6;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_PROGRESS_REMAINING:
    GhosttySnapshotDecoderData = 7;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_RETAIN_CONTINUATION:
    GhosttySnapshotDecoderData = 8;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_COMPRESS_HISTORY:
    GhosttySnapshotDecoderData = 9;
pub const GhosttySnapshotDecoderData_GHOSTTY_SNAPSHOT_DECODER_DATA_MAX_VALUE:
    GhosttySnapshotDecoderData = 2147483647;
pub type GhosttySnapshotDecoderData = ::std::os::raw::c_int;
unsafe extern "C" {
    pub fn ghostty_snapshot_encode(
        terminal: GhosttyTerminal,
        writer: GhosttyWriter,
    ) -> GhosttyResult;
}
#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct GhosttySnapshotEncodeOptions {
    pub size: usize,
    pub kitty_image_data_limit: u64,
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of GhosttySnapshotEncodeOptions"]
        [::std::mem::size_of::<GhosttySnapshotEncodeOptions>() - 16usize];
    ["Alignment of GhosttySnapshotEncodeOptions"]
        [::std::mem::align_of::<GhosttySnapshotEncodeOptions>() - 8usize];
    ["Offset of field: GhosttySnapshotEncodeOptions::size"]
        [::std::mem::offset_of!(GhosttySnapshotEncodeOptions, size) - 0usize];
    ["Offset of field: GhosttySnapshotEncodeOptions::kitty_image_data_limit"]
        [::std::mem::offset_of!(GhosttySnapshotEncodeOptions, kitty_image_data_limit) - 8usize];
};
unsafe extern "C" {
    pub fn ghostty_snapshot_encode_options(
        terminal: GhosttyTerminal,
        writer: GhosttyWriter,
        options: *const GhosttySnapshotEncodeOptions,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_snapshot_decoder_new_buf(
        allocator: *const GhosttyAllocator,
        decoder: *mut GhosttySnapshotDecoder,
        ptr: *const u8,
        len: usize,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_snapshot_decoder_free(decoder: GhosttySnapshotDecoder);
}
unsafe extern "C" {
    pub fn ghostty_snapshot_decoder_set(
        decoder: GhosttySnapshotDecoder,
        option: GhosttySnapshotDecoderOption,
        value: *const ::std::os::raw::c_void,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_snapshot_decoder_ready(
        decoder: GhosttySnapshotDecoder,
        terminal: *mut GhosttyTerminal,
    ) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_snapshot_decoder_next(decoder: GhosttySnapshotDecoder) -> GhosttyResult;
}
unsafe extern "C" {
    pub fn ghostty_snapshot_decoder_get(
        decoder: GhosttySnapshotDecoder,
        data: GhosttySnapshotDecoderData,
        out: *mut ::std::os::raw::c_void,
    ) -> GhosttyResult;
}
