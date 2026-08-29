#[cfg(all(feature = "tokio-console", not(tokio_unstable)))]
compile_error!(
    "feature `tokio-console` requires `--cfg tokio_unstable`; use `just rust-console-check` or set RUSTFLAGS=\"--cfg tokio_unstable\""
);

use tracing_subscriber::{
    EnvFilter, Layer, layer::SubscriberExt, registry::LookupSpan, util::SubscriberInitExt,
};

use crate::{AppError, Result};

pub(crate) fn install(filter: &str) -> Result<()> {
    let env_filter = env_filter(filter)?;

    #[cfg(feature = "tokio-console")]
    install_console_subscriber(env_filter);

    #[cfg(not(feature = "tokio-console"))]
    install_fmt_subscriber(env_filter);

    Ok(())
}

fn env_filter(filter: &str) -> Result<EnvFilter> {
    EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(filter))
        .map_err(|error| AppError::Unsupported {
            reason: format!("invalid log filter: {error}"),
        })
}

fn fmt_layer<S>() -> impl Layer<S> + Send + Sync + 'static
where
    S: tracing::Subscriber + for<'span> LookupSpan<'span>,
{
    tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .compact()
}

#[cfg(feature = "tokio-console")]
fn install_console_subscriber(env_filter: EnvFilter) {
    drop(
        tracing_subscriber::registry()
            .with(console_subscriber::spawn())
            .with(fmt_layer().with_filter(env_filter))
            .try_init(),
    );
}

#[cfg(not(feature = "tokio-console"))]
fn install_fmt_subscriber(env_filter: EnvFilter) {
    drop(
        tracing_subscriber::registry()
            .with(fmt_layer().with_filter(env_filter))
            .try_init(),
    );
}
