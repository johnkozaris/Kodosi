//! The parts of libva that the encoder uses. libva is opened when an encoder first looks for a
//! GPU, so computers without it still encode, in software.

use std::{
    ffi::{c_char, c_int, c_uint, c_void},
    fs::File,
    os::fd::AsRawFd,
    path::Path,
    ptr,
    sync::OnceLock,
};

pub(super) type Id = c_uint;
pub(super) type Status = c_int;
type Handle = *mut c_void;
type Message = unsafe extern "C" fn(*mut c_void, *const c_char);

const SUCCESS: Status = 0;
/// A size that C's `int` cannot hold, which libva never reports itself.
pub(super) const OUT_OF_RANGE: Status = -1;
pub(super) const NO_ID: Id = Id::MAX;

pub(super) const H264_HIGH: c_int = 7;
pub(super) const HEVC_MAIN: c_int = 17;
pub(super) const NO_PROFILE: c_int = -1;
pub(super) const ENCODE: c_int = 6;
pub(super) const ENCODE_LOW_POWER: c_int = 8;
pub(super) const VIDEO_PROCESSING: c_int = 10;

pub(super) const ATTRIBUTE_FORMAT: c_int = 0;
pub(super) const ATTRIBUTE_RATE_CONTROL: c_int = 5;
pub(super) const ATTRIBUTE_PACKED_HEADERS: c_int = 10;
pub(super) const ATTRIBUTE_PREDICTION: c_int = 39;
pub(super) const ATTRIBUTE_HEVC_FEATURES: c_int = 50;
pub(super) const ATTRIBUTE_HEVC_BLOCKS: c_int = 51;
pub(super) const NOT_SUPPORTED: u32 = 0x8000_0000;
/// `VA_PREDICTION_DIRECTION_BI_NOT_EMPTY`: B slices that stand for P frames need both lists.
pub(super) const BOTH_LISTS: u32 = 0x4;
pub(super) const FORMAT_YUV420: u32 = 0x1;
pub(super) const FORMAT_RGB32: u32 = 0x2_0000;
pub(super) const CONSTANT_QUANTIZER: u32 = 0x10;
pub(super) const PACKED_SEQUENCE: u32 = 0x1;
pub(super) const PACKED_SLICE: u32 = 0x4;

pub(super) const FOURCC_NV12: u32 = u32::from_le_bytes(*b"NV12");
/// Bytes blue, green, red and one unused, as BGRA pictures come.
pub(super) const FOURCC_BGRX: u32 = u32::from_le_bytes(*b"BGRX");

const CODED: c_int = 21;
pub(super) const SEQUENCE: c_int = 22;
pub(super) const PICTURE: c_int = 23;
pub(super) const SLICE: c_int = 24;
pub(super) const PACKED_HEADER: c_int = 25;
pub(super) const PACKED_DATA: c_int = 26;
pub(super) const PIPELINE: c_int = 41;

pub(super) const SHORT_TERM_REFERENCE: u32 = 0x8;
/// `VA_PICTURE_HEVC_RPS_ST_CURR_BEFORE`: a reference that shows before the current picture.
pub(super) const CURRENT_BEFORE: u32 = 0x10;
const PICTURE_INVALID: u32 = 0x1;
const PROGRESSIVE: c_int = 1;
const ATTRIBUTE_PIXEL_FORMAT: c_int = 1;
/// `VASurfaceAttribAlignmentSize`: the log2 of the width a coded size must be a multiple of in
/// the low four bits, and of the height in the next four.
const ATTRIBUTE_ALIGNMENT: c_int = 10;
const SETTABLE: u32 = 0x2;
const INTEGER: c_int = 1;

pub(super) const STANDARD_BT709: c_int = 2;
pub(super) const RANGE_LIMITED: u8 = 1;
pub(super) const RANGE_FULL: u8 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ConfigAttribute {
    pub(super) kind: c_int,
    pub(super) value: u32,
}

/// libva's `VAGenericValue`, of which the encoder sets only integers; the pointer gives the
/// union its C size and alignment.
#[repr(C)]
#[derive(Clone, Copy)]
union Value {
    integer: i32,
    pointer: *mut c_void,
}

#[repr(C)]
struct SurfaceAttribute {
    kind: c_int,
    flags: u32,
    value_kind: c_int,
    value: Value,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ImageFormat {
    fourcc: u32,
    byte_order: u32,
    bits_per_pixel: u32,
    depth: u32,
    red_mask: u32,
    green_mask: u32,
    blue_mask: u32,
    alpha_mask: u32,
    reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Image {
    id: Id,
    format: ImageFormat,
    buffer: Id,
    width: u16,
    height: u16,
    data_size: u32,
    planes: u32,
    pitches: [u32; 3],
    offsets: [u32; 3],
    palette_entries: i32,
    entry_bytes: i32,
    component_order: [i8; 4],
    reserved: [u32; 4],
}

#[repr(C)]
struct CodedSegment {
    size: u32,
    bit_offset: u32,
    status: u32,
    reserved: u32,
    data: *const u8,
    next: *const Self,
    padding: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct PackedHeader {
    pub(super) kind: u32,
    pub(super) bit_length: u32,
    pub(super) has_emulation_bytes: u8,
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct Rectangle {
    pub(super) x: i16,
    pub(super) y: i16,
    pub(super) width: u16,
    pub(super) height: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(super) struct ColourProperties {
    pub(super) chroma_sample_location: u8,
    pub(super) range: u8,
    pub(super) primaries: u8,
    pub(super) transfer: u8,
    pub(super) matrix: u8,
    pub(super) reserved: [u8; 3],
}

#[repr(C)]
pub(super) struct Pipeline {
    pub(super) surface: Id,
    pub(super) surface_region: *const Rectangle,
    pub(super) surface_standard: c_int,
    pub(super) output_region: *const Rectangle,
    pub(super) background: u32,
    pub(super) output_standard: c_int,
    pub(super) flags: u32,
    pub(super) filter_flags: u32,
    pub(super) filters: *mut Id,
    pub(super) filter_count: u32,
    pub(super) forward_references: *mut Id,
    pub(super) forward_reference_count: u32,
    pub(super) backward_references: *mut Id,
    pub(super) backward_reference_count: u32,
    pub(super) rotation: u32,
    pub(super) blend: *const c_void,
    pub(super) mirror: u32,
    pub(super) additional_outputs: *mut Id,
    pub(super) additional_output_count: u32,
    pub(super) input_surface_flag: u32,
    pub(super) output_surface_flag: u32,
    pub(super) input_colours: ColourProperties,
    pub(super) output_colours: ColourProperties,
    pub(super) processing_mode: c_int,
    pub(super) hdr_metadata: *mut c_void,
    pub(super) reserved: [u32; 16],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct PictureH264 {
    pub(super) surface: Id,
    pub(super) frame_index: u32,
    pub(super) flags: u32,
    pub(super) top_order: i32,
    pub(super) bottom_order: i32,
    pub(super) reserved: [u32; 4],
}

impl PictureH264 {
    pub(super) const NONE: Self = Self {
        surface: NO_ID,
        frame_index: 0,
        flags: PICTURE_INVALID,
        top_order: 0,
        bottom_order: 0,
        reserved: [0; 4],
    };
}

#[repr(C)]
pub(super) struct SequenceH264 {
    pub(super) sequence_set: u8,
    pub(super) level: u8,
    pub(super) intra_period: u32,
    pub(super) idr_period: u32,
    pub(super) ip_period: u32,
    pub(super) bits_per_second: u32,
    pub(super) reference_frames: u32,
    pub(super) width_in_blocks: u16,
    pub(super) height_in_blocks: u16,
    /// Bit fields from `chroma_format_idc` in the lowest bits up.
    pub(super) fields: u32,
    pub(super) luma_depth_minus8: u8,
    pub(super) chroma_depth_minus8: u8,
    pub(super) frames_in_order_cycle: u8,
    pub(super) offset_for_non_reference: i32,
    pub(super) offset_for_top_to_bottom: i32,
    pub(super) offsets_for_reference: [i32; 256],
    pub(super) cropping: u8,
    pub(super) crop_left: u32,
    pub(super) crop_right: u32,
    pub(super) crop_top: u32,
    pub(super) crop_bottom: u32,
    pub(super) vui_present: u8,
    /// Bit fields from `aspect_ratio_info_present_flag` in the lowest bit up.
    pub(super) vui_fields: u32,
    pub(super) aspect_ratio: u8,
    pub(super) sample_width: u32,
    pub(super) sample_height: u32,
    pub(super) units_in_tick: u32,
    pub(super) time_scale: u32,
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
pub(super) struct PictureParametersH264 {
    pub(super) current: PictureH264,
    pub(super) references: [PictureH264; 16],
    pub(super) coded: Id,
    pub(super) picture_set: u8,
    pub(super) sequence_set: u8,
    pub(super) last_picture: u8,
    pub(super) frame_number: u16,
    pub(super) initial_quantizer: u8,
    pub(super) list0_references_minus1: u8,
    pub(super) list1_references_minus1: u8,
    pub(super) chroma_offset: i8,
    pub(super) second_chroma_offset: i8,
    /// Bit fields from `idr_pic_flag` in the lowest bit up.
    pub(super) fields: u32,
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
pub(super) struct SliceH264 {
    pub(super) first_block: u32,
    pub(super) blocks: u32,
    pub(super) block_info: Id,
    pub(super) slice_type: u8,
    pub(super) picture_set: u8,
    pub(super) idr_id: u16,
    pub(super) order_lsb: u16,
    pub(super) delta_order_bottom: i32,
    pub(super) delta_order: [i32; 2],
    pub(super) direct_spatial: u8,
    pub(super) override_references: u8,
    pub(super) list0_references_minus1: u8,
    pub(super) list1_references_minus1: u8,
    pub(super) list0: [PictureH264; 32],
    pub(super) list1: [PictureH264; 32],
    /// Weighted prediction's tables, which the encoder leaves off.
    pub(super) weights: [u8; 778],
    pub(super) cabac_init: u8,
    pub(super) quantizer_delta: i8,
    pub(super) deblocking_off: u8,
    pub(super) alpha_offset: i8,
    pub(super) beta_offset: i8,
    pub(super) reserved: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct PictureHevc {
    pub(super) surface: Id,
    pub(super) order: i32,
    pub(super) flags: u32,
    pub(super) reserved: [u32; 4],
}

impl PictureHevc {
    pub(super) const NONE: Self = Self {
        surface: NO_ID,
        order: 0,
        flags: PICTURE_INVALID,
        reserved: [0; 4],
    };
}

#[repr(C)]
pub(super) struct SequenceHevc {
    pub(super) profile: u8,
    pub(super) level: u8,
    pub(super) tier: u8,
    pub(super) intra_period: u32,
    pub(super) idr_period: u32,
    pub(super) ip_period: u32,
    pub(super) bits_per_second: u32,
    pub(super) width: u16,
    pub(super) height: u16,
    /// Bit fields from `chroma_format_idc` in the lowest bits up.
    pub(super) fields: u32,
    pub(super) log2_min_block_minus3: u8,
    pub(super) log2_block_range: u8,
    pub(super) log2_min_transform_minus2: u8,
    pub(super) log2_transform_range: u8,
    pub(super) depth_inter: u8,
    pub(super) depth_intra: u8,
    pub(super) pcm_luma_depth_minus1: u32,
    pub(super) pcm_chroma_depth_minus1: u32,
    pub(super) log2_min_pcm_minus3: u32,
    pub(super) log2_max_pcm_minus3: u32,
    pub(super) vui_present: u8,
    pub(super) vui_fields: u32,
    pub(super) aspect_ratio: u8,
    pub(super) sample_width: u32,
    pub(super) sample_height: u32,
    pub(super) units_in_tick: u32,
    pub(super) time_scale: u32,
    pub(super) min_spatial_segmentation: u16,
    pub(super) max_bytes_per_picture_denominator: u8,
    pub(super) max_bits_per_block_denominator: u8,
    pub(super) screen_content_fields: u32,
    pub(super) reserved: [u32; 7],
}

#[repr(C)]
pub(super) struct PictureParametersHevc {
    pub(super) current: PictureHevc,
    pub(super) references: [PictureHevc; 15],
    pub(super) coded: Id,
    pub(super) collocated_reference: u8,
    pub(super) last_picture: u8,
    pub(super) initial_quantizer: u8,
    pub(super) quantizer_delta_depth: u8,
    pub(super) cb_offset: i8,
    pub(super) cr_offset: i8,
    pub(super) tile_columns_minus1: u8,
    pub(super) tile_rows_minus1: u8,
    pub(super) column_widths_minus1: [u8; 19],
    pub(super) row_heights_minus1: [u8; 21],
    pub(super) log2_parallel_merge_minus2: u8,
    pub(super) most_tree_bits: u8,
    pub(super) list0_default_minus1: u8,
    pub(super) list1_default_minus1: u8,
    pub(super) picture_set: u8,
    pub(super) nal_unit_type: u8,
    /// Bit fields from `idr_pic_flag` in the lowest bit up.
    pub(super) fields: u32,
    pub(super) hierarchical_level_plus1: u8,
    pub(super) byte_reserved: u8,
    pub(super) screen_content_fields: u16,
    pub(super) reserved: [u32; 15],
}

#[repr(C)]
pub(super) struct SliceHevc {
    pub(super) address: u32,
    pub(super) trees: u32,
    pub(super) slice_type: u8,
    pub(super) picture_set: u8,
    pub(super) list0_references_minus1: u8,
    pub(super) list1_references_minus1: u8,
    pub(super) list0: [PictureHevc; 15],
    pub(super) list1: [PictureHevc; 15],
    /// Weighted prediction's tables, which the encoder leaves off.
    pub(super) weights: [u8; 182],
    pub(super) merge_candidates: u8,
    pub(super) quantizer_delta: i8,
    pub(super) cb_offset: i8,
    pub(super) cr_offset: i8,
    pub(super) beta_offset: i8,
    pub(super) tc_offset: i8,
    /// Bit fields from `last_slice_of_pic_flag` in the lowest bit up.
    pub(super) fields: u32,
    pub(super) weight_table_offset: u32,
    pub(super) weight_table_length: u32,
    pub(super) reserved: [u32; 6],
}

// libva's layouts on 64-bit Linux.
const _: () = {
    assert!(size_of::<ConfigAttribute>() == 8);
    assert!(size_of::<SurfaceAttribute>() == 24);
    assert!(size_of::<Image>() == 120);
    assert!(size_of::<CodedSegment>() == 48);
    assert!(size_of::<PackedHeader>() == 28);
    assert!(size_of::<Pipeline>() == 224);
    assert!(size_of::<PictureH264>() == 36);
    assert!(size_of::<SequenceH264>() == 1132);
    assert!(size_of::<PictureParametersH264>() == 648);
    assert!(size_of::<SliceH264>() == 3140);
    assert!(size_of::<PictureHevc>() == 28);
    assert!(size_of::<SequenceHevc>() == 116);
    assert!(size_of::<PictureParametersHevc>() == 576);
    assert!(size_of::<SliceHevc>() == 1076);
};

macro_rules! zeroed_by_default {
    ($($structure:ty),*) => {
        $(impl Default for $structure {
            fn default() -> Self {
                // SAFETY: the structure is plain C data: numbers, and pointers that may be null.
                unsafe { std::mem::zeroed() }
            }
        })*
    };
}

zeroed_by_default!(
    Pipeline,
    SequenceH264,
    PictureParametersH264,
    SliceH264,
    SequenceHevc,
    PictureParametersHevc,
    SliceHevc
);

macro_rules! library {
    ($($name:ident = $symbol:literal: fn($($argument:ty),*) -> $result:ty;)*) => {
        struct Library {
            $($name: unsafe extern "C" fn($($argument),*) -> $result,)*
        }

        impl Library {
            /// Opens libva's DRM library, whose lookups also find libva's own functions. It stays
            /// open for the life of the process.
            fn open() -> Option<Self> {
                // SAFETY: the name is a C string.
                let library = unsafe {
                    libc::dlopen(c"libva-drm.so.2".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL)
                };
                if library.is_null() {
                    return None;
                }
                Some(Self {
                    $($name: {
                        // SAFETY: the library is open and the name is a C string.
                        let function = unsafe { libc::dlsym(library, $symbol.as_ptr()) };
                        if function.is_null() {
                            return None;
                        }
                        // SAFETY: libva declares the function with this signature.
                        unsafe {
                            std::mem::transmute::<
                                *mut c_void,
                                unsafe extern "C" fn($($argument),*) -> $result,
                            >(function)
                        }
                    },)*
                })
            }
        }
    };
}

library! {
    display_drm = c"vaGetDisplayDRM": fn(c_int) -> Handle;
    initialize = c"vaInitialize": fn(Handle, *mut c_int, *mut c_int) -> Status;
    terminate = c"vaTerminate": fn(Handle) -> Status;
    error_messages = c"vaSetErrorCallback": fn(Handle, Option<Message>, *mut c_void) -> Option<Message>;
    info_messages = c"vaSetInfoCallback": fn(Handle, Option<Message>, *mut c_void) -> Option<Message>;
    most_entrypoints = c"vaMaxNumEntrypoints": fn(Handle) -> c_int;
    entrypoints = c"vaQueryConfigEntrypoints": fn(Handle, c_int, *mut c_int, *mut c_int) -> Status;
    attributes = c"vaGetConfigAttributes": fn(Handle, c_int, c_int, *mut ConfigAttribute, c_int) -> Status;
    create_config = c"vaCreateConfig": fn(Handle, c_int, c_int, *mut ConfigAttribute, c_int, *mut Id) -> Status;
    destroy_config = c"vaDestroyConfig": fn(Handle, Id) -> Status;
    create_surfaces = c"vaCreateSurfaces": fn(Handle, c_uint, c_uint, c_uint, *mut Id, c_uint, *mut SurfaceAttribute, c_uint) -> Status;
    destroy_surfaces = c"vaDestroySurfaces": fn(Handle, *mut Id, c_int) -> Status;
    create_context = c"vaCreateContext": fn(Handle, Id, c_int, c_int, c_int, *mut Id, c_int, *mut Id) -> Status;
    destroy_context = c"vaDestroyContext": fn(Handle, Id) -> Status;
    create_buffer = c"vaCreateBuffer": fn(Handle, Id, c_int, c_uint, c_uint, *mut c_void, *mut Id) -> Status;
    destroy_buffer = c"vaDestroyBuffer": fn(Handle, Id) -> Status;
    map_buffer = c"vaMapBuffer": fn(Handle, Id, *mut *mut c_void) -> Status;
    unmap_buffer = c"vaUnmapBuffer": fn(Handle, Id) -> Status;
    derive_image = c"vaDeriveImage": fn(Handle, Id, *mut Image) -> Status;
    destroy_image = c"vaDestroyImage": fn(Handle, Id) -> Status;
    begin_picture = c"vaBeginPicture": fn(Handle, Id, Id) -> Status;
    render_picture = c"vaRenderPicture": fn(Handle, Id, *mut Id, c_int) -> Status;
    end_picture = c"vaEndPicture": fn(Handle, Id) -> Status;
    sync_surface = c"vaSyncSurface": fn(Handle, Id) -> Status;
    surface_attributes = c"vaQuerySurfaceAttributes": fn(Handle, Id, *mut SurfaceAttribute, *mut c_uint) -> Status;
}

fn library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY.get_or_init(Library::open).as_ref()
}

fn checked(status: Status) -> Result<(), Status> {
    if status == SUCCESS {
        Ok(())
    } else {
        Err(status)
    }
}

fn int(value: usize) -> Result<c_int, Status> {
    c_int::try_from(value).map_err(|_| OUT_OF_RANGE)
}

fn unsigned(value: usize) -> Result<c_uint, Status> {
    c_uint::try_from(value).map_err(|_| OUT_OF_RANGE)
}

/// An initialized VA display on one GPU's render node.
pub(super) struct Display {
    library: &'static Library,
    handle: Handle,
    _node: File,
}

impl Display {
    pub(super) fn open(node: &Path) -> Option<Self> {
        let library = library()?;
        let node = File::options().read(true).write(true).open(node).ok()?;
        // SAFETY: the descriptor stays open as long as the display.
        let handle = unsafe { (library.display_drm)(node.as_raw_fd()) };
        if handle.is_null() {
            return None;
        }
        let display = Self {
            library,
            handle,
            _node: node,
        };
        let (mut major, mut minor) = (0, 0);
        // SAFETY: the display is valid; without callbacks libva prints nothing.
        let status = unsafe {
            (library.error_messages)(handle, None, ptr::null_mut());
            (library.info_messages)(handle, None, ptr::null_mut());
            (library.initialize)(handle, &raw mut major, &raw mut minor)
        };
        (status == SUCCESS).then_some(display)
    }

    pub(super) fn entrypoints(&self, profile: c_int) -> Vec<c_int> {
        // SAFETY: the display is initialized.
        let most = unsafe { (self.library.most_entrypoints)(self.handle) };
        let mut entrypoints = vec![0; usize::try_from(most).unwrap_or_default()];
        let mut count = 0;
        // SAFETY: the list holds as many entrypoints as libva reports at most.
        let status = unsafe {
            (self.library.entrypoints)(
                self.handle,
                profile,
                entrypoints.as_mut_ptr(),
                &raw mut count,
            )
        };
        let found = if status == SUCCESS {
            usize::try_from(count).unwrap_or_default()
        } else {
            0
        };
        entrypoints.truncate(found);
        entrypoints
    }

    /// The values of `kinds` for an entrypoint; an unsupported attribute has its top bit set.
    pub(super) fn attributes<const N: usize>(
        &self,
        profile: c_int,
        entrypoint: c_int,
        kinds: [c_int; N],
    ) -> Option<[u32; N]> {
        let mut attributes = kinds.map(|kind| ConfigAttribute { kind, value: 0 });
        // SAFETY: the list holds `N` attributes.
        let status = unsafe {
            (self.library.attributes)(
                self.handle,
                profile,
                entrypoint,
                attributes.as_mut_ptr(),
                int(N).ok()?,
            )
        };
        (status == SUCCESS).then(|| attributes.map(|attribute| attribute.value))
    }

    pub(super) fn create_config(
        &self,
        profile: c_int,
        entrypoint: c_int,
        attributes: &[ConfigAttribute],
    ) -> Result<Id, Status> {
        let mut attributes = attributes.to_vec();
        let mut config = NO_ID;
        // SAFETY: the list and the out pointer are valid for the call.
        checked(unsafe {
            (self.library.create_config)(
                self.handle,
                profile,
                entrypoint,
                attributes.as_mut_ptr(),
                int(attributes.len())?,
                &raw mut config,
            )
        })?;
        Ok(config)
    }

    /// The width and height that an encoder's coded size must be multiples of, as far as the
    /// driver says.
    pub(super) fn alignment(&self, profile: c_int, entrypoint: c_int) -> (usize, usize) {
        let Ok(config) = self.create_config(profile, entrypoint, &[]) else {
            return (1, 1);
        };
        let mut count: c_uint = 0;
        // SAFETY: without a list, libva only counts the attributes.
        let counted = unsafe {
            (self.library.surface_attributes)(self.handle, config, ptr::null_mut(), &raw mut count)
        };
        let mut attributes: Vec<SurfaceAttribute> = (0..count)
            .map(|_| SurfaceAttribute {
                kind: 0,
                flags: 0,
                value_kind: 0,
                value: Value { integer: 0 },
            })
            .collect();
        // SAFETY: the list holds `count` attributes.
        let listed = counted == SUCCESS
            && unsafe {
                (self.library.surface_attributes)(
                    self.handle,
                    config,
                    attributes.as_mut_ptr(),
                    &raw mut count,
                )
            } == SUCCESS;
        self.destroy_config(config);
        attributes
            .iter()
            .take(if listed { count as usize } else { 0 })
            .find(|attribute| attribute.kind == ATTRIBUTE_ALIGNMENT)
            .map_or((1, 1), |attribute| {
                // SAFETY: libva sets the alignment as an integer.
                let log2 = unsafe { attribute.value.integer }.cast_unsigned();
                (1 << (log2 & 0xf), 1 << ((log2 >> 4) & 0xf))
            })
    }

    pub(super) fn destroy_config(&self, config: Id) {
        // SAFETY: the config belongs to this display.
        unsafe { (self.library.destroy_config)(self.handle, config) };
    }

    pub(super) fn create_surfaces<const N: usize>(
        &self,
        format: u32,
        fourcc: u32,
        (width, height): (usize, usize),
    ) -> Result<[Id; N], Status> {
        let mut surfaces = [NO_ID; N];
        let mut attribute = SurfaceAttribute {
            kind: ATTRIBUTE_PIXEL_FORMAT,
            flags: SETTABLE,
            value_kind: INTEGER,
            value: Value {
                integer: fourcc.cast_signed(),
            },
        };
        // SAFETY: the list holds `N` surfaces, and the attribute lives for the call.
        checked(unsafe {
            (self.library.create_surfaces)(
                self.handle,
                format,
                unsigned(width)?,
                unsigned(height)?,
                surfaces.as_mut_ptr(),
                unsigned(N)?,
                &raw mut attribute,
                1,
            )
        })?;
        Ok(surfaces)
    }

    pub(super) fn destroy_surfaces(&self, surfaces: &mut [Id]) {
        if let Ok(count) = int(surfaces.len()) {
            // SAFETY: the surfaces belong to this display.
            unsafe { (self.library.destroy_surfaces)(self.handle, surfaces.as_mut_ptr(), count) };
        }
    }

    pub(super) fn create_context(
        &self,
        config: Id,
        (width, height): (usize, usize),
        targets: &[Id],
    ) -> Result<Id, Status> {
        let mut targets = targets.to_vec();
        let mut context = NO_ID;
        // SAFETY: the list and the out pointer are valid for the call.
        checked(unsafe {
            (self.library.create_context)(
                self.handle,
                config,
                int(width)?,
                int(height)?,
                PROGRESSIVE,
                targets.as_mut_ptr(),
                int(targets.len())?,
                &raw mut context,
            )
        })?;
        Ok(context)
    }

    pub(super) fn destroy_context(&self, context: Id) {
        // SAFETY: the context belongs to this display.
        unsafe { (self.library.destroy_context)(self.handle, context) };
    }

    /// A buffer for the encoder to write a frame of at most `size` bytes into.
    pub(super) fn create_coded(&self, context: Id, size: usize) -> Result<Id, Status> {
        let mut buffer = NO_ID;
        // SAFETY: without data, libva allocates `size` bytes.
        checked(unsafe {
            (self.library.create_buffer)(
                self.handle,
                context,
                CODED,
                unsigned(size)?,
                1,
                ptr::null_mut(),
                &raw mut buffer,
            )
        })?;
        Ok(buffer)
    }

    /// A buffer that holds a copy of `bytes`.
    pub(super) fn create_buffer(
        &self,
        context: Id,
        kind: c_int,
        bytes: &[u8],
    ) -> Result<Id, Status> {
        let mut buffer = NO_ID;
        // SAFETY: libva copies the bytes, which live for the call.
        checked(unsafe {
            (self.library.create_buffer)(
                self.handle,
                context,
                kind,
                unsigned(bytes.len())?,
                1,
                bytes.as_ptr().cast_mut().cast(),
                &raw mut buffer,
            )
        })?;
        Ok(buffer)
    }

    /// A buffer that holds a copy of a parameter structure.
    pub(super) fn create_parameters<T>(
        &self,
        context: Id,
        kind: c_int,
        parameters: &T,
    ) -> Result<Id, Status> {
        // SAFETY: the parameters are plain C data that live for the call.
        let bytes = unsafe {
            std::slice::from_raw_parts(ptr::from_ref(parameters).cast::<u8>(), size_of::<T>())
        };
        self.create_buffer(context, kind, bytes)
    }

    pub(super) fn destroy_buffer(&self, buffer: Id) {
        // SAFETY: the buffer belongs to this display.
        unsafe { (self.library.destroy_buffer)(self.handle, buffer) };
    }

    /// Maps a buffer, lets `use_data` read or write it, then unmaps it.
    fn mapped<R>(&self, buffer: Id, use_data: impl FnOnce(*mut c_void) -> R) -> Result<R, Status> {
        let mut data = ptr::null_mut();
        // SAFETY: the buffer belongs to this display.
        checked(unsafe { (self.library.map_buffer)(self.handle, buffer, &raw mut data) })?;
        let result = use_data(data);
        // SAFETY: the buffer was mapped above.
        checked(unsafe { (self.library.unmap_buffer)(self.handle, buffer) })?;
        Ok(result)
    }

    /// Appends what the encoder wrote into a coded buffer, once it is done.
    pub(super) fn read_coded(&self, buffer: Id, into: &mut Vec<u8>) -> Result<(), Status> {
        self.mapped(buffer, |data| {
            let mut segment = data.cast::<CodedSegment>().cast_const();
            // SAFETY: a mapped coded buffer is a list of segments, each pointing at `size`
            // bytes, all valid until the buffer is unmapped.
            while let Some(current) = unsafe { segment.as_ref() } {
                if !current.data.is_null() {
                    // SAFETY: as above.
                    let bytes =
                        unsafe { std::slice::from_raw_parts(current.data, current.size as usize) };
                    into.extend_from_slice(bytes);
                }
                segment = current.next;
            }
        })
    }

    /// An image that maps a surface's own memory, so pictures are written straight into it.
    pub(super) fn derive_image(&self, surface: Id) -> Result<Image, Status> {
        let mut image = Image::default();
        // SAFETY: the surface belongs to this display, and the out pointer is valid.
        checked(unsafe { (self.library.derive_image)(self.handle, surface, &raw mut image) })?;
        Ok(image)
    }

    pub(super) fn destroy_image(&self, image: &Image) {
        // SAFETY: the image belongs to this display.
        unsafe { (self.library.destroy_image)(self.handle, image.id) };
    }

    /// Copies BGRA rows without padding into an image.
    pub(super) fn write(
        &self,
        image: &Image,
        (width, height): (usize, usize),
        bgra: &[u8],
    ) -> Result<(), Status> {
        let row = width * 4;
        let (pitch, offset) = (image.pitches[0] as usize, image.offsets[0] as usize);
        if pitch < row || bgra.len() < row * height {
            return Err(OUT_OF_RANGE);
        }
        self.mapped(image.buffer, |data| {
            let data = data.cast::<u8>();
            for (index, source) in bgra.chunks_exact(row).take(height).enumerate() {
                // SAFETY: the image holds `height` rows of `pitch` bytes after `offset`.
                unsafe {
                    data.add(offset + index * pitch)
                        .copy_from_nonoverlapping(source.as_ptr(), row);
                }
            }
        })
    }

    /// Runs `buffers` on a context into `target`.
    pub(super) fn render(&self, context: Id, target: Id, buffers: &[Id]) -> Result<(), Status> {
        let count = int(buffers.len())?;
        // SAFETY: the context, the target and the buffers belong to this display; libva only
        // reads the list.
        unsafe {
            checked((self.library.begin_picture)(self.handle, context, target))?;
            checked((self.library.render_picture)(
                self.handle,
                context,
                buffers.as_ptr().cast_mut(),
                count,
            ))?;
            checked((self.library.end_picture)(self.handle, context))
        }
    }

    pub(super) fn sync(&self, surface: Id) -> Result<(), Status> {
        // SAFETY: the surface belongs to this display.
        checked(unsafe { (self.library.sync_surface)(self.handle, surface) })
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        // SAFETY: nothing uses the display after this.
        unsafe { (self.library.terminate)(self.handle) };
    }
}
