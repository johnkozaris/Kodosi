//! The shown display's pictures, on a thread of its own with a connection of its own. The X server
//! copies what DAMAGE says changed on its screen into a pixmap in shared memory, at most as often
//! as viewers show pictures, and the thread crops and scales that memory to the size that the
//! host asks for, then encodes it. The pointer goes apart from the pictures, as XFIXES tells its
//! shape, so that viewers draw it.

use std::{
    collections::HashSet,
    ffi::c_void,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    os::fd::OwnedFd,
    ptr::NonNull,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use rustix::{
    event::{PollFd, PollFlags, Timespec},
    fs::MemfdFlags,
    mm::{MapFlags, ProtFlags},
};
use screen::link::{self, FromHelper, Pointer, Shape};
use stream::Region;
use stream_encode::{Crop, Picture};
use tokio::sync::{mpsc, oneshot};
use x11rb::{
    NONE,
    connection::Connection as _,
    protocol::{
        Event as Happened,
        damage::{self, ConnectionExt as _},
        screensaver::ConnectionExt as _,
        shm::{self, ConnectionExt as _},
        xfixes::{self, ConnectionExt as _},
        xproto::{
            self, ConnectionExt as _, CreateGCAux, ImageFormat, ImageOrder, Rectangle, ScreenSaver,
            SubwindowMode, Window,
        },
    },
    rust_connection::RustConnection,
};

use super::{Monitor, problem, session::Event};
use crate::linux::{
    capture::{png, unpremultiplied},
    frames::Frames,
    hand::Hand,
};

/// The shortest time between two encoded pictures: viewers show 60 a second.
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// How often the capture looks where the pointer is, which X11 tells only when asked.
const POINTER_EVERY: Duration = Duration::from_millis(16);
/// A region this close to the whole display shows all of it.
const WHOLE: f64 = 0.5;
/// The largest pointer picture that the helper takes, in pixels.
const LARGEST_POINTER: u16 = 256;

/// What the host asks the capture to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Wanted {
    pub(super) monitor: Monitor,
    /// The part of the display to capture, in its points, or `None` for all of it.
    pub(super) region: Option<Region>,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// What the session and the capture's thread share.
struct Shared {
    wanted: Mutex<Wanted>,
    /// The next picture shows anew, as what is wanted changed or the encoder needs it.
    fresh: AtomicBool,
    stopping: AtomicBool,
}

impl Shared {
    fn wanted(&self) -> Wanted {
        *self.wanted.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn stopping(&self) -> bool {
        self.stopping.load(Ordering::Relaxed)
    }
}

pub(super) struct Capture {
    /// The id of the shown display.
    pub(super) display: u32,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    pub(super) async fn start(
        display: u32,
        wanted: Wanted,
        frames: Arc<Frames>,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<Event>,
        hand: Arc<Hand>,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            wanted: Mutex::new(wanted),
            fresh: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
        });
        let (ready, started) = oneshot::channel();
        let taking = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("kodosi-screen-capture".to_owned())
            .spawn(move || {
                match Screen::open().and_then(|screen| Taking::new(screen, wanted, taking)) {
                    Ok(taking) => {
                        drop(ready.send(Ok(())));
                        taking.run(&frames, &events, Pointing::new(say, hand));
                    }
                    Err(problem) => drop(ready.send(Err(problem))),
                }
            })
            .map_err(|error| error.to_string())?;
        started
            .await
            .unwrap_or_else(|_| Err("the capture did not start".to_owned()))?;
        Ok(Self {
            display,
            shared,
            thread: Some(thread),
        })
    }

    /// Changes the size or the region of the same display, or its place on the screen.
    pub(super) fn update(&self, wanted: Wanted) {
        *self
            .shared
            .wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = wanted;
        self.redraw();
    }

    /// Encodes the newest picture again, as all new.
    pub(super) fn redraw(&self) {
        self.shared.fresh.store(true, Ordering::Relaxed);
    }

    /// Returns once no more pictures come.
    pub(super) async fn stop(mut self) {
        self.shared.stopping.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            drop(tokio::task::spawn_blocking(move || thread.join()).await);
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shared.stopping.store(true, Ordering::Relaxed);
    }
}

/// The capture's connection to the X server, with what it needs there.
struct Screen {
    x: RustConnection,
    root: Window,
    depth: u8,
    /// What changed on the screen since the capture last took it.
    damage: damage::Damage,
    /// Where the damage goes when the capture takes it.
    region: xfixes::Region,
    /// The X server draws into a pixmap in shared memory, so it copies only what changed.
    shared_pixmaps: bool,
}

impl Screen {
    fn open() -> Result<Self, String> {
        let (x, screen) = RustConnection::connect(None).map_err(problem)?;
        let setup = x.setup();
        let shown = setup
            .roots
            .get(screen)
            .ok_or("the X server has no such screen")?;
        let (root, depth) = (shown.root, shown.root_depth);
        let bits = setup
            .pixmap_formats
            .iter()
            .find(|format| format.depth == depth)
            .map(|format| format.bits_per_pixel);
        let visual = shown
            .allowed_depths
            .iter()
            .flat_map(|allowed| &allowed.visuals)
            .find(|visual| visual.visual_id == shown.root_visual);
        let blue_last = visual.is_some_and(|visual| {
            (visual.red_mask, visual.green_mask, visual.blue_mask) == (0xff_0000, 0xff00, 0xff)
        });
        if bits != Some(32) || setup.image_byte_order != ImageOrder::LSB_FIRST || !blue_last {
            return Err(
                "the X server keeps its pixels in a way that Kodosi Screen does not take"
                    .to_owned(),
            );
        }
        let memory = x
            .shm_query_version()
            .map_err(problem)?
            .reply()
            .map_err(problem)?;
        if (memory.major_version, memory.minor_version) < (1, 2) {
            return Err("the X server's MIT-SHM is older than 1.2".to_owned());
        }
        x.damage_query_version(1, 1)
            .map_err(problem)?
            .reply()
            .map_err(problem)?;
        x.xfixes_query_version(5, 0)
            .map_err(problem)?
            .reply()
            .map_err(problem)?;
        let damage = x.generate_id().map_err(problem)?;
        x.damage_create(damage, root, damage::ReportLevel::NON_EMPTY)
            .map_err(problem)?;
        let region = x.generate_id().map_err(problem)?;
        x.xfixes_create_region(region, &[]).map_err(problem)?;
        x.xfixes_select_cursor_input(root, xfixes::CursorNotifyMask::DISPLAY_CURSOR)
            .map_err(problem)?;
        // Opening the screen wakes it, and it stays awake while someone watches.
        drop(x.force_screen_saver(ScreenSaver::RESET));
        if x.screensaver_query_version(1, 1)
            .ok()
            .and_then(|asked| asked.reply().ok())
            .is_some()
        {
            drop(x.screensaver_suspend(1));
        }
        x.flush().map_err(problem)?;
        Ok(Self {
            x,
            root,
            depth,
            damage,
            region,
            shared_pixmaps: memory.shared_pixmaps
                && memory.pixmap_format == u8::from(ImageFormat::Z_PIXMAP),
        })
    }

    /// The rectangles of the screen that changed since the capture last asked.
    fn damaged(&self) -> Result<Vec<Rectangle>, String> {
        self.x
            .damage_subtract(self.damage, NONE, self.region)
            .map_err(problem)?;
        Ok(self
            .x
            .xfixes_fetch_region(self.region)
            .map_err(problem)?
            .reply()
            .map_err(problem)?
            .rectangles)
    }
}

/// Shared memory that holds the shown display's pixels in rows of four bytes each, blue first.
struct Copied {
    monitor: Monitor,
    segment: shm::Seg,
    /// The pixmap over the memory and how it is drawn into, where the X server shares pixmaps.
    pixmap: Option<(xproto::Pixmap, xproto::Gcontext)>,
    map: NonNull<c_void>,
    length: usize,
    _memory: OwnedFd,
}

// SAFETY: the mapping belongs to the capture's thread alone, which only reads it.
unsafe impl Send for Copied {}

impl Copied {
    fn new(screen: &Screen, monitor: Monitor) -> Result<Self, String> {
        let length = monitor.pixels() * 4;
        let memory = rustix::fs::memfd_create("kodosi-screen", MemfdFlags::CLOEXEC)
            .map_err(|error| error.to_string())?;
        rustix::fs::ftruncate(
            &memory,
            u64::try_from(length).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        // SAFETY: a new shared mapping of a file that holds `length` bytes.
        let map = unsafe {
            rustix::mm::mmap(
                std::ptr::null_mut(),
                length,
                ProtFlags::READ,
                MapFlags::SHARED,
                &memory,
                0,
            )
        }
        .map_err(|error| error.to_string())?;
        let map = NonNull::new(map).ok_or("the memory did not map")?;
        let x = &screen.x;
        let segment = x.generate_id().map_err(problem)?;
        let given = memory.try_clone().map_err(|error| error.to_string())?;
        x.shm_attach_fd(segment, given, false).map_err(problem)?;
        let pixmap = if screen.shared_pixmaps {
            let pixmap = x.generate_id().map_err(problem)?;
            x.shm_create_pixmap(
                pixmap,
                screen.root,
                monitor.width,
                monitor.height,
                screen.depth,
                segment,
                0,
            )
            .map_err(problem)?;
            let drawing = x.generate_id().map_err(problem)?;
            x.create_gc(
                drawing,
                pixmap,
                &CreateGCAux::new()
                    .subwindow_mode(SubwindowMode::INCLUDE_INFERIORS)
                    .graphics_exposures(0),
            )
            .map_err(problem)?;
            Some((pixmap, drawing))
        } else {
            None
        };
        let copied = Self {
            monitor,
            segment,
            pixmap,
            map: map.cast(),
            length,
            _memory: memory,
        };
        copied.take(
            screen,
            &[Rectangle {
                x: monitor.x,
                y: monitor.y,
                width: monitor.width,
                height: monitor.height,
            }],
        )?;
        Ok(copied)
    }

    /// Has the X server copy the parts of `changed` on the display into the memory, and returns
    /// once it did.
    fn take(&self, screen: &Screen, changed: &[Rectangle]) -> Result<(), String> {
        let x = &screen.x;
        let parts: Vec<Rectangle> = changed
            .iter()
            .filter_map(|rectangle| within(*rectangle, self.monitor))
            .collect();
        if parts.is_empty() {
            return Ok(());
        }
        if let Some((pixmap, drawing)) = self.pixmap {
            for part in &parts {
                x.copy_area(
                    screen.root,
                    pixmap,
                    drawing,
                    part.x,
                    part.y,
                    part.x - self.monitor.x,
                    part.y - self.monitor.y,
                    part.width,
                    part.height,
                )
                .map_err(problem)?;
            }
            x.get_input_focus()
                .map_err(problem)?
                .reply()
                .map_err(problem)?;
            return Ok(());
        }
        // Whole rows keep the memory's layout, as the image comes in rows as wide as its own.
        let top = parts.iter().map(|part| part.y).min().unwrap_or_default();
        let bottom = parts
            .iter()
            .map(|part| i32::from(part.y) + i32::from(part.height))
            .max()
            .unwrap_or_default();
        let rows = u16::try_from(bottom - i32::from(top)).map_err(|error| error.to_string())?;
        let offset = u32::try_from(
            usize::try_from(top - self.monitor.y).map_err(|error| error.to_string())?
                * usize::from(self.monitor.width)
                * 4,
        )
        .map_err(|error| error.to_string())?;
        x.shm_get_image(
            screen.root,
            self.monitor.x,
            top,
            self.monitor.width,
            rows,
            u32::MAX,
            ImageFormat::Z_PIXMAP.into(),
            self.segment,
            offset,
        )
        .map_err(problem)?
        .reply()
        .map_err(problem)?;
        Ok(())
    }

    fn pixels(&self) -> &[u8] {
        // SAFETY: the mapping holds `length` bytes for as long as `self` lives. The X server
        // writes into it only while the capture's thread waits in `take`, never while it reads.
        unsafe { std::slice::from_raw_parts(self.map.as_ptr().cast::<u8>(), self.length) }
    }

    fn free(&self, x: &RustConnection) {
        if let Some((pixmap, drawing)) = self.pixmap {
            drop(x.free_gc(drawing));
            drop(x.free_pixmap(pixmap));
        }
        drop(x.shm_detach(self.segment));
    }
}

impl Drop for Copied {
    fn drop(&mut self) {
        // SAFETY: the mapping of `length` bytes that `new` made, which nothing reads any more.
        let _ = unsafe { rustix::mm::munmap(self.map.as_ptr(), self.length) };
    }
}

/// The part of a changed rectangle of the screen that lies on the display.
fn within(changed: Rectangle, monitor: Monitor) -> Option<Rectangle> {
    let left = i32::from(changed.x).max(i32::from(monitor.x));
    let top = i32::from(changed.y).max(i32::from(monitor.y));
    let right = (i32::from(changed.x) + i32::from(changed.width))
        .min(i32::from(monitor.x) + i32::from(monitor.width));
    let bottom = (i32::from(changed.y) + i32::from(changed.height))
        .min(i32::from(monitor.y) + i32::from(monitor.height));
    Some(Rectangle {
        x: i16::try_from(left).ok()?,
        y: i16::try_from(top).ok()?,
        width: u16::try_from(right - left)
            .ok()
            .filter(|width| *width > 0)?,
        height: u16::try_from(bottom - top)
            .ok()
            .filter(|height| *height > 0)?,
    })
}

/// The capture's thread.
struct Taking {
    screen: Screen,
    copied: Copied,
    shared: Arc<Shared>,
    wanted: Wanted,
    resizer: Resizer,
    scaled: Vec<u8>,
    /// The screen changed since the capture last took it.
    damaged: bool,
    /// The share of the shown part that changed since the last encoded picture.
    changed: f64,
    /// A change waits for its turn to be encoded.
    waiting: bool,
    encoded_at: Option<Instant>,
}

impl Taking {
    fn new(screen: Screen, wanted: Wanted, shared: Arc<Shared>) -> Result<Self, String> {
        let copied = Copied::new(&screen, wanted.monitor)?;
        Ok(Self {
            screen,
            copied,
            shared,
            wanted,
            resizer: Resizer::new(),
            scaled: Vec::new(),
            damaged: false,
            changed: 1.0,
            waiting: true,
            encoded_at: None,
        })
    }

    fn run(
        mut self,
        frames: &Frames,
        events: &mpsc::UnboundedSender<Event>,
        mut pointing: Pointing,
    ) {
        pointing.learn(&self.screen.x);
        let failed = loop {
            if self.shared.stopping() {
                break None;
            }
            if self.shared.fresh.swap(false, Ordering::Relaxed)
                && let Err(problem) = self.follow()
            {
                break Some(problem);
            }
            loop {
                match self.screen.x.poll_for_event() {
                    Ok(Some(Happened::DamageNotify(_))) => self.damaged = true,
                    Ok(Some(Happened::XfixesCursorNotify(_))) => pointing.learn(&self.screen.x),
                    Ok(Some(_)) => {}
                    Ok(None) => break,
                    Err(error) => return drop(events.send(Event::Failed(problem(error)))),
                }
            }
            pointing.look(&self.screen.x, self.screen.root, self.wanted.monitor);
            let due = self.due();
            if (self.damaged || self.waiting) && due.is_zero() {
                if self.damaged
                    && let Err(problem) = self.take()
                {
                    break Some(problem);
                }
                if self.waiting {
                    self.encode(frames);
                }
            }
            if self.screen.x.flush().is_err() {
                break Some("the X server went away".to_owned());
            }
            let wait = if self.damaged || self.waiting {
                due.min(POINTER_EVERY)
            } else {
                POINTER_EVERY
            };
            let mut fds = [PollFd::new(self.screen.x.stream(), PollFlags::IN)];
            let timeout = Timespec::try_from(wait).unwrap_or(Timespec {
                tv_sec: 0,
                tv_nsec: 16_000_000,
            });
            let _ = rustix::event::poll(&mut fds, Some(&timeout));
        };
        self.copied.free(&self.screen.x);
        drop(self.screen.x.screensaver_suspend(0));
        drop(self.screen.x.flush());
        if let Some(problem) = failed {
            drop(events.send(Event::Failed(problem)));
        }
    }

    /// Follows what the host wants now, which shows anew: another display takes memory of its
    /// own size.
    fn follow(&mut self) -> Result<(), String> {
        let wanted = self.shared.wanted();
        if wanted.monitor != self.copied.monitor {
            let copied = Copied::new(&self.screen, wanted.monitor)?;
            std::mem::replace(&mut self.copied, copied).free(&self.screen.x);
        }
        self.wanted = wanted;
        self.changed = 1.0;
        self.waiting = true;
        Ok(())
    }

    /// How long until the next picture may be encoded.
    fn due(&self) -> Duration {
        self.encoded_at.map_or(Duration::ZERO, |at| {
            FRAME_INTERVAL.saturating_sub(at.elapsed())
        })
    }

    /// Takes what changed on the screen into the memory, and learns how much of the shown part
    /// it was.
    fn take(&mut self) -> Result<(), String> {
        self.damaged = false;
        let changed = self.screen.damaged()?;
        self.copied.take(&self.screen, &changed)?;
        let shown = shown(&self.wanted);
        let area = |part: &Crop| part.width * part.height;
        let monitor = self.wanted.monitor;
        let share: f64 = changed
            .iter()
            .filter_map(|rectangle| within(*rectangle, monitor))
            .filter_map(|part| {
                overlap(
                    &Crop {
                        left: f64::from(part.x - monitor.x),
                        top: f64::from(part.y - monitor.y),
                        width: f64::from(part.width),
                        height: f64::from(part.height),
                    },
                    &shown,
                )
            })
            .map(|part| area(&part))
            .sum::<f64>()
            / area(&shown).max(1.0);
        if share > 0.0 {
            self.changed = (self.changed + share).min(1.0);
            self.waiting = true;
        }
        Ok(())
    }

    /// Encodes the wanted part of the display at the wanted size. A picture that does not come
    /// out waits for the next turn.
    fn encode(&mut self, frames: &Frames) {
        self.encoded_at = Some(Instant::now());
        let monitor = self.copied.monitor;
        let (width, height) = (u32::from(monitor.width), u32::from(monitor.height));
        let target = (self.wanted.width, self.wanted.height);
        let crop = shown(&self.wanted);
        let region = self.wanted.region.filter(|_| !shows_all(&crop, monitor));
        let pixels = self.copied.pixels();
        let picture = if region.is_none() && target == (width, height) {
            Some(pixels)
        } else {
            scale(
                pixels,
                (width, height),
                crop,
                target,
                &mut self.resizer,
                &mut self.scaled,
            )
        };
        if let Some(bgra) = picture {
            frames.encode(
                Picture::Pixels {
                    bgra,
                    width: usize::try_from(target.0).unwrap_or_default(),
                    height: usize::try_from(target.1).unwrap_or_default(),
                },
                region,
                std::mem::take(&mut self.changed),
            );
            self.waiting = false;
        }
    }
}

/// The part of the display that the host wants, in its pixels, which are its points.
fn shown(wanted: &Wanted) -> Crop {
    let (width, height) = wanted.monitor.size();
    let Some(region) = wanted.region else {
        return Crop {
            left: 0.0,
            top: 0.0,
            width,
            height,
        };
    };
    let left = region.x.clamp(0.0, width - 1.0);
    let top = region.y.clamp(0.0, height - 1.0);
    Crop {
        left,
        top,
        width: region.width.clamp(1.0, width - left),
        height: region.height.clamp(1.0, height - top),
    }
}

fn shows_all(crop: &Crop, monitor: Monitor) -> bool {
    let (width, height) = monitor.size();
    crop.left < WHOLE
        && crop.top < WHOLE
        && (crop.width - width).abs() < WHOLE
        && (crop.height - height).abs() < WHOLE
}

fn overlap(one: &Crop, other: &Crop) -> Option<Crop> {
    let left = one.left.max(other.left);
    let top = one.top.max(other.top);
    let right = (one.left + one.width).min(other.left + other.width);
    let bottom = (one.top + one.height).min(other.top + other.height);
    (left < right && top < bottom).then_some(Crop {
        left,
        top,
        width: right - left,
        height: bottom - top,
    })
}

/// `crop` of the display's pixels at `size`, in rows without padding.
fn scale<'a>(
    pixels: &[u8],
    (across, down): (u32, u32),
    crop: Crop,
    (width, height): (u32, u32),
    resizer: &mut Resizer,
    scaled: &'a mut Vec<u8>,
) -> Option<&'a [u8]> {
    let source = ImageRef::new(across, down, pixels, PixelType::U8x4).ok()?;
    scaled.resize(
        usize::try_from(width).ok()? * usize::try_from(height).ok()? * 4,
        0,
    );
    let mut target = Image::from_slice_u8(width, height, scaled, PixelType::U8x4).ok()?;
    let options = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
        .crop(crop.left, crop.top, crop.width, crop.height)
        .use_alpha(false);
    resizer.resize(&source, &mut target, &options).ok()?;
    Some(scaled)
}

/// What the host knows of the pointer.
#[derive(Clone, Copy, PartialEq)]
enum Known {
    Nothing,
    Where(Option<Pointer>),
}

/// Where the pointer is over the shown display and what it looks like, as the host knows it.
struct Pointing {
    say: mpsc::Sender<Bytes>,
    hand: Arc<Hand>,
    told: Known,
    /// The shapes that the host has.
    shapes: HashSet<u64>,
    /// The pointer's shape now, and its picture until the host has it.
    shape: Option<u64>,
    untold: Option<Shape>,
}

impl Pointing {
    fn new(say: mpsc::Sender<Bytes>, hand: Arc<Hand>) -> Self {
        Self {
            say,
            hand,
            told: Known::Nothing,
            shapes: HashSet::new(),
            shape: None,
            untold: None,
        }
    }

    /// Learns the pointer's shape, which XFIXES tells after each change.
    fn learn(&mut self, x: &RustConnection) {
        let Some(image) = x
            .xfixes_get_cursor_image()
            .ok()
            .and_then(|asked| asked.reply().ok())
        else {
            return;
        };
        let (width, height) = (image.width, image.height);
        if width == 0 || height == 0 || width > LARGEST_POINTER || height > LARGEST_POINTER {
            self.shape = None;
            return;
        }
        // XFIXES gives each pixel as one number, alpha first, with the colors multiplied by it.
        let rgba: Vec<u8> = image
            .cursor_image
            .iter()
            .flat_map(|pixel| {
                let [blue, green, red, alpha] = pixel.to_le_bytes();
                [
                    unpremultiplied(red, alpha),
                    unpremultiplied(green, alpha),
                    unpremultiplied(blue, alpha),
                    alpha,
                ]
            })
            .collect();
        let mut hasher = DefaultHasher::new();
        (&rgba, width, height, image.xhot, image.yhot).hash(&mut hasher);
        let id = hasher.finish();
        self.shape = Some(id);
        self.untold = if self.shapes.contains(&id) {
            None
        } else {
            png(&rgba, u32::from(width), u32::from(height)).map(|png| Shape {
                id,
                png: STANDARD.encode(png),
                x: f64::from(image.xhot),
                y: f64::from(image.yhot),
                width: f64::from(width),
                height: f64::from(height),
            })
        };
    }

    /// Looks where the pointer is, and tells the host when it moved over the display or left it.
    fn look(&mut self, x: &RustConnection, root: Window, monitor: Monitor) {
        if let Some(shape) = self.untold.take() {
            let id = shape.id;
            if self
                .say
                .try_send(link::json(&FromHelper::Shape {
                    shape: shape.clone(),
                }))
                .is_ok()
            {
                self.shapes.insert(id);
            } else {
                self.untold = Some(shape);
            }
        }
        let Some(at) = x
            .query_pointer(root)
            .ok()
            .and_then(|asked| asked.reply().ok())
            .filter(|at| at.same_screen)
        else {
            return;
        };
        self.hand
            .shown((f64::from(at.root_x), f64::from(at.root_y)));
        let (left, top) = monitor.origin();
        let (width, height) = monitor.size();
        let (across, down) = (f64::from(at.root_x) - left, f64::from(at.root_y) - top);
        let pointer = self
            .shape
            .filter(|_| self.untold.is_none())
            .filter(|_| (0.0..width).contains(&across) && (0.0..height).contains(&down))
            .map(|shape| Pointer {
                x: across,
                y: down,
                shape,
            });
        if self.told != Known::Where(pointer)
            && self
                .say
                .try_send(link::json(&FromHelper::Pointer { pointer }))
                .is_ok()
        {
            self.told = Known::Where(pointer);
        }
    }
}
