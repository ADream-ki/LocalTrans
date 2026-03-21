//! Edge-TTS implementation
//! 
//! Uses Microsoft Edge's free online TTS service.
//! High quality neural voices in multiple languages.

use anyhow::{Result, Context};
use async_trait::async_trait;
use reqwest::Client;
use tokio::sync::Mutex;
use std::sync::Arc;

use super::{TtsEngine, TtsAudio, VoiceInfo, DEFAULT_VOICES};

const EDGE_TTS_URL: &str = "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud/edge/v1";
const TRUSTED_CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";

/// Edge TTS Engine
pub struct EdgeTtsEngine {
    client: Client,
    voices: Vec<VoiceInfo>,
    cache: Arc<Mutex<Vec<TtsCacheEntry>>>,
}

#[derive(Debug)]
struct TtsCacheEntry {
    text: String,
    voice: String,
    rate: f32,
    pitch: i32,
    audio: TtsAudio,
}

impl EdgeTtsEngine {
    pub fn new() -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client")?;

        let voices = Self::build_voice_list();

        Ok(Self {
            client,
            voices,
            cache: Arc::new(Mutex::new(Vec::new())),
        })
    }

    fn build_voice_list() -> Vec<VoiceInfo> {
        DEFAULT_VOICES.iter()
            .map(|(lang, id, name)| VoiceInfo {
                id: id.to_string(),
                name: name.to_string(),
                language: lang.to_string(),
                gender: if name.contains("女") || name.contains("Female") { "female".to_string() }
                       else if name.contains("男") || name.contains("Male") { "male".to_string() }
                       else { "neutral".to_string() },
                style: None,
                is_custom: false,
                model_path: None,
            })
            .collect()
    }

    async fn check_cache(&self, text: &str, voice: &str, rate: f32, pitch: i32) -> Option<TtsAudio> {
        let cache = self.cache.lock().await;
        cache.iter()
            .find(|e| e.text == text && e.voice == voice && e.rate == rate && e.pitch == pitch)
            .map(|e| e.audio.clone())
    }

    async fn add_to_cache(&self, text: String, voice: String, rate: f32, pitch: i32, audio: TtsAudio) {
        let mut cache = self.cache.lock().await;
        // Limit cache size to 50 entries
        if cache.len() >= 50 {
            cache.remove(0);
        }
        cache.push(TtsCacheEntry { text, voice, rate, pitch, audio });
    }

    fn build_ssml(text: &str, voice: &str, rate: f32, pitch: i32) -> String {
        let rate = rate.clamp(0.5, 2.0);
        let pitch = pitch.clamp(-50, 50);
        let rate_percent = ((rate - 1.0) * 100.0) as i32;
        let pitch_str = if pitch > 0 {
            format!("+{}%", pitch)
        } else if pitch < 0 {
            format!("{}%", pitch)
        } else {
            "+0%".to_string()
        };

        // Escape XML special characters
        let escaped_text = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;");

        format!(
            r#"<speak version="1.0" xmlns="http://www.w3.org/2001/10/synthesis" xml:lang="en-US">
                <voice name="{}">
                    <prosody rate="{}%" pitch="{}">
                        {}
                    </prosody>
                </voice>
            </speak>"#,
            voice, rate_percent, pitch_str, escaped_text
        )
    }

    pub async fn synthesize_with_prosody(
        &self,
        text: &str,
        voice: &str,
        rate: f32,
        pitch: i32,
    ) -> Result<TtsAudio> {
        let rate = rate.clamp(0.5, 2.0);
        let pitch = pitch.clamp(-50, 50);

        if let Some(cached) = self.check_cache(text, voice, rate, pitch).await {
            tracing::debug!("TTS cache hit for: {}", text);
            return Ok(cached);
        }

        tracing::debug!(
            "Synthesizing speech: {} with voice {} (rate={}, pitch={})",
            text,
            voice,
            rate,
            pitch
        );

        let ssml = Self::build_ssml(text, voice, rate, pitch);

        let url = format!(
            "{}?trustedclienttoken={}&Retry-After=0&PRG=1",
            EDGE_TTS_URL, TRUSTED_CLIENT_TOKEN
        );

        let response = self.client
            .post(&url)
            .header("Content-Type", "application/ssml+xml")
            .header("X-Microsoft-OutputFormat", "audio-24khz-48kbitrate-mono-mp3")
            .header("User-Agent", "Mozilla/5.0")
            .body(ssml)
            .send()
            .await
            .context("Failed to send TTS request")?;

        if !response.status().is_success() {
            anyhow::bail!("TTS request failed: {}", response.status());
        }

        let audio_data = response.bytes().await
            .context("Failed to read audio data")?;

        // Decode MP3 to PCM samples
        let samples = decode_mp3_to_f32(&audio_data)?;

        let sample_rate = 24000u32;
        let duration_secs = samples.len() as f32 / sample_rate as f32;

        let audio = TtsAudio {
            samples,
            sample_rate,
            channels: 1,
            duration_secs,
        };

        self.add_to_cache(text.to_string(), voice.to_string(), rate, pitch, audio.clone()).await;

        tracing::debug!("TTS synthesis complete: {:.2}s audio", duration_secs);
        Ok(audio)
    }
}

#[async_trait]
impl TtsEngine for EdgeTtsEngine {
    fn name(&self) -> &str {
        "edge-tts"
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<TtsAudio> {
        self.synthesize_with_prosody(text, voice, 1.0, 0).await
    }

    fn get_voices(&self) -> Vec<VoiceInfo> {
        self.voices.clone()
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn default_voice_for_lang(&self, lang: &str) -> Option<&str> {
        super::get_default_voice(lang)
    }
}

/// Decode MP3 audio to f32 samples
fn decode_mp3_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    use minimp3::{Decoder, Frame};
    
    let mut decoder = Decoder::new(data);
    let mut samples = Vec::new();

    loop {
        match decoder.next_frame() {
            Ok(Frame { data: frame_data, sample_rate: sr, .. }) => {
                // Convert i16 samples to f32
                for sample in frame_data.iter() {
                    samples.push(*sample as f32 / 32768.0);
                }
                let _ = sr; // sample rate info
            }
            Err(minimp3::Error::Eof) => break,
            Err(e) => {
                tracing::warn!("MP3 decode error: {:?}", e);
                break;
            }
        }
    }

    // If decoding failed, return 0.5 second of silence
    if samples.is_empty() {
        tracing::warn!("MP3 decoding produced no samples, returning silence");
        return Ok(vec![0.0f32; 12000]);
    }

    Ok(samples)
}
