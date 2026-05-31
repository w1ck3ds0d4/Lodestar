//! System-audio (loopback) capture + transcription for the live assistant.
//!
//! Captures the default output device via WASAPI loopback (cpal auto-enables
//! loopback when an input stream is built on an output device). Audio is buffered
//! into ~4s chunks and turned into text by the selected engine - Deepgram (cloud)
//! or a local Whisper model (Candle, on-device). Each transcript is emitted to the
//! frontend as a `transcript` event; progress/errors as `listen-status`/`listen-error`.
//!
//! Chunked (a few seconds of latency), not word-by-word streaming. Windows-first:
//! loopback is native here.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::local_stt::LocalStt;

const CHUNK_SECS: u64 = 4;
const SILENCE_PEAK: f32 = 0.005;
const WHISPER_RATE: u32 = 16000;

/// Which transcription backend to use for a listening session.
pub enum EngineConfig {
    Deepgram { key: String },
    Local { models_dir: PathBuf },
}

fn on_stream_error(err: cpal::StreamError) {
    tracing::warn!("audio capture stream error: {err}");
}

fn status(app: &AppHandle, msg: &str) {
    let _ = app.emit("listen-status", msg.to_string());
}

/// Capture loopback audio and emit `transcript` events until `flag` is cleared.
pub fn run(
    app: &AppHandle,
    http: reqwest::Client,
    engine: EngineConfig,
    flag: Arc<AtomicBool>,
) -> Result<()> {
    // The local engine downloads + loads the model up front (slow on first run).
    let mut local: Option<LocalStt> = None;
    if let EngineConfig::Local { models_dir } = &engine {
        local = Some(LocalStt::load(&http, models_dir, &|m| status(app, m))?);
    }
    status(app, "Listening");

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No audio output device found to capture."))?;
    let supported = device
        .default_output_config()
        .map_err(|e| anyhow!("Could not read the output device format: {e}"))?;
    let sample_rate = supported.sample_rate().0;
    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels.max(1) as usize;

    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let buf = buffer.clone();
            device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    buf.lock().unwrap().extend_from_slice(data);
                },
                on_stream_error,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let buf = buffer.clone();
            device.build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    buf.lock()
                        .unwrap()
                        .extend(data.iter().map(|&s| s as f32 / 32768.0));
                },
                on_stream_error,
                None,
            )
        }
        other => return Err(anyhow!("Unsupported audio sample format: {other:?}")),
    }
    .map_err(|e| anyhow!("Could not open loopback capture: {e}"))?;

    stream
        .play()
        .map_err(|e| anyhow!("Could not start audio capture: {e}"))?;

    while flag.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_secs(CHUNK_SECS));
        let samples = {
            let mut g = buffer.lock().unwrap();
            std::mem::take(&mut *g)
        };
        if samples.is_empty() {
            continue;
        }
        let mono = downmix_mono(&samples, channels);
        // Skip near-silent chunks so dead air doesn't spend requests / CPU.
        let peak = mono.iter().fold(0f32, |m, &s| m.max(s.abs()));
        if peak < SILENCE_PEAK {
            continue;
        }

        let text = match &engine {
            EngineConfig::Deepgram { key } => {
                let pcm = mono_to_linear16(&mono);
                tauri::async_runtime::block_on(transcribe_deepgram(&http, key, sample_rate, pcm))
            }
            EngineConfig::Local { .. } => {
                let pcm16 = resample(&mono, sample_rate, WHISPER_RATE);
                local
                    .as_mut()
                    .expect("local engine initialized above")
                    .transcribe(&pcm16)
            }
        };
        match text {
            Ok(t) if !t.trim().is_empty() => {
                let _ = app.emit("transcript", json!({ "text": t }));
            }
            Ok(_) => {}
            Err(e) => {
                let _ = app.emit("listen-error", e.to_string());
            }
        }
    }

    drop(stream);
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

/// POST a raw linear16 chunk to Deepgram's pre-recorded endpoint; returns the
/// transcript text (empty when nothing was recognised).
async fn transcribe_deepgram(
    http: &reqwest::Client,
    key: &str,
    sample_rate: u32,
    pcm: Vec<u8>,
) -> Result<String> {
    let url = format!(
        "https://api.deepgram.com/v1/listen?model=nova-2&encoding=linear16&sample_rate={sample_rate}&channels=1&punctuate=true"
    );
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

    Ok(
        payload["results"]["channels"][0]["alternatives"][0]["transcript"]
            .as_str()
            .unwrap_or("")
            .to_string(),
    )
}
