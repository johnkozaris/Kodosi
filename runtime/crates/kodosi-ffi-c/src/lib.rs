use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    ffi::{CStr, CString, OsStr, OsString, c_char, c_void},
    os::unix::ffi::OsStrExt as _,
    panic::AssertUnwindSafe,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use kodosi_runtime::{
    CommandEnvelope, Config, Error, HostKind, RuntimeHandle, local_host, terminal,
};
use tokio::{runtime::Runtime as Executor, task::JoinSet};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

mod callbacks;
mod instance;
mod subscriptions;
mod terminal_bridge;

pub use instance::kodosi_start;

use callbacks::{CallbackScope, Gate, State, UserData, lock};
use instance::{Instance, caught, code, handles, instance, payload, text};
use subscriptions::{SubscriptionState, Subscriptions};
pub use terminal_bridge::*;

#[cfg(test)]
use instance::Completion;

pub const KODOSI_FFI_ABI_VERSION: u32 = 7;
pub const KODOSI_MAX_FRAME_BYTES: usize = 8_388_608;
pub const KODOSI_TERMINAL_SEMANTIC_CHECKPOINT_MAX_BYTES: usize = 8_388_608;
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
pub const KODOSI_FFI_REQUIRED_CALLBACK_MISSING: i32 = 10;
pub const KODOSI_FFI_PANIC: i32 = -1;
pub const KODOSI_START_INVALID_CALLBACKS: i32 = 1;
pub const KODOSI_START_ALREADY_ACTIVE: i32 = 2;
pub const KODOSI_START_HOST_BUSY: i32 = 3;
pub const KODOSI_START_REJECTED: i32 = 4;
pub const KODOSI_START_FAILED: i32 = 5;
pub const KODOSI_START_FAILURE_MESSAGE_BYTES: usize = 512;
pub const KODOSI_HOST_KIND_UNKNOWN: i32 = 0;
pub const KODOSI_HOST_KIND_APP: i32 = 1;
pub const KODOSI_HOST_KIND_FOREGROUND: i32 = 2;
pub const KODOSI_HOST_KIND_BACKGROUND: i32 = 3;
pub const KODOSI_HOST_STOP_ACCEPTED: i32 = 0;
pub const KODOSI_HOST_STOP_REFUSED: i32 = 1;
pub const KODOSI_HOST_STOP_UNREACHABLE: i32 = 2;
pub const KODOSI_HOST_STOP_FAILED: i32 = 3;
pub const KODOSI_CLI_NOT_INVOKED: i32 = -1;
pub const KODOSI_CLI_FAILED: i32 = 70;

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
    pub host_local_sessions: u32,
    pub message: [c_char; KODOSI_START_FAILURE_MESSAGE_BYTES],
}

static LAST_START_FAILURE: Mutex<Option<KodosiStartFailure>> = Mutex::new(None);

fn start_failure(
    code: i32,
    message: &str,
    status: Option<local_host::HostStatus>,
) -> Box<KodosiStartFailure> {
    let mut failure = Box::new(KodosiStartFailure {
        code,
        host_kind: KODOSI_HOST_KIND_UNKNOWN,
        host_pid: 0,
        host_local_sessions: 0,
        message: [0; KODOSI_START_FAILURE_MESSAGE_BYTES],
    });
    if let Some(status) = status {
        failure.host_kind = match status.host.kind {
            HostKind::App => KODOSI_HOST_KIND_APP,
            HostKind::Foreground => KODOSI_HOST_KIND_FOREGROUND,
            HostKind::Background => KODOSI_HOST_KIND_BACKGROUND,
        };
        failure.host_pid = status.host.pid;
        failure.host_local_sessions = u32::try_from(status.local_sessions).unwrap_or(u32::MAX);
    }
    let mut end = message.len().min(KODOSI_START_FAILURE_MESSAGE_BYTES - 1);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    for (slot, byte) in failure.message.iter_mut().zip(&message.as_bytes()[..end]) {
        *slot = c_char::from_ne_bytes([*byte]);
    }
    failure
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
    caught(|| {
        let Ok(config) = Config::load() else {
            return KODOSI_HOST_STOP_FAILED;
        };
        let Ok(executor) = Executor::new() else {
            return KODOSI_HOST_STOP_FAILED;
        };
        match executor.block_on(local_host::stop_other_host(&config.data_root, force != 0)) {
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
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_cli_main(length: i32, values: *const *const c_char) -> i32 {
    let Ok(count) = usize::try_from(length) else {
        return KODOSI_CLI_NOT_INVOKED;
    };
    if count > 0 && values.is_null() {
        return KODOSI_CLI_NOT_INVOKED;
    }
    let invocation = (0..count)
        .filter_map(|index| {
            let pointer = unsafe { *values.add(index) };
            (!pointer.is_null()).then(|| {
                OsStr::from_bytes(unsafe { CStr::from_ptr(pointer) }.to_bytes()).to_owned()
            })
        })
        .collect::<Vec<OsString>>();
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
        let value: serde_json::Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(_) => return KODOSI_FFI_DESER_FAILED,
        };
        if matches!(
            value.get("type").and_then(serde_json::Value::as_str),
            Some("session.resize" | "session.focus" | "session.blur")
        ) {
            let Some(session) = value
                .get("sessionId")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
            else {
                return KODOSI_FFI_DESER_FAILED;
            };
            let Some(id) = value
                .get("subscriptionId")
                .or_else(|| value.get("clientId"))
                .and_then(serde_json::Value::as_str)
            else {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            };
            let Some(generation) = value
                .get("subscriptionGeneration")
                .and_then(serde_json::Value::as_u64)
            else {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            };
            let Some(incarnation) = value
                .get("expectedRuntimeIncarnationId")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| Uuid::parse_str(id).ok())
            else {
                return KODOSI_FFI_DESER_FAILED;
            };
            let registry = lock(&instance.state.subscriptions);
            let Some(subscription) = registry
                .entries
                .get(&session)
                .filter(|s| s.matches(id, generation) && !s.cancel.is_cancelled())
            else {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            };
            let Some((connection, current)) = *lock(&subscription.connected) else {
                return KODOSI_FFI_SESSION_NOT_FOUND;
            };
            if current != incarnation {
                return KODOSI_FFI_STALE_SUBSCRIPTION;
            }
            let command: CommandEnvelope = match serde_json::from_value(value) {
                Ok(value) => value,
                Err(_) => return KODOSI_FFI_DESER_FAILED,
            };
            let result = instance
                .runtime
                .try_send_terminal(command, connection)
                .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK);
            drop(registry);
            return result;
        }
        let command: CommandEnvelope = match serde_json::from_value(value) {
            Ok(value) => value,
            Err(_) => return KODOSI_FFI_DESER_FAILED,
        };
        instance
            .runtime
            .try_send(command)
            .map_or_else(|error| code(&error), |()| KODOSI_FFI_OK)
    })
}

#[cfg(test)]
mod tests;
