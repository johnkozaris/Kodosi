#![recursion_limit = "256"]

#[tokio::main(flavor = "multi_thread")]
async fn main() -> std::process::ExitCode {
    kodosi_runtime::run_cli().await
}
