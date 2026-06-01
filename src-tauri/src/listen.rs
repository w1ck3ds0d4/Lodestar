//! System-audio (loopback) and/or microphone capture + transcription for the live
//! assistant.
//!
//! Captures the default output device via WASAPI loopback (the other side of a call
//! or a video) and/or the default input device (your mic), per the chosen source.
//! Each source is downmixed to mono, resampled to 16 kHz, and transcribed
//! independently (no mixing) so every `transcript` event carries a speaker role: the
//! microphone is labeled "you"; system/loopback audio is labeled "them" (local
//! engine) or diarized into "Speaker 1/2/..." by Deepgram.
//!
//! Deepgram (cloud) uses the realtime streaming WebSocket API: each source opens its
//! own socket, audio is streamed continuously, and transcripts arrive live - interim
//! (`final: false`) results update the in-progress line as someone speaks, then a
//! `final: true` result commits it. This avoids the word-boundary loss and dropped
//! speech of the old fixed-chunk pre-recorded calls. The local Whisper engine (Candle,
//! on-device) is inherently batch, so it still transcribes ~4s chunks. Progress/errors
//! are emitted as `listen-status`/`listen-error`; capture diagnostics as
//! `listen-device`/`listen-level`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SupportedStreamConfig};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::local_stt::LocalStt;

const CHUNK_SECS: u64 = 4;
const SILENCE_PEAK: f32 = 0.005;
const TARGET_RATE: u32 = 16000;

/// Which transcription backend to use for a listening session.
pub enum EngineConfig {
    /// `language` is a Deepgram language code (e.g. "en", "pt", "es") or "multi" for
    /// the multilingual/code-switching model. Streaming has no auto-detect, so the
    /// language must be chosen up front (default "multi"). `diarize` splits the system
    /// side into "Speaker 1/2/..."; off by default since streaming diarization is
    /// unstable and tends to mislabel one person as several on a 1-on-1 call.
    Deepgram {
        key: String,
        language: String,
        diarize: bool,
    },
    Local {
        models_dir: PathBuf,
    },
}

/// Which audio source(s) to capture.
#[derive(Clone, Copy)]
pub enum CaptureSource {
    System,
    Mic,
    Both,
}

impl CaptureSource {
    pub fn parse(s: &str) -> Self {
        match s {
            "mic" => Self::Mic,
            "both" => Self::Both,
            _ => Self::System,
        }
    }
    fn system(self) -> bool {
        matches!(self, Self::System | Self::Both)
    }
    fn mic(self) -> bool {
        matches!(self, Self::Mic | Self::Both)
    }
}

/// Chosen capture devices by name; `None` means use the system default.
#[derive(Clone, Default)]
pub struct DeviceChoice {
    pub input: Option<String>,
    pub output: Option<String>,
}

/// Enumerate available input and output device names for the device pickers.
pub fn list_devices() -> (Vec<String>, Vec<String>) {
    let host = cpal::default_host();
    let inputs = host
        .input_devices()
        .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    let outputs = host
        .output_devices()
        .map(|ds| ds.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default();
    (inputs, outputs)
}

/// Find a device by name among the host's input or output devices; fall back to
/// the corresponding default when no name is given or no match is found.
fn pick_device(host: &cpal::Host, name: Option<&str>, input: bool) -> Option<Device> {
    if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
        let listed = if input {
            host.input_devices()
        } else {
            host.output_devices()
        };
        if let Ok(mut devs) = listed {
            if let Some(d) = devs.find(|d| d.name().map(|n| n == name).unwrap_or(false)) {
                return Some(d);
            }
        }
    }
    if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SrcKind {
    System,
    Mic,
}

/// One captured source: its accumulation buffer plus native format.
struct Source {
    buffer: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    channels: usize,
    name: String,
    role: &'static str,
    kind: SrcKind,
}

impl Source {
    fn is_system(&self) -> bool {
        self.kind == SrcKind::System
    }
}

fn on_stream_error(err: cpal::StreamError) {
    tracing::warn!("audio capture stream error: {err}");
}

fn status(app: &AppHandle, msg: &str) {
    let _ = app.emit("listen-status", msg.to_string());
}

/// Build an input stream on `device` (loopback for an output device, normal capture
/// for an input device), appending samples as f32 to `buffer`.
fn capture_stream(
    device: &Device,
    supported: &SupportedStreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
) -> Result<cpal::Stream> {
    let config: cpal::StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                buffer.lock().unwrap().extend_from_slice(data);
            },
            on_stream_error,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                buffer
                    .lock()
                    .unwrap()
                    .extend(data.iter().map(|&s| s as f32 / 32768.0));
            },
            on_stream_error,
            None,
        ),
        other => return Err(anyhow!("Unsupported audio sample format: {other:?}")),
    }
    .map_err(|e| anyhow!("Could not open capture stream: {e}"))?;
    Ok(stream)
}

/// Capture audio and emit `transcript` events until `flag` is cleared.
pub fn run(
    app: &AppHandle,
    http: reqwest::Client,
    engine: EngineConfig,
    capture: CaptureSource,
    devices: DeviceChoice,
    flag: Arc<AtomicBool>,
) -> Result<()> {
    // The local engine downloads + loads the model up front (slow on first run).
    let mut local: Option<LocalStt> = None;
    if let EngineConfig::Local { models_dir } = &engine {
        local = Some(LocalStt::load(&http, models_dir, &|m| status(app, m))?);
    }

    let host = cpal::default_host();
    let mut streams: Vec<cpal::Stream> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();

    if capture.system() {
        let device = pick_device(&host, devices.output.as_deref(), false)
            .ok_or_else(|| anyhow!("No audio output device found to capture."))?;
        let name = device.name().unwrap_or_else(|_| "Default output".into());
        let supported = device
            .default_output_config()
            .map_err(|e| anyhow!("Could not read the output device format: {e}"))?;
        let rate = supported.sample_rate().0;
        let channels = supported.config().channels.max(1) as usize;
        let buffer = Arc::new(Mutex::new(Vec::new()));
        streams.push(capture_stream(&device, &supported, buffer.clone())?);
        sources.push(Source {
            buffer,
            rate,
            channels,
            name,
            role: "System (loopback)",
            kind: SrcKind::System,
        });
    }
    if capture.mic() {
        let device = pick_device(&host, devices.input.as_deref(), true)
            .ok_or_else(|| anyhow!("No microphone found to capture."))?;
        let name = device
            .name()
            .unwrap_or_else(|_| "Default microphone".into());
        let supported = device
            .default_input_config()
            .map_err(|e| anyhow!("Could not read the microphone format: {e}"))?;
        let rate = supported.sample_rate().0;
        let channels = supported.config().channels.max(1) as usize;
        let buffer = Arc::new(Mutex::new(Vec::new()));
        streams.push(capture_stream(&device, &supported, buffer.clone())?);
        sources.push(Source {
            buffer,
            rate,
            channels,
            name,
            role: "Microphone",
            kind: SrcKind::Mic,
        });
    }

    for s in &streams {
        s.play()
            .map_err(|e| anyhow!("Could not start audio capture: {e}"))?;
    }
    status(app, "Listening");

    // Tell the UI exactly which devices are being captured (diagnostics panel).
    let device_info: Vec<Value> = sources
        .iter()
        .map(|s| json!({ "role": s.role, "name": s.name, "rate": s.rate, "channels": s.channels }))
        .collect();
    let _ = app.emit("listen-device", json!(device_info));

    // Hand off to the engine-specific driver. Deepgram streams continuously over a
    // WebSocket per source; the local Whisper engine batches ~4s chunks. The cpal
    // streams stay alive (held in `streams`) on this thread; both drivers only touch
    // the shared sample buffers, which fill from the audio callbacks meanwhile.
    match engine {
        EngineConfig::Deepgram {
            key,
            language,
            diarize,
        } => {
            run_deepgram(app, &key, &language, diarize, &sources, &flag);
        }
        EngineConfig::Local { .. } => {
            let local = local.expect("local engine initialized above");
            run_local_chunked(app, local, &sources, &flag);
        }
    }

    drop(streams);
    Ok(())
}

/// Deepgram realtime streaming driver: open one WebSocket per source and stream audio
/// continuously, emitting live `transcript` events. Runs every source concurrently on
/// this thread via the shared async runtime; one source failing does not stop the rest.
fn run_deepgram(
    app: &AppHandle,
    key: &str,
    language: &str,
    diarize: bool,
    sources: &[Source],
    flag: &Arc<AtomicBool>,
) {
    // rustls 0.23 needs a process-default crypto provider before the first wss://
    // handshake; reqwest installs its provider explicitly (not as the default), so we
    // set ring here. Idempotent: a second call just returns Err, which we ignore.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let metas: Vec<SrcMeta> = sources
        .iter()
        .map(|s| SrcMeta {
            buffer: s.buffer.clone(),
            rate: s.rate,
            channels: s.channels,
            kind: s.kind,
        })
        .collect();

    tauri::async_runtime::block_on(async {
        let futs = metas.into_iter().map(|meta| {
            let app = app.clone();
            let key = key.to_string();
            let language = language.to_string();
            let flag = flag.clone();
            async move {
                if let Err(e) = stream_source(&app, &key, &language, diarize, &meta, &flag).await {
                    let _ = app.emit("listen-error", e.to_string());
                }
            }
        });
        futures_util::future::join_all(futs).await;
    });
}

/// Per-source streaming metadata (the cpal buffer plus its native format).
struct SrcMeta {
    buffer: Arc<Mutex<Vec<f32>>>,
    rate: u32,
    channels: usize,
    kind: SrcKind,
}

/// Stream one source to Deepgram until the user stops. The first connection fails
/// fast so auth/config/network errors surface, but a mid-session drop (a network
/// blip, or Deepgram's per-stream time limit) reconnects transparently: the
/// committed transcript already lives in the UI, so the user keeps their context
/// instead of having to restart. Gives up only after several failed reconnects.
async fn stream_source(
    app: &AppHandle,
    key: &str,
    language: &str,
    diarize_enabled: bool,
    meta: &SrcMeta,
    flag: &Arc<AtomicBool>,
) -> Result<()> {
    connect_and_stream(app, key, language, diarize_enabled, meta, flag).await?;

    let mut failures = 0u32;
    while flag.load(Ordering::SeqCst) {
        status(app, "Reconnecting transcription...");
        // Discard audio captured during the gap so we don't ship a stale backlog.
        if let Ok(mut g) = meta.buffer.lock() {
            g.clear();
        }
        // Brief backoff, but bail within ~150ms of a stop request.
        let mut waited = 0;
        while flag.load(Ordering::SeqCst) && waited < 8 {
            tokio::time::sleep(Duration::from_millis(150)).await;
            waited += 1;
        }
        if !flag.load(Ordering::SeqCst) {
            break;
        }
        match connect_and_stream(app, key, language, diarize_enabled, meta, flag).await {
            Ok(()) => failures = 0,
            Err(e) => {
                if !flag.load(Ordering::SeqCst) {
                    break; // user stopped during a reconnect attempt; not an error
                }
                failures += 1;
                if failures >= 5 {
                    return Err(anyhow!(
                        "Lost the transcription connection and could not reconnect: {e}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Open one Deepgram streaming WebSocket, push audio frames as they are captured, and
/// emit `transcript` events for interim + final results until the socket closes or
/// `flag` clears. Returns when the stream ends (cleanly or dropped); the caller
/// decides whether to reconnect.
async fn connect_and_stream(
    app: &AppHandle,
    key: &str,
    language: &str,
    diarize_enabled: bool,
    meta: &SrcMeta,
    flag: &Arc<AtomicBool>,
) -> Result<()> {
    // Only diarize the system side, and only when the user asked for it.
    let diarize = diarize_enabled && meta.kind == SrcKind::System;
    let lang = if language.trim().is_empty() {
        "multi"
    } else {
        language.trim()
    };
    // endpointing=500: wait ~0.5s of silence before ending an utterance, so brief
    // mid-sentence pauses in fast speech don't split a phrase into separate bubbles.
    let mut url = format!(
        "wss://api.deepgram.com/v1/listen?model=nova-2&encoding=linear16&sample_rate={TARGET_RATE}\
         &channels=1&punctuate=true&interim_results=true&endpointing=500&language={lang}"
    );
    if diarize {
        url.push_str("&diarize=true");
    }

    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|e| anyhow!("Could not build Deepgram request: {e}"))?;
    request.headers_mut().insert(
        "Authorization",
        HeaderValue::from_str(&format!("Token {key}"))
            .map_err(|e| anyhow!("Invalid Deepgram key: {e}"))?,
    );

    let (ws, _resp) = connect_async(request)
        .await
        .map_err(|e| anyhow!("Could not connect to Deepgram streaming: {e}"))?;
    let (mut write, mut read) = ws.split();
    // Clear any "Reconnecting..." status once the (re)connection is live.
    status(app, "Listening");

    let role: &str = if diarize { "them" } else { "you" };

    // Sender: drain the capture buffer ~8x/sec and ship it as raw linear16. Keep the
    // socket warm with KeepAlive during silence; on stop, send CloseStream so Deepgram
    // flushes any trailing words before closing.
    let sender = async {
        let frame = Duration::from_millis(120);
        let mut idle = 0u32;
        // Seconds of audio shipped, accumulated locally and flushed to the usage tally
        // periodically (and on exit) so we don't lock the DB on every 120ms frame.
        let mut pending_secs = 0f64;
        loop {
            if !flag.load(Ordering::SeqCst) {
                crate::usage::record_deepgram_seconds(pending_secs);
                let _ = write
                    .send(Message::Text("{\"type\":\"CloseStream\"}".to_string()))
                    .await;
                break;
            }
            tokio::time::sleep(frame).await;
            let samples = {
                let mut g = meta.buffer.lock().unwrap();
                std::mem::take(&mut *g)
            };
            if samples.is_empty() {
                idle += 1;
                if idle >= 40 {
                    let _ = write
                        .send(Message::Text("{\"type\":\"KeepAlive\"}".to_string()))
                        .await;
                    idle = 0;
                }
                continue;
            }
            idle = 0;
            let mono = downmix_mono(&samples, meta.channels);
            let resampled = resample(&mono, meta.rate, TARGET_RATE);
            let peak = resampled.iter().fold(0f32, |m, &s| m.max(s.abs()));
            let _ = app.emit("listen-level", json!({ "peak": peak, "buffered": 0 }));
            pending_secs += resampled.len() as f64 / TARGET_RATE as f64;
            if pending_secs >= 15.0 {
                crate::usage::record_deepgram_seconds(pending_secs);
                pending_secs = 0.0;
            }
            if write
                .send(Message::Binary(mono_to_linear16(&resampled)))
                .await
                .is_err()
            {
                crate::usage::record_deepgram_seconds(pending_secs);
                break;
            }
        }
    };

    // Reader: parse transcript messages as they arrive. The select! lets us bail within
    // ~0.5s of a stop request even if Deepgram is slow to close the socket.
    let reader = async {
        loop {
            tokio::select! {
                msg = read.next() => match msg {
                    Some(Ok(Message::Text(t))) => emit_results(app, &t, diarize, role),
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                },
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    if !flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
        }
    };

    tokio::join!(sender, reader);
    Ok(())
}

/// Parse one Deepgram streaming `Results` message and emit `transcript` events.
///
/// The frontend accumulates a whole utterance into one live bubble, so the events
/// carry two flags: `final` (this span won't change) and `speechFinal` (the speaker
/// paused, so the bubble can be committed). Interim spans update the live tail; final
/// spans extend it; a speech-final span closes it. This keeps fast speech in one
/// flowing line instead of chopping a sentence into pieces at every internal finalize.
fn emit_results(app: &AppHandle, text: &str, diarize: bool, role: &str) {
    let payload: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };
    // Only "Results" carry a transcript; ignore Metadata/UtteranceEnd/etc.
    if payload.get("channel").is_none() {
        return;
    }
    let is_final = payload["is_final"].as_bool().unwrap_or(false);
    let speech_final = payload["speech_final"].as_bool().unwrap_or(false);
    let alt = &payload["channel"]["alternatives"][0];
    let transcript = alt["transcript"].as_str().unwrap_or("").trim().to_string();
    let role_speaker = if role == "them" { "Them" } else { "You" };

    if transcript.is_empty() {
        // An endpoint with no new words: close the live line if one is open.
        if is_final && speech_final {
            let _ = app.emit(
                "transcript",
                json!({ "role": role, "speaker": role_speaker, "text": "", "final": true, "speechFinal": true }),
            );
        }
        return;
    }

    if !is_final {
        let _ = app.emit(
            "transcript",
            json!({ "role": role, "speaker": role_speaker, "text": transcript, "final": false }),
        );
        return;
    }

    // Final span. With diarization on, split it by speaker and commit each piece
    // immediately (speechFinal) so distinct speakers never merge into one bubble.
    if diarize {
        if let Some(words) = alt["words"].as_array().filter(|w| !w.is_empty()) {
            let emit_seg = |spk: i64, buf: &str| {
                if !buf.trim().is_empty() {
                    let _ = app.emit(
                        "transcript",
                        json!({
                            "role": "them",
                            "speaker": format!("Speaker {}", spk + 1),
                            "text": buf.trim(),
                            "final": true,
                            "speechFinal": true,
                        }),
                    );
                }
            };
            let mut cur: Option<i64> = None;
            let mut buf = String::new();
            for w in words {
                let spk = w["speaker"].as_i64().unwrap_or(0);
                let word = w["punctuated_word"]
                    .as_str()
                    .or_else(|| w["word"].as_str())
                    .unwrap_or("");
                if cur != Some(spk) {
                    if let Some(prev) = cur {
                        emit_seg(prev, &buf);
                    }
                    cur = Some(spk);
                    buf.clear();
                }
                buf.push_str(word);
                buf.push(' ');
            }
            if let Some(prev) = cur {
                emit_seg(prev, &buf);
            }
            return;
        }
    }

    // No diarization: extend the live line, and let `speechFinal` decide when to commit.
    let _ = app.emit(
        "transcript",
        json!({ "role": role, "speaker": role_speaker, "text": transcript, "final": true, "speechFinal": speech_final }),
    );
}

/// Local Whisper driver: peek the buffers ~5x/sec for a live input-level meter, and
/// drain + transcribe a ~4s chunk per source on each interval. A flat meter at 0 means
/// no audio is reaching the chosen device (wrong output for loopback); a moving meter
/// with no transcript points at the engine instead.
fn run_local_chunked(
    app: &AppHandle,
    mut local: LocalStt,
    sources: &[Source],
    flag: &Arc<AtomicBool>,
) {
    let tick = Duration::from_millis(200);
    let ticks_per_chunk = (CHUNK_SECS * 1000 / 200).max(1);
    let mut tick_count = 0u64;
    let mut empty_cycles = 0u32;
    while flag.load(Ordering::SeqCst) {
        std::thread::sleep(tick);

        let mut live_peak = 0f32;
        let mut buffered = 0usize;
        for src in sources {
            let g = src.buffer.lock().unwrap();
            buffered += g.len();
            for &s in g.iter() {
                live_peak = live_peak.max(s.abs());
            }
        }
        let _ = app.emit(
            "listen-level",
            json!({ "peak": live_peak, "buffered": buffered }),
        );

        tick_count += 1;
        if tick_count < ticks_per_chunk {
            continue;
        }
        tick_count = 0;

        let mut had_audio = false;
        let mut saw_speech = false;
        for src in sources {
            let samples = {
                let mut g = src.buffer.lock().unwrap();
                std::mem::take(&mut *g)
            };
            if samples.is_empty() {
                continue;
            }
            had_audio = true;
            let mono = downmix_mono(&samples, src.channels);
            let resampled = resample(&mono, src.rate, TARGET_RATE);
            let peak = resampled.iter().fold(0f32, |m, &s| m.max(s.abs()));
            if peak < SILENCE_PEAK {
                continue;
            }
            saw_speech = true;
            status(app, "Transcribing...");
            match local.transcribe(&resampled) {
                Ok(t) if !t.trim().is_empty() => {
                    let (role, speaker) = if src.is_system() {
                        ("them", "Them")
                    } else {
                        ("you", "You")
                    };
                    let _ = app.emit(
                        "transcript",
                        json!({ "role": role, "speaker": speaker, "text": t, "final": true }),
                    );
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = app.emit("listen-error", e.to_string());
                }
            }
        }

        if !had_audio {
            empty_cycles += 1;
            if empty_cycles == 2 {
                status(
                    app,
                    "No audio captured yet - make sure sound is playing through your speakers (or that mic access is granted).",
                );
            }
            continue;
        }
        empty_cycles = 0;
        if !saw_speech {
            status(app, "Listening (only silence so far)...");
        }
    }
}

/// Interleaved frames -> mono.
fn downmix_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    let ch = channels.max(1);
    samples
        .chunks(ch)
        .map(|f| f.iter().copied().sum::<f32>() / f.len() as f32)
        .collect()
}

/// Mono f32 [-1,1] -> signed 16-bit little-endian PCM bytes.
fn mono_to_linear16(mono: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(mono.len() * 2);
    for &s in mono {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

/// Linear-interpolation resample of mono audio to `dst_rate`. Good enough for speech STT.
fn resample(mono: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if src_rate == dst_rate || mono.is_empty() {
        return mono.to_vec();
    }
    let ratio = dst_rate as f32 / src_rate as f32;
    let out_len = (mono.len() as f32 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f32 / ratio;
        let idx = pos as usize;
        let frac = pos - idx as f32;
        let a = mono.get(idx).copied().unwrap_or(0.0);
        let b = mono.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}
