#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(kodosi_browser::run_helper());
}

#[cfg(not(target_os = "macos"))]
fn main() {
    std::process::exit(kodosi_browser::run());
}
