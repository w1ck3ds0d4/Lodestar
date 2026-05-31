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
snippet of conversation/transcript, work out what is actually being asked and answer it directly and \
concisely - lead with the answer, then the key supporting steps. Format with GitHub-flavored Markdown: put \
code in fenced code blocks with a language tag (```python ... ```), and write math in LaTeX - inline as \
$...$ and display equations as $$...$$. Keep it tight, no filler. If the input is ambiguous, state your \
best interpretation in one line, then answer it.";

/// System prompt for auto-answer: watch a live transcript and answer only when
/// something is actually being asked (in any language); otherwise reply "NONE".
const AUTO_PROMPT: &str = "You monitor a live transcript of a conversation or call, which may be in any \
language. Look at the NEW text, using the earlier transcript as context. If it contains a question being \
asked, or a problem to solve (math, code, a factual lookup, a definition, etc.), answer it directly and \
concisely in the SAME language as the speaker, using GitHub-flavored Markdown - fenced code blocks for code \
and LaTeX ($...$ or $$...$$) for math. If the new text is just ordinary conversation with nothing being \
asked and no problem to solve, reply with exactly one word: NONE";

/// Models that accept `thinking: {type: "adaptive"}`. Haiku and any unknown model
/// omit it so a model switch in Settings can never produce a 400.
fn supports_adaptive_thinking(model: &str) -> bool {
    model.starts_with("claude-opus-4") || model == "claude-sonnet-4-6"
}

/// Shared Messages API call: send a system prompt + user message, return the text.
async fn complete(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    user_text: String,
) -> Result<String> {
    let mut body = json!({
        "model": model,
        "max_tokens": 4096,
        "system": [{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{ "role": "user", "content": user_text }],
    });
    if supports_adaptive_thinking(model) {
        // Adaptive thinking improves math/reasoning correctness; give it room.
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

/// Answer a direct question, optionally with surrounding context (a pasted
/// transcript snippet or notes).
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
    complete(http, api_key, model, SYSTEM_PROMPT, user_text).await
}

/// Auto-answer: given the recent transcript and the newest segment, answer it if it
/// is a question/problem (in any language), otherwise return the literal "NONE".
pub async fn auto_answer(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    transcript: &str,
    latest: &str,
) -> Result<String> {
    let user_text =
        format!("Earlier transcript (context):\n{transcript}\n\nNew text to consider:\n{latest}");
    complete(http, api_key, model, AUTO_PROMPT, user_text).await
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
