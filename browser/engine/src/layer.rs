//! A window's page as a Core Animation layer, which the app on this Mac hosts by the layer's
//! context: the window server draws the page into the app's window, with no video between.
//!
//! `CAContext` and `CALayerHost` are private classes of Core Animation, which Chromium uses in
//! the same way to show the pages of its GPU process in its windows.

use std::{ffi::c_void, ptr};

use objc2::{
    encode::{Encoding, RefEncode},
    msg_send,
    rc::{Retained, autoreleasepool},
    runtime::{AnyClass, AnyObject},
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use shared_browser::{Size, link::FromEngine};

use crate::{
    cf::{
        IOSurfaceCopyAllValues, IOSurfaceCreate, IOSurfaceGetBaseAddress,
        IOSurfaceGetBytesPerElement, IOSurfaceGetBytesPerRow, IOSurfaceGetHeight,
        IOSurfaceGetPixelFormat, IOSurfaceGetWidth, IOSurfaceIsInUse, IOSurfaceLock,
        IOSurfaceSetValues, IOSurfaceUnlock, Owned, dictionary, kIOSurfaceBytesPerElement,
        kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth, number_i64,
    },
    engine,
    link::Link,
};

/// The window server holds the picture that it shows, and for a moment the one before, so the
/// layer draws into a surface of its own that it holds neither.
const MOST_SURFACES: usize = 3;
const REDRAW_MS: i64 = 16;
const BGRA: u32 = u32::from_be_bytes(*b"BGRA");
const BYTES_PER_PIXEL: usize = 4;
const METAL_BGRA8: usize = 80;
const METAL_COMPLETED: usize = 4;

/// `IOSurfaceRef`, as Objective-C methods take it.
#[repr(C)]
struct IOSurface {
    _opaque: [u8; 0],
}

// SAFETY: the encoding of `IOSurfaceRef` in Objective-C.
unsafe impl RefEncode for IOSurface {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("__IOSurface", &[]));
}

#[repr(C)]
struct CGColor {
    _opaque: [u8; 0],
}

// SAFETY: the encoding of `CGColorRef` in Objective-C.
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
    /// The tab whose page the layer shows, or none while no view hosts the layer.
    pub(crate) tab: Option<i64>,
    reported: bool,
}

struct Surface {
    raw: Owned,
    width: usize,
    height: usize,
}

impl Surface {
    /// A surface with CEF's format and attachments, such as its color space.
    fn like(source: *mut c_void, width: usize, height: usize) -> Option<Self> {
        // SAFETY: the source is a live surface for the length of the paint call.
        let format = unsafe { IOSurfaceGetPixelFormat(source) };
        let raw = new_surface(width, height, format)?;
        // SAFETY: both surfaces are live; the values are a dictionary that this code then owns.
        unsafe {
            if let Some(values) = Owned::new(IOSurfaceCopyAllValues(source)) {
                IOSurfaceSetValues(raw.mutable(), values.0);
            }
        }
        Some(Self { raw, width, height })
    }

    fn in_use(&self) -> bool {
        // SAFETY: the surface is live while this code holds it.
        unsafe { IOSurfaceIsInUse(self.raw.mutable()) != 0 }
    }
}

fn new_surface(width: usize, height: usize, format: u32) -> Option<Owned> {
    let wide = number_i64(i64::try_from(width).ok()?)?;
    let high = number_i64(i64::try_from(height).ok()?)?;
    let bytes = number_i64(i64::try_from(BYTES_PER_PIXEL).ok()?)?;
    let format = number_i64(i64::from(format))?;
    // SAFETY: the keys are constants of the framework and the values are live numbers.
    let properties = dictionary(&unsafe {
        [
            (kIOSurfaceWidth, wide.0),
            (kIOSurfaceHeight, high.0),
            (kIOSurfaceBytesPerElement, bytes.0),
            (kIOSurfacePixelFormat, format.0),
        ]
    })?;
    // SAFETY: the properties are a live dictionary; the new surface is this code's.
    Owned::new(unsafe { IOSurfaceCreate(properties.0) }.cast_const())
}

impl Layer {
    pub(crate) fn new(window: i64, link: Link) -> Result<Self, String> {
        let options = dictionary(&[]).ok_or("no options")?;
        // SAFETY: the classes are Core Animation's and Metal's, and each message is one that
        // they take, with arguments of their types; the device comes retained.
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
        transaction(|| {
            // SAFETY: the root is a live layer, and white is a live color until its release.
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
        })
    }

    /// Copies a picture that CEF drew on the GPU. CEF takes its surface back once the paint call
    /// returns, so the layer never shows that surface itself.
    pub(crate) fn draw_surface(&mut self, source: *mut c_void, size: Size) {
        if source.is_null() {
            return;
        }
        // SAFETY: CEF gives a live surface for the length of the paint call.
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

    /// A new size of the page shows at once, with the last picture fitted into it until the page
    /// draws at that size, as the views place the page by its new size.
    pub(crate) fn resize(&self, size: Size) {
        if let Some(shown) = &self.shown {
            self.place(shown, size);
        }
    }

    /// A surface of this size that the window server holds no more, or a new one while there
    /// are few. While the window server holds them all, the page draws again a frame later, so
    /// the layer still gets the page as it is now.
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
            // SAFETY: the device, the queue and the surfaces are live, each message is one that
            // Metal takes, and the copy ends before this call returns, while CEF's surface is live.
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

    /// The layer has the page's size in its points. A picture of that size and scale fills it
    /// pixel for pixel from its top left; another one, from before a new size, fits into it.
    fn place(&self, surface: &Surface, size: Size) {
        let scale = f64::from(size.scale);
        let (width, height) = (f64::from(size.width), f64::from(size.height));
        let pixels = |count: usize| u32::try_from(count).map_or(f64::MAX, f64::from);
        let drawn_at_size = (pixels(surface.width) - width * scale).abs() < 1.0
            && (pixels(surface.height) - height * scale).abs() < 1.0;
        transaction(|| {
            // SAFETY: the root is a live layer, the surface is live, and the gravities are
            // constants of the framework.
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

    /// The host gives the views video once the layer fails.
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
        // SAFETY: the context is live; an invalid context shows nothing in the views that host it.
        unsafe {
            let _: () = msg_send![&*self.context, invalidate];
        }
    }
}

/// Without a transaction of its own, Core Animation animates each change of a layer.
fn transaction(change: impl FnOnce()) {
    let Some(class) = AnyClass::get(c"CATransaction") else {
        return change();
    };
    // SAFETY: the messages are ones that `CATransaction` takes, in this order.
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
