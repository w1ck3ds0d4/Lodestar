//! Local, on-device speech-to-text using a Whisper model via Candle (pure Rust).
//!
//! No external tools and no whisper.cpp toolchain: the model runs in-process on
//! the CPU. Weights are downloaded once (via the shared reqwest client) into the
//! app-data dir and cached. This is the local alternative to the Deepgram cloud
//! engine - audio never leaves the machine.
//!
//! NOTE: this was written without the ability to run Whisper inference in the
//! build environment, so the mel-filter computation and the decode loop in
//! particular need a real smoke test (and may need tuning).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{self as whisper, model::Whisper, Config};
use tokenizers::Tokenizer;

/// Multilingual base model: a reasonable accuracy/speed balance on CPU. Swap for
/// "openai/whisper-tiny" for more speed and a smaller download, at some accuracy cost.
const MODEL_ID: &str = "openai/whisper-base";
const MODEL_DIRNAME: &str = "whisper-base";
const TARGET_RATE: usize = 16000;
const WINDOW_SECS: usize = 30; // Whisper's fixed input window
const MAX_DECODE_TOKENS: usize = 224;

pub struct LocalStt {
    model: Whisper,
    tokenizer: Tokenizer,
    mel_filters: Vec<f32>,
    device: Device,
    sot: u32,
    transcribe: u32,
    no_timestamps: u32,
    eot: u32,
    lang_tokens: Vec<u32>,
    default_lang: u32,
}

impl LocalStt {
    /// Download (if needed) and load the Whisper model. `models_dir` is where the
    /// weights are cached; `status` reports coarse progress to the UI.
    pub fn load(http: &reqwest::Client, models_dir: &Path, status: &dyn Fn(&str)) -> Result<Self> {
        let dir = models_dir.join(MODEL_DIRNAME);
        std::fs::create_dir_all(&dir)?;
        let config_path = fetch(http, &dir, "config.json", status)?;
        let tokenizer_path = fetch(http, &dir, "tokenizer.json", status)?;
        let weights_path = fetch(http, &dir, "model.safetensors", status)?;

        status("Loading model...");
        let device = Device::Cpu;
        let config: Config = serde_json::from_str(&std::fs::read_to_string(config_path)?)?;
        let tokenizer =
            Tokenizer::from_file(&tokenizer_path).map_err(|e| anyhow!("Bad tokenizer: {e}"))?;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], whisper::DTYPE, &device)?
        };
        let model = Whisper::load(&vb, config)?;

        let tok = |s: &str| -> Result<u32> {
            tokenizer
                .token_to_id(s)
                .ok_or_else(|| anyhow!("tokenizer is missing the {s} token"))
        };
        // Language tokens look like "<|en|>", "<|es|>", "<|yue|>" - a 2-3 letter
        // lowercase code. Collected so the spoken language can be auto-detected.
        let lang_tokens: Vec<u32> = tokenizer
            .get_vocab(true)
            .into_iter()
            .filter(|(t, _)| {
                t.starts_with("<|") && t.ends_with("|>") && {
                    let inner = &t[2..t.len() - 2];
                    (inner.len() == 2 || inner.len() == 3)
                        && inner.chars().all(|c| c.is_ascii_lowercase())
                }
            })
            .map(|(_, id)| id)
            .collect();
        Ok(Self {
            sot: tok("<|startoftranscript|>")?,
            transcribe: tok("<|transcribe|>")?,
            no_timestamps: tok("<|notimestamps|>")?,
            eot: tok("<|endoftext|>")?,
            default_lang: tok("<|en|>")?,
            lang_tokens,
            mel_filters: mel_filters(model.config.num_mel_bins),
            model,
            tokenizer,
            device,
        })
    }

    /// Transcribe mono 16 kHz PCM of any length. Audio is split into Whisper's
    /// fixed 30 s windows (longer input would overrun the encoder's positional
    /// table and panic with a `narrow` error), each window is decoded, and the
    /// pieces are joined.
    pub fn transcribe(&mut self, pcm16k: &[f32]) -> Result<String> {
        let window = TARGET_RATE * WINDOW_SECS;
        if pcm16k.is_empty() {
            return Ok(String::new());
        }
        let mut out = String::new();
        for chunk in pcm16k.chunks(window) {
            let seg = self.transcribe_window(chunk)?;
            let seg = seg.trim();
            if !seg.is_empty() {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(seg);
            }
        }
        Ok(out)
    }

    /// Transcribe a single window (<= 30 s): pad to the fixed window, clamp the mel
    /// to the encoder's frame limit, auto-detect the language, and greedily decode.
    fn transcribe_window(&mut self, pcm: &[f32]) -> Result<String> {
        let window = TARGET_RATE * WINDOW_SECS;
        let mut padded = pcm.to_vec();
        padded.truncate(window);
        padded.resize(window, 0.0);

        let n_mels = self.model.config.num_mel_bins;
        let mut mel = whisper::audio::pcm_to_mel(&self.model.config, &padded, &self.mel_filters);
        // Guard against any off-by-one from the mel computation: the encoder's
        // positional embedding only spans whisper::N_FRAMES mel frames.
        mel.truncate(n_mels * whisper::N_FRAMES);
        let frames = mel.len() / n_mels;
        let mel = Tensor::from_vec(mel, (1, n_mels, frames), &self.device)?;

        self.model.reset_kv_cache();
        let audio_features = self.model.encoder.forward(&mel, true)?;

        let language = self.detect_language(&audio_features)?;
        let mut tokens = vec![self.sot, language, self.transcribe, self.no_timestamps];
        let prompt_len = tokens.len();
        for _ in 0..MAX_DECODE_TOKENS {
            let tokens_t = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let ys = self
                .model
                .decoder
                .forward(&tokens_t, &audio_features, true)?;
            let (_, seq_len, _) = ys.dims3()?;
            let last = ys.i((.., seq_len - 1..seq_len, ..))?;
            let logits = self.model.decoder.final_linear(&last)?.i((0, 0))?;
            let next = logits.argmax(0)?.to_scalar::<u32>()?;
            if next == self.eot {
                break;
            }
            tokens.push(next);
        }

        let text = self
            .tokenizer
            .decode(&tokens[prompt_len..], true)
            .map_err(|e| anyhow!("decode error: {e}"))?;
        Ok(text.trim().to_string())
    }

    /// Auto-detect the spoken language: right after `<|startoftranscript|>` Whisper
    /// predicts a language token, so pick the highest-scoring one. Falls back to English.
    fn detect_language(&mut self, audio_features: &Tensor) -> Result<u32> {
        if self.lang_tokens.is_empty() {
            return Ok(self.default_lang);
        }
        let sot_t = Tensor::new(&[self.sot], &self.device)?.unsqueeze(0)?;
        let ys = self.model.decoder.forward(&sot_t, audio_features, true)?;
        let logits: Vec<f32> = self
            .model
            .decoder
            .final_linear(&ys.i((.., 0..1, ..))?)?
            .i((0, 0))?
            .to_vec1()?;
        let best = self
            .lang_tokens
            .iter()
            .copied()
            .max_by(|&a, &b| logits[a as usize].total_cmp(&logits[b as usize]))
            .unwrap_or(self.default_lang);
        Ok(best)
    }
}

/// Download `name` from the model repo into `dir` if it isn't cached yet.
fn fetch(http: &reqwest::Client, dir: &Path, name: &str, status: &dyn Fn(&str)) -> Result<PathBuf> {
    let dest = dir.join(name);
    if dest.exists() {
        return Ok(dest);
    }
    status(&format!("Downloading {name} (first run only)..."));
    let url = format!("https://huggingface.co/{MODEL_ID}/resolve/main/{name}");
    let bytes = tauri::async_runtime::block_on(async {
        http.get(&url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await
    })
    .map_err(|e| anyhow!("Could not download {name}: {e}"))?;
    std::fs::write(&dest, &bytes)?;
    Ok(dest)
}

/// librosa-compatible mel filterbank (htk=False, slaney norm), shaped
/// n_mels x (1 + N_FFT/2) row-major - matching what candle's pcm_to_mel expects.
fn mel_filters(n_mels: usize) -> Vec<f32> {
    const SR: f32 = 16000.0;
    const N_FFT: usize = 400;
    let n_freqs = 1 + N_FFT / 2;
    let f_sp = 200.0f32 / 3.0;
    let min_log_hz = 1000.0f32;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4f32).ln() / 27.0;

    let hz_to_mel = |f: f32| {
        if f >= min_log_hz {
            min_log_mel + (f / min_log_hz).ln() / logstep
        } else {
            f / f_sp
        }
    };
    let mel_to_hz = |mel: f32| {
        if mel >= min_log_mel {
            min_log_hz * ((mel - min_log_mel) * logstep).exp()
        } else {
            f_sp * mel
        }
    };

    let mel_min = hz_to_mel(0.0);
    let mel_max = hz_to_mel(SR / 2.0);
    let mel_pts: Vec<f32> = (0..n_mels + 2)
        .map(|i| mel_to_hz(mel_min + (mel_max - mel_min) * i as f32 / (n_mels as f32 + 1.0)))
        .collect();
    let fft_freqs: Vec<f32> = (0..n_freqs).map(|k| k as f32 * SR / N_FFT as f32).collect();

    let mut filters = vec![0.0f32; n_mels * n_freqs];
    for j in 0..n_mels {
        let (left, center, right) = (mel_pts[j], mel_pts[j + 1], mel_pts[j + 2]);
        let enorm = 2.0 / (right - left);
        for (k, &freq) in fft_freqs.iter().enumerate() {
            let lower = (freq - left) / (center - left);
            let upper = (right - freq) / (right - center);
            filters[j * n_freqs + k] = lower.min(upper).max(0.0) * enorm;
        }
    }
    filters
}
