//! System-audio (loopback) capture + cloud transcription for the live assistant.
//!
//! Captures whatever is playing through the default output device (the call or
//! lecture audio) via WASAPI loopback. cpal enables loopback automatically when
//! an input stream is built on an output device, so no whisper.cpp toolchain is
//! needed. Audio is buffered into short chunks, each chunk is sent to Deepgram's
//! pre-recorded endpoint, and the resulting transcript is emitted to the frontend
//! as a `transcript` event.
//!
//! This is chunked (a few seconds of latency), not word-by-word streaming; a
//! streaming WebSocket path is a planned follow-up. Windows-first: loopback is
//! native here. The captured audio is uploaded to Deepgram for transcription.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// Seconds of audio to accumulate before each transcription request.
const CHUNK_SECS: u64 = 4;
/// Peak amplitude below which a chunk is treated as silence and skipped.
const SILENCE_PEAK: f32 = 0.005;

fn on_stream_error(err: cpal::StreamError) {
    tracing::warn!("audio capture stream error: {err}");
}

/// Capture loopback audio and emit `transcript` events until `flag` is cleared.
pub fn run(app: &AppHandle, http: reqwest::Client, key: &str, flag: Arc<AtomicBool>) -> Result<()> {
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

    // The cpal callback runs on its own audio thread and appends interleaved
    // samples (as f32) here; the loop below drains it every CHUNK_SECS.
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
                    let mut g = buf.lock().unwrap();
                    g.extend(data.iter().map(|&s| s as f32 / 32768.0));
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
        // Skip near-silent chunks so dead air doesn't burn transcription requests.
        let peak = samples.iter().fold(0f32, |m, &s| m.max(s.abs()));
        if peak < SILENCE_PEAK {
            continue;
        }
        let pcm = downmix_to_linear16(&samples, channels);
        match tauri::async_runtime::block_on(transcribe(&http, key, sample_rate, pcm)) {
            Ok(text) if !text.trim().is_empty() => {
                let _ = app.emit("transcript", json!({ "text": text }));
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

/// Interleaved f32 frames -> mono signed 16-bit little-endian PCM bytes.
fn downmix_to_linear16(samples: &[f32], channels: usize) -> Vec<u8> {
    let ch = channels.max(1);
    let mut out = Vec::with_capacity(samples.len() / ch * 2);
    for frame in samples.chunks(ch) {
        let avg = frame.iter().copied().sum::<f32>() / frame.len() as f32;
        let s = (avg.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// POST a raw linear16 chunk to Deepgram's pre-recorded endpoint; returns the
/// transcript text (empty string when nothing was recognised).
async fn transcribe(
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
