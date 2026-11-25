#[cfg(target_os = "macos")]
mod cf;
mod client;
mod cursor;
mod engine;
mod filters;
#[cfg(target_os = "macos")]
mod layer;
mod link;
#[cfg(target_os = "macos")]
mod mac;
mod options;
mod renderer;

use std::os::raw::c_int;

use cef::{args::Args, *};

#[cfg(target_os = "macos")]
const NO_FRAMEWORK: i32 = 70;
const NO_START: i32 = 71;
const NO_HOST: i32 = 72;

/// Entry point for the browser process (and on Linux, CEF subprocesses).
pub fn run() -> i32 {
    start(std::ptr::null_mut())
}

/// Disables core dumps so a crashed renderer exits immediately instead of stalling the tab
/// while the OS writes a Chromium-sized core file.
#[cfg(target_os = "linux")]
fn no_core_files() {
    let none = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `none` is a valid `libc::rlimit` for the duration of the call.
    unsafe { libc::setrlimit(libc::RLIMIT_CORE, &raw const none) };
}

/// Entry point invoked by CEF's Windows bootstrap executable after initializing the sandbox.
#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "C" fn RunWinMain(
    _instance: *mut std::ffi::c_void,
    _command_line: *mut u16,
    _show: c_int,
    sandbox: *mut std::ffi::c_void,
    _version: *mut std::ffi::c_void,
) -> c_int {
    start(sandbox.cast())
}

fn start(sandbox: *mut u8) -> i32 {
    #[cfg(target_os = "linux")]
    no_core_files();
    #[cfg(target_os = "macos")]
    let Some(_framework) = mac::load_framework(false) else {
        return NO_FRAMEWORK;
    };
    #[cfg(target_os = "macos")]
    if !mac::install_application() {
        return NO_FRAMEWORK;
    }
    let _ = api_hash(cef_dll_sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let mut app = engine::app();
    let code = execute_process(Some(args.as_main_args()), Some(&mut app), sandbox);
    if code >= 0 {
        return code;
    }
    #[cfg(target_os = "linux")]
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("Kodosi Browser: Chromium's sandbox does not run as root.");
        return shared_browser::link::AS_ROOT;
    }
    filters::load();
    let options = match options::Options::parse(std::env::args()) {
        Ok(options) => options,
        Err(problem) => {
            eprintln!("Kodosi Browser: {problem}");
            return NO_HOST;
        }
    };
    let address = match options.take_address() {
        Ok(address) => address,
        Err(problem) => {
            eprintln!("Kodosi Browser: {problem}");
            return NO_HOST;
        }
    };
    let profile = options.profile.to_string_lossy().into_owned();
    let log = options.profile.join("engine.log");
    drop(std::fs::rename(&log, log.with_extension("log.1")));
    let settings = Settings {
        windowless_rendering_enabled: 1,
        root_cache_path: CefString::from(profile.as_str()),
        cache_path: CefString::from(profile.as_str()),
        persist_session_cookies: 1,
        remote_debugging_port: c_int::from(options.debug_port),
        log_file: CefString::from(log.to_string_lossy().as_ref()),
        log_severity: LogSeverity::WARNING,
        background_color: engine::OPAQUE_WHITE,
        // Use dedicated macOS Keychain names so Kodosi Browser does not share Chromium's cookie
        // encryption key. Never rename these values, or existing encrypted cookies and logins
        // become unreadable.
        keychain_service_name: CefString::from("Kodosi Browser Safe Storage"),
        keychain_account_name: CefString::from("Kodosi Browser"),
        ..Default::default()
    };
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        sandbox,
    ) != 1
    {
        return NO_START;
    }
    engine::start(link::connect(address));
    run_message_loop();
    shutdown();
    0
}

/// Entry point for macOS CEF helper subprocesses.
#[cfg(target_os = "macos")]
pub fn run_helper() -> i32 {
    let args = Args::new();
    let mut sandbox = sandbox::Sandbox::new();
    sandbox.initialize(args.as_main_args());
    let Some(_framework) = mac::load_framework(true) else {
        return NO_FRAMEWORK;
    };
    let _ = api_hash(cef_dll_sys::CEF_API_VERSION_LAST, 0);
    let mut app = engine::app();
    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    )
}
