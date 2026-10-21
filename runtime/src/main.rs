fn main() -> std::process::ExitCode {
    kodosi_os::process::reset_signals();
    let Ok(runtime) = tokio::runtime::Runtime::new() else {
        return std::process::ExitCode::FAILURE;
    };
    std::process::ExitCode::from(
        runtime.block_on(kodosi_runtime::cli::run_with(std::env::args_os().collect())),
    )
}
