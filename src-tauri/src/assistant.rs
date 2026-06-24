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
$...$ and display equations as $$...$$. When you write code, give real, runnable code, never pseudocode. \
Keep it tight, no filler, and do not repeat yourself or restate the question. If the input is ambiguous, \
state your best interpretation in one line, then answer it.";

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

/// Output-format directive appended to the answer prompts per the user's chosen style.
/// "auto" (default) adds nothing so the model picks the best format for the question.
fn style_directive(style: &str) -> &'static str {
    match style {
        "concise" => "\n\nANSWER FORMAT: lead with the direct answer in one line, then at most 3-4 \
short, glanceable bullet points. No preamble or long prose - it is read at a glance mid-conversation.",
        "star" => "\n\nANSWER FORMAT: for behavioral or experience questions, answer in the first \
person using the STAR structure (Situation, Task, Action, Result) - concise but complete. For technical \
or factual questions, answer directly.",
        "code" => "\n\nANSWER FORMAT: lead with one complete, runnable solution in a single \
language-tagged fenced code block - real code, never pseudocode unless explicitly asked. Then at most a \
1-2 line explanation and the time and space complexity. Give exactly one focused solution; do not restate \
the problem, list alternatives, or repeat yourself. Keep prose minimal.",
        _ => "",
    }
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
    complete_content(http, api_key, model, system, json!(user_text), 4096, true, on_token).await
}

/// Like [`complete`] but takes the user message `content` directly, so callers can
/// pass a multimodal content array (e.g. an image block plus text).
async fn complete_content(
    http: &reqwest::Client,
    api_key: &str,
    model: &str,
    system: &str,
    content: Value,
    max_tokens: u32,
    allow_thinking: bool,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    let stream = on_token.is_some();
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "stream": stream,
        "system": [{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" }
        }],
        "messages": [{ "role": "user", "content": content }],
    });
    // Adaptive thinking improves math/reasoning correctness, but visible text only
    // streams AFTER the reasoning block - so the live path (auto_answer) opts out
    // for a snappier time-to-first-token. Thinking needs output headroom.
    if allow_thinking && supports_adaptive_thinking(model) {
        body["thinking"] = json!({ "type": "adaptive" });
        if max_tokens < 8192 {
            body["max_tokens"] = json!(8192);
        }
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
        crate::usage::record_claude(
            model,
            payload["usage"]["input_tokens"].as_u64().unwrap_or(0),
            payload["usage"]["output_tokens"].as_u64().unwrap_or(0),
        );
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
    // Token usage arrives split across events: input on `message_start`, the (cumulative)
    // output on each `message_delta`.
    let mut usage_in = 0u64;
    let mut usage_out = 0u64;
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
                Some("message_start") => {
                    usage_in = ev["message"]["usage"]["input_tokens"].as_u64().unwrap_or(0);
                }
                Some("content_block_delta") => {
                    if let Some(t) = ev["delta"]["text"].as_str() {
                        full.push_str(t);
                        on_token(t);
                    }
                }
                Some("message_delta") => {
                    if let Some(o) = ev["usage"]["output_tokens"].as_u64() {
                        usage_out = o;
                    }
                    if ev["delta"]["stop_reason"] == "refusal" {
                        refusal = true;
                    }
                }
                Some("error") => {
                    let msg = ev["error"]["message"].as_str().unwrap_or("stream error");
                    return Err(anyhow!("Claude API error: {msg}"));
                }
                _ => {}
            }
        }
    }
    crate::usage::record_claude(model, usage_in, usage_out);
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
    style: &str,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    let user_text = match context {
        Some(ctx) if !ctx.trim().is_empty() => format!(
            "Context (conversation/transcript or notes):\n{ctx}\n\nWhat I need help with:\n{question}"
        ),
        _ => question.to_string(),
    };
    let mut system = with_profile(SYSTEM_PROMPT, profile);
    system.push_str(style_directive(style));
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
role and company. When a job description is provided, ground the questions and talking points in its actual \
requirements and responsibilities. Note where you are unsure - you may not have current data about this company.";

/// Generate interview prep for a tracked application (role + company), tailored to
/// the user's profile. When the application came from a saved posting, its job
/// description is passed in `jd` so the prep is grounded in the real requirements.
pub async fn interview_prep(
    http: &reqwest::Client,
    engine: &Engine,
    title: &str,
    org: &str,
    jd: Option<&str>,
    profile: Option<&str>,
) -> Result<String> {
    let mut user_text = if org.trim().is_empty() {
        format!("Role: {title}")
    } else {
        format!("Role: {title}\nCompany: {org}")
    };
    if let Some(jd) = jd.map(str::trim).filter(|s| !s.is_empty()) {
        // Cap the JD so a very long posting doesn't blow up the input.
        let jd: String = jd.chars().take(6000).collect();
        user_text.push_str(&format!("\n\nJob description:\n{jd}"));
    }
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

/// System prompt for the Study tab's plan generator.
const STUDY_PROMPT: &str = "You are a practical learning coach. Given a skill the user wants to build (and \
their goal and role context if provided), produce a focused study plan in GitHub-flavored Markdown: a one- \
or two-line overview, a phased roadmap (Foundations -> Core -> Advanced) with concrete topics under each, a \
short list of recommended resources (docs, courses, books), 3-5 hands-on project ideas, and how to prepare \
for interviews on this skill. Be concrete and ordered; keep it skimmable with headings and bullet lists. Do \
not invent specific URLs you are unsure about.";

/// Generate a study plan for a skill, tailored to the user's goal + profile.
pub async fn study_plan(
    http: &reqwest::Client,
    engine: &Engine,
    skill: &str,
    goal: &str,
    profile: Option<&str>,
) -> Result<String> {
    let user_text = if goal.trim().is_empty() {
        format!("Skill to learn: {skill}")
    } else {
        format!("Skill to learn: {skill}\nGoal: {goal}")
    };
    let system = with_profile(STUDY_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

/// System prompt for drafting a reusable STAR story from the user's rough notes.
const STORY_PROMPT: &str = "You help a candidate turn rough notes about a real experience into a reusable \
interview story. Write a tight first-person STAR story with four short labelled paragraphs - Situation, \
Task, Action, Result. Keep it concrete and concise (the candidate will adapt it live), quantify impact \
where the notes support it, and never invent facts that contradict or go beyond the notes. Output plain \
text with the STAR labels, no preamble.";

/// Draft a STAR story from the user's rough notes about an experience.
pub async fn story_draft(
    http: &reqwest::Client,
    engine: &Engine,
    notes: &str,
    profile: Option<&str>,
) -> Result<String> {
    let user_text = format!("My rough notes about the experience:\n{notes}");
    let system = with_profile(STORY_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

// Application kit: tailor the candidate's materials to a specific job. The candidate's
// resume + profile arrive via the system prompt (profile), so the user input is just
// the job description (or, for a thank-you note, the interview transcript).
const KIT_BULLETS: &str = "You tailor a candidate's resume to a specific job description. Using ONLY \
facts from their resume and profile (never invent experience), rewrite their most relevant experience as \
5-8 strong, quantified, JD-aligned bullet points that mirror the job's key terms and priorities. Output \
just the bullets in GitHub-flavored Markdown.";
const KIT_COVER: &str = "Write a concise, specific cover letter (3-4 short paragraphs) for this job, \
grounded in the candidate's resume and profile - concrete and tailored, no clichés or filler. Address the \
role's key needs and why they fit. GitHub-flavored Markdown.";
const KIT_ATS: &str = "Act as an ATS keyword/fit checker. Compare the candidate's resume + profile to the \
job description and output, in GitHub-flavored Markdown: (1) an estimated match percentage with a one-line \
rationale, (2) a 'Present' list of important JD keywords/skills the resume already covers, (3) a 'Missing' \
list of important JD keywords/skills not found, and (4) concrete, truthful tweaks the candidate could make. \
Never suggest claiming skills they do not have.";
const KIT_THANKS: &str = "Write a short, warm post-interview thank-you / follow-up email (under ~150 \
words) that references one or two specific things from the interview transcript, reaffirms interest and \
fit, and stays sincere and natural. GitHub-flavored Markdown.";

/// Generate an application document. `kind` selects the prompt:
/// "bullets" | "cover" | "ats" use `input` as the job description; "thanks" uses
/// `input` as the interview transcript. The resume/profile come via `profile`.
pub async fn application_doc(
    http: &reqwest::Client,
    engine: &Engine,
    kind: &str,
    input: &str,
    profile: Option<&str>,
) -> Result<String> {
    let base = match kind {
        "bullets" => KIT_BULLETS,
        "cover" => KIT_COVER,
        "ats" => KIT_ATS,
        "thanks" => KIT_THANKS,
        _ => return Err(anyhow!("Unknown application document kind: {kind}")),
    };
    let system = with_profile(base, profile);
    let user_text = if kind == "thanks" {
        format!("Interview transcript / notes:\n{input}")
    } else {
        format!("Job description:\n{input}")
    };
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

const ATS_PROMPT: &str = "Act as an applicant-tracking-system (ATS) resume scorer. Compare the candidate's \
resume and profile to the job description and return ONLY a JSON object (no prose, no code fence) with \
exactly these fields: \"keywords\" (0-100: how well the resume covers the JD's hard skills and keywords), \
\"responsibilities\" (0-100: match to the role's duties and requirements), \"title\" (0-100: job title and \
seniority alignment), \"impact\" (0-100: presence of quantified, relevant achievements), \"format\" (0-100: \
ATS-friendly, cleanly parseable structure), \"matched\" (array of important JD keywords/skills found in the \
resume), \"missing\" (array of important JD keywords/skills not found), and \"fixes\" (array of concrete, \
truthful improvements; never suggest claiming skills the candidate does not have). Score honestly and \
specifically. Output the JSON object only.";

/// A structured ATS-style match score for a resume against a job description. The
/// sub-scores come from the model; `overall` is a weighted blend computed in Rust.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AtsScore {
    pub overall: u8,
    pub keywords: u8,
    pub responsibilities: u8,
    pub title: u8,
    pub impact: u8,
    pub format: u8,
    pub matched: Vec<String>,
    pub missing: Vec<String>,
    pub fixes: Vec<String>,
}

/// Score the stored resume/profile against a job description. Emits strict JSON,
/// parsed leniently; the overall is computed here (not trusted to the model).
pub async fn score_ats(
    http: &reqwest::Client,
    engine: &Engine,
    jd: &str,
    profile: Option<&str>,
) -> Result<AtsScore> {
    let system = with_profile(ATS_PROMPT, profile);
    let user_text = format!("Job description:\n{jd}");
    let raw = match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await?
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await?
        }
    };
    parse_ats_json(&raw)
}

/// Parse the ATS score JSON, tolerating code fences/prose (same approach as
/// parse_cards_json): take the outermost object, clamp each sub-score to 0-100, and
/// compute the weighted overall in Rust so a chatty model can't skew it.
fn parse_ats_json(raw: &str) -> Result<AtsScore> {
    let cleaned = raw.trim();
    let cleaned = cleaned
        .strip_prefix("```json")
        .or_else(|| cleaned.strip_prefix("```"))
        .unwrap_or(cleaned)
        .trim_end_matches("```")
        .trim();
    let slice = match (cleaned.find('{'), cleaned.rfind('}')) {
        (Some(s), Some(e)) if e > s => &cleaned[s..=e],
        _ => return Err(anyhow!("The model did not return a score.")),
    };
    let v: Value =
        serde_json::from_str(slice).map_err(|e| anyhow!("Could not parse the ATS score: {e}"))?;
    let sub = |k: &str| {
        v.get(k)
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .clamp(0.0, 100.0)
    };
    let (keywords, responsibilities, title, impact, format) = (
        sub("keywords"),
        sub("responsibilities"),
        sub("title"),
        sub("impact"),
        sub("format"),
    );
    let overall =
        (keywords * 0.40 + responsibilities * 0.20 + title * 0.15 + impact * 0.15 + format * 0.10)
            .round();
    let list = |k: &str| {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };
    Ok(AtsScore {
        overall: overall as u8,
        keywords: keywords as u8,
        responsibilities: responsibilities as u8,
        title: title as u8,
        impact: impact as u8,
        format: format as u8,
        matched: list("matched"),
        missing: list("missing"),
        fixes: list("fixes"),
    })
}

/// System prompt for the mock-interviewer: ask one realistic question at a time.
const PRACTICE_Q_PROMPT: &str = "You are running a realistic mock interview for the candidate's target role \
and company (see their profile). Ask exactly ONE interview question. Across a session, vary the type - \
behavioral, role/technical, and company/role fit. Do NOT repeat or closely paraphrase anything in the \
'Already asked' list. Output ONLY the question itself - no numbering, preamble, or commentary.";

/// System prompt for scoring a mock-interview answer.
const PRACTICE_FB_PROMPT: &str = "You are an interview coach reviewing one answer in a mock interview. \
Given the question and the candidate's answer (with any delivery stats), give concise feedback in \
GitHub-flavored Markdown: a '**Score: N/5**' line, a 'What worked' list, a 'To improve' list \
(structure, specificity, relevance), brief 'Delivery' notes when the stats suggest filler words or \
rambling, and a tighter '**Model answer**' grounded in the candidate's profile/stories where relevant. \
Be direct and specific - no fluff or flattery.";

/// Ask the next mock-interview question, avoiding the ones already asked.
pub async fn practice_question(
    http: &reqwest::Client,
    engine: &Engine,
    asked: &[String],
    profile: Option<&str>,
) -> Result<String> {
    let asked_list = if asked.is_empty() {
        "(none yet)".to_string()
    } else {
        asked
            .iter()
            .map(|q| format!("- {q}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let user_text = format!("Already asked:\n{asked_list}\n\nAsk the next question.");
    let system = with_profile(PRACTICE_Q_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

/// Score a mock-interview answer and return coaching feedback + a model answer.
pub async fn practice_feedback(
    http: &reqwest::Client,
    engine: &Engine,
    question: &str,
    answer: &str,
    profile: Option<&str>,
) -> Result<String> {
    let user_text = format!("Question:\n{question}\n\nCandidate's answer:\n{answer}");
    let system = with_profile(PRACTICE_FB_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

/// System prompt for reviewing a whole interview session and surfacing what to study.
const REVIEW_PROMPT: &str = "You are an interview coach reviewing a full interview session - the \
conversation transcript and the answers the candidate gave. In GitHub-flavored Markdown, produce: a \
one-line summary, 'What went well', 'Recurring weak spots' (grouped by theme or question type), 'To \
improve next time', and a short prioritized 'Study & prep list' (topics to study or stories to prepare). \
Reference what actually happened in the session. If the material is thin, say so briefly rather than \
inventing detail.";

/// Review a saved session: strengths, weak areas, and a prioritized study list.
pub async fn session_review(
    http: &reqwest::Client,
    engine: &Engine,
    transcript: &str,
    answers: &str,
    profile: Option<&str>,
) -> Result<String> {
    let user_text = format!("Transcript:\n{transcript}\n\nAnswers given:\n{answers}");
    let system = with_profile(REVIEW_PROMPT, profile);
    match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await
        }
    }
}

/// System prompt for turning material into spaced-repetition flashcards.
const CARDS_PROMPT: &str = "Turn the material into spaced-repetition flashcards. Output ONLY a JSON array \
of objects, each exactly {\"front\": \"<question or prompt>\", \"back\": \"<concise answer>\"} - between 6 \
and 12 cards, focused on the weak areas or the key facts/concepts. No prose, no Markdown, no code fences - \
just the raw JSON array.";

/// Generate flashcards (front/back pairs) from arbitrary study material.
pub async fn generate_cards(
    http: &reqwest::Client,
    engine: &Engine,
    source: &str,
    profile: Option<&str>,
) -> Result<Vec<(String, String)>> {
    let user_text = format!("Material:\n{source}");
    let system = with_profile(CARDS_PROMPT, profile);
    let raw = match engine {
        Engine::Claude { api_key, model } => {
            complete(http, api_key, model, &system, user_text, None).await?
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, None).await?
        }
    };
    parse_cards_json(&raw)
}

/// Parse flashcards from a model response, tolerating code fences and surrounding
/// prose: strip a leading ```/```json fence, then take the outermost `[ ... ]` and
/// keep objects with non-empty `front`/`back`. Errors when nothing usable is found.
fn parse_cards_json(raw: &str) -> Result<Vec<(String, String)>> {
    let cleaned = raw.trim();
    let cleaned = cleaned
        .strip_prefix("```json")
        .or_else(|| cleaned.strip_prefix("```"))
        .unwrap_or(cleaned)
        .trim_end_matches("```")
        .trim();
    let start = cleaned.find('[');
    let end = cleaned.rfind(']');
    let slice = match (start, end) {
        (Some(s), Some(e)) if e > s => &cleaned[s..=e],
        _ => return Err(anyhow!("The model did not return flashcards.")),
    };
    let value: Value =
        serde_json::from_str(slice).map_err(|e| anyhow!("Could not parse flashcards: {e}"))?;
    let cards: Vec<(String, String)> = value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let f = c.get("front")?.as_str()?.trim().to_string();
                    let b = c.get("back")?.as_str()?.trim().to_string();
                    (!f.is_empty() && !b.is_empty()).then_some((f, b))
                })
                .collect()
        })
        .unwrap_or_default();
    if cards.is_empty() {
        return Err(anyhow!("No usable flashcards were produced."));
    }
    Ok(cards)
}

/// Auto-answer: given the recent transcript and the newest segment, answer it if it
/// is a question/problem (in any language), otherwise return the literal "NONE".
pub async fn auto_answer(
    http: &reqwest::Client,
    engine: &Engine,
    transcript: &str,
    latest: &str,
    profile: Option<&str>,
    style: &str,
    on_token: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Result<String> {
    // Bound the context so prefill / time-to-first-token / cost stay flat over a
    // long call instead of growing with the whole transcript. Keep the tail on a
    // char boundary (the transcript can be in any language).
    let transcript = {
        const MAX_CTX_BYTES: usize = 4000;
        if transcript.len() > MAX_CTX_BYTES {
            let mut start = transcript.len() - MAX_CTX_BYTES;
            while start < transcript.len() && !transcript.is_char_boundary(start) {
                start += 1;
            }
            &transcript[start..]
        } else {
            transcript
        }
    };
    let user_text =
        format!("Earlier transcript (context):\n{transcript}\n\nNew text to consider:\n{latest}");
    let mut system = with_profile(AUTO_PROMPT, profile);
    system.push_str(style_directive(style));
    match engine {
        Engine::Claude { api_key, model } => {
            // Live path: skip adaptive thinking (faster first token) and bound the
            // output budget - these are concise mid-call answers or "NONE".
            complete_content(http, api_key, model, &system, json!(user_text), 2048, false, on_token)
                .await
        }
        Engine::Ollama { url, model } => {
            ollama_chat(http, url, model, &system, user_text, None, on_token).await
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
            complete_content(http, api_key, model, &system, content, 4096, true, None).await
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
    context: Option<&str>,
) -> Result<String> {
    // When the live conversation is supplied (capture mode's screen watch), give it
    // to the model so the screen answer is informed by what is being discussed.
    let prompt = match context.map(str::trim).filter(|c| !c.is_empty()) {
        Some(convo) => format!(
            "Recent interview conversation, for context:\n{convo}\n\nNow look at the screen: find and \
answer any question or problem visible on it, taking the conversation into account."
        ),
        None => "Find and answer any question or problem visible on this screen.".to_string(),
    };
    vision(http, engine, SCREEN_PROMPT, &prompt, png_base64, profile).await
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

#[cfg(test)]
mod tests {
    use super::{parse_ats_json, parse_cards_json, style_directive};

    #[test]
    fn ats_weighted_overall_and_clamping() {
        // keywords 100*.40 + resp 80*.20 + title 60*.15 + impact 40*.15 + format 20*.10
        // = 40 + 16 + 9 + 6 + 2 = 73. Out-of-range sub-scores are clamped.
        let raw = r#"```json
        {"keywords":100,"responsibilities":80,"title":60,"impact":40,"format":20,
         "matched":["rust"," tokio "],"missing":["k8s"],"fixes":["quantify impact"]}
        ```"#;
        let s = parse_ats_json(raw).unwrap();
        assert_eq!(s.overall, 73);
        assert_eq!(s.keywords, 100);
        assert_eq!(s.matched, vec!["rust".to_string(), "tokio".to_string()]);
        assert_eq!(s.missing, vec!["k8s".to_string()]);
    }

    #[test]
    fn ats_clamps_and_defaults_missing_fields() {
        let s = parse_ats_json(r#"{"keywords":150}"#).unwrap();
        assert_eq!(s.keywords, 100); // clamped from 150
        assert_eq!(s.responsibilities, 0); // missing -> 0
        assert_eq!(s.overall, 40); // 100*.40 only
        assert!(s.fixes.is_empty());
    }

    #[test]
    fn ats_errors_without_an_object() {
        assert!(parse_ats_json("I couldn't score this.").is_err());
    }

    #[test]
    fn code_style_demands_real_code_and_complexity() {
        let d = style_directive("code");
        assert!(d.contains("pseudocode"), "should forbid pseudocode");
        assert!(d.contains("complexity"), "should ask for complexity");
        assert!(
            d.contains("one"),
            "should ask for a single focused solution"
        );
    }

    #[test]
    fn auto_and_unknown_styles_add_no_directive() {
        assert_eq!(style_directive("auto"), "");
        assert_eq!(style_directive("anything-else"), "");
    }

    #[test]
    fn parses_a_plain_json_array() {
        let cards =
            parse_cards_json(r#"[{"front":"Q1","back":"A1"},{"front":"Q2","back":"A2"}]"#).unwrap();
        assert_eq!(
            cards,
            vec![("Q1".into(), "A1".into()), ("Q2".into(), "A2".into())]
        );
    }

    #[test]
    fn tolerates_code_fences_and_prose() {
        let raw =
            "Sure! Here are your cards:\n```json\n[{\"front\":\"a\",\"back\":\"b\"}]\n```\nEnjoy.";
        assert_eq!(
            parse_cards_json(raw).unwrap(),
            vec![("a".into(), "b".into())]
        );
    }

    #[test]
    fn skips_cards_missing_a_side() {
        let raw = r#"[{"front":"keep","back":"yes"},{"front":"  ","back":"x"},{"front":"only"}]"#;
        assert_eq!(
            parse_cards_json(raw).unwrap(),
            vec![("keep".into(), "yes".into())]
        );
    }

    #[test]
    fn errors_when_no_array_present() {
        assert!(parse_cards_json("I could not make any flashcards.").is_err());
    }

    #[test]
    fn errors_on_empty_or_all_invalid_array() {
        assert!(parse_cards_json("[]").is_err());
        assert!(parse_cards_json(r#"[{"front":"","back":""}]"#).is_err());
    }
}
