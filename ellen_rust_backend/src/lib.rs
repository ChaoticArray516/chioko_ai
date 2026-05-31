//! Ellen AI — Rust Backend
//!
//! A Rust rewrite of the TypeScript backend for the Ellen Joe AI companion.
//! Provides WebSocket API, LLM integration (DeepSeek), and TTS integration (GPT-SoVITS).

pub mod config;
pub mod logger;
pub mod llm_client;
pub mod parser;
pub mod persona;
pub mod state;
pub mod tts_bridge;
pub mod ws_server;
