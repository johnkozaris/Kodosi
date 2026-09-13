#[tokio::main(flavor = "multi_thread")]
async fn main() -> std::process::ExitCode {
    kodosi_runtime::cli::run().await
}
