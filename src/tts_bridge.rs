//! TTS bridge — GPT-SoVITS v4 HTTP client with LRU+TTL cache.
//!
//! This module handles all communication with the local GPT-SoVITS TTS service:
//! * Builds JSON requests from parsed LLM responses.
//! * Caches synthesized audio keyed by `SHA256(text:expression_id)` (first 16 hex chars).
//! * Retries transient failures with exponential backoff.
//! * Parses WAV headers to compute audio duration.
//!
//! # Cache
//!
//! A module-level singleton [`TTSCache`] stores up to 50 entries with a 30-minute TTL.
//! On hit the cached `audio_data` / `sample_rate` / `duration` is returned immediately,
//! skipping the HTTP round-trip.  LRU eviction removes the least-recently-used entry
//! when the cache is full.
//!
//! # Example
//!
//! ```ignore
//! use ellen_rust_backend::{state::AppState, parser::ParsedResponse, tts_bridge};
//!
//! async fn speak(state: &AppState, parsed: &ParsedResponse) {
//!     if let Some(result) = tts_bridge::synthesize(state, parsed).await {
//!         println!("Audio: {} bytes base64, {:.2}s",
//!             result.audio_data.len(), result.duration);
//!     } else {
//!         println!("TTS failed — graceful degradation");
//!     }
//! }
//! ```

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::parser::ParsedResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// GPT-SoVITS v4 outputs 32 kHz WAV audio.
const SAMPLE_RATE: u32 = 32_000;

/// Maximum number of HTTP retry attempts.
const MAX_RETRIES: usize = 3;

/// Exponential backoff base in seconds.  Delays are `1 << attempt`:
/// attempt 0 → 1 s, attempt 1 → 2 s, attempt 2 → 4 s.
const BACKOFF_BASE_SECS: u64 = 1;

/// Default cache capacity (number of entries).
const CACHE_MAX_SIZE: usize = 50;

/// Default cache TTL — 30 minutes.
const CACHE_TTL_SECS: u64 = 30 * 60;

// ---------------------------------------------------------------------------
// Public data types
// ---------------------------------------------------------------------------

/// Result of a successful TTS synthesis.
///
/// Returned by [`synthesize`] and [`synthesize_text`].  The `audio_data` field
/// contains a base64-encoded WAV file suitable for embedding in a
/// [`crate::ws_server::WSMessage::MultimodalSync`] payload.
#[derive(Debug, Clone)]
pub struct TTSResult {
    /// Base64-encoded WAV audio data.
    pub audio_data: String,

    /// Sample rate in Hz (GPT-SoVITS v4 outputs 32 000 Hz).
    pub sample_rate: u32,

    /// Audio duration in seconds.
    pub duration: f64,

    /// Motion ID for Live2D animation sync.
    pub motion_id: String,

    /// Expression ID for Live2D facial expression sync.
    pub expression_id: String,

    /// Clean text that was actually synthesized.
    pub text: String,
}

// ---------------------------------------------------------------------------
// Cache internals (LRU + TTL)
// ---------------------------------------------------------------------------

/// One slot in the TTS cache.
struct CacheEntry {
    /// Base64 audio payload.
    audio_data: String,
    /// Sample rate in Hz.
    sample_rate: u32,
    /// Duration in seconds.
    duration: f64,
    /// Creation timestamp — used for TTL expiry.
    created_at: Instant,
    /// Last access timestamp — used for LRU eviction.
    last_accessed: Instant,
}

/// In-memory LRU+TTL cache for TTS audio.
///
/// * **Thread safety** — backed by a [`Mutex`] so it can be used as a global
///   singleton safely across async tasks.
/// * **LRU** — when the map reaches `max_size`, the least-recently-used entry
///   (oldest `last_accessed`) is evicted before insertion.
/// * **TTL** — entries older than `ttl` are discarded on `get` and during
///   periodic cleanup in `put`.
struct TTSCache {
    entries: Mutex<HashMap<String, CacheEntry>>,
    max_size: usize,
    ttl: Duration,
}

impl TTSCache {
    /// Create a new cache with the given capacity and TTL.
    fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_size,
            ttl,
        }
    }

    /// Look up `key`.  Returns `Some((audio_data, sample_rate, duration))` on
    /// hit, `None` on miss or expiry.
    ///
    /// On hit the `last_accessed` timestamp is refreshed.
    fn get(&self, key: &str) -> Option<(String, u32, f64)> {
        let mut entries = self.entries.lock().ok()?;
        let now = Instant::now();

        let entry = entries.get_mut(key)?;

        // TTL check — discard expired entries.
        if now.duration_since(entry.created_at) > self.ttl {
            entries.remove(key);
            return None;
        }

        entry.last_accessed = now;
        Some((
            entry.audio_data.clone(),
            entry.sample_rate,
            entry.duration,
        ))
    }

    /// Insert (or update) an entry.  If the cache is at capacity the
    /// least-recently-used entry is evicted.  Expired entries are also
    /// cleaned up opportunistically.
    fn put(&self, key: String, audio_data: String, sample_rate: u32, duration: f64) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };

        let now = Instant::now();

        // Opportunistically purge expired entries.
        let expired_keys: Vec<String> = entries
            .iter()
            .filter(|(_, e)| now.duration_since(e.created_at) > self.ttl)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired_keys {
            entries.remove(&k);
        }

        // LRU eviction if still at capacity and this is a new key.
        if entries.len() >= self.max_size && !entries.contains_key(&key) {
            let lru_key = entries
                .iter()
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(k, _)| k.clone());
            if let Some(k) = lru_key {
                entries.remove(&k);
            }
        }

        entries.insert(
            key,
            CacheEntry {
                audio_data,
                sample_rate,
                duration,
                created_at: now,
                last_accessed: now,
            },
        );
    }
}

/// Global singleton cache instance (lazily initialised).
static CACHE: OnceLock<TTSCache> = OnceLock::new();

/// Return a reference to the global TTS cache.
fn cache() -> &'static TTSCache {
    CACHE.get_or_init(|| TTSCache::new(CACHE_MAX_SIZE, Duration::from_secs(CACHE_TTL_SECS)))
}

// ---------------------------------------------------------------------------
// GPT-SoVITS request body
// ---------------------------------------------------------------------------

/// JSON payload sent to the GPT-SoVITS `/tts` endpoint.
#[derive(Debug, Serialize)]
struct TTSRequest {
    text: String,
    text_lang: String,
    ref_audio_path: String,
    prompt_text: String,
    prompt_lang: String,
    top_k: u32,
    top_p: f32,
    temperature: f32,
    speed_factor: f32,
    sample_steps: u32,
    #[serde(rename = "super_sampling")]
    super_sampling: bool,
    batch_size: u32,
    streaming_mode: bool,
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute a cache key as the first 16 hex characters of the SHA-256 digest
/// of `"{text}:{expression_id}"`.
fn make_cache_key(text: &str, expression_id: &str) -> String {
    let input = format!("{}:{}", text, expression_id);
    let hash = Sha256::digest(input.as_bytes());
    // First 8 bytes → 16 hex characters.
    hash[..8].iter().fold(String::with_capacity(16), |mut s, b| {
        s.push_str(&format!("{:02x}", b));
        s
    })
}

/// Parse the duration of a WAV file from its 44-byte header.
///
/// Reads:
/// * bytes 22-23 → `num_channels` (u16 LE)
/// * bytes 34-35 → `bits_per_sample` (u16 LE)
/// * bytes 40-43 → `subchunk2_size` (u32 LE)
///
/// Formula:
/// ```text
/// duration = subchunk2_size / (sample_rate * channels * bits_per_sample / 8)
/// ```
///
/// Falls back to `(buffer.len() - 44) / (sample_rate * 2)` when the header is
/// too short or contains zeroes.
fn parse_wav_duration(buffer: &[u8], sample_rate: u32) -> f64 {
    if buffer.len() < 44 {
        return (buffer.len().saturating_sub(44)) as f64 / (sample_rate as f64 * 2.0);
    }

    let num_channels = u16::from_le_bytes([buffer[22], buffer[23]]);
    let bits_per_sample = u16::from_le_bytes([buffer[34], buffer[35]]);
    let subchunk2_size = u32::from_le_bytes([buffer[40], buffer[41], buffer[42], buffer[43]]);

    let byte_depth = sample_rate as f64
        * num_channels as f64
        * bits_per_sample as f64
        / 8.0;

    if byte_depth == 0.0 {
        // Fallback — assume 16-bit mono.
        return (buffer.len().saturating_sub(44)) as f64 / (sample_rate as f64 * 2.0);
    }

    subchunk2_size as f64 / byte_depth
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Synthesize speech from a parsed LLM response.
///
/// Returns `None` on TTS failure so the caller can gracefully degrade to a
/// text-only response.
///
/// # Logic
/// 1. Skip if `parsed.clean_text` is empty.
/// 2. Check the cache — on hit return cached audio.
/// 3. Build a [`TTSRequest`] from `state.config.tts`.
/// 4. `POST {api_url}/tts` with exponential-backoff retry (3 attempts).
/// 5. Parse WAV duration and base64-encode the bytes.
/// 6. Store in cache and return [`TTSResult`].
pub async fn synthesize(state: &AppState, parsed: &ParsedResponse) -> Option<TTSResult> {
    synthesize_text(state, &parsed.clean_text, &parsed.expression_id, &parsed.motion_id).await
}

/// Synthesize speech with explicit text and expression / motion IDs.
///
/// This is the internal entry-point used by [`synthesize`].  It can also be
/// called directly when you already have the individual fields rather than a
/// [`ParsedResponse`].
///
/// The `expression_id` participates in the cache key; `motion_id` is only
/// copied into the returned [`TTSResult`].
///
/// Returns `None` on any failure (network error, non-2xx response, empty body,
/// or exhausted retries).
pub async fn synthesize_text(
    state: &AppState,
    text: &str,
    expression_id: &str,
    motion_id: &str,
) -> Option<TTSResult> {
    // 1. Empty text — nothing to say.
    if text.trim().is_empty() {
        debug!("synthesize_text: empty text, skipping TTS");
        return None;
    }

    // 2. Cache lookup.
    let cache_key = make_cache_key(text, expression_id);
    if let Some((audio_data, sample_rate, duration)) = cache().get(&cache_key) {
        debug!("TTS cache hit for key={}", cache_key);
        return Some(TTSResult {
            audio_data,
            sample_rate,
            duration,
            motion_id: motion_id.to_string(),
            expression_id: expression_id.to_string(),
            text: text.to_string(),
        });
    }
    debug!("TTS cache miss for key={}", cache_key);

    // 3. Build request body from configuration.
    let tts_cfg = &state.config.tts;
    let request = TTSRequest {
        text: text.to_string(),
        text_lang: tts_cfg.language.clone(),
        ref_audio_path: tts_cfg.model.ref_audio.clone(),
        prompt_text: tts_cfg.model.ref_text.clone(),
        prompt_lang: tts_cfg.language.clone(),
        top_k: tts_cfg.params.top_k,
        top_p: tts_cfg.params.top_p,
        temperature: tts_cfg.params.temperature,
        speed_factor: tts_cfg.params.speed_factor,
        sample_steps: tts_cfg.params.sample_steps,
        super_sampling: tts_cfg.params.super_sampling,
        batch_size: tts_cfg.params.batch_size,
        streaming_mode: tts_cfg.params.streaming_mode,
    };

    let api_url = format!("{}/tts", tts_cfg.api_url.trim_end_matches('/'));
    let mut last_error: Option<String> = None;

    // 4. Call TTS API with exponential backoff.
    for attempt in 0..MAX_RETRIES {
        debug!(
            "TTS API call attempt {} to {} (text_len={})",
            attempt + 1,
            api_url,
            text.len()
        );

        match state
            .http_client
            .post(&api_url)
            .json(&request)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if !status.is_success() {
                    warn!(
                        "TTS API returned HTTP {} on attempt {}",
                        status,
                        attempt + 1
                    );
                    last_error = Some(format!("HTTP {}", status));
                    if attempt < MAX_RETRIES - 1 {
                        sleep(Duration::from_secs(BACKOFF_BASE_SECS << attempt)).await;
                    }
                    continue;
                }

                // GPT-SoVITS returns raw WAV bytes (NOT JSON).
                match response.bytes().await {
                    Ok(bytes) => {
                        if bytes.is_empty() {
                            warn!("TTS API returned empty audio on attempt {}", attempt + 1);
                            last_error = Some("empty audio".to_string());
                            if attempt < MAX_RETRIES - 1 {
                                sleep(Duration::from_secs(BACKOFF_BASE_SECS << attempt)).await;
                            }
                            continue;
                        }

                        // 5. Parse WAV duration.
                        let duration = parse_wav_duration(&bytes, SAMPLE_RATE);

                        // 6. Base64 encode.
                        let audio_data = STANDARD.encode(&bytes);

                        // 7. Store in cache.
                        cache().put(
                            cache_key.clone(),
                            audio_data.clone(),
                            SAMPLE_RATE,
                            duration,
                        );

                        info!(
                            "TTS synthesis OK: {} chars base64, {:.2}s",
                            audio_data.len(),
                            duration
                        );

                        // 8. Return result.
                        return Some(TTSResult {
                            audio_data,
                            sample_rate: SAMPLE_RATE,
                            duration,
                            motion_id: motion_id.to_string(),
                            expression_id: expression_id.to_string(),
                            text: text.to_string(),
                        });
                    }
                    Err(e) => {
                        warn!(
                            "TTS API body read failed on attempt {}: {}",
                            attempt + 1,
                            e
                        );
                        last_error = Some(e.to_string());
                        if attempt < MAX_RETRIES - 1 {
                            sleep(Duration::from_secs(BACKOFF_BASE_SECS << attempt)).await;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("TTS API request failed on attempt {}: {}", attempt + 1, e);
                last_error = Some(e.to_string());
                if attempt < MAX_RETRIES - 1 {
                    sleep(Duration::from_secs(BACKOFF_BASE_SECS << attempt)).await;
                }
            }
        }
    }

    warn!(
        "TTS synthesis failed after {} attempts: {:?}",
        MAX_RETRIES, last_error
    );
    None
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- cache key --------------------------------------------------------

    #[test]
    fn test_cache_key_basic() {
        let key1 = make_cache_key("hello world", "lazy");
        let key2 = make_cache_key("hello world", "lazy");
        let key3 = make_cache_key("hello world", "happy");
        let key4 = make_cache_key("different text", "lazy");

        // Deterministic.
        assert_eq!(key1, key2, "same inputs should yield same key");
        // Different expression.
        assert_ne!(key1, key3, "different expression → different key");
        // Different text.
        assert_ne!(key1, key4, "different text → different key");
        // Length.
        assert_eq!(key1.len(), 16, "key must be 16 hex chars");
        assert!(
            key1.chars().all(|c| c.is_ascii_hexdigit()),
            "key must be hex only"
        );
    }

    #[test]
    fn test_cache_key_empty() {
        let key = make_cache_key("", "");
        assert_eq!(key.len(), 16);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_cache_key_unicode() {
        // Japanese text should work fine — SHA-256 operates on UTF-8 bytes.
        let key = make_cache_key("おはようございます", "maid");
        assert_eq!(key.len(), 16);
        assert!(key.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // -- WAV duration ------------------------------------------------------

    /// Build a minimal WAV file in memory.
    ///
    /// `data_payload_len` is the size of the audio data chunk in bytes.
    fn build_wav(num_channels: u16, bits_per_sample: u16, sample_rate: u32, data_payload_len: u32) -> Vec<u8> {
        let byte_rate = sample_rate * num_channels as u32 * bits_per_sample as u32 / 8;
        let block_align = num_channels * bits_per_sample / 8;
        let file_size = 44 + data_payload_len;

        let mut wav = Vec::with_capacity(file_size as usize);

        // RIFF chunk descriptor (12 bytes).
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(file_size - 8).to_le_bytes());
        wav.extend_from_slice(b"WAVE");

        // fmt sub-chunk (24 bytes).
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());          // subchunk1_size
        wav.extend_from_slice(&1_u16.to_le_bytes());           // audio_format (PCM)
        wav.extend_from_slice(&num_channels.to_le_bytes());    // num_channels
        wav.extend_from_slice(&sample_rate.to_le_bytes());     // sample_rate
        wav.extend_from_slice(&byte_rate.to_le_bytes());       // byte_rate
        wav.extend_from_slice(&block_align.to_le_bytes());     // block_align
        wav.extend_from_slice(&bits_per_sample.to_le_bytes()); // bits_per_sample

        // data sub-chunk header (8 bytes) + payload.
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_payload_len.to_le_bytes());
        wav.resize(file_size as usize, 0);

        wav
    }

    #[test]
    fn test_parse_wav_duration_mono_16bit_1s() {
        // 1 second of 32 kHz mono 16-bit → 64 000 bytes of data.
        let data_size = 32_000_u32 * 1 * 16 / 8;
        let wav = build_wav(1, 16, 32_000, data_size);
        let duration = parse_wav_duration(&wav, 32_000);
        assert!(
            (duration - 1.0).abs() < 0.01,
            "expected ~1.0s, got {}",
            duration
        );
    }

    #[test]
    fn test_parse_wav_duration_stereo_16bit_1s() {
        // 1 second of 32 kHz stereo 16-bit → 128 000 bytes.
        let data_size = 32_000_u32 * 2 * 16 / 8;
        let wav = build_wav(2, 16, 32_000, data_size);
        let duration = parse_wav_duration(&wav, 32_000);
        assert!(
            (duration - 1.0).abs() < 0.01,
            "expected ~1.0s for stereo, got {}",
            duration
        );
    }

    #[test]
    fn test_parse_wav_duration_short_buffer_fallback() {
        let short = vec![0u8; 10];
        let duration = parse_wav_duration(&short, 32_000);
        // (10 - 44) saturates to 0 → 0.0 s.
        assert_eq!(duration, 0.0);
    }

    #[test]
    fn test_parse_wav_duration_zero_bps_fallback() {
        // Header with bits_per_sample = 0 triggers the fallback path.
        let mut wav = vec![0u8; 48];

        wav[0..4].copy_from_slice(b"RIFF");
        wav[4..8].copy_from_slice(&40_u32.to_le_bytes());
        wav[8..12].copy_from_slice(b"WAVE");

        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&16_u32.to_le_bytes());
        wav[20..22].copy_from_slice(&1_u16.to_le_bytes());  // PCM
        wav[22..24].copy_from_slice(&1_u16.to_le_bytes());  // mono
        wav[24..28].copy_from_slice(&32_000_u32.to_le_bytes());
        wav[28..32].copy_from_slice(&64_000_u32.to_le_bytes());
        wav[32..34].copy_from_slice(&2_u16.to_le_bytes());
        wav[34..36].copy_from_slice(&0_u16.to_le_bytes());  // bits_per_sample = 0 !

        wav[36..40].copy_from_slice(b"data");
        wav[40..44].copy_from_slice(&64_000_u32.to_le_bytes());

        let duration = parse_wav_duration(&wav, 32_000);
        // Fallback: (48 - 44) / (32000 * 2) = 4 / 64000 = 0.0000625.
        assert!(
            (duration - 0.0000625).abs() < 0.000001,
            "expected fallback ~0.0000625, got {}",
            duration
        );
    }

    // -- cache behaviour ---------------------------------------------------

    #[test]
    fn test_cache_lru_eviction() {
        let cache = TTSCache::new(3, Duration::from_secs(60));

        cache.put("k1".into(), "a1".into(), 32_000, 1.0);
        cache.put("k2".into(), "a2".into(), 32_000, 1.0);
        cache.put("k3".into(), "a3".into(), 32_000, 1.0);

        // Touch k1 so it becomes recently used.
        assert!(cache.get("k1").is_some());

        // Insert k4 — k2 should be evicted (LRU).
        cache.put("k4".into(), "a4".into(), 32_000, 1.0);

        assert!(cache.get("k1").is_some(), "k1 should survive");
        assert!(cache.get("k2").is_none(), "k2 should be evicted (LRU)");
        assert!(cache.get("k3").is_some(), "k3 should survive");
        assert!(cache.get("k4").is_some(), "k4 should exist");
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let cache = TTSCache::new(10, Duration::from_millis(50));

        cache.put("k1".into(), "a1".into(), 32_000, 1.0);
        assert!(cache.get("k1").is_some(), "fresh entry should exist");

        std::thread::sleep(Duration::from_millis(100));
        assert!(cache.get("k1").is_none(), "entry should have expired");
    }

    #[test]
    fn test_cache_update_existing() {
        let cache = TTSCache::new(2, Duration::from_secs(60));

        cache.put("k1".into(), "audio1".into(), 32_000, 1.0);
        cache.put("k1".into(), "audio1_updated".into(), 32_000, 2.5);

        let result = cache.get("k1");
        assert!(result.is_some());
        let (audio, _, duration) = result.unwrap();
        assert_eq!(audio, "audio1_updated");
        assert!((duration - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_cache_purges_expired_on_put() {
        let cache = TTSCache::new(10, Duration::from_millis(50));

        cache.put("old".into(), "audio_old".into(), 32_000, 1.0);
        std::thread::sleep(Duration::from_millis(100));

        // This put should purge the expired "old" entry.
        cache.put("new".into(), "audio_new".into(), 32_000, 1.0);
        assert!(cache.get("old").is_none(), "old should be purged");
        assert!(cache.get("new").is_some(), "new should exist");
    }
}
