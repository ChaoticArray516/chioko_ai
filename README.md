# Ellen AI Rust Backend

Rust rewrite of the Ellen Joe AI Companion backend. Single-binary deployment, zero frontend changes.

## Architecture

```
Frontend (React+Live2D)  <--WebSocket (port 8081)-->  Rust Backend  <--HTTP-->  GPT-SoVITS TTS (port 9880)
                                                          |
                                                          +--HTTPS-->  DeepSeek LLM API
```

| Component | Tech | Note |
|-----------|------|------|
| Frontend | React + Live2D | **Unchanged** — same WebSocket JSON protocol |
| TTS | Python GPT-SoVITS v4 | **Unchanged** — Rust calls `127.0.0.1:9880` |
| LLM | DeepSeek API | **Unchanged** — Rust calls `api.deepseek.com` |
| Backend | ~~TypeScript/Node.js~~ → **Rust** | Single binary, tokio + axum |

## Quick Start

### Prerequisites

- Rust ≥ 1.75 (install via [rustup](https://rustup.rs/))
- GPT-SoVITS v4 running on port 9880
- DeepSeek API Key

### 1. Clone & Enter Directory

```bash
cd ellen_rust_backend
```

### 2. Configure Environment

```bash
cp .env.example .env
# Edit .env and set your DeepSeek API Key:
# LLM_API_KEY=sk-your-api-key-here
```

Or export directly:
```bash
export LLM_API_KEY=sk-your-api-key-here
```

### 3. Start GPT-SoVITS TTS (in another terminal)

```bash
cd /path/to/GPT-SoVITS
python api_v2.py -a 127.0.0.1 -p 9880
```

### 4. Run the Backend

**Linux/macOS:**
```bash
chmod +x start.sh
./start.sh
```

**Windows:**
```powershell
.\start.bat
```

**Manual (with cargo):**
```bash
cargo run --release
```

Expected output:
```
Ellen Rust Backend running on ws://0.0.0.0:8081
Press Ctrl+C to shut down gracefully
```

### 5. Connect Frontend

The frontend connects automatically to `ws://localhost:8081`.

If running the frontend separately:
```bash
cd packages/frontend
npm install
npm run dev
# Open http://localhost:5173
```

## Project Structure

```
ellen_rust_backend/
├── Cargo.toml              # Dependencies & build config
├── config.yaml             # Runtime configuration
├── .env.example            # Environment variable template
├── src/
│   ├── main.rs             # Entry point, graceful shutdown
│   ├── lib.rs              # Module declarations
│   ├── config.rs           # YAML config + env overrides
│   ├── state.rs            # Shared AppState (Arc)
│   ├── ws_server.rs        # WebSocket server (axum)
│   ├── llm_client.rs       # DeepSeek API with SSE streaming
│   ├── tts_bridge.rs       # GPT-SoVITS HTTP bridge + cache
│   ├── parser.rs           # [motion:xxx][exp:yyy] tag parser
│   ├── persona.rs          # Ellen system prompt (Japanese)
│   └── logger.rs           # tracing subscriber setup
├── start.sh                # Linux/macOS startup script
└── start.bat               # Windows startup script
```

## Configuration

Edit `config.yaml`:

```yaml
llm:
  provider: deepseek
  api_key: ""              # Override via LLM_API_KEY env var
  model: "deepseek-chat"
  temperature: 0.7
  max_tokens: 1000

tts:
  api_url: "http://127.0.0.1:9880"
  language: "ja"
  model:
    ref_audio: "path/to/reference.wav"
    ref_text: "参考文本"
  params:
    speed_factor: 0.9
    sample_steps: 32

websocket:
  host: "0.0.0.0"
  port: 8081               # Frontend connects here
```

### Environment Variable Overrides

| Variable | Overrides |
|----------|-----------|
| `LLM_API_KEY` | `llm.api_key` |
| `LLM_PROVIDER` | `llm.provider` |
| `TTS_API_URL` | `tts.api_url` |
| `WS_PORT` | `websocket.port` |

## WebSocket Protocol

### Client → Server
```json
{"type": "message", "content": "Hello Ellen", "timestamp": 1700000000000}
```

### Server → Client: Status
```json
{"type": "status", "status": "thinking"}
{"type": "status", "status": "speaking"}
{"type": "status", "status": "ready"}
{"type": "status", "status": "error", "message": "description"}
```

### Server → Client: Multimodal Sync
```json
{
  "type": "multimodal_sync",
  "motionId": "idle",
  "expressionId": "lazy",
  "text": "おはよう、ご主人様。",
  "hasAudio": true,
  "audioData": "base64WAV...",
  "sampleRate": 32000,
  "duration": 2.5,
  "timestamp": 1700000000000
}
```

## Testing

```bash
# Run unit tests
cargo test

# Run with logging
cargo run --release

# Verbose logging
RUST_LOG=debug cargo run --release
```

## Building for Production

```bash
# Optimized release build
cargo build --release

# Binary location:
# ./target/release/ellen_rust_backend

# Copy binary + config.yaml to deployment directory
cp target/release/ellen_rust_backend ./ellen_backend
cp config.yaml ./config.yaml
./ellen_backend
```

## Graceful Shutdown

Press `Ctrl+C` to trigger graceful shutdown:
1. Stop accepting new WebSocket connections
2. Allow in-flight requests to complete (up to 10s)
3. Close all existing connections
4. Exit cleanly

## Troubleshooting

| Issue | Solution |
|-------|----------|
| `Failed to read config.yaml` | Ensure `config.yaml` is in the current directory |
| `LLM_API_KEY is required` | Set `LLM_API_KEY` env var or in `.env` file |
| TTS not responding | Ensure GPT-SoVITS is running on `127.0.0.1:9880` |
| Port 8081 in use | Change `WS_PORT` env var or edit `config.yaml` |

## License

MIT License
