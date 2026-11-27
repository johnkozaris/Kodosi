//! The shown display's frames: a `PipeWire` stream from the desktop's portal on a thread of its
//! own, cropped and scaled to the size that the host asks for, then encoded. The pointer comes
//! with the frames but apart from them, so that viewers draw it themselves.

use std::{
    cell::RefCell,
    collections::HashSet,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    os::fd::OwnedFd,
    ptr::NonNull,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread::JoinHandle,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use pipewire::{
    self as pw,
    spa::{
        self,
        param::{
            ParamType,
            format::{FormatProperties, MediaSubtype, MediaType},
            video::{VideoFormat, VideoInfoRaw},
        },
        pod::{ChoiceValue, Object, Pod, Property, PropertyFlags, Value, serialize::PodSerializer},
        sys as spa_sys,
        utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes},
    },
    stream::{StreamFlags, StreamRc, StreamState},
    sys as pw_sys,
};
use screen::link::{self, FromHelper, Pointer, Shape};
use stream::{Encoded, Region};
use stream_encode::{Encoder, Output, Picture, Settings};
use tokio::sync::mpsc;

use super::{hand::Hand, session::Event};

/// Frames a second at most, as viewers show them.
const FRAME_RATE: u32 = 60;
/// The largest pointer picture that the helper takes, in pixels.
const LARGEST_POINTER: i32 = 256;
/// The changed rectangles that a frame lists at most.
const DAMAGE_RECTANGLES: i32 = 16;
/// A region this close to the whole display shows all of it.
const WHOLE: f64 = 0.5;

/// What the host asks the capture to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Wanted {
    /// The `PipeWire` node of the display's stream.
    pub(super) node: u32,
    /// Where the display's top left is on the desktop, in points.
    pub(super) origin: (f64, f64),
    /// The display's size in points.
    pub(super) logical: (f64, f64),
    /// The part of the display to capture, in its points, or `None` for all of it.
    pub(super) region: Option<Region>,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// The `PipeWire` thread of one portal session, which shows one display at a time.
pub(super) struct Video {
    orders: pw::channel::Sender<Order>,
    thread: Option<JoinHandle<()>>,
}

enum Order {
    Show(Wanted, Arc<Frames>),
    /// The helper put the pointer here, in the shown display's points, and the desktop did not
    /// show it there: `KWin` at times sends no picture for a move of the pointer alone.
    Point((f64, f64)),
    Hide,
    Quit,
}

impl Video {
    pub(super) fn open(
        remote: OwnedFd,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<Event>,
        hand: Arc<Hand>,
    ) -> Option<Self> {
        let (orders, taken) = pw::channel::channel();
        let thread = std::thread::Builder::new()
            .name("kodosi-screen-video".to_owned())
            .spawn(move || work(remote, taken, &say, &events, &hand))
            .ok()?;
        Some(Self {
            orders,
            thread: Some(thread),
        })
    }

    pub(super) fn show(&self, wanted: Wanted, frames: Arc<Frames>) {
        drop(self.orders.send(Order::Show(wanted, frames)));
    }

    pub(super) fn point(&self, at: (f64, f64)) {
        drop(self.orders.send(Order::Point(at)));
    }

    pub(super) fn hide(&self) {
        drop(self.orders.send(Order::Hide));
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        drop(self.orders.send(Order::Quit));
        if let Some(thread) = self.thread.take() {
            drop(thread.join());
        }
    }
}

fn work(
    remote: OwnedFd,
    taken: pw::channel::Receiver<Order>,
    say: &mpsc::Sender<Bytes>,
    events: &mpsc::UnboundedSender<Event>,
    hand: &Arc<Hand>,
) {
    let failed = |problem: &str| drop(events.send(Event::Failed(problem.to_owned())));
    pw::init();
    let Ok(main) = pw::main_loop::MainLoopRc::new(None) else {
        failed("PipeWire did not start");
        return;
    };
    let Ok(context) = pw::context::ContextRc::new(&main, None) else {
        failed("PipeWire did not start");
        return;
    };
    let Ok(core) = context.connect_fd_rc(remote, None) else {
        failed("PipeWire did not connect");
        return;
    };
    let ended = core
        .add_listener_local()
        .error({
            let (main, events) = (main.clone(), events.clone());
            move |id, _, _, problem| {
                if id == pw::core::PW_ID_CORE {
                    drop(events.send(Event::Failed(problem.to_owned())));
                    main.quit();
                }
            }
        })
        .register();
    let shown: Rc<RefCell<Option<Shown>>> = Rc::default();
    let orders = taken.attach(main.loop_(), {
        let (main, core, shown, say, events, hand) = (
            main.clone(),
            core.clone(),
            Rc::clone(&shown),
            say.clone(),
            events.clone(),
            Arc::clone(hand),
        );
        move |order| match order {
            Order::Show(wanted, frames) => {
                let mut current = shown.borrow_mut();
                if let Some(current) = current.as_mut().filter(|now| now.node == wanted.node) {
                    current.drawing.borrow_mut().want(wanted, frames);
                    return;
                }
                *current = None;
                let pointing = Pointing::new(say.clone(), Arc::clone(&hand));
                match Shown::start(&core, wanted, frames, pointing, events.clone()) {
                    Ok(started) => *current = Some(started),
                    Err(problem) => drop(events.send(Event::Failed(problem))),
                }
            }
            Order::Point(at) => {
                if let Some(current) = shown.borrow().as_ref() {
                    current.drawing.borrow_mut().pointer.place(at);
                }
            }
            Order::Hide => *shown.borrow_mut() = None,
            Order::Quit => {
                *shown.borrow_mut() = None;
                main.quit();
            }
        }
    });
    main.run();
    // Each stream goes before its connection, and each listener before what it listens to.
    shown.borrow_mut().take();
    drop(orders);
    drop(ended);
    drop(core);
}

/// The stream of the shown display.
struct Shown {
    node: u32,
    drawing: Rc<RefCell<Drawing>>,
    _listener: pw::stream::StreamListener<Rc<RefCell<Drawing>>>,
    stream: StreamRc,
}

impl Shown {
    fn start(
        core: &pw::core::CoreRc,
        wanted: Wanted,
        frames: Arc<Frames>,
        pointing: Pointing,
        events: mpsc::UnboundedSender<Event>,
    ) -> Result<Self, String> {
        let stream = StreamRc::new(
            core.clone(),
            "kodosi-screen",
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )
        .map_err(|error| error.to_string())?;
        let drawing = Rc::new(RefCell::new(Drawing::new(wanted, frames, pointing, events)));
        let listener = stream
            .add_local_listener_with_user_data(Rc::clone(&drawing))
            .state_changed(|_, drawing, _, state| {
                if let StreamState::Error(problem) = state {
                    drawing.borrow().failed(&problem);
                }
            })
            .param_changed(|_, drawing, id, param| {
                if id == ParamType::Format.as_raw()
                    && let Some(param) = param
                {
                    drawing.borrow_mut().negotiated(param);
                }
            })
            .remove_buffer(|_, drawing, buffer| drawing.borrow_mut().removed(buffer))
            .process(|stream, drawing| {
                if let Ok(mut drawing) = drawing.try_borrow_mut() {
                    drawing.take(stream);
                }
            })
            .register()
            .map_err(|error| error.to_string())?;
        // The metadata comes with the format, so that the buffers are not made again later and the
        // first frame, which carries the only picture of the pointer until it changes, stays.
        let params: Vec<Vec<u8>> = format().into_iter().chain(metas()).collect();
        let mut params: Vec<&Pod> = params
            .iter()
            .filter_map(|param| Pod::from_bytes(param))
            .collect();
        stream
            .connect(
                spa::utils::Direction::Input,
                Some(wanted.node),
                StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
                &mut params,
            )
            .map_err(|error| error.to_string())?;
        Ok(Self {
            node: wanted.node,
            drawing,
            _listener: listener,
            stream,
        })
    }
}

impl Drop for Shown {
    fn drop(&mut self) {
        if let Some(held) = self.drawing.borrow_mut().held.take() {
            // SAFETY: the stream lent the buffer and has it back once.
            unsafe { self.stream.queue_raw_buffer(held.as_ptr()) };
        }
        drop(self.stream.disconnect());
    }
}

/// What the stream negotiated.
#[derive(Debug, Clone, Copy)]
struct Format {
    width: u32,
    height: u32,
    /// Red comes before blue, which the encoder takes the other way round.
    red_first: bool,
}

/// The frames of one stream, and what the helper draws of them for the encoder.
struct Drawing {
    wanted: Wanted,
    frames: Arc<Frames>,
    events: mpsc::UnboundedSender<Event>,
    format: Option<Format>,
    /// The newest frame, which the stream lends until a newer one comes, so that a new size or
    /// region shows at once, even when the screen does not change.
    held: Option<NonNull<pw_sys::pw_buffer>>,
    resizer: Resizer,
    scaled: Vec<u8>,
    /// The next picture changes all of what viewers see.
    fresh: bool,
    pointer: Pointing,
}

impl Drawing {
    fn new(
        wanted: Wanted,
        frames: Arc<Frames>,
        pointer: Pointing,
        events: mpsc::UnboundedSender<Event>,
    ) -> Self {
        Self {
            wanted,
            frames,
            events,
            format: None,
            held: None,
            resizer: Resizer::new(),
            scaled: Vec::new(),
            fresh: true,
            pointer,
        }
    }

    fn failed(&self, problem: &str) {
        drop(self.events.send(Event::Failed(problem.to_owned())));
    }

    /// Shows the frame in another size or region.
    fn want(&mut self, wanted: Wanted, frames: Arc<Frames>) {
        self.wanted = wanted;
        self.frames = frames;
        self.fresh = true;
        self.draw(1.0);
    }

    fn negotiated(&mut self, param: &Pod) {
        let mut info = VideoInfoRaw::new();
        if info.parse(param).is_err() {
            return;
        }
        let red_first = match info.format() {
            VideoFormat::BGRx | VideoFormat::BGRA => false,
            VideoFormat::RGBx | VideoFormat::RGBA => true,
            _ => {
                self.failed("the display's stream has a format that the helper does not take");
                return;
            }
        };
        let size = info.size();
        self.format = Some(Format {
            width: size.width,
            height: size.height,
            red_first,
        });
        self.fresh = true;
        drop(self.events.send(Event::Sized {
            node: self.wanted.node,
            width: size.width,
            height: size.height,
        }));
    }

    fn removed(&mut self, buffer: *mut pw_sys::pw_buffer) {
        if self.held.is_some_and(|held| held.as_ptr() == buffer) {
            self.held = None;
        }
    }

    /// Takes the frames that came, and draws the newest.
    fn take(&mut self, stream: &pw::stream::Stream) {
        let mut newest = None;
        let mut changed: f64 = 0.0;
        loop {
            // SAFETY: the stream is valid during its callback.
            let buffer = unsafe { pw_sys::pw_stream_dequeue_buffer(stream.as_raw_ptr()) };
            let Some(buffer) = NonNull::new(buffer) else {
                break;
            };
            let scale = self.pixels_per_point();
            // SAFETY: the stream lent the buffer, which stays valid until it goes back.
            let lent = unsafe { Lent::of(buffer) };
            self.pointer.look(&lent, scale, self.wanted.origin);
            if lent.shows_video() {
                changed = (changed + lent.changed(self.format)).min(1.0);
                if let Some(older) = newest.replace(buffer) {
                    // SAFETY: the stream lent the buffer and has it back once.
                    unsafe { stream.queue_raw_buffer(older.as_ptr()) };
                }
            } else {
                // SAFETY: as above.
                unsafe { stream.queue_raw_buffer(buffer.as_ptr()) };
            }
        }
        let Some(newest) = newest else {
            return;
        };
        if let Some(earlier) = self.held.replace(newest) {
            // SAFETY: as above.
            unsafe { stream.queue_raw_buffer(earlier.as_ptr()) };
        }
        self.draw(changed);
    }

    fn pixels_per_point(&self) -> f64 {
        self.format.map_or(1.0, |format| {
            f64::from(format.width) / self.wanted.logical.0.max(1.0)
        })
    }

    /// Encodes the held frame as the host wants it.
    fn draw(&mut self, changed: f64) {
        let (Some(held), Some(format)) = (self.held, self.format) else {
            return;
        };
        // SAFETY: the stream lent the held buffer until the helper gives it back.
        let lent = unsafe { Lent::of(held) };
        let Some((pixels, stride)) = lent.pixels(format) else {
            return;
        };
        let changed = if std::mem::take(&mut self.fresh) {
            1.0
        } else {
            changed
        };
        let (width, height) = (
            usize::try_from(format.width).unwrap_or_default(),
            usize::try_from(format.height).unwrap_or_default(),
        );
        let (crop, region) = crop(&self.wanted, format);
        let target = (self.wanted.width, self.wanted.height);
        if region.is_none() && target == (format.width, format.height) && stride == width * 4 {
            if !format.red_first {
                self.frames.encode(
                    Picture::Pixels {
                        bgra: pixels,
                        width,
                        height,
                    },
                    None,
                    changed,
                );
                return;
            }
            self.scaled.clear();
            self.scaled.extend_from_slice(&pixels[..width * height * 4]);
        } else if !self.scale(pixels, stride, format, crop) {
            return;
        }
        if format.red_first {
            for pixel in self.scaled.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        self.frames.encode(
            Picture::Pixels {
                bgra: &self.scaled,
                width: usize::try_from(target.0).unwrap_or_default(),
                height: usize::try_from(target.1).unwrap_or_default(),
            },
            region,
            changed,
        );
    }

    /// Scales `crop` of the frame into the wanted size.
    fn scale(&mut self, pixels: &[u8], stride: usize, format: Format, crop: Crop) -> bool {
        let Ok(row) = u32::try_from(stride / 4) else {
            return false;
        };
        let Ok(source) = ImageRef::new(row, format.height, pixels, PixelType::U8x4) else {
            return false;
        };
        let (width, height) = (self.wanted.width, self.wanted.height);
        self.scaled.resize(
            usize::try_from(width).unwrap_or_default()
                * usize::try_from(height).unwrap_or_default()
                * 4,
            0,
        );
        let Ok(mut scaled) = Image::from_slice_u8(width, height, &mut self.scaled, PixelType::U8x4)
        else {
            return false;
        };
        let options = ResizeOptions::new()
            .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
            .crop(crop.left, crop.top, crop.width, crop.height)
            .use_alpha(false);
        self.resizer.resize(&source, &mut scaled, &options).is_ok()
    }
}

/// The part of a frame to scale, in its pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Crop {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

/// The pixels of the frame that `wanted` shows, and the region of the display in its points,
/// or `None` for all of it.
fn crop(wanted: &Wanted, format: Format) -> (Crop, Option<Region>) {
    let (width, height) = (f64::from(format.width), f64::from(format.height));
    let whole = Crop {
        left: 0.0,
        top: 0.0,
        width,
        height,
    };
    let Some(region) = wanted.region else {
        return (whole, None);
    };
    let (across, down) = (
        width / wanted.logical.0.max(1.0),
        height / wanted.logical.1.max(1.0),
    );
    let left = (region.x * across).clamp(0.0, width - 1.0);
    let top = (region.y * down).clamp(0.0, height - 1.0);
    let crop = Crop {
        left,
        top,
        width: (region.width * across).clamp(1.0, width - left),
        height: (region.height * down).clamp(1.0, height - top),
    };
    let shows_all = crop.left < WHOLE
        && crop.top < WHOLE
        && (crop.width - width).abs() < WHOLE
        && (crop.height - height).abs() < WHOLE;
    if shows_all {
        (whole, None)
    } else {
        (crop, Some(region))
    }
}

/// A buffer that the stream lent the helper.
struct Lent<'b> {
    buffer: &'b spa_sys::spa_buffer,
}

impl Lent<'_> {
    /// # Safety
    ///
    /// The stream lent `buffer`, and the result goes before the buffer goes back.
    unsafe fn of(buffer: NonNull<pw_sys::pw_buffer>) -> Self {
        // SAFETY: a lent buffer has its SPA buffer, which lives as long as the lending.
        let buffer = unsafe { &*buffer.as_ref().buffer };
        Self { buffer }
    }

    fn data(&self) -> Option<&spa_sys::spa_data> {
        if self.buffer.n_datas == 0 || self.buffer.datas.is_null() {
            return None;
        }
        // SAFETY: the buffer has at least one data.
        Some(unsafe { &*self.buffer.datas })
    }

    fn chunk(&self) -> Option<&spa_sys::spa_chunk> {
        let data = self.data()?;
        // SAFETY: a data's chunk is null or valid.
        unsafe { data.chunk.as_ref() }
    }

    /// A metadata of the buffer and its size in bytes.
    fn meta(&self, kind: u32) -> Option<(NonNull<u8>, usize)> {
        let buffer = std::ptr::from_ref(self.buffer).cast_mut();
        // SAFETY: the buffer is valid, and the lookup only reads it.
        let meta = unsafe { spa_sys::spa_buffer_find_meta(buffer, kind).as_ref() }?;
        let data = NonNull::new(meta.data.cast::<u8>())?;
        Some((data, usize::try_from(meta.size).ok()?))
    }

    /// False for a frame that only moves the pointer, whose picture did not change.
    fn shows_video(&self) -> bool {
        let corrupted = self
            .meta(spa_sys::SPA_META_Header)
            .filter(|(_, size)| *size >= size_of::<spa_sys::spa_meta_header>())
            .is_some_and(|(header, _)| {
                // SAFETY: the metadata holds a header.
                let header = unsafe { read::<spa_sys::spa_meta_header>(header, 0) };
                header.flags & spa_sys::SPA_META_HEADER_FLAG_CORRUPTED != 0
            });
        !corrupted
            && self.chunk().is_some_and(|chunk| {
                chunk.size > 0 && chunk.flags & spa_sys::SPA_CHUNK_FLAG_CORRUPTED.cast_signed() == 0
            })
    }

    /// The bytes of the frame and the length of its rows.
    fn pixels(&self, format: Format) -> Option<(&[u8], usize)> {
        let data = self.data()?;
        let chunk = self.chunk()?;
        let width = usize::try_from(format.width).ok()?;
        let height = usize::try_from(format.height).ok()?;
        let stride = match usize::try_from(chunk.stride) {
            Ok(stride) if stride >= width * 4 => stride,
            _ => width * 4,
        };
        let offset = usize::try_from(chunk.offset).ok()?;
        let length = stride.checked_mul(height)?;
        if data.data.is_null()
            || offset.checked_add(length)? > usize::try_from(data.maxsize).ok()?
        {
            return None;
        }
        // SAFETY: the stream mapped `maxsize` bytes at `data`, which hold the frame.
        let pixels =
            unsafe { std::slice::from_raw_parts(data.data.cast::<u8>().add(offset), length) };
        Some((pixels, stride))
    }

    /// The share of the picture that changed, as far as the stream says.
    fn changed(&self, format: Option<Format>) -> f64 {
        let (Some(format), Some((regions, size))) =
            (format, self.meta(spa_sys::SPA_META_VideoDamage))
        else {
            return 1.0;
        };
        let count = size / size_of::<spa_sys::spa_meta_region>();
        let all = f64::from(format.width) * f64::from(format.height);
        let mut area = 0.0;
        for index in 0..count {
            // SAFETY: the metadata holds `count` regions.
            let region = unsafe {
                read::<spa_sys::spa_meta_region>(
                    regions,
                    index * size_of::<spa_sys::spa_meta_region>(),
                )
            };
            let size = region.region.size;
            if size.width == 0 || size.height == 0 {
                break;
            }
            area = f64::from(size.width).mul_add(f64::from(size.height), area);
        }
        if all > 0.0 {
            (area / all).min(1.0)
        } else {
            1.0
        }
    }

    /// Where the pointer is and its picture when it changed, in the stream's pixels.
    fn cursor(&self) -> Option<Cursor<'_>> {
        let (meta, size) = self.meta(spa_sys::SPA_META_Cursor)?;
        if size < size_of::<spa_sys::spa_meta_cursor>() {
            return None;
        }
        // SAFETY: the metadata holds a cursor.
        let cursor = unsafe { read::<spa_sys::spa_meta_cursor>(meta, 0) };
        if cursor.id == 0 {
            return Some(Cursor {
                at: None,
                picture: None,
            });
        }
        let at = (f64::from(cursor.position.x), f64::from(cursor.position.y));
        let picture = bitmap(meta, size, &cursor).map(|(pixels, format, width, height)| Bitmap {
            pixels,
            format,
            width,
            height,
            hotspot: (f64::from(cursor.hotspot.x), f64::from(cursor.hotspot.y)),
        });
        Some(Cursor {
            at: Some(at),
            picture,
        })
    }
}

/// Copies a structure out of a metadata, whose bytes need not be aligned for it.
///
/// # Safety
///
/// The metadata holds a `T` at `offset`.
unsafe fn read<T>(meta: NonNull<u8>, offset: usize) -> T {
    // SAFETY: as the caller promises.
    unsafe { meta.as_ptr().add(offset).cast::<T>().read_unaligned() }
}

/// A cursor's bitmap, when the metadata holds a whole one.
fn bitmap<'m>(
    meta: NonNull<u8>,
    size: usize,
    cursor: &spa_sys::spa_meta_cursor,
) -> Option<(&'m [u8], u32, u32, u32)> {
    let offset = usize::try_from(cursor.bitmap_offset).ok()?;
    if offset < size_of::<spa_sys::spa_meta_cursor>()
        || offset.checked_add(size_of::<spa_sys::spa_meta_bitmap>())? > size
    {
        return None;
    }
    // SAFETY: the bitmap's header lies within the metadata.
    let bitmap = unsafe { read::<spa_sys::spa_meta_bitmap>(meta, offset) };
    let (width, height) = (bitmap.size.width, bitmap.size.height);
    let stride = usize::try_from(bitmap.stride).ok()?;
    let start = offset.checked_add(usize::try_from(bitmap.offset).ok()?)?;
    let row = usize::try_from(width).ok()?.checked_mul(4)?;
    let length = stride.checked_mul(usize::try_from(height).ok()?)?;
    if width == 0
        || height == 0
        || stride < row
        || start.checked_add(length)? > size
        || i32::try_from(width).ok()? > LARGEST_POINTER
        || i32::try_from(height).ok()? > LARGEST_POINTER
    {
        return None;
    }
    // SAFETY: the bitmap's pixels lie within the metadata.
    let pixels = unsafe { std::slice::from_raw_parts(meta.as_ptr().add(start), length) };
    Some((pixels, bitmap.format, width, height))
}

struct Cursor<'b> {
    at: Option<(f64, f64)>,
    picture: Option<Bitmap<'b>>,
}

struct Bitmap<'b> {
    pixels: &'b [u8],
    format: u32,
    width: u32,
    height: u32,
    hotspot: (f64, f64),
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
    shape: Option<u64>,
    /// A shape that the host does not have yet. The desktop sends each picture of the pointer
    /// once, so it waits for room on the link rather than going.
    unsent: Option<Shape>,
}

impl Pointing {
    fn new(say: mpsc::Sender<Bytes>, hand: Arc<Hand>) -> Self {
        Self {
            say,
            hand,
            told: Known::Nothing,
            shapes: HashSet::new(),
            shape: None,
            unsent: None,
        }
    }

    fn look(&mut self, lent: &Lent<'_>, scale: f64, origin: (f64, f64)) {
        let Some(cursor) = lent.cursor() else {
            return;
        };
        if let Some((x, y)) = cursor.at {
            self.hand
                .shown((origin.0 + x / scale, origin.1 + y / scale));
        }
        if let Some(picture) = cursor.picture {
            self.learn(&picture, scale);
        }
        if let Some(shape) = &self.unsent {
            let id = shape.id;
            let message = link::json(&FromHelper::Shape {
                shape: shape.clone(),
            });
            if self.say.try_send(message).is_err() {
                return;
            }
            self.shapes.insert(id);
            self.unsent = None;
        }
        let pointer = match (cursor.at, self.shape) {
            (Some((x, y)), Some(shape)) => Some(Pointer {
                x: x / scale,
                y: y / scale,
                shape,
            }),
            (Some(_), None) => return,
            (None, _) => None,
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

    /// Tells the host where the pointer is, in the display's points, in its last shape.
    fn place(&mut self, at: (f64, f64)) {
        let Some(shape) = self.shape else {
            return;
        };
        let pointer = Some(Pointer {
            x: at.0,
            y: at.1,
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

    /// Names a pointer picture by its pixels and hotspot, for the host to have it once.
    fn learn(&mut self, picture: &Bitmap<'_>, scale: f64) {
        let Some(rgba) = straight(picture) else {
            return;
        };
        let mut hasher = DefaultHasher::new();
        rgba.hash(&mut hasher);
        picture.width.hash(&mut hasher);
        picture.hotspot.0.to_bits().hash(&mut hasher);
        picture.hotspot.1.to_bits().hash(&mut hasher);
        let id = hasher.finish();
        if !self.shapes.contains(&id) {
            let Some(png) = png(&rgba, picture.width, picture.height) else {
                return;
            };
            self.unsent = Some(Shape {
                id,
                png: STANDARD.encode(png),
                x: picture.hotspot.0 / scale,
                y: picture.hotspot.1 / scale,
                width: f64::from(picture.width) / scale,
                height: f64::from(picture.height) / scale,
            });
        }
        self.shape = Some(id);
    }
}

/// The pointer's pixels as rows of straight red, green, blue and alpha.
fn straight(picture: &Bitmap<'_>) -> Option<Vec<u8>> {
    let order: [usize; 4] = match picture.format {
        spa_sys::SPA_VIDEO_FORMAT_RGBA => [0, 1, 2, 3],
        spa_sys::SPA_VIDEO_FORMAT_BGRA => [2, 1, 0, 3],
        spa_sys::SPA_VIDEO_FORMAT_ARGB => [1, 2, 3, 0],
        spa_sys::SPA_VIDEO_FORMAT_ABGR => [3, 2, 1, 0],
        _ => return None,
    };
    let width = usize::try_from(picture.width).ok()?;
    let height = usize::try_from(picture.height).ok()?;
    let stride = picture.pixels.len() / height;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in picture.pixels.chunks_exact(stride).take(height) {
        for pixel in row[..width * 4].as_chunks::<4>().0 {
            let alpha = pixel[order[3]];
            for channel in &order[..3] {
                rgba.push(unpremultiplied(pixel[*channel], alpha));
            }
            rgba.push(alpha);
        }
    }
    Some(rgba)
}

/// Desktops hand pointer pictures over with their colors multiplied by their alpha.
fn unpremultiplied(color: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    let straight = u16::from(color) * 255 / u16::from(alpha);
    u8::try_from(straight).unwrap_or(u8::MAX)
}

fn png(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    writer.finish().ok()?;
    Some(bytes)
}

/// The format that the helper takes: four bytes a pixel, at most as many frames as viewers
/// show.
fn format() -> Option<Vec<u8>> {
    let object = spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ),
        spa::pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle {
                width: 1,
                height: 1
            },
            Rectangle {
                width: 16384,
                height: 16384
            }
        ),
        spa::pod::property!(
            FormatProperties::VideoFramerate,
            Fraction,
            Fraction { num: 0, denom: 1 }
        ),
        spa::pod::property!(
            FormatProperties::VideoMaxFramerate,
            Choice,
            Range,
            Fraction,
            Fraction {
                num: FRAME_RATE,
                denom: 1
            },
            Fraction { num: 1, denom: 1 },
            Fraction {
                num: FRAME_RATE,
                denom: 1
            }
        ),
    );
    serialize(object)
}

/// The metadata that the helper reads: the header, the changed rectangles and the pointer.
fn metas() -> Vec<Vec<u8>> {
    let cursor = |side: i32| {
        i32::try_from(size_of::<spa_sys::spa_meta_cursor>() + size_of::<spa_sys::spa_meta_bitmap>())
            .unwrap_or_default()
            + side * side * 4
    };
    let region = i32::try_from(size_of::<spa_sys::spa_meta_region>()).unwrap_or_default();
    let header = i32::try_from(size_of::<spa_sys::spa_meta_header>()).unwrap_or_default();
    [
        (spa_sys::SPA_META_Header, Value::Int(header)),
        (
            spa_sys::SPA_META_VideoDamage,
            range(
                region * DAMAGE_RECTANGLES,
                region,
                region * DAMAGE_RECTANGLES,
            ),
        ),
        (
            spa_sys::SPA_META_Cursor,
            range(cursor(64), cursor(1), cursor(LARGEST_POINTER)),
        ),
    ]
    .into_iter()
    .filter_map(|(kind, size)| {
        serialize(Object {
            type_: spa_sys::SPA_TYPE_OBJECT_ParamMeta,
            id: spa_sys::SPA_PARAM_Meta,
            properties: vec![
                Property {
                    key: spa_sys::SPA_PARAM_META_type,
                    flags: PropertyFlags::empty(),
                    value: Value::Id(Id(kind)),
                },
                Property {
                    key: spa_sys::SPA_PARAM_META_size,
                    flags: PropertyFlags::empty(),
                    value: size,
                },
            ],
        })
    })
    .collect()
}

const fn range(default: i32, min: i32, max: i32) -> Value {
    Value::Choice(ChoiceValue::Int(Choice(
        ChoiceFlags::empty(),
        ChoiceEnum::Range { default, min, max },
    )))
}

fn serialize(object: Object) -> Option<Vec<u8>> {
    PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &Value::Object(object))
        .ok()
        .map(|(bytes, _)| bytes.into_inner())
}

/// The encoder, which the capture's thread feeds and the link steers.
pub(super) struct Frames {
    encoding: Mutex<Encoding>,
}

struct Encoding {
    encoder: Encoder,
    settings: Settings,
}

impl Frames {
    pub(super) fn new(
        settings: Settings,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<Event>,
    ) -> Arc<Self> {
        Arc::new(Self {
            encoding: Mutex::new(Encoding {
                encoder: Encoder::new(settings, Arc::new(Sent { say, events })),
                settings,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Encoding> {
        self.encoding.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn encode_as(&self, settings: Settings) {
        let mut encoding = self.lock();
        if settings.format != encoding.settings.format {
            encoding.encoder.set_format(settings.format);
        }
        if settings.bitrate != encoding.settings.bitrate {
            encoding.encoder.set_bitrate(settings.bitrate);
        }
        if settings.pace != encoding.settings.pace {
            encoding.encoder.set_pace(settings.pace);
        }
        encoding.settings = settings;
    }

    pub(super) fn encoder(&self, work: impl FnOnce(&mut Encoder)) {
        work(&mut self.lock().encoder);
    }

    fn encode(&self, picture: Picture<'_>, region: Option<Region>, changed: f64) {
        self.lock().encoder.encode(picture, region, changed);
    }
}

/// Sends encoded frames to the host. A full link drops a frame, which the encoder repairs.
struct Sent {
    say: mpsc::Sender<Bytes>,
    events: mpsc::UnboundedSender<Event>,
}

impl Output for Sent {
    fn frame(&self, frame: Encoded) -> bool {
        self.say.try_send(link::frame(&frame)).is_ok()
    }

    fn failed(&self, problem: String) {
        drop(self.events.send(Event::Failed(problem)));
    }
}
