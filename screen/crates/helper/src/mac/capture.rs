//! One `ScreenCaptureKit` stream of a display or a window, whose frames go to the encoder as they
//! come.

use std::{
    ptr,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use bytes::Bytes;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    AllocAnyThread, DefinedClass, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol, ProtocolObject},
};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFNumber, CFRetained, CFString, CGPoint, CGRect, CGSize, Type,
};
use objc2_core_graphics::{kCGColorSpaceSRGB, kCGDisplayStreamYCbCrMatrix_ITU_R_709_2};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_foundation::{NSArray, NSError, NSString};
use objc2_screen_capture_kit::{
    SCCaptureResolutionType, SCContentFilter, SCFrameStatus, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamFrameInfoDirtyRects, SCStreamFrameInfoScreenRect,
    SCStreamFrameInfoStatus, SCStreamOutput, SCStreamOutputType,
};
use screen::link::{self, Source};
use stream::{Encoded, Region};
use stream_encode::{Encoder, Output, Picture, Settings, refine::Cleanup};
use tokio::sync::{mpsc, watch};

use super::{completed, describe, item, rect, session::Event, sources::Content, value};

/// Frames that `ScreenCaptureKit` keeps for us: one it draws, one waiting for the encoder, two in
/// the encoder and the last one, which the encoder holds to sharpen it.
const QUEUE_DEPTH: isize = 6;
const FRAME_RATE: i32 = 60;
/// A frame whose region is this close to the whole display shows all of it.
const WHOLE: f64 = 0.5;

/// What the host asks a capture to show.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Wanted {
    pub(super) source: Source,
    pub(super) region: Option<Region>,
    pub(super) width: u32,
    pub(super) height: u32,
}

pub(super) struct Capture {
    pub(super) source: Source,
    stream: Retained<SCStream>,
    configuration: Retained<SCStreamConfiguration>,
    frames: Arc<Frames>,
    _taker: Retained<Taker>,
    _queue: DispatchRetained<DispatchQueue>,
}

impl Capture {
    pub(super) async fn start(
        content: &Content,
        wanted: Wanted,
        frames: Arc<Frames>,
    ) -> Result<Self, String> {
        let (filter, display) = filter(content, wanted.source)?;
        // SAFETY: a new configuration.
        let configuration = unsafe { SCStreamConfiguration::new() };
        configure(&configuration, wanted, display);
        frames.shows(display);
        let taker = Taker::new(Arc::clone(&frames));
        let queue = DispatchQueue::new("com.kodosi.screen.frames", None);
        // SAFETY: the filter and configuration are valid, and the delegate lives as long as the
        // stream.
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &configuration,
                Some(ProtocolObject::from_ref(&*taker)),
            )
        };
        // SAFETY: the output lives as long as the stream, and the queue is serial, so frames come
        // one at a time.
        unsafe {
            stream.addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*taker),
                SCStreamOutputType::Screen,
                Some(&queue),
            )
        }
        .map_err(|error| describe(&error))?;
        // SAFETY: the completion block takes the error that it is given.
        completed(|done| unsafe { stream.startCaptureWithCompletionHandler(Some(done)) }).await?;
        Ok(Self {
            source: wanted.source,
            stream,
            configuration,
            frames,
            _taker: taker,
            _queue: queue,
        })
    }

    /// Changes the size or the region of the same source.
    pub(super) async fn update(&self, content: &Content, wanted: Wanted) -> Result<(), String> {
        let display = match wanted.source {
            // SAFETY: a getter of a snapshot.
            Source::Display { id } => Some(
                content
                    .display(id)
                    .map(|display| unsafe { display.frame() })
                    .ok_or("the display is gone")?,
            ),
            Source::Window { .. } => None,
        };
        configure(&self.configuration, wanted, display);
        // SAFETY: the configuration is valid, and the completion block takes the error that it
        // is given.
        completed(|done| unsafe {
            self.stream
                .updateConfiguration_completionHandler(&self.configuration, Some(done));
        })
        .await?;
        self.frames.shows(display);
        Ok(())
    }

    pub(super) async fn stop(self) {
        // SAFETY: the completion block takes the error that it is given.
        let stopped =
            completed(|done| unsafe { self.stream.stopCaptureWithCompletionHandler(Some(done)) })
                .await;
        drop(stopped);
    }
}

fn filter(
    content: &Content,
    source: Source,
) -> Result<(Retained<SCContentFilter>, Option<CGRect>), String> {
    match source {
        Source::Display { id } => {
            let display = content.display(id).ok_or("the display is gone")?;
            // SAFETY: a filter of a listed display that leaves nothing out.
            let filter = unsafe {
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &NSArray::new(),
                )
            };
            // SAFETY: a getter of a snapshot.
            Ok((filter, Some(unsafe { display.frame() })))
        }
        Source::Window { id } => {
            let window = content.window(id).ok_or("the window is gone")?;
            // SAFETY: a filter of a listed window.
            let filter = unsafe {
                SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window)
            };
            Ok((filter, None))
        }
    }
}

/// `ScreenCaptureKit` captures a window whole, so only a display's capture takes a region.
fn configure(configuration: &SCStreamConfiguration, wanted: Wanted, display: Option<CGRect>) {
    let area = display.map(|display| {
        wanted.region.map_or_else(
            || CGRect::new(CGPoint::ZERO, display.size),
            |region| {
                CGRect::new(
                    CGPoint::new(region.x, region.y),
                    CGSize::new(region.width, region.height),
                )
            },
        )
    });
    let frame_interval = CMTime {
        value: 1,
        timescale: FRAME_RATE,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    };
    // SAFETY: setters with values in their documented ranges; the colour constants are Core
    // Graphics strings.
    unsafe {
        configuration.setWidth(wanted.width as usize);
        configuration.setHeight(wanted.height as usize);
        configuration.setPixelFormat(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange);
        configuration.setColorMatrix(kCGDisplayStreamYCbCrMatrix_ITU_R_709_2);
        configuration.setColorSpaceName(kCGColorSpaceSRGB);
        configuration.setMinimumFrameInterval(frame_interval);
        configuration.setQueueDepth(QUEUE_DEPTH);
        configuration.setShowsCursor(false);
        configuration.setScalesToFit(true);
        configuration.setIgnoreShadowsSingleWindow(true);
        configuration.setCaptureResolution(SCCaptureResolutionType::Best);
        if let Some(area) = area {
            configuration.setSourceRect(area);
        }
    }
}

/// The encoder, and what the capture's queue and the session share about the frames.
pub(super) struct Frames {
    encoding: Mutex<Encoding>,
    events: mpsc::UnboundedSender<Event>,
    /// The number of the last picture, while it needs passes to sharpen it.
    drawn: watch::Sender<u64>,
}

struct Encoding {
    encoder: Encoder,
    settings: Settings,
    cleanup: Cleanup,
    /// The shown display's frame in global points, or `None` for a window, which frames show
    /// whole.
    display: Option<CGRect>,
    /// The next frame starts a capture, which changes all of the picture.
    fresh: bool,
}

impl Frames {
    pub(super) fn new(
        settings: Settings,
        say: mpsc::Sender<Bytes>,
        events: mpsc::UnboundedSender<Event>,
    ) -> (Arc<Self>, watch::Receiver<u64>) {
        let (drawn, drawing) = watch::channel(0);
        let sent = Arc::new(Sent {
            say,
            events: events.clone(),
        });
        let frames = Arc::new(Self {
            encoding: Mutex::new(Encoding {
                encoder: Encoder::new(settings, sent),
                settings,
                cleanup: Cleanup::default(),
                display: None,
                fresh: true,
            }),
            events,
            drawn,
        });
        (frames, drawing)
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

    /// Encodes the last picture again while it needs sharpening; false once it is clean.
    pub(super) fn refine(&self, picture: u64) -> bool {
        let mut encoding = self.lock();
        if !encoding.cleanup.due(picture) {
            return false;
        }
        if encoding.encoder.refine() {
            encoding.cleanup.refined();
        }
        true
    }

    fn shows(&self, display: Option<CGRect>) {
        let mut encoding = self.lock();
        encoding.display = display;
        encoding.fresh = true;
    }

    /// Runs on the capture's queue for each frame.
    fn took(&self, sample: &CMSampleBuffer) {
        let Some(info) = info(sample) else {
            return;
        };
        if status(&info) != Some(SCFrameStatus::Complete) {
            return;
        }
        // SAFETY: the sample is valid during the call.
        let Some(picture) = (unsafe { sample.image_buffer() }) else {
            return;
        };
        let pixels =
            number(CVPixelBufferGetWidth(&picture)) * number(CVPixelBufferGetHeight(&picture));
        let shown = value::<CFDictionary>(&info, frame_info(Key::ScreenRect)).and_then(rect);
        let changed = dirty(&info).map_or(1.0, |dirty| (dirty / pixels.max(1.0)).min(1.0));
        let mut encoding = self.lock();
        let region = encoding
            .display
            .zip(shown)
            .and_then(|(display, shown)| region(display, shown));
        let changed = if std::mem::take(&mut encoding.fresh) {
            1.0
        } else {
            changed
        };
        encoding.encoder.encode(
            Picture::Nv12(CFRetained::as_ptr(&picture).as_ptr().cast()),
            region,
        );
        if let Some(picture) = encoding.cleanup.drawn(changed) {
            self.drawn.send_replace(picture);
        }
    }

    fn failed(&self, problem: String) {
        drop(self.events.send(Event::Failed(problem)));
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

define_class!(
    /// Takes the stream's frames on its queue, and hears when it stops.
    #[unsafe(super(NSObject))]
    #[name = "KodosiScreenFrames"]
    #[ivars = Arc<Frames>]
    struct Taker;

    unsafe impl NSObjectProtocol for Taker {}

    unsafe impl SCStreamOutput for Taker {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn took(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind == SCStreamOutputType::Screen {
                self.ivars().took(sample);
            }
        }
    }

    unsafe impl SCStreamDelegate for Taker {
        #[unsafe(method(stream:didStopWithError:))]
        fn stopped(&self, _stream: &SCStream, error: &NSError) {
            self.ivars().failed(describe(error));
        }
    }
);

impl Taker {
    fn new(frames: Arc<Frames>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(frames);
        // SAFETY: `NSObject`'s `init` takes nothing.
        unsafe { msg_send![super(this), init] }
    }
}

#[derive(Clone, Copy)]
enum Key {
    Status,
    ScreenRect,
    DirtyRects,
}

fn frame_info(key: Key) -> &'static CFString {
    // SAFETY: `ScreenCaptureKit` constants, whose strings are Core Foundation strings.
    unsafe {
        let key: &NSString = match key {
            Key::Status => SCStreamFrameInfoStatus,
            Key::ScreenRect => SCStreamFrameInfoScreenRect,
            Key::DirtyRects => SCStreamFrameInfoDirtyRects,
        };
        &*ptr::from_ref(key).cast::<CFString>()
    }
}

/// What `ScreenCaptureKit` says about a frame.
fn info(sample: &CMSampleBuffer) -> Option<CFRetained<CFDictionary>> {
    // SAFETY: the sample is valid during the call.
    let attachments = unsafe { sample.sample_attachments_array(false) }?;
    item::<CFDictionary>(&attachments, 0).map(Type::retain)
}

fn status(info: &CFDictionary) -> Option<SCFrameStatus> {
    value::<CFNumber>(info, frame_info(Key::Status))?
        .as_i64()
        .and_then(|status| isize::try_from(status).ok())
        .map(SCFrameStatus)
}

/// The area that changed since the last frame, in pixels, if `ScreenCaptureKit` says.
fn dirty(info: &CFDictionary) -> Option<f64> {
    let rects = value::<CFArray>(info, frame_info(Key::DirtyRects))?;
    Some(
        (0..rects.count())
            .filter_map(|index| item::<CFDictionary>(rects, index).and_then(rect))
            .map(|rect| rect.size.width * rect.size.height)
            .sum(),
    )
}

/// The part of the display that a frame shows, or `None` when it shows all of it.
fn region(display: CGRect, shown: CGRect) -> Option<Region> {
    let region = Region {
        x: shown.origin.x - display.origin.x,
        y: shown.origin.y - display.origin.y,
        width: shown.size.width,
        height: shown.size.height,
    };
    let whole = region.x.abs() < WHOLE
        && region.y.abs() < WHOLE
        && (region.width - display.size.width).abs() < WHOLE
        && (region.height - display.size.height).abs() < WHOLE;
    (!whole).then_some(region)
}

fn number(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}
