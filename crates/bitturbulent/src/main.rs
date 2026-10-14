use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "bitturbulent starting");
    tracing::debug!("debug logging enabled");

    Ok(())
}

/// Logs to stderr. The level comes from `RUST_LOG`, defaulting to `info`.
fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
