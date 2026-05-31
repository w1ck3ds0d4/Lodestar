//! System-audio (loopback) and/or microphone capture + transcription for the live
//! assistant.
//!
//! Captures the default output device via WASAPI loopback (the other side of a call
//! or a video) and/or the default input device (your mic), per the chosen source.
//! Each source is downmixed to mono, resampled to 16 kHz, and transcribed
//! independently (no mixing) so every `transcript` event carries a speaker role: the
//! microphone is labeled "you"; system/loopback audio is labeled "them" (local
//! engine) or diarized into "Speaker 1/2/..." by Deepgram. Audio is buffered into ~4s
//! chunks and turned into text by the selected engine - Deepgram (cloud) or a local
//! Whisper model (Candle, on-device). Progress/errors are emitted as
//! `listen-status`/`listen-error`; capture diagnostics as `listen-device`/`listen-level`.
//!
//! KNOWN LIMITATION: Deepgram speaker numbering is only stable within a single
//! request, and each chunk is a separate pre-recorded call - so "Speaker 1" in one
//! chunk is not guaranteed to be the same person in the next (true cross-chunk
//! diarization needs the streaming WebSocket API, out of scope here).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SupportedStreamConfig};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::local_stt::LocalStt;

const CHUNK_SECS: u64 = 4;
const SILENCE_PEAK: f32 = 0.005;
const TARGET_RATE: u32 = 16000;

/// Which transcription backend to use for a listening session.
pub enum EngineConfig {
    Deepgram { key: String },
    Local { models_dir: PathBuf },
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

    // Peek the buffers ~5x/sec for a live input-level meter, and drain + transcribe
    // once every CHUNK_SECS. A flat meter at 0 means no audio is reaching the chosen
    // device (wrong output for loopback); a moving meter with no transcript points at
    // the STT engine instead.
    let tick = Duration::from_millis(200);
    let ticks_per_chunk = (CHUNK_SECS * 1000 / 200).max(1);
    let mut tick_count = 0u64;
    let mut empty_cycles = 0u32;
    while flag.load(Ordering::SeqCst) {
        std::thread::sleep(tick);

        let mut live_peak = 0f32;
        let mut buffered = 0usize;
        for src in &sources {
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

        // Transcribe each source SEPARATELY (no mixing) so every transcript is
        // attributed: the mic is "you"; system/loopback is "them" (or diarized into
        // "Speaker N" by Deepgram). Speaker numbering is only stable within a single
        // ~4s request, not across chunks (true cross-chunk diarization needs the
        // streaming API, out of scope).
        let mut had_audio = false;
        let mut saw_speech = false;
        for src in &sources {
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
            // Skip near-silent audio so dead air doesn't spend requests / CPU.
            let peak = resampled.iter().fold(0f32, |m, &s| m.max(s.abs()));
            if peak < SILENCE_PEAK {
                continue;
            }
            saw_speech = true;
            status(app, "Transcribing...");

            let result: Result<Vec<(String, String)>> = match &engine {
                EngineConfig::Deepgram { key } => {
                    let pcm = mono_to_linear16(&resampled);
                    tauri::async_runtime::block_on(transcribe_deepgram(
                        &http,
                        key,
                        TARGET_RATE,
                        pcm,
                        src.is_system(),
                    ))
                }
                EngineConfig::Local { .. } => local
                    .as_mut()
                    .expect("local engine initialized above")
                    .transcribe(&resampled)
                    .map(|t| vec![(String::new(), t)]),
            };
            match result {
                Ok(segs) => {
                    for (sp, text) in segs {
                        if text.trim().is_empty() {
                            continue;
                        }
                        let (role, speaker) = if src.is_system() {
                            (
                                "them",
                                if sp.is_empty() {
                                    "Them".to_string()
                                } else {
                                    sp
                                },
                            )
                        } else {
                            ("you", "You".to_string())
                        };
                        let _ = app.emit(
                            "transcript",
                            json!({ "role": role, "speaker": speaker, "text": text }),
                        );
                    }
                }
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

    drop(streams);
    Ok(())
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

/// POST a raw linear16 chunk to Deepgram's pre-recorded endpoint. Returns labeled
/// utterances as (speaker, text). When `diarize` is set, the call audio is split by
/// speaker into "Speaker 1/2/..."; otherwise a single utterance with an empty speaker
/// label is returned (the caller assigns the role).
async fn transcribe_deepgram(
    http: &reqwest::Client,
    key: &str,
    sample_rate: u32,
    pcm: Vec<u8>,
    diarize: bool,
) -> Result<Vec<(String, String)>> {
    let mut url = format!(
        "https://api.deepgram.com/v1/listen?model=nova-2&encoding=linear16&sample_rate={sample_rate}&channels=1&punctuate=true&detect_language=true"
    );
    if diarize {
        url.push_str("&diarize=true");
    }
    let resp = http
        .post(url)
        .header("Authorization", format!("Token {key}"))
        .header("Content-Type", "audio/raw")
        .body(pcm)
        .send()
        .await
        .map_err(|e| anyhow!("Could not reach Deepgram: {e}"))?;

    let status = resp.status();
    let payload: Value = resp
        .json()
        .await
        .map_err(|e| anyhow!("Unexpected response from Deepgram: {e}"))?;

    if !status.is_success() {
        let detail = payload["err_msg"]
            .as_str()
            .or_else(|| payload["error"].as_str())
            .unwrap_or("");
        let code = status.as_u16();
        return Err(match code {
            401 => anyhow!("Invalid Deepgram API key. Check it in the Assistant tab."),
            _ if !detail.is_empty() => anyhow!("Deepgram error ({code}): {detail}"),
            _ => anyhow!("Deepgram error ({code})."),
        });
    }

    let alt = &payload["results"]["channels"][0]["alternatives"][0];
    if diarize {
        // Group consecutive words by their speaker index into utterances.
        if let Some(words) = alt["words"].as_array().filter(|w| !w.is_empty()) {
            let mut out: Vec<(String, String)> = Vec::new();
            let mut cur: Option<i64> = None;
            let mut buf = String::new();
            let flush = |out: &mut Vec<(String, String)>, spk: i64, buf: &str| {
                if !buf.trim().is_empty() {
                    out.push((format!("Speaker {}", spk + 1), buf.trim().to_string()));
                }
            };
            for w in words {
                let spk = w["speaker"].as_i64().unwrap_or(0);
                let word = w["punctuated_word"]
                    .as_str()
                    .or_else(|| w["word"].as_str())
                    .unwrap_or("");
                if cur != Some(spk) {
                    if let Some(prev) = cur {
                        flush(&mut out, prev, &buf);
                    }
                    cur = Some(spk);
                    buf.clear();
                }
                buf.push_str(word);
                buf.push(' ');
            }
            if let Some(prev) = cur {
                flush(&mut out, prev, &buf);
            }
            if !out.is_empty() {
                return Ok(out);
            }
        }
    }

    // No diarization (mic side), or diarize returned no words: flat transcript.
    let t = alt["transcript"].as_str().unwrap_or("").to_string();
    if t.trim().is_empty() {
        Ok(vec![])
    } else {
        Ok(vec![(
            if diarize {
                "Speaker 1".to_string()
            } else {
                String::new()
            },
            t,
        )])
    }
}
