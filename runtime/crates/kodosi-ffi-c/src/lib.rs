use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, OsString, c_char, c_void},
    panic::AssertUnwindSafe,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use kodosi_runtime::{CommandEnvelope, Config, Error, protocol::Command, terminal};
#[cfg(feature = "host")]
use kodosi_runtime::{HostKind, local_host};
use tokio::{runtime::Runtime as Executor, task::JoinSet};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

mod app;
mod callbacks;
mod instance;
#[cfg_attr(feature = "host", path = "host_link.rs")]
#[cfg_attr(not(feature = "host"), path = "viewer_link.rs")]
mod link;
mod subscriptions;
mod terminal_bridge;

pub use app::{kodosi_close_app, kodosi_forward_activation, kodosi_log, kodosi_prepare};
pub use instance::kodosi_start;

use callbacks::{CallbackScope, Gate, State, UserData, lock};
use instance::{Instance, caught, code, handles, instance, payload, text};
use subscriptions::{SubscriptionState, Subscriptions};
pub use terminal_bridge::*;

#[cfg(test)]
use instance::Completion;

pub const KODOSI_FFI_ABI_VERSION: u32 = 10;
pub const KODOSI_MAX_FRAME_BYTES: usize = 8_388_608;
pub const KODOSI_TERMINAL_SNAPSHOT_MAX_BYTES: usize = 281_018_368;
pub const KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES: u64 = 67_108_864;
pub const KODOSI_TERMINAL_CONTROL_MAX_BYTES: usize = 65_536;
pub const KODOSI_FFI_OK: i32 = 0;
pub const KODOSI_FFI_NULL_HANDLE: i32 = 2;
pub const KODOSI_FFI_DESER_FAILED: i32 = 3;
pub const KODOSI_FFI_PAYLOAD_TOO_LARGE: i32 = 4;
pub const KODOSI_FFI_RUNTIME_STOPPED: i32 = 5;
pub const KODOSI_FFI_BUSY: i32 = 6;
pub const KODOSI_FFI_SESSION_NOT_FOUND: i32 = 7;
pub const KODOSI_FFI_STALE_SUBSCRIPTION: i32 = 8;
pub const KODOSI_FFI_TERMINAL_CHECKPOINT_REJECTED: i32 = 9;
pub const KODOSI_FFI_PANIC: i32 = -1;
pub const KODOSI_START_INVALID_CALLBACKS: i32 = 1;
pub const KODOSI_START_ALREADY_ACTIVE: i32 = 2;
pub const KODOSI_START_HOST_OUTDATED: i32 = 3;
pub const KODOSI_START_REJECTED: i32 = 4;
pub const KODOSI_START_FAILED: i32 = 5;
pub const KODOSI_START_FAILURE_MESSAGE_BYTES: usize = 512;
pub const KODOSI_HOST_KIND_UNKNOWN: i32 = 0;
pub const KODOSI_HOST_KIND_FOREGROUND: i32 = 1;
pub const KODOSI_HOST_KIND_BACKGROUND: i32 = 2;
pub const KODOSI_HOST_STOP_ACCEPTED: i32 = 0;
pub const KODOSI_HOST_STOP_REFUSED: i32 = 1;
pub const KODOSI_HOST_STOP_UNREACHABLE: i32 = 2;
pub const KODOSI_HOST_STOP_FAILED: i32 = 3;
pub const KODOSI_CLI_NOT_INVOKED: i32 = -1;
pub const KODOSI_CLI_FAILED: i32 = 70;
pub const KODOSI_APP_OWNER: i32 = 0;
pub const KODOSI_APP_SECONDARY: i32 = 1;
pub const KODOSI_APP_FORWARDED: i32 = 2;
pub const KODOSI_APP_FAILED: i32 = 3;
pub const KODOSI_ACTIVATION_MAX_BYTES: usize = 16_384;
pub const KODOSI_LOG_INFO: i32 = 0;
pub const KODOSI_LOG_WARNING: i32 = 1;
pub const KODOSI_LOG_ERROR: i32 = 2;

#[cfg(feature = "host")]
const _: () = assert!(KODOSI_ACTIVATION_MAX_BYTES == local_host::MAX_ACTIVATION_BYTES);
const _: () = assert!(KODOSI_TERMINAL_SNAPSHOT_MAX_BYTES == terminal::TERMINAL_SNAPSHOT_MAX_BYTES);
const _: () =
    assert!(KODOSI_TERMINAL_IMAGE_STORAGE_MAX_BYTES == terminal::TERMINAL_IMAGE_STORAGE_MAX_BYTES);

const MAX_SUBSCRIPTIONS: usize = 4_096;
const STOP_BUDGET: Duration = Duration::from_secs(10);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_HANDLE: AtomicUsize = AtomicUsize::new(1);
static HANDLES: OnceLock<Mutex<HashMap<usize, Arc<Instance>>>> = OnceLock::new();

thread_local! {
    static CALLBACK_DEPTH: Cell<usize> = const { Cell::new(0) };
}

type EventCb = Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>;
type TerminalDataCb = Option<
    unsafe extern "C" fn(*const c_char, *const c_char, u64, u64, *const u8, usize, *mut c_void),
>;
type TerminalControlCb =
    Option<unsafe extern "C" fn(*const c_char, *const c_char, u64, *const u8, usize, *mut c_void)>;
type TerminalConnectResultCb =
    Option<unsafe extern "C" fn(*const c_char, *const c_char, u64, i32, *mut c_void)>;
type TerminalCheckpointCb = Option<
    unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        u64,
        u64,
        u16,
        u16,
        bool,
        *const u8,
        usize,
        *mut c_void,
    ) -> i32,
>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KodosiCallbacks {
    pub on_event: EventCb,
    pub on_terminal_data: TerminalDataCb,
    pub on_terminal_control: TerminalControlCb,
    pub on_terminal_connect_result: TerminalConnectResultCb,
    pub on_terminal_checkpoint: TerminalCheckpointCb,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct KodosiStartFailure {
    pub code: i32,
    pub host_kind: i32,
    pub host_pid: u32,
    pub host_terminals: u32,
    pub message: [c_char; KODOSI_START_FAILURE_MESSAGE_BYTES],
}

static LAST_START_FAILURE: Mutex<Option<KodosiStartFailure>> = Mutex::new(None);

fn start_failure(code: i32, message: &str) -> Box<KodosiStartFailure> {
    let mut failure = Box::new(KodosiStartFailure {
        code,
        host_kind: KODOSI_HOST_KIND_UNKNOWN,
        host_pid: 0,
        host_terminals: 0,
        message: [0; KODOSI_START_FAILURE_MESSAGE_BYTES],
    });
    let mut end = message.len().min(KODOSI_START_FAILURE_MESSAGE_BYTES - 1);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    for (slot, byte) in failure.message.iter_mut().zip(&message.as_bytes()[..end]) {
        *slot = c_char::from_ne_bytes([*byte]);
    }
    failure
}

#[cfg(feature = "host")]
fn start_error(error: &Error) -> Box<KodosiStartFailure> {
    match error {
        Error::HostOutdated {
            kind,
            pid,
            terminals,
        } => {
            let mut failure = start_failure(KODOSI_START_HOST_OUTDATED, &error.to_string());
            failure.host_kind = match kind {
                HostKind::Foreground => KODOSI_HOST_KIND_FOREGROUND,
                HostKind::Background => KODOSI_HOST_KIND_BACKGROUND,
            };
            failure.host_pid = *pid;
            failure.host_terminals = u32::try_from(*terminals).unwrap_or(u32::MAX);
            failure
        }
        Error::Invalid(_) => start_failure(KODOSI_START_REJECTED, &error.to_string()),
        _ => start_failure(KODOSI_START_FAILED, &error.to_string()),
    }
}

fn record_start_failure(failure: Option<&KodosiStartFailure>) {
    *lock(&LAST_START_FAILURE) = failure.copied();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_stop(handle: *mut c_void) {
    let Some(instance) = ({ lock(handles()).get(&(handle as usize)).cloned() }) else {
        return;
    };
    instance.state.close();
    if !CallbackScope::active() {
        instance.state.gate.wait();
        instance.stopped.wait();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_abi_version() -> u32 {
    KODOSI_FFI_ABI_VERSION
}
#[unsafe(no_mangle)]
pub extern "C" fn kodosi_protocol_version() -> u32 {
    kodosi_runtime::protocol::VERSION
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_last_start_failure(out: *mut KodosiStartFailure) -> i32 {
    caught(|| {
        if out.is_null() {
            return KODOSI_FFI_NULL_HANDLE;
        }
        let failure = *lock(&LAST_START_FAILURE);
        failure.map_or(0, |failure| {
            unsafe { out.write(failure) };
            1
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_host_stop(force: i32) -> i32 {
    caught(|| stop_host(force != 0))
}

#[cfg(feature = "host")]
fn stop_host(force: bool) -> i32 {
    let Ok(config) = app::config() else {
        return KODOSI_HOST_STOP_FAILED;
    };
    let Ok(executor) = Executor::new() else {
        return KODOSI_HOST_STOP_FAILED;
    };
    match executor.block_on(local_host::stop_other_host(&config.data_root, force)) {
        Ok(()) => KODOSI_HOST_STOP_ACCEPTED,
        Err(Error::HostBusy(_)) => KODOSI_HOST_STOP_REFUSED,
        Err(Error::Io(error))
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            KODOSI_HOST_STOP_UNREACHABLE
        }
        Err(_) => KODOSI_HOST_STOP_FAILED,
    }
}

#[cfg(not(feature = "host"))]
const fn stop_host(_force: bool) -> i32 {
    KODOSI_HOST_STOP_UNREACHABLE
}

#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "Windows arguments can fail UTF-8 validation"
)]
fn os_argument(bytes: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStrExt as _;
    Some(std::ffi::OsStr::from_bytes(bytes).to_owned())
}

#[cfg(windows)]
fn os_argument(bytes: &[u8]) -> Option<OsString> {
    std::str::from_utf8(bytes).ok().map(OsString::from)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_cli_main(length: i32, values: *const *const c_char) -> i32 {
    let Ok(count) = usize::try_from(length) else {
        return KODOSI_CLI_NOT_INVOKED;
    };
    if count > 0 && values.is_null() {
        return KODOSI_CLI_NOT_INVOKED;
    }
    let Some(invocation) = (0..count)
        .filter_map(|index| {
            let pointer = unsafe { *values.add(index) };
            (!pointer.is_null()).then(|| os_argument(unsafe { CStr::from_ptr(pointer) }.to_bytes()))
        })
        .collect::<Option<Vec<OsString>>>()
    else {
        return KODOSI_CLI_FAILED;
    };
    run_cli(invocation)
}

#[cfg(feature = "cli")]
fn run_cli(invocation: Vec<OsString>) -> i32 {
    if !kodosi_runtime::cli::is_invocation(&invocation) {
        return KODOSI_CLI_NOT_INVOKED;
    }
    std::panic::catch_unwind(AssertUnwindSafe(|| {
        Executor::new().map_or(KODOSI_CLI_FAILED, |executor| {
            i32::from(executor.block_on(kodosi_runtime::cli::run_with(invocation)))
        })
    }))
    .unwrap_or(KODOSI_CLI_FAILED)
}

#[cfg(not(feature = "cli"))]
fn run_cli(invocation: Vec<OsString>) -> i32 {
    drop(invocation);
    KODOSI_CLI_NOT_INVOKED
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_send_command(
    handle: *mut c_void,
    bytes: *const u8,
    len: usize,
) -> i32 {
    caught(|| {
        let Some(instance) = instance(handle) else {
            return KODOSI_FFI_NULL_HANDLE;
        };
        if len > kodosi_runtime::protocol::MAX_COMMAND_BYTES {
            return KODOSI_FFI_PAYLOAD_TOO_LARGE;
        }
        let bytes = match unsafe { payload(bytes, len) } {
            Ok(value) => value,
            Err(error) => return error,
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let Ok(envelope) = serde_json::from_value::<CommandEnvelope>(value.clone()) else {
            return KODOSI_FFI_DESER_FAILED;
        };
        let target = match &envelope.command {
            Command::Resize {
                session_id,
                identity,
            } => Some((
                session_id.clone(),
                identity.subscription_id.clone(),
                identity.subscription_generation,
                identity.expected_runtime_incarnation_id.clone(),
            )),
            Command::Focus {
                session_id,
                expected_runtime_incarnation_id,
                client_id,
                subscription_generation,
                ..
            }
            | Command::Blur {
                session_id,
                expected_runtime_incarnation_id,
                client_id,
                subscription_generation,
                ..
            } => Some((
                session_id.clone(),
                client_id.clone(),
                *subscription_generation,
                expected_runtime_incarnation_id.clone(),
            )),
            _ => None,
        };
        match target {
            Some((session, subscription, generation, incarnation)) => control(
                &instance,
                &session,
                &subscription,
                generation,
                &incarnation,
                &envelope,
            ),
            None => instance.link.command(value, &envelope),
        }
    })
}

fn control(
    instance: &Instance,
    session: &str,
    subscription: &str,
    generation: u64,
    incarnation: &str,
    envelope: &CommandEnvelope,
) -> i32 {
    let (Ok(session), Ok(incarnation)) = (Uuid::parse_str(session), Uuid::parse_str(incarnation))
    else {
        return KODOSI_FFI_DESER_FAILED;
    };
    let Some(entry) = instance
        .state
        .subscription(session, subscription, generation)
    else {
        return KODOSI_FFI_STALE_SUBSCRIPTION;
    };
    let guard = lock(&entry.connected);
    let Some(connected) = guard.as_ref() else {
        return KODOSI_FFI_SESSION_NOT_FOUND;
    };
    if connected.incarnation != incarnation {
        return KODOSI_FFI_STALE_SUBSCRIPTION;
    }
    let result = connected.control(instance, &entry, envelope);
    drop(guard);
    result
}

#[cfg(test)]
mod tests;
