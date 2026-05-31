//! Ellen AI Rust Backend — Main entry point.
//!
//! Orchestrates the full startup sequence:
//! 1. Initialise tracing (structured logging)
//! 2. Load configuration (`config.yaml` + env overrides)
//! 3. Build shared [`AppState`]
//! 4. Start the WebSocket server (axum, port 8081)
//! 5. Block until a shutdown signal (Ctrl-C / SIGTERM) is received
//! 6. Graceful teardown
//!
//! ```bash
//! cargo run --release
//! ```

use std::sync::Arc;

use ellen_rust_backend::config::Config;
use ellen_rust_backend::logger;
use ellen_rust_backend::state::AppState;
use ellen_rust_backend::ws_server;

/// Maximum seconds to wait for active connections to drain during shutdown.
const SHUTDOWN_TIMEOUT_SECS: u64 = 10;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── 1. Logging ───────────────────────────────────────────────────────────
    logger::init_logger();

    let version = env!("CARGO_PKG_VERSION");
    let name = env!("CARGO_PKG_NAME");
    logger::info!("{} v{} starting up…", name, version);

    // ── 2. Configuration ─────────────────────────────────────────────────────
    let config = Config::load().map_err(|e| {
        logger::error!("Failed to load configuration: {}", e);
        e
    })?;

    let host = config.websocket.host.clone();
    let port = config.websocket.port;
    logger::info!(
        character = %config.project.character,
        llm_provider = %config.llm.provider,
        llm_model = %config.llm.model,
        tts_url = %config.tts.api_url,
        "Configuration loaded"
    );

    // ── 3. Shared application state ──────────────────────────────────────────
    let state: Arc<AppState> = AppState::new(config);
    logger::info!("Application state initialised");

    // ── 4. Start WebSocket server ────────────────────────────────────────────
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server_state = Arc::clone(&state);
    let server_handle = tokio::spawn(async move {
        if let Err(e) = ws_server::start_server(server_state, shutdown_rx).await {
            logger::error!("WebSocket server error: {}", e);
        }
    });

    logger::info!(
        "Ellen Rust Backend running on ws://{}:{}",
        host, port
    );
    logger::info!("Press Ctrl+C to shut down gracefully");

    // ── 5. Wait for shutdown signal ──────────────────────────────────────────
    tokio::signal::ctrl_c().await?;
    logger::info!("Ctrl+C received, initiating graceful shutdown…");

    // Signal the axum server to stop accepting new connections.
    let _ = shutdown_tx.send(());

    // ── 6. Graceful teardown ─────────────────────────────────────────────────
    logger::info!(
        "Waiting up to {}s for connections to drain…",
        SHUTDOWN_TIMEOUT_SECS
    );
    match tokio::time::timeout(
        tokio::time::Duration::from_secs(SHUTDOWN_TIMEOUT_SECS),
        server_handle,
    )
    .await
    {
        Ok(Ok(())) => logger::info!("Server shut down cleanly"),
        Ok(Err(join_err)) => logger::warn!("Server task panicked: {}", join_err),
        Err(_) => logger::warn!("Server shutdown timed out after {}s", SHUTDOWN_TIMEOUT_SECS),
    }

    logger::info!("Shutdown complete. さようなら、ご主人様。");
    Ok(())
}
