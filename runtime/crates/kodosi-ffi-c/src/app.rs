use kodosi_runtime::diagnostics;
#[cfg(feature = "host")]
use kodosi_runtime::local_host::{self, AppClaim, Forwarded};
#[cfg(feature = "host")]
use tokio_util::sync::CancellationToken;

#[cfg(feature = "host")]
use super::{
    Arc, Duration, KODOSI_ACTIVATION_MAX_BYTES, KODOSI_APP_FORWARDED, KODOSI_APP_OWNER,
    KODOSI_APP_SECONDARY, KODOSI_START_ALREADY_ACTIVE, KODOSI_START_INVALID_CALLBACKS, Mutex,
    callbacks::{CallbackScope, Gate, UserData, lock},
    payload,
};
use super::{
    AssertUnwindSafe, Config, KODOSI_APP_FAILED, KODOSI_FFI_DESER_FAILED, KODOSI_FFI_OK,
    KODOSI_LOG_ERROR, KODOSI_LOG_INFO, KODOSI_LOG_WARNING, KODOSI_START_FAILED,
    KODOSI_START_REJECTED, KodosiStartFailure, c_char, c_void, caught, record_start_failure,
    start_failure,
};

#[cfg(feature = "host")]
const FORWARD_WAIT: Duration = Duration::from_secs(10);

type ActivationCb = Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>;

#[cfg(feature = "host")]
#[derive(Clone, Copy)]
struct Activation {
    callback: unsafe extern "C" fn(*const u8, usize, *mut c_void),
    userdata: UserData,
}

#[cfg(feature = "host")]
struct App {
    config: Config,
    preferences: Option<std::ffi::CString>,
    activation: Activation,
    owner: Option<Owner>,
}

#[cfg(feature = "host")]
struct Owner {
    gate: Arc<Gate>,
    stop: CancellationToken,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "host")]
static APP: Mutex<Option<App>> = Mutex::new(None);

type Outcome = std::result::Result<i32, Box<KodosiStartFailure>>;

fn failed(code: i32, message: &str) -> Box<KodosiStartFailure> {
    start_failure(code, message)
}

fn settle(action: impl FnOnce() -> Outcome) -> i32 {
    match std::panic::catch_unwind(AssertUnwindSafe(action)) {
        Ok(Ok(claim)) => {
            record_start_failure(None);
            claim
        }
        Ok(Err(failure)) => {
            record_start_failure(Some(&failure));
            KODOSI_APP_FAILED
        }
        Err(_) => {
            record_start_failure(Some(&failed(
                KODOSI_START_FAILED,
                "The runtime panicked while preparing the app.",
            )));
            KODOSI_APP_FAILED
        }
    }
}

/// The runtime owns the `preferences` string until `kodosi_close_app`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_prepare(
    on_activation: ActivationCb,
    userdata: *mut c_void,
    preferences: *mut *const c_char,
) -> i32 {
    settle(|| unsafe { claim_app(on_activation, userdata, preferences) })
}

#[cfg(feature = "host")]
unsafe fn claim_app(
    on_activation: ActivationCb,
    userdata: *mut c_void,
    preferences: *mut *const c_char,
) -> Outcome {
    let (Some(callback), false) = (on_activation, preferences.is_null()) else {
        return Err(failed(
            KODOSI_START_INVALID_CALLBACKS,
            "The activation callback or the preferences output is missing.",
        ));
    };
    let activation = Activation {
        callback,
        userdata: UserData(userdata),
    };
    let config =
        Config::load().map_err(|error| failed(KODOSI_START_REJECTED, &error.to_string()))?;
    unsafe { prepare(config, activation, preferences) }
}

#[cfg(feature = "host")]
unsafe fn prepare(
    config: Config,
    activation: Activation,
    preferences: *mut *const c_char,
) -> Outcome {
    let mut app = lock(&APP);
    if app.is_some() {
        return Err(failed(
            KODOSI_START_ALREADY_ACTIVE,
            "This process already prepared the Kodosi app.",
        ));
    }
    let preferences_path = config
        .preferences
        .as_deref()
        .map(|path| {
            path.to_str()
                .and_then(|path| std::ffi::CString::new(path).ok())
                .ok_or_else(|| {
                    failed(
                        KODOSI_START_REJECTED,
                        "The isolated preferences path must be UTF-8.",
                    )
                })
        })
        .transpose()?;
    let owner = local_host::claim_app(&config.data_root)
        .map_err(|error| failed(KODOSI_START_REJECTED, &error.to_string()))?
        .map(|claim| own(&config, claim, activation))
        .transpose()?;
    let claim = if owner.is_some() {
        KODOSI_APP_OWNER
    } else {
        KODOSI_APP_SECONDARY
    };
    let prepared = app.insert(App {
        config,
        preferences: preferences_path,
        activation,
        owner,
    });
    let path = prepared
        .preferences
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr());
    drop(app);
    unsafe { preferences.write(path) };
    Ok(claim)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_forward_activation(json: *const u8, length: usize) -> i32 {
    settle(|| unsafe { forward(json, length) })
}

#[cfg(feature = "host")]
unsafe fn forward(json: *const u8, length: usize) -> Outcome {
    let activation = (length <= KODOSI_ACTIVATION_MAX_BYTES)
        .then(|| unsafe { payload(json, length) }.ok())
        .flatten()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(bytes).ok())
        .filter(serde_json::Value::is_object)
        .ok_or_else(|| {
            failed(
                KODOSI_START_REJECTED,
                "An activation must be a JSON object within its bound.",
            )
        })?;
    let root = match lock(&APP).as_ref() {
        None => {
            return Err(failed(
                KODOSI_START_FAILED,
                "Prepare the Kodosi app before forwarding an activation.",
            ));
        }
        Some(app) if app.owner.is_some() => return Ok(KODOSI_APP_OWNER),
        Some(app) => app.config.data_root.clone(),
    };
    let forwarded = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| failed(KODOSI_START_FAILED, &error.to_string()))?
        .block_on(local_host::forward_activation(
            &root,
            &activation,
            FORWARD_WAIT,
        ))
        .map_err(|error| failed(KODOSI_START_FAILED, &error.to_string()))?;
    let Forwarded::Claimed(claim) = forwarded else {
        return Ok(KODOSI_APP_FORWARDED);
    };
    let mut prepared = lock(&APP);
    let Some(app) = prepared.as_mut() else {
        return Err(failed(
            KODOSI_START_FAILED,
            "The Kodosi app closed while it forwarded an activation.",
        ));
    };
    app.owner = Some(own(&app.config, claim, app.activation)?);
    drop(prepared);
    Ok(KODOSI_APP_OWNER)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kodosi_log(level: i32, utf8: *const u8, length: usize) -> i32 {
    caught(|| {
        let level = match level {
            KODOSI_LOG_INFO => tracing::Level::INFO,
            KODOSI_LOG_WARNING => tracing::Level::WARN,
            KODOSI_LOG_ERROR => tracing::Level::ERROR,
            _ => return KODOSI_FFI_DESER_FAILED,
        };
        let message = match (utf8.is_null(), length) {
            (_, 0) => &[][..],
            (true, _) => return KODOSI_FFI_DESER_FAILED,
            (false, _) => unsafe { std::slice::from_raw_parts(utf8, length) },
        };
        diagnostics::client(level, message);
        KODOSI_FFI_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn kodosi_close_app() {
    close_app();
}

#[cfg(feature = "host")]
fn close_app() {
    let app = lock(&APP).take();
    let Some(mut owner) = app.and_then(|app| app.owner) else {
        return;
    };
    owner.gate.close();
    owner.stop.cancel();
    if !CallbackScope::active() {
        owner.gate.wait();
        if let Some(thread) = owner.thread.take() {
            drop(thread.join());
        }
    }
}

#[cfg(feature = "host")]
pub(super) fn start_config() -> std::result::Result<Config, Box<KodosiStartFailure>> {
    match lock(&APP).as_ref() {
        None => Config::load().map_err(|error| failed(KODOSI_START_REJECTED, &error.to_string())),
        Some(app) if app.owner.is_some() => Ok(app.config.clone()),
        Some(_) => Err(failed(
            KODOSI_START_REJECTED,
            "Another Kodosi app owns this data folder.",
        )),
    }
}

#[cfg(feature = "host")]
pub(super) fn config() -> kodosi_runtime::Result<Config> {
    lock(&APP)
        .as_ref()
        .map_or_else(Config::load, |app| Ok(app.config.clone()))
}

#[cfg(feature = "host")]
fn own(
    config: &Config,
    claim: AppClaim,
    activation: Activation,
) -> std::result::Result<Owner, Box<KodosiStartFailure>> {
    let gate = Arc::new(Gate::default());
    let stop = CancellationToken::new();
    let (ready, started) = std::sync::mpsc::sync_channel(1);
    let thread = std::thread::Builder::new()
        .name("kodosi-app".into())
        .spawn({
            let gate = Arc::clone(&gate);
            let stop = stop.clone();
            move || serve(claim, activation, gate, &stop, &ready)
        })
        .map_err(|error| failed(KODOSI_START_FAILED, &error.to_string()))?;
    let message = match started.recv() {
        Ok(Ok(())) => {
            diagnostics::install(&config.data_root, diagnostics::Role::App);
            return Ok(Owner {
                gate,
                stop,
                thread: Some(thread),
            });
        }
        Ok(Err(message)) => message,
        Err(_) => "The app endpoint stopped before it was ready.".to_owned(),
    };
    drop(thread.join());
    Err(failed(KODOSI_START_FAILED, &message))
}

#[cfg(feature = "host")]
fn serve(
    claim: AppClaim,
    activation: Activation,
    gate: Arc<Gate>,
    stop: &CancellationToken,
    ready: &std::sync::mpsc::SyncSender<std::result::Result<(), String>>,
) {
    let executor = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(executor) => executor,
        Err(error) => {
            drop(ready.send(Err(error.to_string())));
            return;
        }
    };
    executor.block_on(async {
        let deliver = move |bytes: Vec<u8>| {
            let Some(_permit) = gate.enter() else {
                return false;
            };
            let _scope = CallbackScope::enter();
            unsafe {
                (activation.callback)(bytes.as_ptr(), bytes.len(), activation.userdata.get());
            }
            true
        };
        match local_host::serve_activations(claim, deliver).await {
            Ok(server) => {
                drop(ready.send(Ok(())));
                stop.cancelled().await;
                server.shutdown().await;
            }
            Err(error) => drop(ready.send(Err(error.to_string()))),
        }
    });
}

#[cfg(not(feature = "host"))]
unsafe fn claim_app(
    _on_activation: ActivationCb,
    _userdata: *mut c_void,
    _preferences: *mut *const c_char,
) -> Outcome {
    Err(no_app_claim())
}

#[cfg(not(feature = "host"))]
unsafe fn forward(_json: *const u8, _length: usize) -> Outcome {
    Err(no_app_claim())
}

#[cfg(not(feature = "host"))]
const fn close_app() {}

#[cfg(not(feature = "host"))]
fn no_app_claim() -> Box<KodosiStartFailure> {
    failed(
        KODOSI_START_REJECTED,
        "A viewer runtime has no app instance to claim.",
    )
}

#[cfg(not(feature = "host"))]
pub(super) fn start_config() -> std::result::Result<Config, Box<KodosiStartFailure>> {
    Config::load().map_err(|error| failed(KODOSI_START_REJECTED, &error.to_string()))
}

#[cfg(all(test, feature = "host"))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    struct Probe {
        delivered: std::sync::mpsc::SyncSender<Vec<u8>>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    unsafe extern "C" fn record(json: *const u8, length: usize, userdata: *mut c_void) {
        let probe = unsafe { &*userdata.cast::<Probe>() };
        let json = unsafe { std::slice::from_raw_parts(json, length) }.to_vec();
        probe.delivered.send(json).unwrap();
        lock(&probe.release).recv().unwrap();
    }

    fn launch(root: &std::path::Path, route: &str) -> std::thread::JoinHandle<bool> {
        let root = root.to_owned();
        let activation = serde_json::json!({ "route": route });
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(local_host::forward_activation(
                    &root,
                    &activation,
                    Duration::from_secs(5),
                ))
                .is_ok()
        })
    }

    #[test]
    fn a_prepared_app_serves_activations_and_closes_after_its_callback() {
        let directory = tempfile::tempdir().unwrap();
        let isolated = kodosi_os::fs::canonicalize(directory.path()).unwrap();
        let config = Config {
            data_root: isolated.join("core"),
            preferences: Some(isolated.join("preferences.ini")),
            home: isolated.clone(),
            initial_shell: None,
            backend_url: "http://127.0.0.1:1".parse().unwrap(),
            oidc_issuer: "http://127.0.0.1:1".to_owned(),
            oidc_client_id: "kodosi-test".to_owned(),
            oidc_scopes: vec!["openid".to_owned()],
            secret_service: "kodosi.isolated".to_owned(),
            isolated: true,
            host: kodosi_runtime::HostKind::App,
        };
        kodosi_os::fs::create_private_dir(&config.data_root).unwrap();
        let root = config.data_root.clone();
        let (delivered, received) = std::sync::mpsc::sync_channel(4);
        let (release, released) = std::sync::mpsc::channel();
        let probe = Probe {
            delivered,
            release: Mutex::new(released),
        };
        let activation = Activation {
            callback: record,
            userdata: UserData((&raw const probe).cast_mut().cast()),
        };
        let mut preferences = std::ptr::null();
        assert!(matches!(
            unsafe { prepare(config.clone(), activation, &raw mut preferences) },
            Ok(KODOSI_APP_OWNER)
        ));
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(preferences) }
                .to_str()
                .unwrap(),
            isolated.join("preferences.ini").to_str().unwrap()
        );
        assert!(unsafe { prepare(config, activation, &raw mut preferences) }.is_err());
        assert!(start_config().is_ok_and(|config| config.data_root == root));

        release.send(()).unwrap();
        assert!(launch(&root, "kodosi://first").join().unwrap());
        assert_eq!(received.recv().unwrap(), br#"{"route":"kodosi://first"}"#);

        let second = launch(&root, "kodosi://second");
        assert_eq!(received.recv().unwrap(), br#"{"route":"kodosi://second"}"#);
        let (signal, finished) = std::sync::mpsc::channel();
        let closing = std::thread::spawn(move || {
            kodosi_close_app();
            signal.send(()).unwrap();
        });
        assert!(finished.recv_timeout(Duration::from_millis(200)).is_err());
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        closing.join().unwrap();
        assert!(second.join().unwrap());

        assert!(start_config().is_ok_and(|config| config.data_root != root));
        // A process that another test forks holds our open files until it execs.
        assert!((0..100).any(|_| {
            let claimed = local_host::claim_app(&root).unwrap().is_some();
            if !claimed {
                std::thread::sleep(Duration::from_millis(10));
            }
            claimed
        }));
        kodosi_close_app();
    }
}
