//! Thin wrapper over `whisper-rs` (whisper.cpp + Metal). Loads a ggml model
//! once and transcribes 16 kHz mono f32 PCM into the unified [`Segment`] type.
//! The heavy `WhisperContext` is meant to be created once and kept resident on
//! the transcription worker thread (see the engine in Phase 2).

use crate::transcript::Segment;
use anyhow::{Context, Result};
use std::path::Path;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadParams,
};

pub struct Whisper {
    ctx: WhisperContext,
}

impl Whisper {
    /// Load a ggml model file (e.g. `ggml-large-v3-turbo.bin`). GPU/Metal is
    /// enabled implicitly by the crate's `metal` feature.
    pub fn load(model: &Path) -> Result<Self> {
        let path = model.to_str().context("model path is not valid UTF-8")?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .context("load whisper model")?;
        Ok(Self { ctx })
    }

    /// Transcribe mono 16 kHz f32 PCM. `lang` is an ISO-639-1 code or `None`
    /// for auto-detect. `on_progress` receives 0..=100 during inference.
    ///
    /// `vad_model` is an optional path to a Silero VAD ggml model
    /// (`ggml-silero-v6.2.0.bin`). When provided, whisper.cpp runs a voice-
    /// activity pre-pass that drops silence / non-speech regions — large-v3 was
    /// trained on VAD-filtered audio, so this keeps the decoder in-distribution
    /// and suppresses the confident repetition loops raw static can trigger.
    pub fn transcribe<F>(
        &self,
        pcm: &[f32],
        lang: Option<&str>,
        vad_model: Option<&str>,
        on_progress: F,
    ) -> Result<Vec<Segment>>
    where
        F: FnMut(i32) + 'static,
    {
        let mut state = self.ctx.create_state().context("create whisper state")?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(lang.unwrap_or("auto")));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_progress_callback_safe(on_progress);

        // Silero VAD pre-pass when the bundled model is available. Defaults
        // already match the reference config (threshold 0.5, min-speech 250 ms,
        // min-silence 100 ms, pad 30 ms, overlap 0.1). Set the path before
        // enabling — `enable_vad` panics if the model path is null.
        if let Some(vad) = vad_model {
            params.set_vad_model_path(Some(vad));
            params.set_vad_params(WhisperVadParams::new());
            params.enable_vad(true);
        }

        state.full(params, pcm).context("whisper inference")?;

        let n = state.full_n_segments();
        let mut out = Vec::with_capacity(n.max(0) as usize);
        for i in 0..n {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };
            let text = seg
                .to_str_lossy()
                .map(|c| c.trim().to_string())
                .unwrap_or_default();
            if text.is_empty() {
                continue;
            }
            // whisper timestamps are in centiseconds (10 ms units)
            out.push(Segment {
                start_ms: seg.start_timestamp().max(0) as u64 * 10,
                end_ms: seg.end_timestamp().max(0) as u64 * 10,
                text,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode_to_mono_16k;
    use std::path::PathBuf;
    use whisper_rs::{WhisperVadContext, WhisperVadContextParams};

    /// Guards that the bundled Silero VAD model loads into this whisper-rs /
    /// whisper.cpp build and actually runs — i.e. the model version and the
    /// linked library agree. Uses only the 864 KB model committed in the repo
    /// (no large model, no audio fixture), so it runs in CI on macOS and Windows.
    #[test]
    fn bundled_silero_vad_loads_and_runs() {
        let model = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources/models/ggml-silero-v6.2.0.bin");
        assert!(model.exists(), "bundled VAD model missing: {}", model.display());

        let mut params = WhisperVadContextParams::new();
        params.set_use_gpu(false); // CPU — no Metal on the Windows CI runner
        let mut vad = WhisperVadContext::new(model.to_str().unwrap(), params)
            .expect("load bundled silero vad model");

        // ~1 s of 16 kHz mono: half silence, half a 220 Hz tone.
        let sr = 16_000usize;
        let mut pcm = vec![0.0f32; sr / 2];
        for i in 0..sr / 2 {
            let t = i as f32 / sr as f32;
            pcm.push((2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.3);
        }
        vad.detect_speech(&pcm).expect("vad detect_speech runs");
        assert!(!vad.probabilities().is_empty(), "vad produced no probabilities");
    }

    /// Real end-to-end inference on Metal. Ignored by default (needs a model +
    /// audio fixture). Run with:
    ///   WHISPER_TEST_MODEL=/tmp/ggml-tiny.en.bin \
    ///   WHISPER_TEST_WAV=/tmp/jfk.wav \
    ///   cargo test --lib whisper -- --ignored --nocapture
    #[test]
    #[ignore]
    fn transcribes_real_audio_on_metal() {
        let model = PathBuf::from(std::env::var("WHISPER_TEST_MODEL").expect("WHISPER_TEST_MODEL"));
        let wav = PathBuf::from(std::env::var("WHISPER_TEST_WAV").expect("WHISPER_TEST_WAV"));

        let (pcm, dur) = decode_to_mono_16k(&wav).expect("decode fixture");
        assert!(dur > 0.0);

        let w = Whisper::load(&model).expect("load model");
        let segs = w
            .transcribe(&pcm, Some("en"), None, |p| eprintln!("progress {p}%"))
            .expect("transcribe");

        let text = segs
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        eprintln!("--- transcript ---\n{text}\n------------------");
        assert!(!segs.is_empty(), "no segments produced");
        // jfk.wav: "...ask not what your country can do for you..."
        assert!(text.contains("country"), "expected 'country' in: {text}");
    }
}
