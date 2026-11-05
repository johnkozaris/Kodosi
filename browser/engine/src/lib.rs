//! Kodosi Browser: Chromium through CEF, with no window. It renders each page off screen, encodes
//! the page that its host watches, and talks with its host on one local link.

mod client;
mod cursor;
mod encode;
mod engine;
mod link;
#[cfg(target_os = "macos")]
mod mac;
mod options;
mod renderer;

use std::os::raw::c_int;

use cef::{args::Args, *};

const NO_FRAMEWORK: i32 = 70;
const NO_START: i32 = 71;
const NO_HOST: i32 = 72;

/// The browser process, and on Linux and Windows also each of its child processes.
pub fn run() -> i32 {
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
    let code = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if code >= 0 {
        return code;
    }
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
    let settings = Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: c_int::from(!cfg!(target_os = "macos")),
        root_cache_path: CefString::from(profile.as_str()),
        cache_path: CefString::from(profile.as_str()),
        persist_session_cookies: 1,
        remote_debugging_port: c_int::from(options.debug_port),
        log_file: CefString::from(
            options
                .profile
                .join("engine.log")
                .to_string_lossy()
                .as_ref(),
        ),
        log_severity: LogSeverity::WARNING,
        background_color: engine::OPAQUE_WHITE,
        ..Default::default()
    };
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
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
