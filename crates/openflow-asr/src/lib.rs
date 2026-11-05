//! Local speech-to-text via whisper.cpp (whisper-rs), implementing
//! [`openflow_core::AsrEngine`]. Expects 16 kHz mono f32 samples.

use std::path::Path;

use openflow_core::asr::{AsrCaps, AsrEngine, AsrEngineId, Transcript};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// A loaded whisper model ready to transcribe utterances.
pub struct WhisperEngine {
    ctx: WhisperContext,
}

impl WhisperEngine {
    /// Load a GGML/GGUF whisper model from `model_path`.
    pub fn load(model_path: &Path) -> anyhow::Result<Self> {
        let path = model_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("model path is not valid UTF-8"))?;
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(|e| anyhow::anyhow!("failed to load whisper model: {e}"))?;
        Ok(Self { ctx })
    }

    /// Transcribe a longer local recording using whisper.cpp's normal chunking.
    pub fn transcribe_long(&self, pcm16k: &[f32]) -> anyhow::Result<Transcript> {
        self.transcribe_configured(pcm16k, false)
    }

    fn transcribe_configured(&self, pcm16k: &[f32], single_segment: bool) -> anyhow::Result<Transcript> {
        let mut state = self.ctx.create_state().map_err(|e| anyhow::anyhow!("whisper create_state: {e}"))?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        // Meeting rooms contain music and room noise; do not turn non-speech
        // tokens into stage directions such as "[sad music]" in the transcript.
        params.set_suppress_nst(true);
        params.set_single_segment(single_segment);
        params.set_no_context(true);
        state.full(params, pcm16k).map_err(|e| anyhow::anyhow!("whisper full: {e}"))?;
        let mut text = String::new();
        for i in 0..state.full_n_segments() {
            if let Some(seg) = state.get_segment(i) {
                if seg.no_speech_probability() < 0.6 {
                    text.push_str(&seg.to_string());
                }
            }
        }
        Ok(Transcript { text: text.trim().to_string(), confidence: None })
    }
}

impl AsrEngine for WhisperEngine {
    fn id(&self) -> AsrEngineId {
        AsrEngineId::WhisperCpp
    }

    fn caps(&self) -> AsrCaps {
        AsrCaps {
            supports_streaming: false,
        }
    }

    fn transcribe(&self, pcm16k: &[f32]) -> anyhow::Result<Transcript> {
        // Push-to-talk utterances are always one short clip, not long-form audio.
        // Without this, whisper.cpp can split it into multiple internal segments
        // that repeat the same words — which then get concatenated below,
        // producing the sentence twice. Single-segment mode avoids that.
        self.transcribe_configured(pcm16k, true)
    }
}
