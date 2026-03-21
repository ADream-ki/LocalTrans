//! Whisper ASR Engine implementation
//!
//! This module provides integration with whisper.cpp via whisper-rs.
//! When the `whisper-backend` feature is not enabled, a mock implementation
//! is provided for development and testing.

use anyhow::Result;
use super::{
    AsrEngine, AsrConfig, AsrError, Transcription, Segment, 
    PartialTranscription, ModelInfo,
};

#[cfg(feature = "whisper-backend")]
mod whisper_impl {
    use super::*;
    use whisper_rs::{WhisperContext, WhisperContextParameters, FullParams, SamplingStrategy};
    
    /// Real Whisper ASR Engine using whisper.cpp
    pub struct WhisperEngine {
        ctx: Option<WhisperContext>,
        config: Option<AsrConfig>,
        language: Option<String>,
    }
    
    impl WhisperEngine {
        pub fn new(config: AsrConfig) -> Result<Self, AsrError> {
            let ctx_params = WhisperContextParameters::default();
            
            let ctx = if config.model_path.exists() {
                Some(WhisperContext::new_with_params(
                    config.model_path.to_str().unwrap(),
                    ctx_params,
                ).map_err(|e| AsrError::InitializationFailed(format!("{:?}", e)))?)
            } else {
                None
            };
            
            Ok(Self {
                ctx,
                config: Some(config),
                language: None,
            })
        }
    }
    
    impl AsrEngine for WhisperEngine {
        fn init(config: AsrConfig) -> Result<Self, AsrError> {
            Self::new(config)
        }
        
        fn transcribe(&mut self, audio: &[f32], sample_rate: u32) -> Result<Transcription, AsrError> {
            let ctx = self.ctx.as_ref()
                .ok_or_else(|| AsrError::InitializationFailed("Whisper context not initialized".to_string()))?;
            
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            
            if let Some(ref lang) = self.language {
                params.set_language(Some(lang));
            }
            
            params.set_translate(false);
            params.set_no_context(true);
            params.set_single_segment(false);
            
            let state = ctx.create_state()
                .map_err(|e| AsrError::TranscriptionFailed(format!("{:?}", e)))?;
            
            state.full(params, audio)
                .map_err(|e| AsrError::TranscriptionFailed(format!("{:?}", e)))?;
            
            let num_segments = state.full_n_segments()
                .map_err(|e| AsrError::TranscriptionFailed(format!("{:?}", e)))?;
            
            let mut segments = Vec::new();
            let mut full_text = String::new();
            
            for i in 0..num_segments {
                let text = state.full_get_segment_text(i)
                    .map_err(|e| AsrError::TranscriptionFailed(format!("{:?}", e)))?;
                
                let start = state.full_get_segment_t0(i)
                    .map_err(|e| AsrError::TranscriptionFailed(format!("{:?}", e)))?;
                let end = state.full_get_segment_t1(i)
                    .map_err(|e| AsrError::TranscriptionFailed(format!("{:?}", e)))?;
                
                // Convert from whisper time units (centiseconds) to seconds
                let start_sec = start as f64 * 0.01;
                let end_sec = end as f64 * 0.01;
                
                segments.push(Segment {
                    start: start_sec,
                    end: end_sec,
                    text: text.clone(),
                    confidence: 0.9,
                    token_confidences: None,
                    words: None,
                });
                
                full_text.push_str(&text);
            }
            
            Ok(Transcription {
                text: full_text,
                segments,
                language: self.language.clone().unwrap_or_else(|| "auto".to_string()),
                confidence: 0.9,
                processing_time_ms: 0,
                is_partial: false,
            })
        }
        
        fn transcribe_streaming(&mut self, _audio: &[f32]) -> Result<Option<PartialTranscription>, AsrError> {
            // Whisper doesn't support true streaming, so we return None
            Ok(None)
        }
        
        fn set_language(&mut self, lang: &str) {
            self.language = Some(lang.to_string());
        }
        
        fn get_language(&self) -> Option<&str> {
            self.language.as_deref()
        }
        
        fn get_supported_languages(&self) -> Vec<String> {
            vec![
                "en".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "es".to_string(),
                "ru".to_string(),
                "it".to_string(),
                "pt".to_string(),
                "ar".to_string(),
                "hi".to_string(),
            ]
        }
        
        fn reset(&mut self) {
            // No state to reset for batch processing
        }
        
        fn is_gpu_enabled(&self) -> bool {
            false // TODO: Check actual GPU usage
        }
        
        fn get_model_info(&self) -> ModelInfo {
            ModelInfo {
                model_size: self.config.as_ref()
                    .map(|c| format!("{:?}", c.model_size))
                    .unwrap_or_default(),
                language_count: 99,
                is_multilingual: true,
                is_english_only: false,
                gpu_enabled: false,
                memory_usage_mb: self.config.as_ref()
                    .map(|c| c.estimated_memory_mb())
                    .unwrap_or(0),
            }
        }
    }
}

#[cfg(not(feature = "whisper-backend"))]
mod whisper_impl {
    use super::*;
    
    /// Mock Whisper ASR Engine for development without libclang
    pub struct WhisperEngine {
        config: Option<AsrConfig>,
        language: Option<String>,
    }
    
    impl WhisperEngine {
        pub fn new(config: AsrConfig) -> Result<Self, AsrError> {
            tracing::info!("Mock ASR initialized with config: {:?}", config.model_path);
            Ok(Self {
                config: Some(config),
                language: None,
            })
        }
    }
    
    impl AsrEngine for WhisperEngine {
        fn init(config: AsrConfig) -> Result<Self, AsrError> {
            Self::new(config)
        }
        
        fn transcribe(&mut self, audio: &[f32], _sample_rate: u32) -> Result<Transcription, AsrError> {
            // Mock implementation: analyze audio energy and generate placeholder
            let energy: f32 = if audio.is_empty() {
                0.0
            } else {
                (audio.iter().map(|x| x * x).sum::<f32>() / audio.len() as f32).sqrt()
            };
            
            let text = if energy > 0.01 {
                // Generate mock text based on detected language
                match self.language.as_deref() {
                    Some("zh") => "[检测到语音内容 - Mock transcription]".to_string(),
                    Some("ja") => "[音声が検出されました]".to_string(),
                    Some("ko") => "[음성이 감지되었습니다]".to_string(),
                    _ => "[Speech detected - Mock transcription]".to_string(),
                }
            } else {
                String::new()
            };
            
            let duration = audio.len() as f64 / 16000.0;
            
            Ok(Transcription {
                text: text.clone(),
                segments: vec![Segment {
                    start: 0.0,
                    end: duration,
                    text,
                    confidence: if energy > 0.01 { 0.8 } else { 0.0 },
                    token_confidences: None,
                    words: None,
                }],
                language: self.language.clone().unwrap_or_else(|| "auto".to_string()),
                confidence: if energy > 0.01 { 0.8 } else { 0.0 },
                processing_time_ms: 10,
                is_partial: false,
            })
        }
        
        fn transcribe_streaming(&mut self, _audio: &[f32]) -> Result<Option<PartialTranscription>, AsrError> {
            Ok(None)
        }
        
        fn set_language(&mut self, lang: &str) {
            self.language = Some(lang.to_string());
        }
        
        fn get_language(&self) -> Option<&str> {
            self.language.as_deref()
        }
        
        fn get_supported_languages(&self) -> Vec<String> {
            vec![
                "en".to_string(),
                "zh".to_string(),
                "ja".to_string(),
                "ko".to_string(),
                "fr".to_string(),
                "de".to_string(),
                "es".to_string(),
                "ru".to_string(),
            ]
        }
        
        fn reset(&mut self) {}
        
        fn is_gpu_enabled(&self) -> bool {
            false
        }
        
        fn get_model_info(&self) -> ModelInfo {
            ModelInfo {
                model_size: self.config.as_ref()
                    .map(|c| format!("{:?}", c.model_size))
                    .unwrap_or_default(),
                language_count: 8,
                is_multilingual: true,
                is_english_only: false,
                gpu_enabled: false,
                memory_usage_mb: 250,
            }
        }
    }
}

pub use whisper_impl::WhisperEngine;

/// Whisper-specific error type
#[derive(Debug, Clone)]
pub enum WhisperError {
    ModelLoadFailed(String),
    TranscriptionFailed(String),
    InvalidAudio(String),
    NotInitialized,
}

impl std::fmt::Display for WhisperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ModelLoadFailed(msg) => write!(f, "Model load failed: {}", msg),
            Self::TranscriptionFailed(msg) => write!(f, "Transcription failed: {}", msg),
            Self::InvalidAudio(msg) => write!(f, "Invalid audio: {}", msg),
            Self::NotInitialized => write!(f, "Engine not initialized"),
        }
    }
}

impl std::error::Error for WhisperError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_whisper_engine_new() {
        let config = AsrConfig::default();
        let engine = WhisperEngine::new(config).unwrap();
        assert!(engine.get_supported_languages().contains(&"en".to_string()));
    }
    
    #[test]
    fn test_set_language() {
        let config = AsrConfig::default();
        let mut engine = WhisperEngine::new(config).unwrap();
        engine.set_language("zh");
        assert_eq!(engine.get_language(), Some("zh"));
    }
}
