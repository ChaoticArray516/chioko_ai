//! Thin logging wrapper around the `tracing` ecosystem.
//!
//! Call [`init_logger`] once at the start of `main` to install a
//! `tracing_subscriber` that prints human-readable logs to stdout.
//!
//! Convenience re-exports of the `tracing` level macros are provided so that
//! downstream modules can write:
//!
//! ```ignore
//! use ellen_rust_backend::logger::{info, debug, error};
//! info!(target = "ws", "Client connected");
//! ```

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize the global tracing subscriber.
///
/// Reads the `RUST_LOG` environment variable; if unset, defaults to `INFO`.
/// Output includes ANSI colours, timestamps, and target metadata.
///
/// # Example
///
/// ```ignore
/// fn main() {
///     ellen_rust_backend::logger::init_logger();
///     // …
/// }
/// ```
pub fn init_logger() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

// ---------------------------------------------------------------------------
// Re-exports of tracing macros for convenience
// ---------------------------------------------------------------------------

/// Re-export of [`tracing::trace`].
pub use tracing::trace;

/// Re-export of [`tracing::debug`].
pub use tracing::debug;

/// Re-export of [`tracing::info`].
pub use tracing::info;

/// Re-export of [`tracing::warn`].
pub use tracing::warn;

/// Re-export of [`tracing::error`].
pub use tracing::error;
