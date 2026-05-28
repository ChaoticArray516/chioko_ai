//! Application shared state, accessible from all request handlers and WebSocket tasks.
//!
//! Wraps the parsed [`Config`], a shared HTTP client, and the character [`Persona`].
//! Use [`Arc<AppState>`] to share ownership across async boundaries.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Client as HttpClient;

use crate::config::Config;
use crate::persona::Persona;

/// Global application state shared across all handlers.
///
/// Created once at startup via [`AppState::new`] and then cloned as an [`Arc`]
/// into every Axum route handler and WebSocket connection task.
pub struct AppState {
    /// Parsed application configuration (YAML + env overrides).
    pub config: Config,

    /// Shared HTTP client with a 60-second request timeout.
    ///
    /// Re-using a single client instance enables connection pooling
    /// and avoids the cost of creating a new client per request.
    pub http_client: HttpClient,

    /// Character persona data (system prompt fragments, mood state, etc.).
    pub persona: Persona,
}

impl AppState {
    /// Construct a new [`AppState`] and wrap it in an [`Arc`].
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be built (extremely unlikely with the
    /// default TLS configuration).
    pub fn new(config: Config) -> Arc<Self> {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");

        Arc::new(Self {
            config,
            http_client,
            persona: Persona::new(),
        })
    }
}
