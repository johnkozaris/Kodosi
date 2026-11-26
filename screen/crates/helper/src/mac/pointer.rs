//! Where the pointer is over the shown source and what it looks like, read on the main thread,
//! where `AppKit` keeps the cursor.

use std::{
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    ptr::{self, NonNull},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use block2::RcBlock;
use bytes::Bytes;
use objc2::{AllocAnyThread, rc::Retained};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSCursor, NSImage};
use objc2_core_foundation::CGPoint;
use objc2_core_graphics::{CGDataProvider, CGEvent, CGImage};
use objc2_foundation::{NSDictionary, NSTimer};
use screen::link::{self, FromHelper, Pointer, Shape, Source};
use tokio::sync::mpsc;

use super::{app, sources};

/// Seconds between two looks at the pointer: as often as frames come.
const EVERY: f64 = 1.0 / 60.0;
/// Pixels per point of the pointer pictures, at most, for viewers that zoom in on them.
const SHARPEST: f64 = 6.0;

/// What the host knows of the pointer.
#[derive(Clone, Copy, PartialEq)]
enum Known {
    Nothing,
    Where(Option<Pointer>),
}

pub(super) struct Pointing {
    say: mpsc::Sender<Bytes>,
    source: Option<Source>,
    timer: Option<Retained<NSTimer>>,
    told: Known,
    /// The shapes that the host has.
    shapes: HashSet<u64>,
    /// The last cursor picture and its shape, to know it again without reading its pixels.
    last: Option<(Retained<NSImage>, u64)>,
}

impl Pointing {
    pub(super) fn new(say: mpsc::Sender<Bytes>) -> Self {
        Self {
            say,
            source: None,
            timer: None,
            told: Known::Nothing,
            shapes: HashSet::new(),
            last: None,
        }
    }

    /// Follows the pointer over a source, or stops when there is none.
    pub(super) fn follow(&mut self, source: Option<Source>) {
        self.source = source;
        self.told = Known::Nothing;
        match (source, &self.timer) {
            (Some(_), None) => {
                let look = RcBlock::new(|_: NonNull<NSTimer>| app::pointing(Self::look));
                // SAFETY: the block takes the timer that it is given.
                self.timer = Some(unsafe {
                    NSTimer::scheduledTimerWithTimeInterval_repeats_block(EVERY, true, &look)
                });
            }
            (None, Some(timer)) => {
                timer.invalidate();
                self.timer = None;
            }
            _ => {}
        }
    }

    fn look(&mut self) {
        let Some(bounds) = self.source.and_then(sources::bounds) else {
            return;
        };
        let Some(event) = CGEvent::new(None) else {
            return;
        };
        let at = CGEvent::location(Some(&event));
        let pointer = if sources::contains(bounds, at) {
            let Some(shape) = self.shape() else {
                return;
            };
            Some(Pointer {
                x: at.x - bounds.origin.x,
                y: at.y - bounds.origin.y,
                shape,
            })
        } else {
            None
        };
        if self.told != Known::Where(pointer)
            && self
                .say
                .try_send(link::json(&FromHelper::Pointer { pointer }))
                .is_ok()
        {
            self.told = Known::Where(pointer);
        }
    }

    /// The current cursor's shape, once the host has its picture.
    fn shape(&mut self) -> Option<u64> {
        #[expect(
            deprecated,
            reason = "viewers draw the pointer themselves, and only this tells what it looks like"
        )]
        let cursor = NSCursor::currentSystemCursor()?;
        let image = cursor.image();
        let hotspot = cursor.hotSpot();
        let id = match &self.last {
            Some((last, id)) if Retained::as_ptr(last) == Retained::as_ptr(&image) => *id,
            _ => identity(&image, hotspot)?,
        };
        if self.shapes.insert(id) {
            let told = picture(&image, hotspot, id).is_some_and(|shape| {
                self.say
                    .try_send(link::json(&FromHelper::Shape { shape }))
                    .is_ok()
            });
            if !told {
                self.shapes.remove(&id);
                return None;
            }
        }
        self.last = Some((image, id));
        Some(id)
    }
}

/// Names a cursor by its pixels and hotspot, so that the same shape keeps its number.
fn identity(image: &NSImage, hotspot: CGPoint) -> Option<u64> {
    // SAFETY: without a rectangle, context or hints, the image draws at its own size.
    let picture =
        unsafe { image.CGImageForProposedRect_context_hints(ptr::null_mut(), None, None) }?;
    let pixels = CGDataProvider::data(CGImage::data_provider(Some(&picture)).as_deref())?;
    let mut hasher = DefaultHasher::new();
    // SAFETY: nothing changes the data while it is read.
    unsafe { pixels.as_bytes_unchecked() }.hash(&mut hasher);
    hotspot.x.to_bits().hash(&mut hasher);
    hotspot.y.to_bits().hash(&mut hasher);
    Some(hasher.finish())
}

/// The cursor's sharpest picture up to `SHARPEST`, as a PNG with its size and hotspot in points.
fn picture(image: &NSImage, hotspot: CGPoint, id: u64) -> Option<Shape> {
    let size = image.size();
    let mut pictures = image.representations().to_vec();
    pictures.sort_by_key(|picture| picture.pixelsWide());
    let sharpest = pictures
        .iter()
        .rev()
        .find(|picture| {
            let wide = i32::try_from(picture.pixelsWide()).unwrap_or(i32::MAX);
            f64::from(wide) <= size.width * SHARPEST
        })
        .or_else(|| pictures.first())?;
    // SAFETY: without a rectangle, context or hints, the picture draws at its own size.
    let drawn =
        unsafe { sharpest.CGImageForProposedRect_context_hints(ptr::null_mut(), None, None) }?;
    let bitmap = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &drawn);
    // SAFETY: PNG takes no properties.
    let png = unsafe {
        bitmap.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(Shape {
        id,
        png: STANDARD.encode(png.to_vec()),
        x: hotspot.x,
        y: hotspot.y,
        width: size.width,
        height: size.height,
    })
}
