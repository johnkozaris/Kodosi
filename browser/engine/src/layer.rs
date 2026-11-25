//! Renders a tab into a Core Animation layer hosted in the macOS app via `CAContext` /
//! `CALayerHost`, letting the window server composite frames directly without video encoding.
//! Both classes are private Core Animation APIs, which Chromium uses the same way to show the
//! pages of its GPU process.

use std::{
    ffi::c_void,
    ptr,
    sync::Arc,
    time::{Duration, Instant},
};

use objc2::{
    encode::{Encoding, RefEncode},
    msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject},
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use shared_browser::link::FromEngine;
use stream::{Format, Size};
use stream_encode::{Encoder, Picture, Settings};

use crate::{
    cf::{
        IOSurfaceCopyAllValues, IOSurfaceCreate, IOSurfaceGetBaseAddress,
        IOSurfaceGetBytesPerElement, IOSurfaceGetBytesPerRow, IOSurfaceGetHeight,
        IOSurfaceGetPixelFormat, IOSurfaceGetWidth, IOSurfaceIsInUse, IOSurfaceLock,
        IOSurfaceSetValues, IOSurfaceUnlock, Owned, dictionary, kIOSurfaceBytesPerElement,
        kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth, number_i64,
    },
    engine,
    link::{Link, Pictures},
};

/// Triple-buffer cap: the window server can hold both the current and previous `IOSurface`,
/// leaving one spare surface for the next paint.
const MOST_SURFACES: usize = 3;
const REDRAW_MS: i64 = 16;
const BGRA: u32 = u32::from_be_bytes(*b"BGRA");
const BYTES_PER_PIXEL: usize = 4;
const METAL_BGRA8: usize = 80;
const METAL_COMPLETED: usize = 4;
const SETTLE: Duration = Duration::from_millis(500);
/// Rate-limits still snapshots because each one encodes a full keyframe.
const STILL_EVERY: Duration = Duration::from_secs(3);
/// High bitrate because stills are single keyframes with no follow-up refinement passes.
const STILL_BITRATE: u32 = 40_000_000;

#[repr(C)]
struct IOSurface {
    _opaque: [u8; 0],
}

// SAFETY: matches Objective-C's type encoding for `IOSurfaceRef`.
unsafe impl RefEncode for IOSurface {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__IOSurface", &[]));
}

#[repr(C)]
struct CGColor {
    _opaque: [u8; 0],
}

// SAFETY: matches Objective-C's type encoding for `CGColorRef`.
unsafe impl RefEncode for CGColor {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGSMainConnectionID() -> u32;
    fn CGColorCreateSRGB(red: f64, green: f64, blue: f64, alpha: f64) -> *mut CGColor;
    fn CGColorRelease(color: *mut CGColor);
}

#[link(name = "Metal", kind = "framework")]
unsafe extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
}

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    static kCAGravityTopLeft: &'static AnyObject;
    static kCAGravityResizeAspect: &'static AnyObject;
}

pub(crate) struct Layer {
    window: i64,
    link: Link,
    context: Retained<AnyObject>,
    root: Retained<AnyObject>,
    device: Retained<AnyObject>,
    queue: Retained<AnyObject>,
    shown: Option<Surface>,
    spare: Vec<Surface>,
    /// Active tab bound to this layer, or `None` while detached from views.
    pub(crate) tab: Option<i64>,
    reported: bool,
    /// Encodes standalone still frames so views hosting this layer have a fallback image when
    /// the layer is unavailable (for example, while the browser sleeps).
    encoder: Encoder,
    stills: Stills,
    looking: bool,
}

/// Schedules a still after the page stays idle for `SETTLE`, or after `STILL_EVERY` of
/// continuous updates, and never sooner than `STILL_EVERY` after the previous still.
#[derive(Default)]
struct Stills {
    /// Timestamp of the first change not yet captured in a still.
    waiting: Option<Instant>,
    drawn: Option<Instant>,
    made: Option<Instant>,
}

impl Stills {
    fn drawn(&mut self, now: Instant) {
        self.waiting.get_or_insert(now);
        self.drawn = Some(now);
    }

    fn wanted(&mut self, now: Instant) {
        self.waiting.get_or_insert(now);
    }

    fn due_in(&self, now: Instant) -> Option<Duration> {
        let waiting = self.waiting?;
        let rested = self.drawn.unwrap_or(waiting) + SETTLE;
        let due = rested.min(waiting + STILL_EVERY);
        let due = self.made.map_or(due, |made| due.max(made + STILL_EVERY));
        Some(due.saturating_duration_since(now))
    }

    fn made(&mut self, now: Instant) {
        self.waiting = None;
        self.made = Some(now);
    }
}

struct Surface {
    raw: Owned,
    width: usize,
    height: usize,
}

impl Surface {
    /// Allocates a surface matching `source`'s pixel format and metadata (such as color space).
    fn like(source: *mut c_void, width: usize, height: usize) -> Option<Self> {
        // SAFETY: `source` is a valid `IOSurface` for the duration of the paint callback.
        let format = unsafe { IOSurfaceGetPixelFormat(source) };
        let raw = new_surface(width, height, format)?;
        // SAFETY: both surfaces are valid; `IOSurfaceCopyAllValues` returns an owned CFDictionary.
        unsafe {
            if let Some(values) = Owned::new(IOSurfaceCopyAllValues(source)) {
                IOSurfaceSetValues(raw.mutable(), values.0);
            }
        }
        Some(Self { raw, width, height })
    }

    fn in_use(&self) -> bool {
        // SAFETY: `self.raw` owns a valid `IOSurface`.
        unsafe { IOSurfaceIsInUse(self.raw.mutable()) != 0 }
    }
}

fn new_surface(width: usize, height: usize, format: u32) -> Option<Owned> {
    let wide = number_i64(i64::try_from(width).ok()?)?;
    let high = number_i64(i64::try_from(height).ok()?)?;
    let bytes = number_i64(i64::try_from(BYTES_PER_PIXEL).ok()?)?;
    let format = number_i64(i64::from(format))?;
    // SAFETY: the keys are framework CF constants and the values are live CFNumber objects.
    let properties = dictionary(&unsafe {
        [
            (kIOSurfaceWidth, wide.0),
            (kIOSurfaceHeight, high.0),
            (kIOSurfaceBytesPerElement, bytes.0),
            (kIOSurfacePixelFormat, format.0),
        ]
    })?;
    // SAFETY: `properties` is a valid CFDictionary; `IOSurfaceCreate` returns an owned surface.
    Owned::new(unsafe { IOSurfaceCreate(properties.0) }.cast_const())
}

impl Layer {
    pub(crate) fn new(window: i64, link: Link) -> Result<Self, String> {
        let options = dictionary(&[]).ok_or("no options")?;
        // SAFETY: the selectors and argument types match Core Animation and Metal APIs, and
        // `MTLCreateSystemDefaultDevice` returns a retained object.
        let (context, root, device, queue) = unsafe {
            let context_class =
                AnyClass::get(c"CAContext").ok_or("Core Animation has no CAContext")?;
            let layer_class = AnyClass::get(c"CALayer").ok_or("Core Animation has no CALayer")?;
            let context: Option<Retained<AnyObject>> = msg_send![
                context_class,
                contextWithCGSConnection: CGSMainConnectionID(),
                options: options.0.cast::<AnyObject>()
            ];
            let context = context.ok_or("Core Animation made no context")?;
            let root: Option<Retained<AnyObject>> = msg_send![layer_class, new];
            let root = root.ok_or("Core Animation made no layer")?;
            let device =
                Retained::from_raw(MTLCreateSystemDefaultDevice()).ok_or("no Metal device")?;
            let queue: Option<Retained<AnyObject>> = msg_send![&*device, newCommandQueue];
            (context, root, device, queue.ok_or("no Metal queue")?)
        };
        let stills = Pictures {
            window,
            still: true,
            link: link.clone(),
        };
        let encoder = Encoder::new(
            Settings {
                format: Format::default(),
                bitrate: STILL_BITRATE,
            },
            Arc::new(stills),
        );
        transaction(|| {
            // SAFETY: `root` and `context` are valid CA objects, and `white` stays live until
            // `CGColorRelease`.
            unsafe {
                let white = CGColorCreateSRGB(1.0, 1.0, 1.0, 1.0);
                let _: () = msg_send![&*root, setBackgroundColor: white];
                CGColorRelease(white);
                let _: () = msg_send![&*root, setAnchorPoint: CGPoint::ZERO];
                let _: () = msg_send![&*root, setPosition: CGPoint::ZERO];
                let _: () = msg_send![&*root, setMasksToBounds: true];
                let _: () = msg_send![&*root, setOpaque: true];
                let _: () = msg_send![&*context, setLayer: &*root];
            }
        });
        Ok(Self {
            window,
            link,
            context,
            root,
            device,
            queue,
            shown: None,
            spare: Vec::new(),
            tab: None,
            reported: false,
            encoder,
            stills: Stills::default(),
            looking: false,
        })
    }

    /// Copies CEF's GPU `IOSurface` into a surface owned by this layer, since CEF reclaims its
    /// surface once the paint callback returns.
    pub(crate) fn draw_surface(&mut self, source: *mut c_void, size: Size) {
        // SAFETY: CEF provides a valid `IOSurface` for the duration of the paint callback.
        let (width, height, bytes) = unsafe {
            (
                IOSurfaceGetWidth(source),
                IOSurfaceGetHeight(source),
                IOSurfaceGetBytesPerElement(source),
            )
        };
        if bytes != BYTES_PER_PIXEL {
            self.fail("CEF's picture has another format");
            return;
        }
        let Some(target) = self.free(width, height, || Surface::like(source, width, height)) else {
            return;
        };
        if let Err(problem) = self.copy(source, &target) {
            self.spare.push(target);
            self.fail(&problem);
            return;
        }
        self.show(target, size);
    }

    pub(crate) fn draw_pixels(&mut self, pixels: &[u8], width: usize, height: usize, size: Size) {
        let Some(target) = self.free(width, height, || {
            new_surface(width, height, BGRA).map(|raw| Surface { raw, width, height })
        }) else {
            return;
        };
        let row = width * BYTES_PER_PIXEL;
        // SAFETY: the surface is locked while its rows are written, nothing else uses it, and
        // each row is at most `bytes_per_row` long.
        unsafe {
            let surface = target.raw.mutable();
            IOSurfaceLock(surface, 0, ptr::null_mut());
            let base = IOSurfaceGetBaseAddress(surface);
            let stride = IOSurfaceGetBytesPerRow(surface);
            if !base.is_null() && stride >= row && pixels.len() >= row * height {
                for line in 0..height {
                    ptr::copy_nonoverlapping(
                        pixels.as_ptr().add(line * row),
                        base.add(line * stride),
                        row,
                    );
                }
            }
            IOSurfaceUnlock(surface, 0, ptr::null_mut());
        }
        self.show(target, size);
    }

    /// Updates layer bounds immediately, as views place the page by its new size, so the previous
    /// surface scales to fit until the page paints at the new dimensions.
    pub(crate) fn resize(&self, size: Size) {
        if let Some(shown) = &self.shown {
            self.place(shown, size);
        }
    }

    pub(crate) fn want_still(&mut self) {
        self.stills.wanted(Instant::now());
        self.look();
    }

    pub(crate) fn looked(&mut self) {
        self.looking = false;
        self.look();
    }

    /// Encodes a still when due, or schedules a timer until then. Skips encoding while no tab
    /// is bound; rebinding a tab invalidates the view and retriggers this path.
    fn look(&mut self) {
        if self.looking {
            return;
        }
        let now = Instant::now();
        let Some(wait) = self.stills.due_in(now) else {
            return;
        };
        if !wait.is_zero() {
            self.looking = true;
            engine::look_at_stills(
                self.window,
                i64::try_from(wait.as_millis()).unwrap_or(i64::MAX),
            );
            return;
        }
        if let (Some(_), Some(shown)) = (self.tab, &self.shown) {
            self.encoder.want_key();
            self.encoder.encode(Picture::Surface(shown.raw.mutable()));
            self.stills.made(now);
        }
    }

    /// Returns an unused surface of matching dimensions or allocates up to `MOST_SURFACES`. If
    /// the window server still holds all surfaces, schedules a redraw one frame later.
    fn free(
        &mut self,
        width: usize,
        height: usize,
        make: impl FnOnce() -> Option<Surface>,
    ) -> Option<Surface> {
        self.spare
            .retain(|surface| (surface.width, surface.height) == (width, height));
        if let Some(index) = self.spare.iter().position(|surface| !surface.in_use()) {
            return Some(self.spare.swap_remove(index));
        }
        if self.spare.len() + usize::from(self.shown.is_some()) < MOST_SURFACES {
            return make();
        }
        engine::redraw(self.window, REDRAW_MS);
        None
    }

    fn copy(&self, source: *mut c_void, target: &Surface) -> Result<(), String> {
        autoreleasepool(|_| {
            // SAFETY: device, queue, and surfaces are valid, selectors match Metal's API, and
            // `waitUntilCompleted` finishes the blit before CEF reclaims `source`.
            unsafe {
                let descriptor_class = AnyClass::get(c"MTLTextureDescriptor").ok_or("no Metal")?;
                let descriptor: Option<Retained<AnyObject>> = msg_send![
                    descriptor_class,
                    texture2DDescriptorWithPixelFormat: METAL_BGRA8,
                    width: target.width,
                    height: target.height,
                    mipmapped: false
                ];
                let descriptor = descriptor.ok_or("no texture descriptor")?;
                let texture = |surface: *mut c_void| -> Option<Retained<AnyObject>> {
                    msg_send![
                        &*self.device,
                        newTextureWithDescriptor: &*descriptor,
                        iosurface: surface.cast::<IOSurface>(),
                        plane: 0_usize
                    ]
                };
                let (from, to) = texture(source)
                    .zip(texture(target.raw.mutable()))
                    .ok_or("no texture of the picture")?;
                let buffer: Option<Retained<AnyObject>> = msg_send![&*self.queue, commandBuffer];
                let buffer = buffer.ok_or("no command buffer")?;
                let blit: Option<Retained<AnyObject>> = msg_send![&*buffer, blitCommandEncoder];
                let blit = blit.ok_or("no blit encoder")?;
                let _: () = msg_send![&*blit, copyFromTexture: &*from, toTexture: &*to];
                let _: () = msg_send![&*blit, endEncoding];
                let _: () = msg_send![&*buffer, commit];
                let _: () = msg_send![&*buffer, waitUntilCompleted];
                let status: usize = msg_send![&*buffer, status];
                (status == METAL_COMPLETED)
                    .then_some(())
                    .ok_or_else(|| format!("the copy of the picture failed: {status}"))
            }
        })
    }

    fn show(&mut self, surface: Surface, size: Size) {
        self.place(&surface, size);
        if let Some(earlier) = self.shown.replace(surface) {
            self.spare.push(earlier);
        }
        self.stills.drawn(Instant::now());
        self.look();
        if !self.reported {
            self.reported = true;
            // SAFETY: the context is live.
            let context: u32 = unsafe { msg_send![&*self.context, contextId] };
            self.link.send(&FromEngine::Layer {
                window: self.window,
                context,
            });
        }
    }

    /// Uses top-left 1:1 gravity when the surface matches the viewport size and scale, or
    /// aspect-fit gravity for a stale surface from before a resize.
    fn place(&self, surface: &Surface, size: Size) {
        let scale = f64::from(size.scale);
        let (width, height) = (f64::from(size.width), f64::from(size.height));
        let pixels = |count: usize| u32::try_from(count).map_or(f64::MAX, f64::from);
        let drawn_at_size = (pixels(surface.width) - width * scale).abs() < 1.0
            && (pixels(surface.height) - height * scale).abs() < 1.0;
        transaction(|| {
            // SAFETY: `self.root` and `surface` are valid, and the gravity statics are
            // QuartzCore constants.
            unsafe {
                let root = &*self.root;
                let bounds = CGRect::new(CGPoint::ZERO, CGSize::new(width, height));
                let gravity = if drawn_at_size {
                    kCAGravityTopLeft
                } else {
                    kCAGravityResizeAspect
                };
                let _: () = msg_send![root, setBounds: bounds];
                let _: () = msg_send![root, setContentsScale: scale];
                let _: () = msg_send![root, setContentsGravity: gravity];
                let _: () = msg_send![root, setContents: surface.raw.0.cast::<AnyObject>()];
            }
        });
    }

    /// Notifies the host to fall back to video streaming.
    fn fail(&mut self, problem: &str) {
        self.tab = None;
        self.link.send(&FromEngine::NoLayer {
            window: self.window,
            problem: problem.to_owned(),
        });
    }
}

impl Drop for Layer {
    fn drop(&mut self) {
        // SAFETY: `self.context` is a valid `CAContext`; invalidating detaches hosted views.
        unsafe {
            let _: () = msg_send![&*self.context, invalidate];
        }
    }
}

/// Wraps layer updates in a `CATransaction` with actions disabled to suppress implicit
/// animations.
fn transaction(change: impl FnOnce()) {
    let Some(class) = AnyClass::get(c"CATransaction") else {
        return change();
    };
    // SAFETY: `CATransaction` accepts `begin`, `setDisableActions:`, and `commit` in this order.
    unsafe {
        let _: () = msg_send![class, begin];
        let _: () = msg_send![class, setDisableActions: true];
    }
    change();
    // SAFETY: as above.
    unsafe {
        let _: () = msg_send![class, commit];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_waits_for_the_page_to_rest_and_comes_once_in_its_time_while_the_page_changes() {
        let start = Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let mut stills = Stills::default();
        assert_eq!(stills.due_in(start), None);
        stills.drawn(start);
        assert_eq!(stills.due_in(start), Some(SETTLE));
        stills.made(at(500));
        stills.drawn(at(600));
        assert_eq!(stills.due_in(at(1100)), Some(Duration::from_millis(2400)));
        stills.made(at(3500));
        for frame in 0..188 {
            stills.drawn(at(3600 + frame * 16));
        }
        assert_eq!(stills.due_in(at(6599)), Some(Duration::from_millis(1)));
        assert_eq!(stills.due_in(at(6600)), Some(Duration::ZERO));
        stills.made(at(6600));
        stills.wanted(at(20_000));
        assert_eq!(stills.due_in(at(20_000)), Some(Duration::ZERO));
    }
}
