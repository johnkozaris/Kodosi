//! The displays and windows that viewers can watch, as `ScreenCaptureKit` lists them, and where
//! each one is on the screens.

use std::sync::{Mutex, PoisonError};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFString, CGPoint, CGRect};
use objc2_core_graphics::{
    CGDisplayBounds, CGDisplayCopyDisplayMode, CGDisplayMode, CGMainDisplayID,
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGWindowBounds, kCGWindowLayer,
    kCGWindowNumber, kCGWindowOwnerPID,
};
use objc2_foundation::NSError;
use objc2_screen_capture_kit::{SCDisplay, SCShareableContent, SCWindow};
use screen::link::{Display, Source, Sources, Window};
use tokio::sync::oneshot;

use super::{item, rect, value};

/// Windows smaller than this, in points, are palettes and badges rather than something to watch.
const SMALLEST_WINDOW: f64 = 64.0;
/// The layer of ordinary app windows, below menus, the Dock and overlays.
const APP_LAYER: isize = 0;

pub(super) struct Content(Retained<SCShareableContent>);

#[expect(
    clippy::non_send_fields_in_send_ty,
    reason = "objc2 marks every Objective-C object as !Send"
)]
// SAFETY: shareable content is a snapshot that nothing changes once `ScreenCaptureKit` made it.
unsafe impl Send for Content {}

impl Content {
    pub(super) async fn now() -> Result<Self, String> {
        let (done, finished) = oneshot::channel();
        let done = Mutex::new(Some(done));
        let block = RcBlock::new(
            move |content: *mut SCShareableContent, error: *mut NSError| {
                // SAFETY: each is null or valid during the call.
                let answer =
                    match unsafe { (Retained::retain(content), error.as_ref()) } {
                        (Some(content), _) => Ok(Self(content)),
                        (None, error) => Err(error
                            .map_or_else(|| "macOS listed no screens".to_owned(), super::describe)),
                    };
                let done = done.lock().unwrap_or_else(PoisonError::into_inner).take();
                if let Some(done) = done {
                    drop(done.send(answer));
                }
            },
        );
        // SAFETY: the block takes what the completion handler gets.
        unsafe {
            SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
                true, true, &block,
            );
        }
        finished
            .await
            .unwrap_or_else(|_| Err("macOS did not answer".to_owned()))
    }

    /// The main display comes first.
    pub(super) fn sources(&self) -> Sources {
        let main = CGMainDisplayID();
        // SAFETY: getters of a snapshot.
        let mut displays: Vec<(Display, CGRect)> = unsafe { self.0.displays() }
            .to_vec()
            .iter()
            .map(|display| {
                // SAFETY: getters of a snapshot.
                let (id, frame) = unsafe { (display.displayID(), display.frame()) };
                let display = Display {
                    id,
                    width: frame.size.width,
                    height: frame.size.height,
                    scale: scale(id),
                };
                (display, frame)
            })
            .collect();
        displays.sort_by_key(|(display, _)| display.id != main);
        // SAFETY: a getter of a snapshot.
        let windows = unsafe { self.0.windows() }
            .to_vec()
            .iter()
            .filter_map(|window| listed(window, &displays))
            .collect();
        Sources {
            displays: displays.into_iter().map(|(display, _)| display).collect(),
            windows,
        }
    }

    pub(super) fn display(&self, id: u32) -> Option<Retained<SCDisplay>> {
        // SAFETY: getters of a snapshot.
        unsafe { self.0.displays() }
            .to_vec()
            .into_iter()
            .find(|display| unsafe { display.displayID() } == id)
    }

    pub(super) fn window(&self, id: u32) -> Option<Retained<SCWindow>> {
        // SAFETY: getters of a snapshot.
        unsafe { self.0.windows() }
            .to_vec()
            .into_iter()
            .find(|window| unsafe { window.windowID() } == id)
    }
}

fn listed(window: &SCWindow, displays: &[(Display, CGRect)]) -> Option<Window> {
    // SAFETY: getters of a snapshot.
    let (layer, shown, frame) =
        unsafe { (window.windowLayer(), window.isOnScreen(), window.frame()) };
    if layer != APP_LAYER
        || !shown
        || frame.size.width < SMALLEST_WINDOW
        || frame.size.height < SMALLEST_WINDOW
    {
        return None;
    }
    // SAFETY: getters of a snapshot.
    let (id, app, title) = unsafe {
        (
            window.windowID(),
            window.owningApplication()?,
            window.title(),
        )
    };
    let middle = CGPoint {
        x: frame.origin.x + frame.size.width / 2.0,
        y: frame.origin.y + frame.size.height / 2.0,
    };
    let scale = displays
        .iter()
        .find(|(_, display)| contains(*display, middle))
        .or_else(|| displays.first())
        .map_or(1.0, |(display, _)| display.scale);
    Some(Window {
        id,
        // SAFETY: a getter of a snapshot.
        app: unsafe { app.applicationName() }.to_string(),
        title: title.map_or_else(String::new, |title| title.to_string()),
        width: frame.size.width,
        height: frame.size.height,
        scale,
    })
}

/// Pixels per point of a display in its current mode.
fn scale(display: u32) -> f64 {
    let mode = CGDisplayCopyDisplayMode(display);
    let (pixels, points) = (
        CGDisplayMode::pixel_width(mode.as_deref()),
        CGDisplayMode::width(mode.as_deref()),
    );
    if points == 0 {
        1.0
    } else {
        number(pixels) / number(points)
    }
}

fn number(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

pub(super) fn contains(rect: CGRect, point: CGPoint) -> bool {
    point.x >= rect.origin.x
        && point.y >= rect.origin.y
        && point.x < rect.origin.x + rect.size.width
        && point.y < rect.origin.y + rect.size.height
}

/// Where a source is now, in global points from the top left of the main display.
pub(super) fn bounds(source: Source) -> Option<CGRect> {
    match source {
        Source::Display { id } => Some(CGDisplayBounds(id)),
        Source::Window { id } => placed_window(id).map(|window| window.bounds),
    }
}

/// A window as the window server places it now.
pub(super) struct Placed {
    pub(super) id: u32,
    pub(super) owner: i32,
    pub(super) bounds: CGRect,
    layer: i64,
}

pub(super) fn placed_window(id: u32) -> Option<Placed> {
    let windows = CGWindowListCopyWindowInfo(CGWindowListOption::OptionIncludingWindow, id)?;
    placed(&windows).into_iter().find(|window| window.id == id)
}

/// The app window in front at a point, which a click there reaches.
pub(super) fn front_window_at(at: CGPoint) -> Option<u32> {
    let windows = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    )?;
    placed(&windows)
        .into_iter()
        .find(|window| window.layer == 0 && contains(window.bounds, at))
        .map(|window| window.id)
}

/// The window server's descriptions of windows, front to back.
fn placed(windows: &CFArray) -> Vec<Placed> {
    (0..windows.count())
        .filter_map(|index| {
            let window = item::<CFDictionary>(windows, index)?;
            // SAFETY: Core Graphics constants.
            let (id, owner, layer, bounds) = unsafe {
                (
                    kCGWindowNumber,
                    kCGWindowOwnerPID,
                    kCGWindowLayer,
                    kCGWindowBounds,
                )
            };
            let number = |key: &CFString| value::<CFNumber>(window, key)?.as_i64();
            Some(Placed {
                id: u32::try_from(number(id)?).ok()?,
                owner: i32::try_from(number(owner)?).ok()?,
                bounds: value::<CFDictionary>(window, bounds).and_then(rect)?,
                layer: number(layer)?,
            })
        })
        .collect()
}
