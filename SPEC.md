# Ellen AI Rust Backend — Specification

## Overview
Replace TypeScript/Node.js backend with Rust. Single binary, zero frontend changes.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Rust Backend (port 8081)                  │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ WS Server │  │ LLM Client│  │TTS Bridge│  │  Parser   │   │
│  │ (axum ws) │  │(DeepSeek) │  │(GPT-SoVIT)│  │(tag extr) │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
│       │              │              │              │         │
│       └──────────────┴──────────────┴──────────────┘         │
│                          │                                   │
│                   ┌──────┴──────┐                          │
│                   │   AppState  │                          │
│                   └──────┬──────┘                          │
│                          │                                   │
│                   ┌──────┴──────┐                          │
│                   │   Config    │                          │
│                   └─────────────┘                          │
└─────────────────────────────────────────────────────────────┘
        ▲                                  │ HTTP POST
        │ WS (json)                        ▼
┌───────┴────────┐              ┌─────────────────────┐
│  React Frontend │              │ Python GPT-SoVITS   │
│  (Live2D)       │              │ 127.0.0.1:9880      │
└─────────────────┘              └─────────────────────┘
```

## Port & Host
- WebSocket: `8081` (matching config.yaml `websocket.port` and frontend WS_URL)
- Host: `0.0.0.0`

## WebSocket Message Protocol (MUST match existing frontend exactly)

### Client → Server
```json
{"type": "message", "content": "user text", "timestamp": 1700000000000}
```

### Server → Client: Status
```json
{"type": "status", "status": "thinking", "timestamp": 1700000000000}
{"type": "status", "status": "speaking", "timestamp": 1700000000000}
{"type": "status", "status": "ready", "timestamp": 1700000000000}
{"type": "status", "status": "error", "message": "error desc", "timestamp": 1700000000000}
```

### Server → Client: Multimodal Sync (main payload)
```json
{
  "type": "multimodal_sync",
  "motionId": "idle",
  "expressionId": "lazy",
  "text": "clean text without tags",
  "hasAudio": true,
  "audioData": "base64encoded...",
  "sampleRate": 32000,
  "duration": 3.5,
  "timestamp": 1700000000000
}
```

Note: `hasAudio: false` when TTS is offline/failed. `audioData` omitted when no audio.

## File Structure

```
ellen_rust_backend/
├── Cargo.toml
├── config.yaml
├── .env.example
├── src/
│   ├── main.rs           # tokio runtime, graceful shutdown, service orchestration
│   ├── lib.rs            # module declarations
│   ├── config.rs         # Config loading from YAML + env overrides
│   ├── state.rs          # AppState: shared config, HTTP client, WS connections
│   ├── ws_server.rs      # WebSocket server (axum), heartbeat, message handling
│   ├── llm_client.rs     # DeepSeek API client with SSE streaming
│   ├── tts_bridge.rs     # GPT-SoVITS HTTP bridge, cache, retry
│   ├── parser.rs         # [motion:xxx][exp:yyy] tag extraction
│   ├── persona.rs        # Ellen system prompt (hardcoded Japanese)
│   └── logger.rs         # tracing wrapper (optional, can use tracing directly)
├── start.sh
└── start.bat
```

## Module Specifications

### config.rs
- Load `config.yaml` from project root (same dir as binary)
- Env overrides: `LLM_API_KEY` > `config.llm.api_key`, `LLM_PROVIDER`, `TTS_API_URL`, `WS_PORT`
- Serde structs matching config.yaml structure
- `Config::load() -> Result<Self>`
- `Config::llm_api_key() -> String` (reads env var LLM_API_KEY first)

### state.rs
```rust
pub struct AppState {
    pub config: Config,
    pub http_client: reqwest::Client,
    pub persona: Persona,
}

impl AppState {
    pub fn new(config: Config) -> Self;
}
```

### persona.rs
- `Persona::system_prompt() -> &'static str` — returns full Japanese prompt
- Content matches existing TypeScript `EllenPersona.getSystemPrompt()` exactly

### parser.rs
```rust
#[derive(Debug, Clone, Default)]
pub struct ParsedResponse {
    pub motion_id: String,
    pub expression_id: String,
    pub clean_text: String,
    pub raw_text: String,
}

pub fn parse_llm_response(raw: &str) -> ParsedResponse;
```
- Regex: `\[motion:([a-zA-Z0-9_]+)\]`, `\[exp:([a-zA-Z0-9_]+)\]`
- Valid motions: `["idle", "idle2"]`
- Valid expressions: `["lazy", "maid", "predator", "hangry", "shy", "surprised", "happy"]`
- Invalid tags → defaults (`idle`, `lazy`)
- TAG_STRIP regex removes all `[motion:xxx]` and `[exp:yyy]` tags

### llm_client.rs
```rust
pub async fn stream_chat(
    state: &AppState,
    user_message: &str,
) -> Result<String, LLMError>;
```
- POST to `https://api.deepseek.com/chat/completions`
- Headers: `Authorization: Bearer {api_key}`, `Content-Type: application/json`
- Body: `{ "model": "deepseek-chat", "messages": [{"role":"system","content":PROMPT},{"role":"user","content":MSG}], "stream": true, "temperature": 0.7, "max_tokens": 1000 }`
- SSE streaming via `eventsource-stream` crate
- Collects full response into a single String
- Returns the complete LLM text response

### tts_bridge.rs
```rust
#[derive(Debug, Clone)]
pub struct TTSResult {
    pub audio_data: String,  // base64
    pub sample_rate: u32,
    pub duration: f64,
    pub motion_id: String,
    pub expression_id: String,
    pub text: String,
}

pub async fn synthesize(
    state: &AppState,
    parsed: &ParsedResponse,
) -> Option<TTSResult>;
```
- Check cache first (LRU+TTL, 50 max, 30min TTL)
- POST to `{tts_api_url}/tts` with form data or JSON
- Exponential backoff retry: 1s → 2s → 4s (max 3 attempts)
- Parse WAV duration from buffer header
- Cache result on success
- Returns `None` on failure (graceful degradation)

### ws_server.rs
- Axum WebSocket server on `0.0.0.0:8081`
- `ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>)`
- Per-connection tasks:
  - Read loop: parse JSON, extract `type: "message"`, call handler
  - Write loop: send responses back
- Heartbeat: server sends ping every 30s, terminates dead connections
- `broadcast_multimodal()` — send multimodal_sync to all clients
- `broadcast_status()` — send status to all clients

### main.rs
```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Load config
    // 2. Create AppState
    // 3. Start WS server (spawns axum)
    // 4. Register Ctrl+C handler for graceful shutdown
    // 5. Wait on shutdown signal
}
```

## Dependencies (Cargo.toml)
```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
axum = { version = "0.7", features = ["ws"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
reqwest = { version = "0.12", features = ["json", "stream", "rustls-tls"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
config = "0.14"
futures = "0.3"
tokio-tungstenite = "0.21"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1.0"
thiserror = "1.0"
eventsource-stream = "0.2"
bytes = "1"
sha2 = "0.10"
```

## Error Handling
- All errors return JSON to frontend via `{"type":"status","status":"error","message":"..."}`
- Never panic on client-facing paths
- LLM errors: log error, return error status to client
- TTS errors: log warning, return `hasAudio: false` in multimodal_sync
- WebSocket parse errors: log warning, ignore invalid message

## Startup Script Logic (start.sh)
1. Check env var `LLM_API_KEY` exists (non-empty)
2. Check `127.0.0.1:9880` reachable (nc/telnet/curl timeout 3s)
3. `cargo run --release`
4. Output: "Ellen Rust Backend running on ws://0.0.0.0:8081"
