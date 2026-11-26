//! The desktop duplication of one display on a thread of its own, whose frames go to the encoder
//! as they come, at most as often as viewers show them.

use std::{
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use bytes::Bytes;
use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use screen::link::{self, Source};
use stream::{Encoded, Region};
use stream_encode::{Encoder, Output, Picture, Settings};
use tokio::sync::{mpsc, oneshot};
use windows::{
    Win32::{
        Foundation::{E_ACCESSDENIED, HMODULE, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::{
                D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
                ID3D11DeviceContext, ID3D11Texture2D,
            },
            Dxgi::{
                Common::{
                    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_MODE_ROTATION, DXGI_MODE_ROTATION_ROTATE90,
                    DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270, DXGI_SAMPLE_DESC,
                },
                DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
                DXGI_ERROR_SESSION_DISCONNECTED, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
                DXGI_OUTDUPL_MOVE_RECT, IDXGIOutput5, IDXGIOutputDuplication, IDXGIResource,
            },
        },
    },
    core::Interface as _,
};

use super::{
    pointer::Pointing,
    session::Event,
    sources::{self, Screen},
};

/// The shortest time between two encoded pictures: viewers show 60 a second.
const FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// How long the capture waits for the desktop to change before it looks at what is wanted.
const WAIT: u32 = 100;
/// How long the capture waits before it duplicates the desktop again after Windows took it away.
const AGAIN: Duration = Duration::from_millis(250);
/// How long Windows may take the desktop away while it shows the same one, as for a new display
/// mode, before the capture fails.
const LOST_LIMIT: Duration = Duration::from_secs(5);

/// What the host asks a capture to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Wanted {
    pub(super) source: Source,
    pub(super) region: Option<Region>,
    pub(super) width: u32,
    pub(super) height: u32,
}

/// What the session and the capture's thread share.
struct Shared {
    wanted: Mutex<Wanted>,
    /// What is wanted changed, so the picture shows anew.
    changed: AtomicBool,
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
    pub(super) source: Source,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Capture {
    pub(super) async fn start(
        screen: Screen,
        wanted: Wanted,
        frames: Arc<Frames>,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<Event>,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            wanted: Mutex::new(wanted),
            changed: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
        });
        let (ready, started) = oneshot::channel();
        let taking = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("kodosi-screen-capture".to_owned())
            .spawn(move || match Duplicated::new(&screen.device) {
                Ok(duplicated) => {
                    drop(ready.send(Ok(())));
                    Taking::new(duplicated, taking, frames, say, events).run();
                }
                Err(problem) => drop(ready.send(Err(problem))),
            })
            .map_err(|error| error.to_string())?;
        started
            .await
            .unwrap_or_else(|_| Err("the capture did not start".to_owned()))?;
        Ok(Self {
            source: wanted.source,
            shared,
            thread: Some(thread),
        })
    }

    /// Changes the size or the region of the same display.
    pub(super) fn update(&self, wanted: Wanted) {
        *self
            .shared
            .wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = wanted;
        self.shared.changed.store(true, Ordering::Relaxed);
    }

    /// Returns once no more frames come.
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

/// A display's desktop as DXGI duplicates it, and the device that copies its pictures.
struct Duplicated {
    /// Windows' name of the display.
    name: String,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    output: IDXGIOutput5,
    duplication: Option<IDXGIOutputDuplication>,
    /// How the display turns the desktop; DXGI's pictures come unturned.
    rotation: DXGI_MODE_ROTATION,
    /// The desktop's size as it shows, in pixels.
    width: u32,
    height: u32,
    scale: f64,
}

impl Duplicated {
    fn new(name: &str) -> Result<Self, String> {
        let (adapter, output) = sources::outputs()
            .into_iter()
            .find(|(_, output)| {
                sources::describe(output)
                    .is_some_and(|described| sources::device(&described) == name)
            })
            .ok_or("the display is gone")?;
        let (mut device, mut context) = (None, None);
        // SAFETY: both pointers are valid to write.
        unsafe {
            D3D11CreateDevice(
                &adapter,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&raw mut device),
                None,
                Some(&raw mut context),
            )
        }
        .map_err(|error| error.message())?;
        let (Some(device), Some(context)) = (device, context) else {
            return Err("Windows made no graphics device".to_owned());
        };
        let mut duplicated = Self {
            name: name.to_owned(),
            device,
            context,
            output: output.cast().map_err(|error| error.message())?,
            duplication: None,
            rotation: DXGI_MODE_ROTATION::default(),
            width: 0,
            height: 0,
            scale: 1.0,
        };
        duplicated.duplicate().map_err(|error| error.message())?;
        Ok(duplicated)
    }

    /// Duplicates the desktop anew, which comes whole in the first frame.
    fn duplicate(&mut self) -> windows::core::Result<()> {
        self.duplication = None;
        // SAFETY: the device outlives the duplication, which is dropped first.
        let duplication = unsafe {
            self.output
                .DuplicateOutput1(&self.device, 0, &[DXGI_FORMAT_B8G8R8A8_UNORM])
        }?;
        // SAFETY: getters.
        let (described, output) = unsafe { (duplication.GetDesc(), self.output.GetDesc()?) };
        let place = output.DesktopCoordinates;
        self.rotation = described.Rotation;
        self.width = u32::try_from(place.right - place.left).unwrap_or_default();
        self.height = u32::try_from(place.bottom - place.top).unwrap_or_default();
        self.scale = sources::scale(output.Monitor);
        self.duplication = Some(duplication);
        Ok(())
    }

    /// The next picture of the desktop, or `None` when it did not change for `wait` milliseconds.
    fn next(
        &self,
        wait: u32,
    ) -> windows::core::Result<Option<(DXGI_OUTDUPL_FRAME_INFO, IDXGIResource)>> {
        let Some(duplication) = &self.duplication else {
            return Err(DXGI_ERROR_ACCESS_LOST.into());
        };
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource = None;
        // SAFETY: both pointers are valid to write.
        match unsafe { duplication.AcquireNextFrame(wait, &raw mut info, &raw mut resource) } {
            Ok(()) => {
                if resource.is_none() {
                    self.release();
                }
                Ok(resource.map(|resource| (info, resource)))
            }
            Err(error) if error.code() == DXGI_ERROR_WAIT_TIMEOUT => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn release(&self) {
        if let Some(duplication) = &self.duplication {
            // SAFETY: the helper holds a frame, which it no longer reads.
            drop(unsafe { duplication.ReleaseFrame() });
        }
    }

    /// The desktop's rectangle `shown` in the pixels of DXGI's unturned pictures.
    fn unturned(&self, shown: RECT) -> RECT {
        let (width, height) = (signed(self.width), signed(self.height));
        match self.rotation {
            DXGI_MODE_ROTATION_ROTATE90 => RECT {
                left: shown.top,
                top: width - shown.right,
                right: shown.bottom,
                bottom: width - shown.left,
            },
            DXGI_MODE_ROTATION_ROTATE180 => RECT {
                left: width - shown.right,
                top: height - shown.bottom,
                right: width - shown.left,
                bottom: height - shown.top,
            },
            DXGI_MODE_ROTATION_ROTATE270 => RECT {
                left: height - shown.bottom,
                top: shown.left,
                right: height - shown.top,
                bottom: shown.right,
            },
            _ => shown,
        }
    }

    /// The rectangle `taken` of DXGI's unturned pictures as the desktop shows it.
    fn turned(&self, taken: RECT) -> RECT {
        let (width, height) = (signed(self.width), signed(self.height));
        match self.rotation {
            DXGI_MODE_ROTATION_ROTATE90 => RECT {
                left: width - taken.bottom,
                top: taken.left,
                right: width - taken.top,
                bottom: taken.right,
            },
            DXGI_MODE_ROTATION_ROTATE180 => RECT {
                left: width - taken.right,
                top: height - taken.bottom,
                right: width - taken.left,
                bottom: height - taken.top,
            },
            DXGI_MODE_ROTATION_ROTATE270 => RECT {
                left: taken.top,
                top: height - taken.right,
                right: taken.bottom,
                bottom: height - taken.left,
            },
            _ => taken,
        }
    }
}

/// The capture's thread.
struct Taking {
    duplicated: Duplicated,
    shared: Arc<Shared>,
    frames: Arc<Frames>,
    events: mpsc::UnboundedSender<Event>,
    pointing: Pointing,
    /// The desktop's last picture, which a new size or region shows again without a change on
    /// the desktop.
    desktop: Option<ID3D11Texture2D>,
    /// Where the shown part goes from the graphics card to the processor, and its size.
    staging: Option<(ID3D11Texture2D, u32, u32)>,
    resizer: Resizer,
    turned: Vec<u8>,
    scaled: Vec<u8>,
    moved: Vec<DXGI_OUTDUPL_MOVE_RECT>,
    dirty: Vec<RECT>,
    /// The share of the shown part that changed since the last encoded picture.
    changed: f64,
    /// A change waits for its turn to be encoded.
    waiting: bool,
    encoded_at: Option<Instant>,
}

impl Taking {
    fn new(
        duplicated: Duplicated,
        shared: Arc<Shared>,
        frames: Arc<Frames>,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<Event>,
    ) -> Self {
        Self {
            duplicated,
            shared,
            frames,
            events,
            pointing: Pointing::new(say),
            desktop: None,
            staging: None,
            resizer: Resizer::new(),
            turned: Vec::new(),
            scaled: Vec::new(),
            moved: Vec::new(),
            dirty: Vec::new(),
            changed: 1.0,
            waiting: true,
            encoded_at: None,
        }
    }

    fn run(mut self) {
        let mut lost_at = None;
        while !self.shared.stopping() {
            if self.shared.changed.swap(false, Ordering::Relaxed) {
                self.changed = 1.0;
                self.waiting = true;
            }
            match self.duplicated.next(self.wait()) {
                Ok(Some((info, resource))) => {
                    lost_at = None;
                    self.took(&info, resource);
                    self.duplicated.release();
                }
                Ok(None) => {}
                Err(error) => {
                    if !self.recover(&error, &mut lost_at) {
                        return;
                    }
                    continue;
                }
            }
            if self.waiting && self.wait() == 0 {
                self.encode();
            }
        }
    }

    /// Milliseconds until the next picture may be encoded while a change waits, or until the
    /// capture looks again.
    fn wait(&self) -> u32 {
        if !self.waiting || self.desktop.is_none() {
            return WAIT;
        }
        self.encoded_at.map_or(0, |at| {
            let left = FRAME_INTERVAL.saturating_sub(at.elapsed());
            u32::try_from(left.as_millis()).unwrap_or(WAIT).min(WAIT)
        })
    }

    fn took(&mut self, info: &DXGI_OUTDUPL_FRAME_INFO, resource: IDXGIResource) {
        if info.LastPresentTime != 0 {
            let copied = resource
                .cast::<ID3D11Texture2D>()
                .is_ok_and(|picture| self.copy(&picture));
            if copied {
                let changed = self.dirty(info);
                self.changed = (self.changed + changed).min(1.0);
                self.waiting = true;
            }
        }
        drop(resource);
        if info.LastMouseUpdateTime != 0
            && let Some(duplication) = &self.duplicated.duplication
        {
            self.pointing
                .update(duplication, info, self.duplicated.scale);
        }
    }

    /// Keeps the desktop's picture, so that DXGI can draw the next one.
    fn copy(&mut self, picture: &ID3D11Texture2D) -> bool {
        let mut described = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: `described` is valid to write.
        unsafe { picture.GetDesc(&raw mut described) };
        let fits = self.desktop.as_ref().is_some_and(|desktop| {
            let mut kept = D3D11_TEXTURE2D_DESC::default();
            // SAFETY: `kept` is valid to write.
            unsafe { desktop.GetDesc(&raw mut kept) };
            (kept.Width, kept.Height, kept.Format)
                == (described.Width, described.Height, described.Format)
        });
        if !fits {
            self.desktop = texture(
                &self.duplicated.device,
                D3D11_TEXTURE2D_DESC {
                    Width: described.Width,
                    Height: described.Height,
                    Format: described.Format,
                    Usage: D3D11_USAGE_DEFAULT,
                    ..plain()
                },
            );
            self.changed = 1.0;
        }
        let Some(desktop) = &self.desktop else {
            return false;
        };
        // SAFETY: both textures have the same size and format.
        unsafe { self.duplicated.context.CopyResource(desktop, picture) };
        true
    }

    /// The share of the shown part that a frame changed, from the rectangles that DXGI says it
    /// moved and drew.
    fn dirty(&mut self, info: &DXGI_OUTDUPL_FRAME_INFO) -> f64 {
        let Some(duplication) = &self.duplicated.duplication else {
            return 1.0;
        };
        let size = info.TotalMetadataBufferSize;
        if size == 0 {
            return 1.0;
        }
        let bytes = usize::try_from(size).unwrap_or_default();
        self.moved.resize(
            bytes / size_of::<DXGI_OUTDUPL_MOVE_RECT>() + 1,
            DXGI_OUTDUPL_MOVE_RECT::default(),
        );
        self.dirty
            .resize(bytes / size_of::<RECT>() + 1, RECT::default());
        let (mut moved, mut dirty) = (0, 0);
        // SAFETY: each buffer holds at least `size` bytes, and the counts are valid to write.
        let read = unsafe {
            duplication
                .GetFrameMoveRects(size, self.moved.as_mut_ptr(), &raw mut moved)
                .and_then(|()| {
                    duplication.GetFrameDirtyRects(size, self.dirty.as_mut_ptr(), &raw mut dirty)
                })
        };
        if read.is_err() {
            return 1.0;
        }
        let moved =
            usize::try_from(moved).unwrap_or_default() / size_of::<DXGI_OUTDUPL_MOVE_RECT>();
        let dirty = usize::try_from(dirty).unwrap_or_default() / size_of::<RECT>();
        let shown = Crop::of(&self.shared.wanted(), &self.duplicated).bounds(&self.duplicated);
        let area =
            |rect: RECT| f64::from(rect.right - rect.left) * f64::from(rect.bottom - rect.top);
        let changed: f64 = self.moved[..moved.min(self.moved.len())]
            .iter()
            .map(|moved| moved.DestinationRect)
            .chain(self.dirty[..dirty.min(self.dirty.len())].iter().copied())
            .filter_map(|taken| intersection(self.duplicated.turned(taken), shown))
            .map(area)
            .sum();
        (changed / area(shown).max(1.0)).min(1.0)
    }

    /// Encodes the wanted part of the desktop's last picture at the wanted size. A picture that
    /// does not come out waits for the next turn.
    fn encode(&mut self) {
        self.encoded_at = Some(Instant::now());
        let wanted = self.shared.wanted();
        let crop = Crop::of(&wanted, &self.duplicated);
        let held = crop.bounds(&self.duplicated);
        let taken = self.duplicated.unturned(held);
        let (Ok(across), Ok(down)) = (
            u32::try_from(taken.right - taken.left),
            u32::try_from(taken.bottom - taken.top),
        ) else {
            return;
        };
        let (Some(desktop), Some(staging)) = (self.desktop.clone(), self.staging(across, down))
        else {
            return;
        };
        let context = &self.duplicated.context;
        let copied = D3D11_BOX {
            left: unsigned(taken.left),
            top: unsigned(taken.top),
            front: 0,
            right: unsigned(taken.right),
            bottom: unsigned(taken.bottom),
            back: 1,
        };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the box lies within the desktop's picture and fits the staging texture, which
        // the processor may read.
        let read = unsafe {
            context.CopySubresourceRegion(
                &staging,
                0,
                0,
                0,
                0,
                &desktop,
                0,
                Some(&raw const copied),
            );
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped))
        };
        if read.is_err() {
            return;
        }
        let pitch = usize::try_from(mapped.RowPitch).unwrap_or_default();
        let rows = usize::try_from(down).unwrap_or_default();
        // SAFETY: the mapped texture holds `rows` rows of `pitch` bytes until it is unmapped.
        let pixels = unsafe { std::slice::from_raw_parts(mapped.pData.cast::<u8>(), pitch * rows) };
        let shows = Shows {
            pixels,
            pitch,
            width: across,
            height: down,
        };
        let shows = if matches!(
            self.duplicated.rotation,
            DXGI_MODE_ROTATION_ROTATE90
                | DXGI_MODE_ROTATION_ROTATE180
                | DXGI_MODE_ROTATION_ROTATE270
        ) {
            turn(&shows, self.duplicated.rotation, &mut self.turned)
        } else {
            shows
        };
        let inside = crop.within(held);
        let picture = scale(
            &shows,
            inside,
            (wanted.width, wanted.height),
            &mut self.resizer,
            &mut self.scaled,
        );
        if let Some(bgra) = picture {
            self.frames.encode(
                Picture::Pixels {
                    bgra,
                    width: usize::try_from(wanted.width).unwrap_or_default(),
                    height: usize::try_from(wanted.height).unwrap_or_default(),
                },
                wanted.region,
                std::mem::take(&mut self.changed),
            );
            self.waiting = false;
        }
        // SAFETY: the texture was mapped above and nothing reads it any longer.
        unsafe { context.Unmap(&staging, 0) };
    }

    fn staging(&mut self, width: u32, height: u32) -> Option<ID3D11Texture2D> {
        if let Some((staging, across, down)) = &self.staging
            && (*across, *down) == (width, height)
        {
            return Some(staging.clone());
        }
        let staging = texture(
            &self.duplicated.device,
            D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Usage: D3D11_USAGE_STAGING,
                CPUAccessFlags: u32::try_from(D3D11_CPU_ACCESS_READ.0).unwrap_or_default(),
                ..plain()
            },
        )?;
        self.staging = Some((staging.clone(), width, height));
        Some(staging)
    }

    /// Duplicates the desktop again after Windows took it away: for another desktop, such as the
    /// lock screen or an administrator's prompt, a new display mode or arrangement, a program that
    /// took the whole display, or a graphics driver that started again. Returns false once the
    /// capture ends.
    fn recover(&mut self, error: &windows::core::Error, lost_at: &mut Option<Instant>) -> bool {
        if ![
            DXGI_ERROR_ACCESS_LOST,
            DXGI_ERROR_DEVICE_REMOVED,
            DXGI_ERROR_DEVICE_RESET,
        ]
        .contains(&error.code())
        {
            self.fail(error);
            return false;
        }
        let since = *lost_at.get_or_insert_with(Instant::now);
        let mut told = false;
        while !self.shared.stopping() {
            match self.duplicate_again() {
                Ok(()) => {
                    self.desktop = None;
                    self.changed = 1.0;
                    self.waiting = true;
                    return true;
                }
                // Another desktop or account shows, which the session tells the host.
                Err(error) if denied(&error) => {
                    if !std::mem::replace(&mut told, true) {
                        drop(self.events.send(Event::Changed));
                    }
                }
                Err(error) if since.elapsed() >= LOST_LIMIT => {
                    self.fail(&error);
                    return false;
                }
                Err(_) => {}
            }
            std::thread::sleep(AGAIN);
        }
        false
    }

    /// Duplicates the display again, through a new device when its output or its device is gone,
    /// as after the displays changed or the driver started again.
    fn duplicate_again(&mut self) -> windows::core::Result<()> {
        let error = match self.duplicated.duplicate() {
            Ok(()) => return Ok(()),
            Err(error) if denied(&error) => return Err(error),
            Err(error) => error,
        };
        let fresh = Duplicated::new(&self.duplicated.name).map_err(|_| error)?;
        self.duplicated = fresh;
        self.staging = None;
        Ok(())
    }

    fn fail(&self, error: &windows::core::Error) {
        drop(self.events.send(Event::Failed(error.message())));
    }
}

/// The part of the desktop that a capture shows, in its pixels.
#[derive(Debug, Clone, Copy)]
struct Crop {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

impl Crop {
    fn of(wanted: &Wanted, duplicated: &Duplicated) -> Self {
        let (width, height) = (
            f64::from(duplicated.width).max(1.0),
            f64::from(duplicated.height).max(1.0),
        );
        let Some(region) = wanted.region else {
            return Self {
                left: 0.0,
                top: 0.0,
                width,
                height,
            };
        };
        let scale = duplicated.scale;
        let left = (region.x * scale).clamp(0.0, width - 1.0);
        let top = (region.y * scale).clamp(0.0, height - 1.0);
        Self {
            left,
            top,
            width: (region.width * scale).clamp(1.0, width - left),
            height: (region.height * scale).clamp(1.0, height - top),
        }
    }

    /// The whole pixels of the desktop that hold the part.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the part lies on a display, whose pixels fit an i32"
    )]
    fn bounds(self, duplicated: &Duplicated) -> RECT {
        let left = self.left.floor() as i32;
        let top = self.top.floor() as i32;
        let right = ((self.left + self.width).ceil() as i32).min(signed(duplicated.width));
        let bottom = ((self.top + self.height).ceil() as i32).min(signed(duplicated.height));
        RECT {
            left,
            top,
            right: right.max(left + 1),
            bottom: bottom.max(top + 1),
        }
    }

    /// The part within `bounds`, which start at its corner.
    fn within(self, bounds: RECT) -> Self {
        Self {
            left: self.left - f64::from(bounds.left),
            top: self.top - f64::from(bounds.top),
            ..self
        }
    }
}

/// Rows of BGRA pixels.
struct Shows<'a> {
    pixels: &'a [u8],
    pitch: usize,
    width: u32,
    height: u32,
}

/// Turns DXGI's unturned pixels the way the display shows them.
fn turn<'a>(shows: &Shows<'_>, rotation: DXGI_MODE_ROTATION, turned: &'a mut Vec<u8>) -> Shows<'a> {
    let (across, down) = (
        usize::try_from(shows.width).unwrap_or_default(),
        usize::try_from(shows.height).unwrap_or_default(),
    );
    let sideways = rotation != DXGI_MODE_ROTATION_ROTATE180;
    let (width, height) = if sideways {
        (down, across)
    } else {
        (across, down)
    };
    turned.resize(width * height * 4, 0);
    for row in 0..height {
        for column in 0..width {
            let (x, y) = match rotation {
                DXGI_MODE_ROTATION_ROTATE90 => (row, down - 1 - column),
                DXGI_MODE_ROTATION_ROTATE270 => (across - 1 - row, column),
                _ => (across - 1 - column, down - 1 - row),
            };
            let from = y * shows.pitch + x * 4;
            let to = (row * width + column) * 4;
            turned[to..to + 4].copy_from_slice(&shows.pixels[from..from + 4]);
        }
    }
    Shows {
        pixels: turned,
        pitch: width * 4,
        width: u32::try_from(width).unwrap_or_default(),
        height: u32::try_from(height).unwrap_or_default(),
    }
}

/// The part `inside` of the pixels at `size`, in rows without padding.
fn scale<'a>(
    shows: &'a Shows<'_>,
    inside: Crop,
    size: (u32, u32),
    resizer: &mut Resizer,
    scaled: &'a mut Vec<u8>,
) -> Option<&'a [u8]> {
    let (width, height) = size;
    let row = usize::try_from(width).ok()? * 4;
    let rows = usize::try_from(height).ok()?;
    let whole = inside.left.abs() < f64::EPSILON
        && inside.top.abs() < f64::EPSILON
        && (inside.width - f64::from(shows.width)).abs() < f64::EPSILON
        && (inside.height - f64::from(shows.height)).abs() < f64::EPSILON;
    if whole && (shows.width, shows.height) == size {
        if shows.pitch == row {
            return shows.pixels.get(..row * rows);
        }
        scaled.clear();
        for line in shows.pixels.chunks(shows.pitch).take(rows) {
            scaled.extend_from_slice(line.get(..row)?);
        }
        return Some(scaled);
    }
    let source = ImageRef::new(
        u32::try_from(shows.pitch / 4).ok()?,
        shows.height,
        shows.pixels,
        PixelType::U8x4,
    )
    .ok()?;
    scaled.resize(row * rows, 0);
    let mut target = Image::from_slice_u8(width, height, scaled, PixelType::U8x4).ok()?;
    let options = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
        .crop(inside.left, inside.top, inside.width, inside.height)
        .use_alpha(false);
    resizer.resize(&source, &mut target, &options).ok()?;
    Some(scaled)
}

/// Windows shows a desktop that only it may see, or another account's session.
fn denied(error: &windows::core::Error) -> bool {
    error.code() == E_ACCESSDENIED || error.code() == DXGI_ERROR_SESSION_DISCONNECTED
}

fn intersection(one: RECT, other: RECT) -> Option<RECT> {
    let both = RECT {
        left: one.left.max(other.left),
        top: one.top.max(other.top),
        right: one.right.min(other.right),
        bottom: one.bottom.min(other.bottom),
    };
    (both.left < both.right && both.top < both.bottom).then_some(both)
}

fn texture(device: &ID3D11Device, described: D3D11_TEXTURE2D_DESC) -> Option<ID3D11Texture2D> {
    let mut texture = None;
    // SAFETY: the description is complete and `texture` is valid to write.
    unsafe { device.CreateTexture2D(&raw const described, None, Some(&raw mut texture)) }.ok()?;
    texture
}

/// A texture of one picture that only copies use.
fn plain() -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        MipLevels: 1,
        ArraySize: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        ..D3D11_TEXTURE2D_DESC::default()
    }
}

fn signed(pixels: u32) -> i32 {
    i32::try_from(pixels).unwrap_or(i32::MAX)
}

fn unsigned(pixel: i32) -> u32 {
    u32::try_from(pixel).unwrap_or_default()
}

/// The encoder, which the session and the capture's thread share.
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
