//! WebSocket server module for the Ellen AI backend.
//!
//! Handles bidirectional WebSocket communication with the frontend using axum's
//! built-in WebSocket support. Each connection spawns concurrent read and write
//! tasks, with a heartbeat mechanism and broadcast support for status messages.
//!
//! ## Message Flow
//!
//! ```text
//! Frontend ──"message"──> Read Task ──process_message()──> mpsc::channel
//!                                                         Write Task ──> Frontend
//!                                                         (status / multimodal_sync)
//! ```
//!
//! ## Frontend Protocol
//!
//! **Incoming:**
//! ```json
//! {"type": "message", "content": "user input", "timestamp": 1700000000000}
//! ```
//!
//! **Outgoing:**
//! - Status: `{"type": "status", "status": "thinking|speaking|ready|error", ...}`
//! - MultimodalSync: `{"type": "multimodal_sync", "motionId": "...", ...}`

use std::sync::Arc;

use axum::{
    extract::{State, WebSocketUpgrade},
    extract::ws::{CloseFrame, Message, WebSocket},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

use crate::llm_client;
use crate::parser::parse_llm_response;
use crate::state::AppState;
use crate::tts_bridge;

// ---------------------------------------------------------------------------
//  Data models
// ---------------------------------------------------------------------------

/// Message types sent **to** connected WebSocket clients.
///
/// Uses `#[serde(tag = "type")]` so that the `type` JSON field drives variant
/// selection.  Field names are camelCase to match the frontend contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WSMessage {
    /// A multimodal response packet containing text, motion, expression and
    /// optionally base-64 encoded WAV audio.
    #[serde(rename = "multimodal_sync")]
    MultimodalSync {
        /// Animation motion identifier, e.g. `"idle"`.
        motionId: String,
        /// Facial expression identifier, e.g. `"lazy"`.
        expressionId: String,
        /// Clean text (all tags stripped) for display / subtitles.
        text: String,
        /// Whether the packet includes audio data.
        hasAudio: bool,
        /// Base-64 encoded WAV audio.  Omitted when `has_audio` is `false`.
        #[serde(skip_serializing_if = "Option::is_none")]
        audioData: Option<String>,
        /// Audio sample rate in Hz (e.g. 32 000).
        sampleRate: u32,
        /// Audio duration in seconds.
        duration: f64,
        /// Milliseconds since UNIX epoch.
        timestamp: u64,
    },

    /// A lifecycle status update sent to the frontend.
    #[serde(rename = "status")]
    Status {
        /// One of `thinking`, `speaking`, `ready`, `error`.
        status: String,
        /// Human-readable message, present only for `error` status.
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        /// Milliseconds since UNIX epoch.
        timestamp: u64,
    },
}

/// Incoming JSON message from the frontend.
#[derive(Debug, Deserialize)]
struct ClientMessage {
    /// Always `"message"` in the current protocol.
    #[serde(rename = "type")]
    msg_type: String,
    /// Raw text typed by the user.
    content: String,
    /// Client-generated timestamp (optional).
    #[serde(default)]
    timestamp: Option<u64>,
}

// ---------------------------------------------------------------------------
//  Connection tracking
// ---------------------------------------------------------------------------

/// Number of slots reserved in each per-client MPSC channel.
const CHANNEL_CAPACITY: usize = 64;

/// Seconds between heartbeat pings.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Seconds to wait for a pong before force-closing a connection.
const HEARTBEAT_TIMEOUT_SECS: u64 = 60;

/// Maximum number of (user, assistant) message pairs kept in conversation history.
const MAX_HISTORY: usize = 20;

/// Global, lazily-initialised list of connected client senders.
///
/// A `Mutex<Vec<mpsc::Sender<WSMessage>>>` is used instead of `broadcast`
/// because each client needs its own back-pressured channel and we want
/// messages to be delivered even to clients that join *after* the message
/// is sent (broadcast would drop the message for late joiners).
static CONNECTED_CLIENTS: std::sync::OnceLock<Mutex<Vec<mpsc::Sender<WSMessage>>>> =
    std::sync::OnceLock::new();

/// Initialise (or return) the global client list.
fn get_client_list() -> &'static Mutex<Vec<mpsc::Sender<WSMessage>>> {
    CONNECTED_CLIENTS.get_or_init(|| Mutex::new(Vec::new()))
}

/// Add a new sender to the global client list.
async fn add_client(sender: mpsc::Sender<WSMessage>) {
    let mut clients = get_client_list().lock().await;
    clients.push(sender);
    let count = clients.len();
    drop(clients);
    info!("new WebSocket connection; total clients: {}", count);
}

/// Remove a sender from the global client list by matching channel capacity.
///
/// In practice we compare `sender.capacity()` as a cheap proxy; since MPSC
/// senders are cloned per-task this works well enough for bookkeeping.
async fn remove_client(target: &mpsc::Sender<WSMessage>) {
    let mut clients = get_client_list().lock().await;
    // Drain and re-insert all *other* senders.
    let mut keep = Vec::with_capacity(clients.len().saturating_sub(1));
    for s in clients.drain(..) {
        if s.same_channel(target) {
            continue;
        }
        keep.push(s);
    }
    let removed = clients.len() + 1 != keep.len() + 1; // we removed one
    let count = keep.len();
    *clients = keep;
    drop(clients);
    if removed {
        info!("WebSocket client disconnected; remaining clients: {}", count);
    } else {
        warn!("tried to remove a client that was not in the list");
    }
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

/// Return the current wall-clock time as milliseconds since the UNIX epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
//  Public API
// ---------------------------------------------------------------------------

/// Build the axum router and start listening on the configured host:port.
///
/// Accepts a [`tokio::sync::oneshot::Receiver<()>`] for graceful shutdown
/// coordination with the main task.
///
/// # Errors
///
/// Returns an error if the TCP listener cannot be bound or the axum server
/// exits abnormally.
pub async fn start_server(
    state: Arc<AppState>,
    shutdown_rx: tokio::sync::oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let addr = format!("{}:{}", state.config.websocket.host, state.config.websocket.port);

    let app = Router::new()
        .route("/", get(ws_handler))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("WebSocket server listening on ws://{}/", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
            info!("shutdown signal received, stopping WS server…");
        })
        .await?;
    Ok(())
}

/// Send a status message to **all** currently connected WebSocket clients.
///
/// Non-blocking: messages are fire-and-forget via `try_send` so slow clients
/// do not block the caller.
pub async fn broadcast_status(status: &str, message: Option<String>) {
    let payload = WSMessage::Status {
        status: status.to_string(),
        message,
        timestamp: now_ms(),
    };

    let clients = get_client_list().lock().await;
    let mut dropped = Vec::new();
    for (idx, sender) in clients.iter().enumerate() {
        if let Err(e) = sender.try_send(payload.clone()) {
            debug!("broadcast_status: client {} channel full or closed: {}", idx, e);
            dropped.push(idx);
        }
    }
    let count = clients.len();
    drop(clients);

    if !dropped.is_empty() {
        debug!(
            "broadcast_status: {} / {} clients dropped",
            dropped.len(),
            count
        );
    }
}

/// Send a `multimodal_sync` packet to **all** currently connected clients.
///
/// Use this when audio / animation data is produced outside the normal
/// per-message pipeline (e.g. push notifications, system events).
pub async fn broadcast_multimodal(
    motion_id: &str,
    expression_id: &str,
    text: &str,
    has_audio: bool,
    audio_data: Option<String>,
    sample_rate: u32,
    duration: f64,
) {
    let payload = WSMessage::MultimodalSync {
        motionId: motion_id.to_string(),
        expressionId: expression_id.to_string(),
        text: text.to_string(),
        hasAudio: has_audio,
        audioData: audio_data,
        sampleRate: sample_rate,
        duration,
        timestamp: now_ms(),
    };

    let clients = get_client_list().lock().await;
    let mut dropped = Vec::new();
    for (idx, sender) in clients.iter().enumerate() {
        if let Err(e) = sender.try_send(payload.clone()) {
            debug!(
                "broadcast_multimodal: client {} channel full or closed: {}",
                idx, e
            );
            dropped.push(idx);
        }
    }
    let count = clients.len();
    drop(clients);

    if !dropped.is_empty() {
        debug!(
            "broadcast_multimodal: {} / {} clients dropped",
            dropped.len(),
            count
        );
    }
}

// ---------------------------------------------------------------------------
//  Internal handlers
// ---------------------------------------------------------------------------

/// axum handler that upgrades an HTTP request to a WebSocket connection.
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

/// Handle a single WebSocket connection from upgrade until close.
///
/// Spawns three concurrent units of work:
/// 1. **Read task**   – deserialises incoming JSON and drives the LLM → TTS
///    pipeline, sending outbound `WSMessage`s over an MPSC channel.
/// 2. **Write task**  – pulls messages from the MPSC channel and forwards
///    them to the WebSocket.
/// 3. **Heartbeat**   – sends `Message::Ping` every 30 s from the write side.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Channel that the read task uses to feed the write task.
    let (out_tx, mut out_rx) = mpsc::channel::<WSMessage>(CHANNEL_CAPACITY);

    // Register this client so that `broadcast_*` functions can reach it.
    add_client(out_tx.clone()).await;

    // ------------------------------------------------------------------
    //  Heartbeat: shared atomic flag set by the read task when a pong
    //  or any frame arrives.
    // ------------------------------------------------------------------
    let alive = Arc::new(Mutex::new(true));
    let alive_read = alive.clone();
    let alive_write = alive.clone();

    // ------------------------------------------------------------------
    //  Write task
    // ------------------------------------------------------------------
    let write_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS));

        loop {
            tokio::select! {
                // Forward outbound messages to the WebSocket.
                maybe_msg = out_rx.recv() => {
                    match maybe_msg {
                        Some(msg) => {
                            let json = match serde_json::to_string(&msg) {
                                Ok(j) => j,
                                Err(e) => {
                                    error!("failed to serialise WSMessage: {}", e);
                                    continue;
                                }
                            };
                            debug!(">>> {}", json);
                            if let Err(e) = ws_sender.send(Message::Text(json)).await {
                                warn!("WebSocket send error (client probably gone): {}", e);
                                break;
                            }
                        }
                        None => {
                            // Channel closed – read task has finished.
                            break;
                        }
                    }
                }

                // Periodic heartbeat ping.
                _ = interval.tick() => {
                    // Check whether the read task has signalled life recently.
                    let is_alive = *alive_write.lock().await;
                    if !is_alive {
                        warn!("heartbeat timeout – closing connection");
                        let _ = ws_sender
                            .send(Message::Close(Some(CloseFrame {
                                code: axum::extract::ws::close_code::NORMAL,
                                reason: std::borrow::Cow::Borrowed("heartbeat timeout"),
                            })))
                            .await;
                        break;
                    }

                    // Send ping and clear the alive flag.  The read task will
                    // set it back to `true` when the corresponding pong (or
                    // any other frame) arrives.
                    if let Err(e) = ws_sender.send(Message::Ping(vec![])).await {
                        warn!("failed to send heartbeat ping: {}", e);
                        break;
                    }
                    *alive_write.lock().await = false;
                    debug!("heartbeat ping sent");
                }
            }
        }

        // Graceful close – attempt to flush remaining messages.
        let _ = ws_sender.close().await;
    });

    // ------------------------------------------------------------------
    //  Read task
    // ------------------------------------------------------------------
    let out_tx_read = out_tx.clone();
    let mut conversation_history: Vec<(String, String)> = Vec::new();
    let read_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            // Any incoming frame (text, binary, pong) counts as "alive".
            *alive_read.lock().await = true;

            match msg {
                Message::Text(text) => {
                    debug!("<<< {}", text);
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(client_msg) if client_msg.msg_type == "message" => {
                            process_message(
                                &client_msg.content,
                                &state,
                                &out_tx_read,
                                &mut conversation_history,
                            )
                            .await;
                        }
                        Ok(client_msg) => {
                            warn!(
                                "unknown client message type '{}', ignoring",
                                client_msg.msg_type
                            );
                        }
                        Err(e) => {
                            warn!("failed to parse client message: {}", e);
                            let _ = out_tx_read
                                .send(WSMessage::Status {
                                    status: "error".to_string(),
                                    message: Some(format!("invalid JSON: {}", e)),
                                    timestamp: now_ms(),
                                })
                                .await;
                        }
                    }
                }
                Message::Binary(_) => {
                    warn!("received unexpected binary frame, ignoring");
                }
                Message::Ping(data) => {
                    // axum automatically responds with pong; just log.
                    debug!("received ping ({} bytes)", data.len());
                }
                Message::Pong(_) => {
                    debug!("received pong");
                }
                Message::Close(frame) => {
                    if let Some(ref f) = frame {
                        info!(
                            "client closed connection: code={}, reason='{}'",
                            f.code, f.reason
                        );
                    } else {
                        info!("client closed connection (no close frame)");
                    }
                    break;
                }
            }
        }

        // Drop our end of the outbound channel so the write task exits.
        drop(out_tx_read);
    });

    // Wait for either task to finish, then abort the other.
    tokio::select! {
        _ = read_task => {
            debug!("read task ended");
        }
        _ = write_task => {
            debug!("write task ended");
        }
    }

    // Clean up global client list.
    remove_client(&out_tx).await;
}

// ---------------------------------------------------------------------------
//  Message processing pipeline
// ---------------------------------------------------------------------------

/// Run the full LLM → parse → TTS → multimodal_sync pipeline for a single
/// user message.
///
/// Status sequence:
/// 1. `thinking`
/// 2. `speaking`
/// 3. `multimodal_sync` (with optional audio)
/// 4. `ready`
///
/// On error: `error` → 1 s sleep → `ready`.
async fn process_message(
    content: &str,
    state: &AppState,
    sender: &mpsc::Sender<WSMessage>,
    history: &mut Vec<(String, String)>,
) {
    // 1. thinking
    if sender
        .send(WSMessage::Status {
            status: "thinking".to_string(),
            message: None,
            timestamp: now_ms(),
        })
        .await
        .is_err()
    {
        warn!("process_message: client channel closed during thinking status");
        return;
    }

    // Append user message to history before calling the LLM.
    history.push(("user".to_string(), content.to_string()));

    // 2. Call LLM (passes full history for multi-turn context).
    match llm_client::stream_chat(state, content, &*history).await {
        Ok(llm_response) => {
            let parsed = parse_llm_response(&llm_response);
            info!(
                "LLM response parsed: motion='{}' expression='{}' text_len={}",
                parsed.motion_id,
                parsed.expression_id,
                parsed.clean_text.len()
            );

            // Append assistant response to conversation history.
            history.push(("assistant".to_string(), parsed.clean_text.clone()));
            // Trim oldest entries when the cap is exceeded.
            while history.len() > MAX_HISTORY {
                history.remove(0);
            }

            // 3. speaking
            if sender
                .send(WSMessage::Status {
                    status: "speaking".to_string(),
                    message: None,
                    timestamp: now_ms(),
                })
                .await
                .is_err()
            {
                warn!("process_message: client channel closed during speaking status");
                return;
            }

            // 4. TTS synthesis
            let tts_result = tts_bridge::synthesize(state, &parsed).await;
            let has_audio = tts_result.is_some();
            let (audio_data, sample_rate, duration) = tts_result
                .map(|r| (Some(r.audio_data), r.sample_rate, r.duration))
                .unwrap_or((None, 32000, 0.0));

            if has_audio {
                debug!(
                    "TTS: audio_len={} bytes, sample_rate={}, duration={:.2}s",
                    audio_data.as_ref().map(|s| s.len()).unwrap_or(0),
                    sample_rate,
                    duration
                );
            } else {
                debug!("TTS: no audio produced");
            }

            // 5. multimodal_sync
            if sender
                .send(WSMessage::MultimodalSync {
                    motionId: parsed.motion_id,
                    expressionId: parsed.expression_id,
                    text: parsed.clean_text,
                    hasAudio: has_audio,
                    audioData: audio_data,
                    sampleRate: sample_rate,
                    duration,
                    timestamp: now_ms(),
                })
                .await
                .is_err()
            {
                warn!("process_message: client channel closed during multimodal_sync");
                return;
            }

            // 6. ready
            if sender
                .send(WSMessage::Status {
                    status: "ready".to_string(),
                    message: None,
                    timestamp: now_ms(),
                })
                .await
                .is_err()
            {
                warn!("process_message: client channel closed during ready status");
            }
        }
        Err(e) => {
            error!("LLM stream failed: {}", e);

            // Send error status
            let _ = sender
                .send(WSMessage::Status {
                    status: "error".to_string(),
                    message: Some(e.to_string()),
                    timestamp: now_ms(),
                })
                .await;

            // Brief pause so the frontend can render the error toast.
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Return to ready so the user can try again.
            let _ = sender
                .send(WSMessage::Status {
                    status: "ready".to_string(),
                    message: None,
                    timestamp: now_ms(),
                })
                .await;
        }
    }
}
