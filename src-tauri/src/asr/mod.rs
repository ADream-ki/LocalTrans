//! ASR (Automatic Speech Recognition) Module
//!
//! This module provides production-ready speech recognition capabilities
//! using Whisper.cpp as the backend engine.

mod sensevoice;
pub mod sherpa;
mod streaming;
mod whisper;

pub use sensevoice::SenseVoiceEngine;
pub use sherpa::SherpaAsrEngine;
pub use streaming::{StreamingAsrEngine, StreamingConfig, StreamingResult, StreamingState};
pub use whisper::{WhisperEngine, WhisperError};

/// Whisper model size variants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelSize {
    Tiny,
    TinyEn,
    #[default]
    Base,
    BaseEn,
    Small,
    SmallEn,
    Medium,
    MediumEn,
    LargeV1,
    LargeV2,
    LargeV3,
}

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// ASR module specific errors
#[derive(Debug, Error)]
pub enum AsrError {
    #[error("Model not found at path: {0}")]
    ModelNotFound(PathBuf),

    #[error("Failed to initialize ASR engine: {0}")]
    InitializationFailed(String),

    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    #[error("Invalid audio format: {0}")]
    InvalidAudioFormat(String),

    #[error("Model download failed: {0}")]
    DownloadFailed(String),

    #[error("GPU not available: {0}")]
    GpuNotAvailable(String),

    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    #[error("Streaming error: {0}")]
    StreamingError(String),
}

/// Complete transcription result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcription {
    /// Transcribed text
    pub text: String,
    /// Individual segments with timing information
    pub segments: Vec<Segment>,
    /// Detected or configured language
    pub language: String,
    /// Overall confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
    /// Whether this is a partial result from streaming
    pub is_partial: bool,
}

impl Default for Transcription {
    fn default() -> Self {
        Self {
            text: String::new(),
            segments: Vec::new(),
            language: "auto".to_string(),
            confidence: 0.0,
            processing_time_ms: 0,
            is_partial: false,
        }
    }
}

/// A single transcription segment with timing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    /// Start time in seconds
    pub start: f64,
    /// End time in seconds
    pub end: f64,
    /// Segment text
    pub text: String,
    /// Confidence score for this segment
    pub confidence: f32,
    /// Token-level confidence (optional)
    pub token_confidences: Option<Vec<f32>>,
    /// Word-level timestamps (optional)
    pub words: Option<Vec<Word>>,
}

/// Word-level timing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub confidence: f32,
}

/// Partial transcription result for streaming mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialTranscription {
    /// Current partial text
    pub text: String,
    /// Whether this result is final
    pub is_final: bool,
    /// Confidence score
    pub confidence: f32,
    /// Detected language (if auto-detect enabled)
    pub detected_language: Option<String>,
    /// Number of segments processed
    pub segment_count: usize,
}

impl Default for PartialTranscription {
    fn default() -> Self {
        Self {
            text: String::new(),
            is_final: false,
            confidence: 0.0,
            detected_language: None,
            segment_count: 0,
        }
    }
}

/// ASR engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    /// Path to the Whisper model file
    pub model_path: PathBuf,
    /// Model size (for automatic model selection)
    pub model_size: ModelSize,
    /// Target language (None for auto-detect)
    pub language: Option<String>,
    /// Enable GPU acceleration
    pub gpu_enabled: bool,
    /// Number of CPU threads to use
    pub threads: usize,
    /// Enable word-level timestamps
    pub word_timestamps: bool,
    /// Translation mode (translate to English)
    pub translate: bool,
    /// Temperature for sampling (0.0 = greedy)
    pub temperature: f32,
    /// Maximum segment length in characters
    pub max_segment_length: usize,
    /// Suppress non-speech tokens
    pub suppress_blank: bool,
    /// Initial prompt for context
    pub initial_prompt: Option<String>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            model_size: ModelSize::Base,
            language: None,
            gpu_enabled: false,
            threads: 4,
            word_timestamps: true,
            translate: false,
            temperature: 0.0,
            max_segment_length: 0, // 0 = no limit
            suppress_blank: true,
            initial_prompt: None,
        }
    }
}

impl AsrConfig {
    /// Create a new configuration with the specified model path
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            ..Default::default()
        }
    }

    /// Set the model size
    pub fn with_model_size(mut self, size: ModelSize) -> Self {
        self.model_size = size;
        self
    }

    /// Set the target language
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Enable GPU acceleration
    pub fn with_gpu(mut self, enabled: bool) -> Self {
        self.gpu_enabled = enabled;
        self
    }

    /// Set the number of threads
    pub fn with_threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// Enable translation mode
    pub fn with_translation(mut self, translate: bool) -> Self {
        self.translate = translate;
        self
    }

    /// Set temperature for sampling
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    /// Set initial prompt
    pub fn with_initial_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.initial_prompt = Some(prompt.into());
        self
    }

    /// Get the model filename based on size
    pub fn model_filename(&self) -> String {
        let ext = if self.gpu_enabled { ".en" } else { "" };
        match self.model_size {
            ModelSize::Tiny => format!("ggml-tiny{}.bin", ext),
            ModelSize::TinyEn => "ggml-tiny.en.bin".to_string(),
            ModelSize::Base => format!("ggml-base{}.bin", ext),
            ModelSize::BaseEn => "ggml-base.en.bin".to_string(),
            ModelSize::Small => format!("ggml-small{}.bin", ext),
            ModelSize::SmallEn => "ggml-small.en.bin".to_string(),
            ModelSize::Medium => format!("ggml-medium{}.bin", ext),
            ModelSize::MediumEn => "ggml-medium.en.bin".to_string(),
            ModelSize::LargeV1 => "ggml-large-v1.bin".to_string(),
            ModelSize::LargeV2 => "ggml-large-v2.bin".to_string(),
            ModelSize::LargeV3 => "ggml-large-v3.bin".to_string(),
        }
    }

    /// Get the download URL for the model
    pub fn model_download_url(&self) -> String {
        let filename = self.model_filename();
        format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}",
            filename
        )
    }

    /// Estimate memory requirements in MB
    pub fn estimated_memory_mb(&self) -> usize {
        match self.model_size {
            ModelSize::Tiny | ModelSize::TinyEn => 150,
            ModelSize::Base | ModelSize::BaseEn => 250,
            ModelSize::Small | ModelSize::SmallEn => 500,
            ModelSize::Medium | ModelSize::MediumEn => 1500,
            ModelSize::LargeV1 | ModelSize::LargeV2 | ModelSize::LargeV3 => 3000,
        }
    }
}

/// Trait defining the ASR engine interface
pub trait AsrEngine: Send + Sync {
    /// Initialize the engine with the given configuration
    fn init(config: AsrConfig) -> Result<Self, AsrError>
    where
        Self: Sized;

    /// Transcribe a complete audio buffer
    fn transcribe(&mut self, audio: &[f32], sample_rate: u32) -> Result<Transcription, AsrError>;

    /// Process a chunk of audio for streaming transcription
    fn transcribe_streaming(
        &mut self,
        audio: &[f32],
    ) -> Result<Option<PartialTranscription>, AsrError>;

    /// Set the target language
    fn set_language(&mut self, lang: &str);

    /// Get the current language setting
    fn get_language(&self) -> Option<&str>;

    /// Get list of supported languages
    fn get_supported_languages(&self) -> Vec<String>;

    /// Reset the engine state (for new utterance)
    fn reset(&mut self);

    /// Check if GPU acceleration is being used
    fn is_gpu_enabled(&self) -> bool;

    /// Get model information
    fn get_model_info(&self) -> ModelInfo;
}

/// Information about the loaded model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_size: String,
    pub language_count: usize,
    pub is_multilingual: bool,
    pub is_english_only: bool,
    pub gpu_enabled: bool,
    pub memory_usage_mb: usize,
}

/// Get the default models directory
pub fn get_default_models_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
}

/// Supported language codes for Whisper
pub const SUPPORTED_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "Chinese"),
    ("de", "German"),
    ("es", "Spanish"),
    ("ru", "Russian"),
    ("ko", "Korean"),
    ("fr", "French"),
    ("ja", "Japanese"),
    ("pt", "Portuguese"),
    ("tr", "Turkish"),
    ("pl", "Polish"),
    ("ca", "Catalan"),
    ("nl", "Dutch"),
    ("ar", "Arabic"),
    ("sv", "Swedish"),
    ("it", "Italian"),
    ("id", "Indonesian"),
    ("hi", "Hindi"),
    ("fi", "Finnish"),
    ("vi", "Vietnamese"),
    ("he", "Hebrew"),
    ("uk", "Ukrainian"),
    ("el", "Greek"),
    ("ms", "Malay"),
    ("cs", "Czech"),
    ("ro", "Romanian"),
    ("da", "Danish"),
    ("hu", "Hungarian"),
    ("ta", "Tamil"),
    ("no", "Norwegian"),
    ("th", "Thai"),
    ("ur", "Urdu"),
    ("hr", "Croatian"),
    ("bg", "Bulgarian"),
    ("lt", "Lithuanian"),
    ("la", "Latin"),
    ("mi", "Maori"),
    ("ml", "Malayalam"),
    ("cy", "Welsh"),
    ("sk", "Slovak"),
    ("te", "Telugu"),
    ("fa", "Persian"),
    ("lv", "Latvian"),
    ("bn", "Bengali"),
    ("sr", "Serbian"),
    ("az", "Azerbaijani"),
    ("sl", "Slovenian"),
    ("kn", "Kannada"),
    ("et", "Estonian"),
    ("mk", "Macedonian"),
    ("br", "Breton"),
    ("eu", "Basque"),
    ("is", "Icelandic"),
    ("hy", "Armenian"),
    ("ne", "Nepali"),
    ("mn", "Mongolian"),
    ("bs", "Bosnian"),
    ("kk", "Kazakh"),
    ("sq", "Albanian"),
    ("sw", "Swahili"),
    ("gl", "Galician"),
    ("mr", "Marathi"),
    ("pa", "Punjabi"),
    ("si", "Sinhala"),
    ("km", "Khmer"),
    ("sn", "Shona"),
    ("yo", "Yoruba"),
    ("so", "Somali"),
    ("af", "Afrikaans"),
    ("oc", "Occitan"),
    ("ka", "Georgian"),
    ("be", "Belarusian"),
    ("tg", "Tajik"),
    ("sd", "Sindhi"),
    ("gu", "Gujarati"),
    ("am", "Amharic"),
    ("yi", "Yiddish"),
    ("lo", "Lao"),
    ("uz", "Uzbek"),
    ("fo", "Faroese"),
    ("ht", "Haitian Creole"),
    ("ps", "Pashto"),
    ("tk", "Turkmen"),
    ("nn", "Nynorsk"),
    ("mt", "Maltese"),
    ("sa", "Sanskrit"),
    ("lb", "Luxembourgish"),
    ("my", "Myanmar"),
    ("bo", "Tibetan"),
    ("tl", "Tagalog"),
    ("mg", "Malagasy"),
    ("as", "Assamese"),
    ("tt", "Tatar"),
    ("haw", "Hawaiian"),
    ("ln", "Lingala"),
    ("ha", "Hausa"),
    ("ba", "Bashkir"),
    ("jw", "Javanese"),
    ("su", "Sundanese"),
    ("yue", "Cantonese"),
];

/// Check if a language code is supported
pub fn is_language_supported(lang: &str) -> bool {
    SUPPORTED_LANGUAGES.iter().any(|(code, _)| *code == lang)
}

/// Get language name from code
pub fn get_language_name(code: &str) -> Option<&'static str> {
    SUPPORTED_LANGUAGES
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, name)| *name)
}

/// Mock ASR engine for development and testing
/// Generates simulated transcription results
#[cfg(feature = "mock-asr")]
pub struct MockAsrEngine {
    language: Option<String>,
    call_count: std::sync::atomic::AtomicU64,
}

#[cfg(feature = "mock-asr")]
impl AsrEngine for MockAsrEngine {
    fn init(config: AsrConfig) -> Result<Self, AsrError> {
        tracing::info!("Initializing Mock ASR engine (development mode)");
        let _ = config;
        Ok(Self {
            language: None,
            call_count: std::sync::atomic::AtomicU64::new(0),
        })
    }

    fn transcribe(&mut self, audio: &[f32], sample_rate: u32) -> Result<Transcription, AsrError> {
        let count = self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let duration = audio.len() as f32 / sample_rate as f32;
        
        // Generate mock transcription based on duration
        let mock_texts = [
            "这是模拟的语音识别结果。",
            "Hello, this is a mock transcription.",
            "今天天气很好，适合出门散步。",
            "The quick brown fox jumps over the lazy dog.",
            "欢迎使用实时语音翻译系统。",
        ];
        
        let text = mock_texts[count as usize % mock_texts.len()];
        
        Ok(Transcription {
            text: text.to_string(),
            segments: vec![Segment {
                start: 0.0,
                end: duration as f64,
                text: text.to_string(),
                confidence: 0.95,
                token_confidences: None,
                words: None,
            }],
            language: self.language.clone().unwrap_or_else(|| "zh".to_string()),
            confidence: 0.95,
            processing_time_ms: 100,
            is_partial: false,
        })
    }

    fn transcribe_streaming(&mut self, _audio: &[f32]) -> Result<Option<PartialTranscription>, AsrError> {
        let count = self.call_count.load(std::sync::atomic::Ordering::SeqCst);
        
        let partial_texts = ["正在识别...", "识别中...", "Processing...", "听取中..."];
        let text = partial_texts[count as usize % partial_texts.len()];
        
        Ok(Some(PartialTranscription {
            text: text.to_string(),
            is_final: false,
            confidence: 0.5,
            detected_language: self.language.clone(),
            segment_count: 1,
        }))
    }

    fn set_language(&mut self, lang: &str) {
        self.language = Some(lang.to_string());
    }

    fn get_language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    fn get_supported_languages(&self) -> Vec<String> {
        vec!["zh".to_string(), "en".to_string(), "ja".to_string()]
    }

    fn reset(&mut self) {
        self.call_count.store(0, std::sync::atomic::Ordering::SeqCst);
    }

    fn is_gpu_enabled(&self) -> bool {
        false
    }

    fn get_model_info(&self) -> ModelInfo {
        ModelInfo {
            model_size: "mock".to_string(),
            language_count: 3,
            is_multilingual: true,
            is_english_only: false,
            gpu_enabled: false,
            memory_usage_mb: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asr_config_default() {
        let config = AsrConfig::default();
        assert_eq!(config.threads, 4);
        assert!(!config.gpu_enabled);
        assert!(config.language.is_none());
    }

    #[test]
    fn test_asr_config_builder() {
        let config = AsrConfig::new(PathBuf::from("/path/to/model"))
            .with_language("zh")
            .with_threads(8)
            .with_gpu(true);

        assert_eq!(config.language, Some("zh".to_string()));
        assert_eq!(config.threads, 8);
        assert!(config.gpu_enabled);
    }

    #[test]
    fn test_model_filename() {
        let config = AsrConfig::default().with_model_size(ModelSize::Tiny);
        assert_eq!(config.model_filename(), "ggml-tiny.bin");

        let config = AsrConfig::default().with_model_size(ModelSize::LargeV3);
        assert_eq!(config.model_filename(), "ggml-large-v3.bin");
    }

    #[test]
    fn test_language_support() {
        assert!(is_language_supported("en"));
        assert!(is_language_supported("zh"));
        assert!(is_language_supported("ja"));
        assert!(!is_language_supported("xx"));

        assert_eq!(get_language_name("en"), Some("English"));
        assert_eq!(get_language_name("zh"), Some("Chinese"));
    }

    #[test]
    fn test_memory_estimation() {
        let config = AsrConfig::default().with_model_size(ModelSize::Tiny);
        assert_eq!(config.estimated_memory_mb(), 150);

        let config = AsrConfig::default().with_model_size(ModelSize::LargeV3);
        assert_eq!(config.estimated_memory_mb(), 3000);
    }
}
