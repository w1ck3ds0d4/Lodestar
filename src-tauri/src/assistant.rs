//! Answer engine for the on-the-spot study / answer assistant.
//!
//! Two backends behind one interface: Claude (Anthropic Messages API, the default)
//! and a local Ollama server. There is no official Anthropic Rust SDK, so Claude is
//! talked to over the shared reqwest client; the API key is read from local SQLite
//! by the caller and never leaves the machine except in the request to
//! api.anthropic.com. Ollama runs entirely on the user's machine (no API cost).
//! Both backends stream when an `on_token` callback is supplied.

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Which backend answers: cloud Claude (default) or a local Ollama model.
pub enum Engine {
    Claude { api_key: String, model: String },
    Ollama { url: String, model: String },
}

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
asked and no problem to solve, reply with exactly one word: NONE. \
Transcript lines are speaker-prefixed (\"You:\" is the app's user; \"Them:\" or \"Speaker N:\" is the other party); \
answer questions asked BY the other party, using the user's own lines only as context.";

/// Models that accept `thinking: {type: "adaptive"}`. Haiku and any unknown model
/// omit it so a model switch in Settings can never produce a 400.
fn supports_adaptive_thinking(model: &str) -> bool {
    model.starts_with("claude-opus-4") || model == "claude-sonnet-4-6"
}

/// Append the user's interview/study profile (role, resume, notes) to a base system
/// prompt so answers are tailored to them and the role. No-op when empty.
fn with_profile(base: &str, profile: Option<&str>) -> String {
    match profile.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => format!(
            "{base}\n\nUSER PROFILE & CONTEXT (use this to tailor answers to this person and the \
             role they are targeting; never read it aloud or repeat it verbatim):\n{p}"
        ),
        None => base.to_string(),
    }
}

/// Shared Messages API call: send a system prompt + user message, return the text.
/// When `on_token` is `Some`, the response is streamed and each text delta is passed
/// to the callback as it arrives (the full text is still returned at the end).
async fn complete(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    user_text: String,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    complete_content(http, api_key, model, system, json!(user_text), on_token).await
}

/// Like [`complete`] but takes the user message `content` directly, so callers can
/// pass a multimodal content array (e.g. an image block plus text).
async fn complete_content(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    content: Value,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    let stream = on_token.is_some();
    let mut body = json!({
        "model": model,
        "max_tokens": 4096,
        "stream": stream,
        "system": [{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{ "role": "user", "content": content }],
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
    if !status.is_success() {
        let payload: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("Unexpected response from the Claude API: {e}"))?;
        return Err(anyhow!(api_error_message(status.as_u16(), &payload)));
    }

    let Some(on_token) = on_token else {
        // Non-streaming: the answer is the first text block in `content`.
        let payload: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("Unexpected response from the Claude API: {e}"))?;
        let answer = payload["content"]
            .as_array()
            .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
            .and_then(|b| b["text"].as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        return match answer {
            Some(a) => Ok(a.to_string()),
            None if payload["stop_reason"] == "refusal" => {
                Err(anyhow!("The model declined to answer this request."))
            }
            None => Err(anyhow!("The assistant returned an empty response.")),
        };
    };

    // Streaming: parse the SSE event stream, forwarding text deltas to the callback.
    let mut resp = resp;
    let mut full = String::new();
    let mut buf = String::new();
    let mut refusal = false;
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| anyhow!("Lost connection to the Claude API: {e}"))?
    {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let Some(data) = line.trim_end().strip_prefix("data:") else {
                continue;
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<Value>(data) else {
                continue;
            };
            match ev["type"].as_str() {
                Some("content_block_delta") => {
                    if let Some(t) = ev["delta"]["text"].as_str() {
                        full.push_str(t);
                        on_token(t);
                    }
                }
                Some("message_delta") if ev["delta"]["stop_reason"] == "refusal" => {
                    refusal = true;
                }
                Some("error") => {
                    let msg = ev["error"]["message"].as_str().unwrap_or("stream error");
                    return Err(anyhow!("Claude API error: {msg}"));
                }
                _ => {}
            }
        }
    }
    if full.trim().is_empty() {
        return Err(if refusal {
            anyhow!("The model declined to answer this request.")
        } else {
            anyhow!("The assistant returned an empty response.")
        });
    }
    Ok(full.trim().to_string())
}

/// Answer a direct question, optionally with surrounding context (a pasted
/// transcript snippet or notes).
pub async fn ask(
    http: &reqwest::Client,
    engine: &Engine,
    question: &str,
    context: Option<&str>,
    profile: Option<&str>,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    let user_text = match context {
        Some(ctx) if !ctx.trim().is_empty() => format!(
            "Context (conversation/transcript or notes):\n{ctx}\n\nWhat I need help with:\n{question}"
        ),
        _ => question.to_string(),
    };
    let system = with_profile(SYSTEM_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, on_token).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, on_token).await
        }
    }
}

/// System prompt for the Tracker's per-application interview prep.
const PREP_PROMPT: &str = "You are an interview coach. Given a role and company the user is interviewing for \
(plus their profile if provided), produce concise prep in GitHub-flavored Markdown: likely interview \
questions grouped by type (behavioral, role/technical, and about the company), strong talking points or \
angles the candidate can lean on, and a short 'research before the interview' checklist. Be specific to the \
role and company. Note where you are unsure - you may not have current data about this company.";

/// Generate interview prep for a tracked application (role + company), tailored to
/// the user's profile.
pub async fn interview_prep(
    http: &reqwest::Client,
    engine: &Engine,
    title: &str,
    org: &str,
    profile: Option<&str>,
) -> Result<String> {
    let user_text = if org.trim().is_empty() {
        format!("Role: {title}")
    } else {
        format!("Role: {title}\nCompany: {org}")
    };
    let system = with_profile(PREP_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

/// Auto-answer: given the recent transcript and the newest segment, answer it if it
/// is a question/problem (in any language), otherwise return the literal "NONE".
pub async fn auto_answer(
    http: &reqwest::Client,
    engine: &Engine,
    transcript: &str,
    latest: &str,
    profile: Option<&str>,
) -> Result<String> {
    let user_text =
        format!("Earlier transcript (context):\n{transcript}\n\nNew text to consider:\n{latest}");
    let system = with_profile(AUTO_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

/// System prompt for the screen watcher: read a screenshot and answer any
/// question/problem visible on it, otherwise reply "NONE".
const SCREEN_PROMPT: &str = "You are looking at a screenshot of the user's screen. Find any question, \
quiz item, prompt, or problem to solve shown on it - math, code, multiple choice, a form question, an \
exercise, etc. Answer it directly and concisely in the SAME language it appears in, using GitHub-flavored \
Markdown: fenced code blocks for code (```python ... ```) and LaTeX ($...$ or $$...$$) for math. If several \
questions are visible, answer each briefly with a short label. If no question or problem is visible (just \
an ordinary app, desktop, or chat with nothing being asked), reply with exactly one word: NONE";

/// System prompt for an explicit user-selected screen region ("snip & explain").
const EXPLAIN_PROMPT: &str = "You are shown a region of the user's screen that they selected with a snipping \
tool because they want help understanding it. Explain what it shows and help with it concisely: define terms, \
summarize, and if it contains a question or problem (math, code, a diagram, an error message, a form), answer \
or resolve it. Use GitHub-flavored Markdown - fenced code blocks for code (```python ... ```) and LaTeX \
($...$ or $$...$$) for math. If the image is unreadable, say so briefly.";

/// Shared vision call: send a base64 PNG plus a short instruction under `system`.
async fn vision(
    http: &reqwest::Client,
    engine: &Engine,
    system: &str,
    user_prompt: &str,
    png_base64: &str,
    profile: Option<&str>,
) -> Result<String> {
    let system = with_profile(system, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            let content = json!([
                {
                    "type": "image",
                    "source": { "type": "base64", "media_type": "image/png", "data": png_base64 }
                },
                { "type": "text", "text": user_prompt }
            ]);
            complete_content(http, api_key, model, &system, content, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(
                http,
                url,
                model,
                &system,
                user_prompt.to_string(),
                Some(png_base64),
                None,
            )
            .await
        }
    }
}

/// Look at a screenshot (base64 PNG) and answer any question/problem on it, or
/// return the literal "NONE" when there is nothing to answer.
pub async fn answer_screen(
    http: &reqwest::Client,
    engine: &Engine,
    png_base64: &str,
    profile: Option<&str>,
) -> Result<String> {
    vision(
        http,
        engine,
        SCREEN_PROMPT,
        "Find and answer any question or problem visible on this screen.",
        png_base64,
        profile,
    )
    .await
}

/// Explain a user-selected screen region (the "snip & explain" tool).
pub async fn explain_image(
    http: &reqwest::Client,
    engine: &Engine,
    png_base64: &str,
    profile: Option<&str>,
) -> Result<String> {
    vision(
        http,
        engine,
        EXPLAIN_PROMPT,
        "Explain and help me with what's shown in this screen selection.",
        png_base64,
        profile,
    )
    .await
}

/// Chat with a local Ollama model via `POST {url}/api/chat`. Streams NDJSON when
/// `on_token` is set (each line carries a `message.content` delta), and supports a
/// base64 image for vision models. Returns the full assistant text.
async fn ollama_chat(
    http: &reqwest::Client,
    url: &str,
    model: &str,
    system: &str,
    user_text: String,
    image_base64: Option<&str>,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    let base = url.trim_end_matches('/');
    let mut user_msg = json!({ "role": "user", "content": user_text });
    if let Some(img) = image_base64 {
        user_msg["images"] = json!([img]);
    }
    let body = json!({
        "model": model,
        "stream": on_token.is_some(),
        "messages": [
            { "role": "system", "content": system },
            user_msg,
        ],
    });

    let resp = http
        .post(format!("{base}/api/chat"))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            anyhow!("Could not reach Ollama at {base}. Is `ollama serve` running? ({e})")
        })?;

    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        let detail = serde_json::from_str::<Value>(&detail)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_string))
            .unwrap_or(detail);
        return Err(match status.as_u16() {
            404 => anyhow!("Ollama model '{model}' not found. Pull it with `ollama pull {model}`."),
            _ => anyhow!("Ollama error ({}): {detail}", status.as_u16()),
        });
    }

    if on_token.is_none() {
        let payload: Value = resp
            .json()
            .await
            .map_err(|e| anyhow!("Unexpected response from Ollama: {e}"))?;
        let text = payload["message"]["content"].as_str().unwrap_or("").trim();
        return if text.is_empty() {
            Err(anyhow!("Ollama returned an empty response."))
        } else {
            Ok(text.to_string())
        };
    }

    // Streaming: one JSON object per line, each with an incremental message.content.
    let on_token = on_token.unwrap();
    let mut resp = resp;
    let mut full = String::new();
    let mut buf = String::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| anyhow!("Lost connection to Ollama: {e}"))?
    {
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line: String = buf.drain(..=nl).collect();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(ev) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if let Some(t) = ev["message"]["content"].as_str() {
                if !t.is_empty() {
                    full.push_str(t);
                    on_token(t);
                }
            }
            if let Some(err) = ev["error"].as_str() {
                return Err(anyhow!("Ollama error: {err}"));
            }
        }
    }
    if full.trim().is_empty() {
        return Err(anyhow!("Ollama returned an empty response."));
    }
    Ok(full.trim().to_string())
}

/// List locally-available Ollama model names via `GET {url}/api/tags`.
pub async fn ollama_models(http: &reqwest::Client, url: &str) -> Result<Vec<String>> {
    let base = url.trim_end_matches('/');
    let payload: Value = http
        .get(format!("{base}/api/tags"))
        .send()
        .await
        .map_err(|e| anyhow!("Could not reach Ollama at {base}: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow!("Ollama error: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow!("Unexpected response from Ollama: {e}"))?;
    Ok(payload["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|m| m["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
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
