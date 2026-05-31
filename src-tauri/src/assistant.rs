//! Claude API client for the on-the-spot study / answer assistant.
//!
//! There is no official Anthropic Rust SDK, so this talks to the Messages API
//! (`POST /v1/messages`) directly over the shared reqwest client. The API key is
//! read from local SQLite by the caller and passed in here; it never leaves the
//! machine except in the request to api.anthropic.com.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

const SYSTEM_PROMPT: &str = "You are a real-time study and conversation assistant embedded in a desktop app. \
The user is often mid-call, in a lecture, or studying and needs fast, correct help. Given a question or a \
snippet of conversation/transcript, work out what is actually being asked and answer it directly. Lead with \
the answer, then give only the key supporting steps. For math and technical problems, show the final result \
first and keep the working concise. Respond in plain text: no LaTeX, no markdown headings, and use / for \
division and ^ for exponents. If the input is ambiguous, state your best interpretation in one line, then \
answer it.";

/// Models that accept `thinking: {type: "adaptive"}`. Haiku and any unknown model
/// omit it so a model switch in Settings can never produce a 400.
fn supports_adaptive_thinking(model: &str) -> bool {
    model.starts_with("claude-opus-4") || model == "claude-sonnet-4-6"
}

/// Ask the assistant a question, optionally with surrounding context (a pasted
/// transcript snippet or notes). Returns the answer text.
pub async fn ask(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    question: &str,
    context: Option<&str>,
) -> Result<String> {
    let user_text = match context {
        Some(ctx) if !ctx.trim().is_empty() => format!(
            "Context (conversation/transcript or notes):\n{ctx}\n\nWhat I need help with:\n{question}"
        ),
        _ => question.to_string(),
    };

    let mut body = json!({
        "model": model,
        "max_tokens": 4096,
        "system": [{
            "type": "text",
            "text": SYSTEM_PROMPT,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{ "role": "user", "content": user_text }],
    });
    if supports_adaptive_thinking(model) {
        // Adaptive thinking improves math/reasoning correctness; give it room
        // beyond the short final answer.
        body["thinking"] = json!({ "type": "adaptive" });
        body["max_tokens"] = json!(8192);
    }

    let resp = http
        .post(API_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&body)
        .send()
        .await
        .map_err(|e| anyhow!("Could not reach the Claude API: {e}"))?;

    let status = resp.status();
    let payload: Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Unexpected response from the Claude API: {e}"))?;

    if !status.is_success() {
        return Err(anyhow!(api_error_message(status.as_u16(), &payload)));
    }

    // The answer is the first text block in `content`; thinking blocks (when
    // present) are returned separately and skipped here.
    let answer = payload["content"]
        .as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());

    match answer {
        Some(a) => Ok(a.to_string()),
        None if payload["stop_reason"] == "refusal" => {
            Err(anyhow!("The model declined to answer this request."))
        }
        None => Err(anyhow!("The assistant returned an empty response.")),
    }
}

/// Map an API error status + body to a concise, user-facing message.
fn api_error_message(status: u16, payload: &Value) -> String {
    let detail = payload["error"]["message"].as_str().unwrap_or("");
    match status {
        401 => "Invalid Anthropic API key. Check the key in the Assistant tab.".to_string(),
        403 => "This API key is not permitted to use the requested model.".to_string(),
        404 => "Unknown model. Pick a different model in the Assistant tab.".to_string(),
        429 => "Rate limited by the Claude API. Wait a moment and try again.".to_string(),
        500 | 529 => "The Claude API is busy right now. Try again in a few seconds.".to_string(),
        _ if !detail.is_empty() => format!("Claude API error ({status}): {detail}"),
        _ => format!("Claude API error ({status})."),
    }
}
