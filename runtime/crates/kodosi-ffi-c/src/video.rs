//! Watched video pipes: the runtime hands each one's messages and the frames that its decoder can
//! take to the app, on a thread of its own, and the app answers each frame.

use kodosi_runtime::video::{Frame, FrameKind, Shown, Took};
use tokio::sync::{mpsc, oneshot};

use super::{
    Arc, CString, CancellationToken, KODOSI_FFI_BUSY, KODOSI_FFI_DESER_FAILED,
    KODOSI_FFI_NULL_HANDLE, KODOSI_FFI_OK, KODOSI_FFI_RUNTIME_STOPPED,
    KODOSI_FFI_SESSION_NOT_FOUND, KODOSI_VIDEO_ANCHOR, KODOSI_VIDEO_DELTA, KODOSI_VIDEO_FAILED,
    KODOSI_VIDEO_KEY, KodosiVideoFrame, KodosiVideoRegion, Mutex, State, Uuid, c_char, c_void,
    callbacks::{CallbackScope, Gate},
    caught, instance, lock, text,
};

/// More watches than the host keeps pipes open would be a leak in the app.
const MOST_WATCHES: usize = 32;
const THREAD_QUEUE: usize = 1;

pub(super) struct Watch {
    pub(super) pipe: CString,
    pub(super) gate: Gate,
    pub(super) cancel: CancellationToken,
    /// Where the app's restarts go once the watch reached the runtime.
    took: Mutex<Option<mpsc::Sender<Took>>>,
}

impl Watch {
    pub(super) fn new(pipe: CString, cancel: CancellationToken) -> Self {
        Self {
            pipe,
            gate: Gate::default(),
            cancel,
            took: Mutex::new(None),
        }
    }

    /// Returns once no call of this watch runs and none will come, except inside its own call,
    /// where it returns at once.
    pub(super) fn end(&self) {
        self.retire();
        if !CallbackScope::within(&self.gate) {
            self.gate.wait();
        }
    }

    pub(super) fn retire(&self) {
        self.gate.close();
        self.cancel.cancel();
        lock(&self.took).take();
    }
}

/// Carries one connection of a watch until `shown` ends or the watch does: the pipe's thread calls
/// the app with what `shown` brings and sends the decoder's answers to `took`. It returns once that
/// thread made its last call, so the next connection's thread never calls the app at the same time.
pub(super) async fn serve(
    state: Arc<State>,
    watch: Arc<Watch>,
    mut shown: mpsc::Receiver<Shown>,
    took: mpsc::Sender<Took>,
) {
    *lock(&watch.took) = Some(took.clone());
    let (items, mut inbox) = mpsc::channel(THREAD_QUEUE);
    let (called, finished) = oneshot::channel::<()>();
    let thread = {
        let watch = Arc::clone(&watch);
        std::thread::Builder::new()
            .name("kodosi-video".into())
            .spawn(move || {
                call(&state, &watch, &mut inbox, &took);
                drop(called);
            })
    };
    if let Err(error) = thread {
        tracing::warn!(%error, "a video could not get its thread");
        return;
    }
    loop {
        let next = tokio::select! {
            biased;
            () = watch.cancel.cancelled() => break,
            next = shown.recv() => next,
        };
        let Some(next) = next else { break };
        if items.send(next).await.is_err() {
            break;
        }
    }
    drop(items);
    drop(finished.await);
}

fn call(
    state: &State,
    watch: &Watch,
    inbox: &mut mpsc::Receiver<Shown>,
    took: &mpsc::Sender<Took>,
) {
    while let Some(next) = inbox.blocking_recv() {
        let answer = match next {
            Shown::Message(message) => state
                .video_message(watch, message.get(1..).unwrap_or_default())
                .map(|()| None),
            Shown::Stale(flag) => state.video_stale(watch, flag).map(|()| None),
            Shown::Frame(message) => state.video_frame(watch, &message).map(Some),
        };
        match answer {
            None => return,
            Some(Some(answer)) => {
                if took.blocking_send(answer).is_err() {
                    return;
                }
            }
            Some(None) => {}
        }
    }
}

impl State {
    fn video_call<R>(&self, watch: &Watch, action: impl FnOnce() -> R) -> Option<R> {
        let _permit = self.gate.enter()?;
        let _watch = watch.gate.enter()?;
        let _scope = CallbackScope::enter(&self.gate);
        let _own = CallbackScope::enter(&watch.gate);
        Some(action())
    }

    /// `None` once the watch or the runtime ended, so no call may come.
    fn video_frame(&self, watch: &Watch, message: &bytes::Bytes) -> Option<Took> {
        let Some(frame) = Frame::decode(message) else {
            return Some(Took::Failed);
        };
        let callback = self.callbacks.on_video_frame?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a region came as 32-bit floats"
        )]
        let region = frame.region.map(|region| KodosiVideoRegion {
            x: region.x as f32,
            y: region.y as f32,
            width: region.width as f32,
            height: region.height as f32,
        });
        let picture = KodosiVideoFrame {
            codec: frame.codec.wire(),
            kind: match frame.kind {
                FrameKind::Delta => KODOSI_VIDEO_DELTA,
                FrameKind::Key => KODOSI_VIDEO_KEY,
                FrameKind::Anchor => KODOSI_VIDEO_ANCHOR,
            },
            region: region.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
            data: frame.data.as_ptr(),
            length: frame.data.len(),
        };
        let result = self.video_call(watch, || unsafe {
            callback(watch.pipe.as_ptr(), &raw const picture, self.userdata.get())
        })?;
        Some(if result == KODOSI_VIDEO_FAILED {
            Took::Failed
        } else {
            Took::Taken
        })
    }

    fn video_message(&self, watch: &Watch, json: &[u8]) -> Option<()> {
        let callback = self.callbacks.on_video_message?;
        self.video_call(watch, || unsafe {
            callback(
                watch.pipe.as_ptr(),
                json.as_ptr(),
                json.len(),
                self.userdata.get(),
            );
        })
    }

    fn video_stale(&self, watch: &Watch, stale: bool) -> Option<()> {
        let callback = self.callbacks.on_video_stale?;
        self.video_call(watch, || unsafe {
            callback(watch.pipe.as_ptr(), stale, self.userdata.get());
        })
    }
}

/// Parses the pipe of a video call, or the result code that refuses it.
unsafe fn pipe_of(pipe: *const c_char) -> Result<(Uuid, CString), i32> {
    let text = unsafe { text(pipe) }.ok_or(KODOSI_FFI_DESER_FAILED)?;
    let id = Uuid::parse_str(text).map_err(|_| KODOSI_FFI_DESER_FAILED)?;
    let name = CString::new(text).map_err(|_| KODOSI_FFI_DESER_FAILED)?;
    Ok((id, name))
}

/// The frames and messages of `pipe`, a video pipe that the app opened with `document.open`, go
/// to the video callbacks from now on, until `kodosi_video_unwatch` or the pipe's close.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_video_watch(handle: *mut c_void, pipe: *const c_char) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let (id, name) = match unsafe { pipe_of(pipe) } {
            Ok(pipe) => pipe,
            Err(code) => return code,
        };
        let watch = Arc::new(Watch::new(name, instance.state.cancel.child_token()));
        let prior = {
            let mut watches = lock(&instance.state.watches);
            if instance.state.cancel.is_cancelled() {
                return KODOSI_FFI_RUNTIME_STOPPED;
            }
            watches.retain(|_, watch| !watch.gate.is_done());
            if !watches.contains_key(&id) && watches.len() >= MOST_WATCHES {
                return KODOSI_FFI_BUSY;
            }
            watches.insert(id, Arc::clone(&watch))
        };
        if let Some(prior) = prior {
            prior.end();
        }
        if !instance.link.watch_video(&instance, id, Arc::clone(&watch)) {
            watch.retire();
            return KODOSI_FFI_RUNTIME_STOPPED;
        }
        KODOSI_FFI_OK
    })
}

/// The app's decoder of `pipe` started over outside a call, so it needs a keyframe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_video_restart(handle: *mut c_void, pipe: *const c_char) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let id = match unsafe { pipe_of(pipe) } {
            Ok((id, _)) => id,
            Err(code) => return code,
        };
        let Some(watch) = lock(&instance.state.watches).get(&id).cloned() else {
            return KODOSI_FFI_SESSION_NOT_FOUND;
        };
        let took = lock(&watch.took).clone();
        match took.map(|took| took.try_send(Took::Restart)) {
            Some(Err(mpsc::error::TrySendError::Closed(_))) => KODOSI_FFI_SESSION_NOT_FOUND,
            _ => KODOSI_FFI_OK,
        }
    })
}

/// Returns once no video call for `pipe` runs and none will come, except inside that pipe's own
/// call, where it returns at once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_video_unwatch(handle: *mut c_void, pipe: *const c_char) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        let id = match unsafe { pipe_of(pipe) } {
            Ok((id, _)) => id,
            Err(code) => return code,
        };
        let Some(watch) = lock(&instance.state.watches).remove(&id) else {
            return KODOSI_FFI_SESSION_NOT_FOUND;
        };
        watch.end();
        KODOSI_FFI_OK
    })
}
