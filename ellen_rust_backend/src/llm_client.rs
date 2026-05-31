use crate::state::AppState;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde::Deserialize;
use thiserror::Error;

/// Errors that can occur during LLM API calls.
#[derive(Error, Debug)]
pub enum LLMError {
    /// The HTTP request to the LLM provider failed.
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    /// An SSE event could not be parsed.
    #[error("SSE parse error: {0}")]
    SSEError(String),

    /// The API returned an error response.
    #[error("API error: {0}")]
    ApiError(String),

    /// The streamed response contained no content.
    #[error("No response content")]
    EmptyResponse,
}

/// A single message in the chat completion request.
#[derive(Debug, serde::Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// The JSON body sent to the DeepSeek chat completions endpoint.
#[derive(Debug, serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    temperature: f32,
    max_tokens: u32,
}

/// A streaming chunk returned by DeepSeek.
///
/// Example JSON:
/// ```json
/// {
///   "choices": [
///     {
///       "delta": { "content": "hello" },
///       "index": 0,
///       "finish_reason": null
///     }
///   ],
///   "id": "...",
///   "object": "chat.completion.chunk"
/// }
/// ```
#[derive(Debug, Deserialize)]
struct StreamChunk {
    choices: Vec<Choice>,
}

/// A single choice within a streaming chunk.
#[derive(Debug, Deserialize)]
struct Choice {
    delta: DeltaContent,
    finish_reason: Option<String>,
}

/// The delta object containing the content fragment.
#[derive(Debug, Deserialize)]
struct DeltaContent {
    content: Option<String>,
}

/// Call the DeepSeek API with SSE streaming and collect the full response.
///
/// # Arguments
///
/// * `state` - Shared application state containing the API key, persona, and config.
/// * `user_message` - The message from the user to send to the LLM.
///
/// # Returns
///
/// The complete response text concatenated from all SSE chunks, which should
/// contain `[motion:xxx][exp:yyy]` tags followed by Japanese text.
///
/// # Errors
///
/// Returns [`LLMError::HttpError`] if the HTTP request fails,
/// [`LLMError::SSEError`] if an SSE event cannot be parsed,
/// [`LLMError::ApiError`] if the API returns an error, or
/// [`LLMError::EmptyResponse`] if the stream produces no content.
pub async fn stream_chat(
    state: &AppState,
    user_message: &str,
    history: &[(String, String)],
) -> Result<String, LLMError> {
    let api_key = state.config.llm.api_key();
    let system_prompt = state.persona.system_prompt();
    let temperature = state.config.llm.temperature;
    let max_tokens = state.config.llm.max_tokens;

    let mut messages: Vec<ChatMessage> = Vec::with_capacity(history.len() + 2);
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: system_prompt.to_string(),
    });
    for (role, content) in history {
        messages.push(ChatMessage {
            role: role.clone(),
            content: content.clone(),
        });
    }
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_message.to_string(),
    });

    let request_body = ChatRequest {
        model: "deepseek-chat".to_string(),
        messages,
        stream: true,
        temperature,
        max_tokens,
    };

    tracing::info!(
        "Sending streaming chat request to DeepSeek (temperature={}, max_tokens={})",
        temperature,
        max_tokens
    );

    let response = state
        .http_client
        .post("https://api.deepseek.com/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .header("Accept", "text/event-stream")
        .json(&request_body)
        .send()
        .await?;

    if let Err(e) = response.error_for_status_ref() {
        let status = response.status();
        let body_text = response
            .text()
            .await
            .unwrap_or_else(|_| "<could not read body>".to_string());
        tracing::warn!(
            "DeepSeek API returned error status {}: {}",
            status,
            body_text
        );
        return Err(LLMError::ApiError(format!(
            "HTTP {}: {}",
            status, body_text
        )));
    }

    tracing::debug!("DeepSeek response received, starting SSE stream");

    let mut stream = response.bytes_stream().eventsource();
    let mut full_text = String::new();

    while let Some(event) = stream.next().await {
        match event {
            Ok(ev) => {
                if ev.data == "[DONE]" {
                    tracing::debug!("Received [DONE] event, stream complete");
                    break;
                }

                tracing::trace!("SSE chunk: {}", ev.data);

                let chunk: StreamChunk = match serde_json::from_str(&ev.data) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to parse SSE chunk: {} | data: {}", e, ev.data);
                        return Err(LLMError::SSEError(format!(
                            "JSON parse failed: {} | data: {}",
                            e, ev.data
                        )));
                    }
                };

                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        tracing::debug!("Content fragment: {}", content);
                        full_text.push_str(content);
                    }
                    if choice.finish_reason.as_deref() == Some("stop") {
                        tracing::debug!("Received finish_reason=stop, ending stream");
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("SSE stream error: {}", e);
                return Err(LLMError::SSEError(e.to_string()));
            }
        }
    }

    if full_text.trim().is_empty() {
        tracing::warn!("LLM returned empty response after streaming");
        return Err(LLMError::EmptyResponse);
    }

    tracing::info!(
        "Streaming complete, collected {} characters",
        full_text.len()
    );
    Ok(full_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test parsing a normal content chunk from DeepSeek SSE.
    #[test]
    fn test_parse_stream_chunk_with_content() {
        let json_data = r#"{"choices":[{"delta":{"content":"こんにちは"},"index":0,"finish_reason":null}],"id":"test-1","object":"chat.completion.chunk"}"#;

        let chunk: StreamChunk = serde_json::from_str(json_data).expect("should parse");
        assert_eq!(chunk.choices.len(), 1);
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("こんにちは"));
        assert_eq!(chunk.choices[0].finish_reason, None);
    }

    /// Test parsing a chunk with null content (should be skipped).
    #[test]
    fn test_parse_stream_chunk_with_null_content() {
        let json_data = r#"{"choices":[{"delta":{"content":null},"index":0,"finish_reason":null}],"id":"test-2","object":"chat.completion.chunk"}"#;

        let chunk: StreamChunk = serde_json::from_str(json_data).expect("should parse");
        assert!(chunk.choices[0].delta.content.is_none());
    }

    /// Test parsing a chunk with finish_reason="stop".
    #[test]
    fn test_parse_stream_chunk_stop() {
        let json_data = r#"{"choices":[{"delta":{"content":""},"index":0,"finish_reason":"stop"}],"id":"test-3","object":"chat.completion.chunk"}"#;

        let chunk: StreamChunk = serde_json::from_str(json_data).expect("should parse");
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some(""));
    }

    /// Test parsing multiple chunks and concatenating content.
    #[test]
    fn test_parse_multiple_chunks_concatenation() {
        let chunks_json = vec![
            r#"{"choices":[{"delta":{"content":"[motion:idle]"},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"[exp:lazy]"},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":"何か用？"},"index":0,"finish_reason":null}]}"#,
            r#"{"choices":[{"delta":{"content":""},"index":0,"finish_reason":"stop"}]}"#,
        ];

        let mut full_text = String::new();
        for json in chunks_json {
            let chunk: StreamChunk = serde_json::from_str(json).unwrap();
            if let Some(choice) = chunk.choices.first() {
                if let Some(content) = &choice.delta.content {
                    full_text.push_str(content);
                }
            }
        }

        assert_eq!(full_text, "[motion:idle][exp:lazy]何か用？");
    }

    /// Test the Display formatting of LLMError variants.
    #[test]
    fn test_llm_error_display() {
        let err = LLMError::SSEError("bad json".to_string());
        assert_eq!(format!("{}", err), "SSE parse error: bad json");

        let err = LLMError::ApiError("rate limited".to_string());
        assert_eq!(format!("{}", err), "API error: rate limited");

        let err = LLMError::EmptyResponse;
        assert_eq!(format!("{}", err), "No response content");
    }
}
