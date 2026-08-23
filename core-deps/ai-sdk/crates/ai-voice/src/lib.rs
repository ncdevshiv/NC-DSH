//! Voice (PRD §3.5): audio types, an energy-based VAD with adaptive noise
//! floor, WAV parsing/writing, configurable STT/TTS adapters, and a full-
//! duplex realtime voice session ([`DuplexSession`]) with barge-in over the
//! [`ai_protocols`] WebSocket transport.
//!
//! Layering note: `ai-voice` depends on `ai-protocols` because the duplex
//! session orchestrates protocol-level realtime events (`RealtimeConnection`,
//! `RealtimeClientEvent`) with voice-domain primitives (VAD, PCM audio).
//! No other crate depends on either, so the direction stays acyclic.

mod session;
mod vad;
mod wav;

pub use session::{BargeIn, DuplexSession, DuplexSessionConfig, JitterBuffer, SessionNotification};
pub use vad::{VadConfig, VadDecision, VoiceActivityDetector};
pub use wav::{parse_wav, wav_from_pcm, wav_from_pcm_stereo};

use async_trait::async_trait;

use ai_errors::{AiError, WebError};

/// PCM audio (16-bit little-endian, mono).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Audio {
    /// Raw PCM samples.
    pub samples: Vec<i16>,
    /// Sample rate in Hz (e.g. 16_000).
    pub sample_rate: u32,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

impl Audio {
    pub fn from_samples(samples: Vec<i16>, sample_rate: u32) -> Self {
        let duration_ms = if sample_rate == 0 {
            0
        } else {
            (samples.len() as u64 * 1000) / sample_rate as u64
        };
        Self {
            samples,
            sample_rate,
            duration_ms,
        }
    }

    /// Resamples to a target rate (linear interpolation) -- real, simple
    /// resampling used before provider upload.
    pub fn resample(&self, target_rate: u32) -> Self {
        if target_rate == 0 || target_rate == self.sample_rate || self.samples.is_empty() {
            return self.clone();
        }
        let ratio = self.sample_rate as f64 / target_rate as f64;
        let new_len = (self.samples.len() as f64 / ratio) as usize;
        let mut out = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src = (i as f64 * ratio) as usize;
            let a = self.samples.get(src).copied().unwrap_or(0);
            let b = self.samples.get(src + 1).copied().unwrap_or(a);
            let frac = (i as f64 * ratio) - src as f64;
            out.push((a as f64 + (b as f64 - a as f64) * frac) as i16);
        }
        Self::from_samples(out, target_rate)
    }

    /// Encodes samples as little-endian PCM16 bytes.
    pub fn to_pcm_le_bytes(&self) -> Vec<u8> {
        self.samples.iter().flat_map(|s| s.to_le_bytes()).collect()
    }
}

/// Speech-to-text provider.
#[async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(&self, audio: &Audio) -> Result<String, AiError>;
}

/// Text-to-speech provider.
#[async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Returns audio bytes (WAV/MP3 per provider) plus mime type.
    async fn synthesize(&self, text: &str) -> Result<(Vec<u8>, String), AiError>;
}

/// Pure construction of the STT multipart form fields (no HTTP involved).
///
/// Extracted so builder wiring is testable without a wire round-trip.
fn build_stt_form_fields(
    model: &str,
    language: Option<&str>,
    prompt: Option<&str>,
) -> Vec<(String, String)> {
    let mut fields = vec![("model".to_string(), model.to_string())];
    if let Some(lang) = language {
        fields.push(("language".to_string(), lang.to_string()));
    }
    if let Some(prompt) = prompt {
        fields.push(("prompt".to_string(), prompt.to_string()));
    }
    fields
}

/// Pure construction of the TTS JSON request body (no HTTP involved).
fn build_tts_request_body(model: &str, voice: &str, input: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "input": input,
        "voice": voice,
    })
}

/// Real OpenAI-compatible STT adapter (`POST {base}/audio/transcriptions`,
/// whisper model). Requires an API key. Defaults: model `whisper-1`, no
/// language/prompt override; override via the fluent builders.
pub struct OpenAiCompatSpeechToText {
    base_url: String,
    api_key: String,
    model: String,
    language: Option<String>,
    prompt: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatSpeechToText {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self, AiError> {
        Ok(Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: "whisper-1".to_string(),
            language: None,
            prompt: None,
            client: reqwest::Client::builder()
                .user_agent("ai-sdk-voice/0.1")
                .build()
                .map_err(|e| AiError::Web(WebError::new("stt client", e.to_string())))?,
        })
    }

    /// Overrides the transcription model (default `whisper-1`).
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Pins the spoken language as an ISO-639-1 hint (e.g. `"en"`).
    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Provides a spelling/vocabulary hint prompt.
    pub fn prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }
}

#[async_trait]
impl SpeechToText for OpenAiCompatSpeechToText {
    async fn transcribe(&self, audio: &Audio) -> Result<String, AiError> {
        let url = format!(
            "{}/audio/transcriptions",
            self.base_url.trim_end_matches('/')
        );
        let audio = audio.resample(16_000);
        let wav = wav_from_pcm(&audio.to_pcm_le_bytes(), audio.sample_rate);

        let mut form = reqwest::multipart::Form::new();
        for (name, value) in build_stt_form_fields(
            &self.model,
            self.language.as_deref(),
            self.prompt.as_deref(),
        ) {
            form = form.text(name, value);
        }
        form = form.part(
            "file",
            reqwest::multipart::Part::bytes(wav)
                .file_name("audio.wav")
                .mime_str("audio/wav")
                .map_err(|e| AiError::Web(WebError::new("stt", e.to_string())))?,
        );

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| AiError::Web(WebError::new("stt", e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AiError::Web(WebError::new(
                "stt",
                format!("HTTP {status}: {body}"),
            )));
        }
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AiError::Web(WebError::new("stt parse", e.to_string())))?;
        Ok(json
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string())
    }
}

/// Real OpenAI-compatible TTS adapter (`POST {base}/audio/speech`).
/// Defaults: model `tts-1`, voice `alloy`; override via the fluent builders.
pub struct OpenAiCompatTextToSpeech {
    base_url: String,
    api_key: String,
    model: String,
    voice: String,
    client: reqwest::Client,
}

impl OpenAiCompatTextToSpeech {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Result<Self, AiError> {
        Ok(Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: "tts-1".to_string(),
            voice: "alloy".to_string(),
            client: reqwest::Client::builder()
                .user_agent("ai-sdk-voice/0.1")
                .build()
                .map_err(|e| AiError::Web(WebError::new("tts client", e.to_string())))?,
        })
    }

    /// Overrides the synthesis model (default `tts-1`).
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Selects the provider voice (default `alloy`).
    pub fn voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = voice.into();
        self
    }
}

#[async_trait]
impl TextToSpeech for OpenAiCompatTextToSpeech {
    async fn synthesize(&self, text: &str) -> Result<(Vec<u8>, String), AiError> {
        let url = format!("{}/audio/speech", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&build_tts_request_body(&self.model, &self.voice, text))
            .send()
            .await
            .map_err(|e| AiError::Web(WebError::new("tts", e.to_string())))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AiError::Web(WebError::new(
                "tts",
                format!("HTTP {status}: {body}"),
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("audio/mpeg")
            .to_string();
        let bytes = response
            .bytes()
            .await
            .map_err(|e| AiError::Web(WebError::new("tts body", e.to_string())))?
            .to_vec();
        Ok((bytes, content_type))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_duration_is_computed() {
        let audio = Audio::from_samples(vec![0i16; 16_000], 16_000);
        assert_eq!(audio.duration_ms, 1000);
    }

    #[test]
    fn resample_halves_and_doubles() {
        let audio = Audio::from_samples(vec![0i16; 32_000], 32_000);
        let half = audio.resample(16_000);
        assert_eq!(half.sample_rate, 16_000);
        assert!(half.samples.len() < audio.samples.len());
        let back = half.resample(32_000);
        assert_eq!(back.sample_rate, 32_000);
    }

    #[test]
    fn stt_form_fields_reflect_builder_defaults_and_overrides() {
        // Defaults preserved: just whisper-1, no extras.
        assert_eq!(
            build_stt_form_fields("whisper-1", None, None),
            vec![("model".to_string(), "whisper-1".to_string())]
        );
        // Builder overrides flow through to the wire fields.
        let adapter = OpenAiCompatSpeechToText::new("http://localhost", "key")
            .unwrap()
            .model("whisper-large-v3")
            .language("de")
            .prompt("SIREN, barge-in");
        assert_eq!(
            build_stt_form_fields(
                &adapter.model,
                adapter.language.as_deref(),
                adapter.prompt.as_deref()
            ),
            vec![
                ("model".into(), "whisper-large-v3".into()),
                ("language".into(), "de".into()),
                ("prompt".into(), "SIREN, barge-in".into()),
            ]
        );
    }

    #[test]
    fn tts_request_body_reflects_builder_defaults_and_overrides() {
        // Defaults preserved: tts-1 + alloy.
        assert_eq!(
            build_tts_request_body("tts-1", "alloy", "hello"),
            serde_json::json!({"model": "tts-1", "input": "hello", "voice": "alloy"})
        );
        let adapter = OpenAiCompatTextToSpeech::new("http://localhost", "key")
            .unwrap()
            .model("tts-1-hd")
            .voice("nova");
        assert_eq!(
            build_tts_request_body(&adapter.model, &adapter.voice, "hi"),
            serde_json::json!({"model": "tts-1-hd", "input": "hi", "voice": "nova"})
        );
    }
}
