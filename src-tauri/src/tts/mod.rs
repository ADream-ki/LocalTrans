//! TTS (Text-to-Speech) Module
//! 
//! Supports multiple TTS engines for real-time speech synthesis
//! - Edge TTS: Microsoft's online TTS (free, high quality)
//! - Piper TTS: Local neural TTS (offline, fast)
//! - GPT-SoVITS: Custom voice cloning (local)
//! - RVC: Real-time Voice Conversion

pub mod edge_tts;
pub mod playback;
pub mod custom_voice;
pub mod piper_tts;

// Re-export main types
pub use edge_tts::EdgeTtsEngine;
pub use custom_voice::CustomVoiceEngine;
pub use piper_tts::PiperTtsEngine;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Voice information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: String,
    pub style: Option<String>,
    /// Is this a custom voice?
    pub is_custom: bool,
    /// Path to custom voice model (if custom)
    pub model_path: Option<String>,
}

/// TTS audio result
#[derive(Debug, Clone)]
pub struct TtsAudio {
    /// Audio samples (f32, normalized to [-1.0, 1.0])
    pub samples: Vec<f32>,
    /// Sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Duration in seconds
    pub duration_secs: f32,
}

impl TtsAudio {
    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        (self.duration_secs * 1000.0) as u64
    }
}

/// TTS engine trait
#[async_trait]
pub trait TtsEngine: Send + Sync {
    /// Get engine name
    fn name(&self) -> &str;
    
    /// Check if engine is available
    fn is_ready(&self) -> bool;
    
    /// Get available voices
    fn get_voices(&self) -> Vec<VoiceInfo>;
    
    /// Synthesize text to audio
    async fn synthesize(&self, text: &str, voice: &str) -> Result<TtsAudio>;
    
    /// Get default voice for language
    fn default_voice_for_lang(&self, lang: &str) -> Option<&str>;
}

/// TTS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    /// Engine type
    pub engine: TtsEngineType,
    
    /// Selected voice ID
    pub voice: String,
    
    /// Speech rate (0.5 - 2.0)
    pub rate: f32,
    
    /// Pitch adjustment (-50 to 50)
    pub pitch: i32,
    
    /// Volume (0.0 - 1.0)
    pub volume: f32,
    
    /// Output sample rate
    pub sample_rate: u32,
    
    /// Enable real-time streaming
    pub streaming: bool,
    
    /// Output device name (None = default, Some = specific device like VB-Audio Cable)
    pub output_device: Option<String>,
    
    /// Custom voice settings
    pub custom_voice: Option<CustomVoiceConfig>,
}

/// Custom voice configuration for GPT-SoVITS, RVC, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomVoiceConfig {
    /// Voice model type
    pub model_type: CustomVoiceType,
    /// Path to model file
    pub model_path: PathBuf,
    /// Reference audio for voice cloning (optional)
    pub reference_audio: Option<PathBuf>,
    /// Reference text for voice cloning (optional)
    pub reference_text: Option<String>,
    /// Voice similarity threshold (0.0 - 1.0)
    pub similarity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomVoiceType {
    /// GPT-SoVITS voice cloning
    GptSoVits,
    /// RVC (Real-time Voice Conversion)
    Rvc,
    /// Piper TTS model
    Piper,
    /// VITS model
    Vits,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsEngineType {
    /// Microsoft Edge TTS (online, free, high quality)
    EdgeTts,
    /// Custom voice (GPT-SoVITS, RVC, etc.)
    Custom,
    /// Piper TTS (offline, fast)
    Piper,
    /// System TTS (platform native)
    System,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            engine: TtsEngineType::EdgeTts,
            voice: "zh-CN-XiaoxiaoNeural".to_string(),
            rate: 1.0,
            pitch: 0,
            volume: 1.0,
            sample_rate: 24000,
            streaming: true,
            output_device: None,
            custom_voice: None,
        }
    }
}

impl TtsConfig {
    pub fn new(voice: &str) -> Self {
        Self {
            voice: voice.to_string(),
            ..Default::default()
        }
    }
    
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = rate.clamp(0.5, 2.0);
        self
    }
    
    pub fn with_pitch(mut self, pitch: i32) -> Self {
        self.pitch = pitch.clamp(-50, 50);
        self
    }
    
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set output device (for virtual audio routing)
    pub fn with_output_device(mut self, device: Option<String>) -> Self {
        self.output_device = device;
        self
    }
    
    /// Set custom voice configuration
    pub fn with_custom_voice(mut self, config: CustomVoiceConfig) -> Self {
        self.custom_voice = Some(config);
        self.engine = TtsEngineType::Custom;
        self
    }
}

/// Default voices for common languages (Edge TTS)
pub const DEFAULT_VOICES: &[(&str, &str, &str)] = &[
    // Chinese (Simplified)
    ("zh-CN", "zh-CN-XiaoxiaoNeural", "晓晓 (女声)"),
    ("zh-CN", "zh-CN-YunxiNeural", "云希 (男声)"),
    ("zh-CN", "zh-CN-YunyangNeural", "云扬 (男声)"),
    ("zh-CN", "zh-CN-XiaoyiNeural", "晓伊 (女声)"),
    // Chinese (Traditional)
    ("zh-TW", "zh-TW-HsiaoChenNeural", "曉臻 (女聲)"),
    ("zh-TW", "zh-TW-YunJheNeural", "雲哲 (男聲)"),
    // English (US)
    ("en-US", "en-US-JennyNeural", "Jenny (Female)"),
    ("en-US", "en-US-GuyNeural", "Guy (Male)"),
    ("en-US", "en-US-AriaNeural", "Aria (Female)"),
    // English (GB)
    ("en-GB", "en-GB-SoniaNeural", "Sonia (Female)"),
    ("en-GB", "en-GB-RyanNeural", "Ryan (Male)"),
    // Japanese
    ("ja-JP", "ja-JP-NanamiNeural", "七海 (女声)"),
    ("ja-JP", "ja-JP-KeitaNeural", "圭太 (男声)"),
    // Korean
    ("ko-KR", "ko-KR-SunHiNeural", "선히 (여성)"),
    ("ko-KR", "ko-KR-InJoonNeural", "인준 (남성)"),
    // French
    ("fr-FR", "fr-FR-DeniseNeural", "Denise (Femme)"),
    ("fr-FR", "fr-FR-HenriNeural", "Henri (Homme)"),
    // German
    ("de-DE", "de-DE-KatjaNeural", "Katja"),
    ("de-DE", "de-DE-ConradNeural", "Conrad"),
    // Spanish
    ("es-ES", "es-ES-ElviraNeural", "Elvira"),
    ("es-ES", "es-ES-AlvaroNeural", "Álvaro"),
    // Russian
    ("ru-RU", "ru-RU-SvetlanaNeural", "Светлана"),
    ("ru-RU", "ru-RU-DmitryNeural", "Дмитрий"),
    // Portuguese
    ("pt-BR", "pt-BR-FranciscaNeural", "Francisca"),
    // Italian
    ("it-IT", "it-IT-ElsaNeural", "Elsa"),
    // Arabic
    ("ar-SA", "ar-SA-ZariyahNeural", "زارية"),
    // Hindi
    ("hi-IN", "hi-IN-SwaraNeural", "स्वरा"),
];

/// Get default voice for a language
pub fn get_default_voice(lang: &str) -> Option<&'static str> {
    DEFAULT_VOICES.iter()
        .find(|(l, _, _)| l == &lang)
        .map(|(_, voice, _)| *voice)
}

/// Get all voices for a language
pub fn get_voices_for_lang(lang: &str) -> Vec<VoiceInfo> {
    DEFAULT_VOICES.iter()
        .filter(|(l, _, _)| l == &lang)
        .map(|(l, voice, name)| VoiceInfo {
            id: voice.to_string(),
            name: name.to_string(),
            language: l.to_string(),
            gender: if name.contains("女") || name.contains("Female") || name.contains("Femme") { 
                "female".to_string() 
            } else if name.contains("男") || name.contains("Male") || name.contains("Homme") { 
                "male".to_string() 
            } else { 
                "neutral".to_string() 
            },
            style: None,
            is_custom: false,
            model_path: None,
        })
        .collect()
}

/// Get all available voices
pub fn get_all_voices() -> Vec<VoiceInfo> {
    DEFAULT_VOICES.iter()
        .map(|(l, voice, name)| VoiceInfo {
            id: voice.to_string(),
            name: name.to_string(),
            language: l.to_string(),
            gender: if name.contains("女") || name.contains("Female") || name.contains("Femme") { 
                "female".to_string() 
            } else if name.contains("男") || name.contains("Male") { 
                "male".to_string() 
            } else { 
                "neutral".to_string() 
            },
            style: None,
            is_custom: false,
            model_path: None,
        })
        .collect()
}