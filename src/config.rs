//! Application configuration loaded from `config.yaml` with environment variable overrides.
//!
//! The configuration file is expected to be named `config.yaml` and located in the
//! current working directory. Environment variables can override specific fields:
//!
//! | Environment Variable | Overrides        |
//! |----------------------|------------------|
//! | `LLM_API_KEY`        | `llm.api_key`    |
//! | `LLM_PROVIDER`       | `llm.provider`   |
//! | `TTS_API_URL`        | `tts.api_url`    |
//! | `WS_PORT`            | `websocket.port` |

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Top-level application configuration.
///
/// Mirrors the structure of `config.yaml` and is deserialized via `serde_yaml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub project: ProjectConfig,
    pub llm: LLMConfig,
    pub tts: TTSConfig,
    pub websocket: WebSocketConfig,
}

impl Config {
    /// Load configuration from `config.yaml` in the current directory.
    ///
    /// After parsing the YAML, environment variable overrides are applied:
    /// - `LLM_API_KEY`  → `llm.api_key`
    /// - `LLM_PROVIDER` → `llm.provider`
    /// - `TTS_API_URL`  → `tts.api_url`
    /// - `WS_PORT`      → `websocket.port`
    pub fn load() -> Result<Self> {
        let yaml_str = std::fs::read_to_string("config.yaml")
            .context("Failed to read config.yaml from current directory")?;

        let mut config: Config = serde_yaml::from_str(&yaml_str)
            .context("Failed to parse config.yaml — check YAML syntax")?;

        // Apply environment variable overrides
        config.apply_env_overrides();

        Ok(config)
    }

    /// Apply environment variable overrides to the parsed configuration.
    fn apply_env_overrides(&mut self) {
        if let Ok(api_key) = std::env::var("LLM_API_KEY") {
            if !api_key.is_empty() {
                self.llm.api_key = api_key;
            }
        }
        if let Ok(provider) = std::env::var("LLM_PROVIDER") {
            if !provider.is_empty() {
                self.llm.provider = provider;
            }
        }
        if let Ok(api_url) = std::env::var("TTS_API_URL") {
            if !api_url.is_empty() {
                self.tts.api_url = api_url;
            }
        }
        if let Ok(port_str) = std::env::var("WS_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                self.websocket.port = port;
            }
        }
    }
}

/// Project metadata configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectConfig {
    pub name: String,
    pub version: String,
    pub character: String,
    pub cv: String,
}

/// LLM (Large Language Model) API configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LLMConfig {
    /// LLM provider identifier (e.g. `"deepseek"`).
    pub provider: String,

    /// API key for the LLM service.
    ///
    /// **Prefer** reading via [`Self::api_key()`] which checks the `LLM_API_KEY`
    /// environment variable first.
    pub api_key: String,

    /// Base URL of the LLM API.
    pub base_url: String,

    /// Model name to use (e.g. `"deepseek-chat"`).
    pub model: String,

    /// Sampling temperature (0.0 – 2.0).
    pub temperature: f32,

    /// Maximum number of tokens to generate in a single response.
    pub max_tokens: u32,

    /// Whether to stream responses token-by-token.
    pub stream: bool,
}

impl LLMConfig {
    /// Return the API key, preferring the `LLM_API_KEY` environment variable.
    ///
    /// Falls back to the value stored in `config.yaml` if the env var is not set or empty.
    pub fn api_key(&self) -> String {
        std::env::var("LLM_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.api_key.clone())
    }
}

/// Text-to-Speech (TTS) service configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TTSConfig {
    /// URL of the GPT-SoVITS TTS API endpoint.
    pub api_url: String,

    /// Language code for synthesis (e.g. `"ja"` for Japanese).
    pub language: String,

    /// Model file paths and reference audio/text.
    pub model: TTSModelConfig,

    /// Generation parameters (sampling, speed, etc.).
    pub params: TTSParams,
}

/// GPT-SoVITS model file locations and reference material.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TTSModelConfig {
    /// Path to the GPT model checkpoint (`.ckpt`).
    pub gpt_path: String,

    /// Path to the SoVITS model weights (`.pth`).
    pub sovits_path: String,

    /// Path to the reference audio file used for voice cloning.
    pub ref_audio: String,

    /// Transcript text of the reference audio.
    pub ref_text: String,
}

/// TTS generation parameters.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TTSParams {
    /// Top-K sampling parameter.
    pub top_k: u32,

    /// Top-P (nucleus) sampling parameter.
    pub top_p: f32,

    /// Sampling temperature.
    pub temperature: f32,

    /// Speech speed multiplier (< 1.0 = slower, > 1.0 = faster).
    pub speed_factor: f32,

    /// Number of diffusion sampling steps.
    pub sample_steps: u32,

    /// Whether to apply super-sampling (audio upscaling).
    pub super_sampling: bool,

    /// Inference batch size.
    pub batch_size: u32,

    /// Whether to stream audio chunks as they are generated.
    pub streaming_mode: bool,
}

/// WebSocket server configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WebSocketConfig {
    /// Bind address (e.g. `"0.0.0.0"` to listen on all interfaces).
    pub host: String,

    /// TCP port to listen on.
    pub port: u16,

    /// Heart-beat interval in seconds.
    pub heartbeat_interval: u64,

    /// Maximum number of simultaneous WebSocket connections.
    pub max_connections: usize,

    /// Number of reconnection attempts for transient failures.
    pub reconnect_attempts: u32,

    /// Delay between reconnection attempts in milliseconds.
    pub reconnect_delay: u64,
}
