mod client;
mod cursor;
mod encode;
mod engine;
mod filters;
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

/// The browser process, and on Linux also each of its child processes.
pub fn run() -> i32 {
    start(std::ptr::null_mut())
}

/// A page process that crashes ends at once. Otherwise the system first writes a core file of
/// Chromium's size, which takes a minute, and the tab waits for it.
#[cfg(target_os = "linux")]
fn no_core_files() {
    let none = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: `setrlimit` only reads the limit, which lives for the call.
    unsafe { libc::setrlimit(libc::RLIMIT_CORE, &raw const none) };
}

/// CEF's bootstrap program calls this in each process on Windows, once it started the sandbox.
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
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let args = Args::new();
    let mut app = engine::app();
    let code = execute_process(Some(args.as_main_args()), Some(&mut app), sandbox);
    if code >= 0 {
        return code;
    }
    #[cfg(target_os = "linux")]
    // SAFETY: `geteuid` only reads the user of this process.
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

/// A child process on a Mac, which runs from one of the helper apps.
#[cfg(target_os = "macos")]
pub fn run_helper() -> i32 {
    let args = Args::new();
    let mut sandbox = sandbox::Sandbox::new();
    sandbox.initialize(args.as_main_args());
    let Some(_framework) = mac::load_framework(true) else {
        return NO_FRAMEWORK;
    };
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);
    let mut app = engine::app();
    execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    )
}
