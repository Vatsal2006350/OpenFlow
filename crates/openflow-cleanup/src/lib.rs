//! HTTP and local-inference adapters for cleanup providers. Jac assembles cloud
//! request payloads and interprets provider response envelopes. On any failure
//! the caller falls back to local cleanup or the raw transcript.

use std::time::Duration;

pub mod publik;
pub use publik::{BalanceSnapshot, Provisioned, PublikError, PublikProvider, Wallet};

/// Default OpenAI Chat Completions endpoint.
const OPENAI_DEFAULT_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Cleanup via the OpenAI Chat Completions API — or any OpenAI-compatible
/// endpoint (OpenRouter, a local server, etc.) when `base_url` is set.
/// OpenRouter in particular speaks this exact wire format at
/// `https://openrouter.ai/api/v1/chat/completions`.
pub struct OpenAiProvider {
    client: reqwest::blocking::Client,
    api_key: String,
    /// Full chat-completions URL. Defaults to OpenAI's when empty.
    url: String,
}

impl OpenAiProvider {
    pub fn new(api_key: String) -> Self {
        Self::with_base_url(api_key, None)
    }

    /// `base_url` is the API root (e.g. `https://openrouter.ai/api/v1`), without
    /// the `/chat/completions` suffix. `None` or empty uses OpenAI directly.
    pub fn with_base_url(
        api_key: String,
        base_url: Option<String>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        let url = match base_url.map(|s| s.trim().trim_end_matches('/').to_string()) {
            Some(base) if !base.is_empty() => format!("{base}/chat/completions"),
            _ => OPENAI_DEFAULT_URL.to_string(),
        };
        Self {
            client,
            api_key,
            url,
        }
    }
}

impl OpenAiProvider {
    /// Send a request body assembled by the Jac cleanup policy.
    pub fn cleanup_with_request_body(
        &self,
        body: &serde_json::Value,
    ) -> anyhow::Result<String> {
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()?;

        // Return provider facts to the Jac client. It owns interpretation of
        // status and response fields; this adapter only performs HTTP I/O.
        let status = resp.status().as_u16();
        let response_body = resp.text()?;
        Ok(serde_json::json!({ "status": status, "body": response_body }).to_string())
    }
}

/// Cleanup via the Anthropic Messages API. Same shared system prompt; the only
/// difference from OpenAI is the wire envelope (top-level `system`, `x-api-key`).
pub struct AnthropicProvider {
    client: reqwest::blocking::Client,
    api_key: String,
}

impl AnthropicProvider {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            api_key,
        }
    }
}

impl AnthropicProvider {
    /// Send a request body assembled by the Jac cleanup policy.
    pub fn cleanup_with_request_body(
        &self,
        body: &serde_json::Value,
    ) -> anyhow::Result<String> {
        let resp = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()?;

        // Return provider facts to the Jac client. It owns interpretation of
        // status and response fields; this adapter only performs HTTP I/O.
        let status = resp.status().as_u16();
        let response_body = resp.text()?;
        Ok(serde_json::json!({ "status": status, "body": response_body }).to_string())
    }
}

/// Default OpenAI transcription endpoint.
const OPENAI_ASR_DEFAULT_URL: &str = "https://api.openai.com/v1/audio/transcriptions";

/// Speech-to-text via any OpenAI-compatible `/audio/transcriptions` API — OpenAI
/// itself, or Groq's Whisper endpoint (`https://api.groq.com/openai/v1`), which
/// speaks the identical multipart wire format. Same `base_url` convention as
/// [`OpenAiProvider`], and reuses the same API key.
pub struct CloudAsr {
    client: reqwest::blocking::Client,
    api_key: String,
    model: String,
    /// Full transcriptions URL. Defaults to OpenAI's when empty.
    url: String,
}

impl CloudAsr {
    /// `base_url` is the API root (e.g. `https://api.groq.com/openai/v1`), without
    /// the `/audio/transcriptions` suffix. `None` or empty uses OpenAI directly.
    pub fn with_base_url(
        api_key: String,
        model: impl Into<String>,
        base_url: Option<String>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        let url = match base_url.map(|s| s.trim().trim_end_matches('/').to_string()) {
            Some(base) if !base.is_empty() => format!("{base}/audio/transcriptions"),
            _ => OPENAI_ASR_DEFAULT_URL.to_string(),
        };
        Self {
            client,
            api_key,
            model: model.into(),
            url,
        }
    }
}

impl openflow_core::AsrEngine for CloudAsr {
    fn id(&self) -> openflow_core::AsrEngineId {
        openflow_core::AsrEngineId::CloudWhisper
    }

    fn transcribe(&self, pcm16k: &[f32]) -> anyhow::Result<openflow_core::asr::Transcript> {
        let wav = encode_wav_16k_mono(pcm16k)?;
        let part = reqwest::blocking::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;
        let form = reqwest::blocking::multipart::Form::new()
            .part("file", part)
            .text("model", self.model.clone())
            .text("response_format", "json");

        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()?;

        // Jac owns provider response interpretation. Return only transport
        // facts so the platform caller can pass this envelope through its Jac
        // bridge before any text is cleaned, logged, or pasted.
        let status = resp.status().as_u16();
        let response_body = resp.text()?;
        let text = serde_json::json!({ "status": status, "body": response_body }).to_string();
        Ok(openflow_core::asr::Transcript {
            text,
            confidence: None,
        })
    }
}

/// Encode 16 kHz mono f32 samples as a 16-bit PCM WAV in memory, for upload to a
/// transcription API that expects an audio file rather than raw samples.
fn encode_wav_16k_mono(pcm: &[f32]) -> anyhow::Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut buf, spec)?;
        for &s in pcm {
            writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        writer.finalize()?;
    }
    Ok(buf.into_inner())
}
