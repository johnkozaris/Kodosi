//! Core Foundation and `IOSurface`, as the encoder and the layer use them.

use std::{ffi::c_void, ptr};

pub(crate) type Ref = *const c_void;

pub(crate) const NUMBER_I64: isize = 4;
const NUMBER_F64: isize = 13;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn CFRelease(object: Ref);
    fn CFDictionaryCreate(
        allocator: Ref,
        keys: *const Ref,
        values: *const Ref,
        count: isize,
        key_callbacks: Ref,
        value_callbacks: Ref,
    ) -> Ref;
    pub(crate) fn CFDictionaryGetValue(dictionary: Ref, key: Ref) -> Ref;
    fn CFArrayCreate(allocator: Ref, values: *const Ref, count: isize, callbacks: Ref) -> Ref;
    pub(crate) fn CFArrayGetCount(array: Ref) -> isize;
    pub(crate) fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    fn CFNumberCreate(allocator: Ref, kind: isize, value: *const c_void) -> Ref;
    pub(crate) fn CFNumberGetValue(number: Ref, kind: isize, value: *mut c_void) -> u8;
    pub(crate) static kCFBooleanTrue: Ref;
    pub(crate) static kCFBooleanFalse: Ref;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
    static kCFTypeArrayCallBacks: c_void;
}

#[link(name = "IOSurface", kind = "framework")]
unsafe extern "C" {
    pub(crate) fn IOSurfaceCreate(properties: Ref) -> *mut c_void;
    pub(crate) fn IOSurfaceGetWidth(surface: *mut c_void) -> usize;
    pub(crate) fn IOSurfaceGetHeight(surface: *mut c_void) -> usize;
    pub(crate) fn IOSurfaceGetPixelFormat(surface: *mut c_void) -> u32;
    pub(crate) fn IOSurfaceGetBytesPerElement(surface: *mut c_void) -> usize;
    pub(crate) fn IOSurfaceGetBytesPerRow(surface: *mut c_void) -> usize;
    pub(crate) fn IOSurfaceGetBaseAddress(surface: *mut c_void) -> *mut u8;
    pub(crate) fn IOSurfaceLock(surface: *mut c_void, options: u32, seed: *mut u32) -> i32;
    pub(crate) fn IOSurfaceUnlock(surface: *mut c_void, options: u32, seed: *mut u32) -> i32;
    pub(crate) fn IOSurfaceIsInUse(surface: *mut c_void) -> u8;
    pub(crate) fn IOSurfaceCopyAllValues(surface: *mut c_void) -> Ref;
    pub(crate) fn IOSurfaceSetValues(surface: *mut c_void, values: Ref);
    pub(crate) static kIOSurfaceWidth: Ref;
    pub(crate) static kIOSurfaceHeight: Ref;
    pub(crate) static kIOSurfaceBytesPerElement: Ref;
    pub(crate) static kIOSurfacePixelFormat: Ref;
}

/// A Core Foundation object that this code holds one reference to.
pub(crate) struct Owned(pub(crate) Ref);

impl Owned {
    pub(crate) fn new(object: Ref) -> Option<Self> {
        (!object.is_null()).then_some(Self(object))
    }

    pub(crate) fn mutable(&self) -> *mut c_void {
        self.0.cast_mut()
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the object is not null and this code holds one reference to it.
        unsafe { CFRelease(self.0) };
    }
}

pub(crate) fn number_i64(value: i64) -> Option<Owned> {
    // SAFETY: the value lives for the call, and CFNumberCreate copies it.
    Owned::new(unsafe { CFNumberCreate(ptr::null(), NUMBER_I64, (&raw const value).cast()) })
}

pub(crate) fn number_f64(value: f64) -> Option<Owned> {
    // SAFETY: as in `number_i64`.
    Owned::new(unsafe { CFNumberCreate(ptr::null(), NUMBER_F64, (&raw const value).cast()) })
}

pub(crate) fn array(values: &[Ref]) -> Option<Owned> {
    // SAFETY: the values are live CF objects, and the array retains them.
    Owned::new(unsafe {
        CFArrayCreate(
            ptr::null(),
            values.as_ptr(),
            isize::try_from(values.len()).ok()?,
            (&raw const kCFTypeArrayCallBacks).cast(),
        )
    })
}

pub(crate) fn dictionary(pairs: &[(Ref, Ref)]) -> Option<Owned> {
    let keys: Vec<Ref> = pairs.iter().map(|(key, _)| *key).collect();
    let values: Vec<Ref> = pairs.iter().map(|(_, value)| *value).collect();
    // SAFETY: the keys and values are live CF objects, and the dictionary retains them.
    Owned::new(unsafe {
        CFDictionaryCreate(
            ptr::null(),
            keys.as_ptr(),
            values.as_ptr(),
            isize::try_from(pairs.len()).ok()?,
            (&raw const kCFTypeDictionaryKeyCallBacks).cast(),
            (&raw const kCFTypeDictionaryValueCallBacks).cast(),
        )
    })
}
