#[tokio::main(flavor = "multi_thread")]
async fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(kodosi_runtime::cli::run_with(std::env::args_os().collect()).await)
}
