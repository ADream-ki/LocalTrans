//! Sherpa-rs ASR Engine implementation
//!
//! This module provides integration with sherpa-rs for offline ASR.
//! Uses VAD + offline recognition for real-time-like processing.

use super::{
    AsrConfig, AsrEngine, AsrError, ModelInfo, PartialTranscription, Segment, Transcription,
};
use anyhow::Result;
#[cfg(feature = "sherpa-backend")]
use std::path::PathBuf;

#[cfg(feature = "sherpa-backend")]
mod sherpa_impl {
    use super::*;
    use dirs;
    use parking_lot::Mutex;
    use sherpa_rs::silero_vad::{SileroVad, SileroVadConfig};
    use sherpa_rs::zipformer::{ZipFormer, ZipFormerConfig};
    use std::sync::Arc;
    use std::time::Instant;

    /// Sherpa-rs based ASR engine using VAD + offline recognition
    pub struct SherpaAsrEngine {
        recognizer: Option<Arc<Mutex<ZipFormer>>>,
        vad: Option<Arc<Mutex<SileroVad>>>,
        config: AsrConfig,
        language: Option<String>,
        initialized: bool,
        audio_buffer: Vec<f32>,
        sample_rate: u32,
    }

    // SAFETY: All internal state is protected by Mutex
    unsafe impl Send for SherpaAsrEngine {}
    unsafe impl Sync for SherpaAsrEngine {}

    impl SherpaAsrEngine {
        /// Create a new Sherpa ASR engine with the given configuration
        pub fn new(config: AsrConfig) -> Result<Self, AsrError> {
            tracing::info!(
                "Initializing Sherpa ASR engine with model: {:?}",
                config.model_path
            );

            Ok(Self {
                recognizer: None,
                vad: None,
                config,
                language: None,
                initialized: false,
                audio_buffer: Vec::new(),
                sample_rate: 16000,
            })
        }

        /// Get VAD model path (silero_vad.onnx)
        fn get_vad_model_path() -> Result<PathBuf, AsrError> {
            // Check common locations
            let data_dir = dirs::data_local_dir().ok_or_else(|| {
                AsrError::InitializationFailed("Cannot find data directory".to_string())
            })?;

            let vad_model = data_dir
                .join("LocalTrans")
                .join("models")
                .join("vad")
                .join("silero_vad.onnx");

            if vad_model.exists() {
                return Ok(vad_model);
            }

            // Return path even if doesn't exist (will fail at VAD creation)
            Ok(vad_model)
        }

        /// Initialize the recognizer and VAD
        fn init_components(&mut self) -> Result<(), AsrError> {
            if self.initialized {
                return Ok(());
            }

            // Accept both layouts:
            // - models/asr/{tokens.txt, encoder*.onnx, decoder*.onnx, joiner*.onnx}
            // - models/asr/<model-name>/{...}
            let resolved_dir = Self::resolve_asr_model_dir(&self.config.model_path);
            if resolved_dir != self.config.model_path {
                tracing::info!(
                    "ASR model files not found in {:?}; using {:?}",
                    self.config.model_path,
                    resolved_dir
                );
                self.config.model_path = resolved_dir;
            }

            let model_dir = &self.config.model_path;

            if !model_dir.exists() {
                return Err(AsrError::ModelNotFound(model_dir.clone()));
            }

            // Try to detect model files
            let encoder_path = Self::find_model_file(model_dir, "encoder");
            let decoder_path = Self::find_model_file(model_dir, "decoder");
            let joiner_path = Self::find_model_file(model_dir, "joiner");
            let tokens_path = model_dir.join("tokens.txt");

            // Verify required files exist
            if !encoder_path.exists() || !decoder_path.exists() || !joiner_path.exists() {
                return Err(AsrError::InitializationFailed(format!(
                    "Missing model files. Required: encoder, decoder, joiner in {:?}",
                    model_dir
                )));
            }

            if !tokens_path.exists() {
                return Err(AsrError::ModelNotFound(tokens_path));
            }

            // Create ZipFormer recognizer
            tracing::info!("Creating ZipFormer with config:");
            tracing::info!("  encoder: {:?}", encoder_path);
            tracing::info!("  decoder: {:?}", decoder_path);
            tracing::info!("  joiner: {:?}", joiner_path);
            tracing::info!("  tokens: {:?}", tokens_path);
            tracing::info!("  threads: {}", self.config.threads);

            let zipformer_config = ZipFormerConfig {
                encoder: encoder_path.to_string_lossy().to_string(),
                decoder: decoder_path.to_string_lossy().to_string(),
                joiner: joiner_path.to_string_lossy().to_string(),
                tokens: tokens_path.to_string_lossy().to_string(),
                num_threads: Some(self.config.threads as i32),
                debug: false,
                ..Default::default()
            };

            tracing::info!("Calling ZipFormer::new()...");
            let recognizer = ZipFormer::new(zipformer_config).map_err(|e| {
                tracing::error!("ZipFormer::new() failed: {}", e);
                AsrError::InitializationFailed(format!("Failed to create ZipFormer: {}", e))
            })?;
            tracing::info!("ZipFormer created successfully");

            // Create VAD with silero model
            let vad_model_path = Self::get_vad_model_path()?;
            let vad_config = SileroVadConfig {
                model: vad_model_path.to_string_lossy().to_string(),
                sample_rate: 16000,
                ..Default::default()
            };

            // Buffer size of 30 seconds for VAD
            if vad_model_path.exists() {
                match SileroVad::new(vad_config, 30.0) {
                    Ok(vad) => {
                        self.vad = Some(Arc::new(Mutex::new(vad)));
                        tracing::info!("VAD initialized successfully");
                    }
                    Err(e) => {
                        tracing::warn!("VAD initialization failed, using simple buffering: {}", e);
                        self.vad = None;
                    }
                }
            } else {
                tracing::warn!(
                    "VAD model not found at {:?}, using simple audio buffering",
                    vad_model_path
                );
                self.vad = None;
            }

            self.recognizer = Some(Arc::new(Mutex::new(recognizer)));
            self.initialized = true;

            tracing::info!("Sherpa ASR engine initialized successfully");
            Ok(())
        }

        fn is_valid_asr_model_dir(model_dir: &PathBuf) -> bool {
            if !model_dir.exists() || !model_dir.is_dir() {
                return false;
            }
            if !model_dir.join("tokens.txt").exists() {
                return false;
            }
            let encoder_path = Self::find_model_file(model_dir, "encoder");
            let decoder_path = Self::find_model_file(model_dir, "decoder");
            let joiner_path = Self::find_model_file(model_dir, "joiner");
            encoder_path.exists() && decoder_path.exists() && joiner_path.exists()
        }

        fn resolve_asr_model_dir(base_dir: &PathBuf) -> PathBuf {
            if Self::is_valid_asr_model_dir(base_dir) {
                return base_dir.clone();
            }

            let entries = match std::fs::read_dir(base_dir) {
                Ok(e) => e,
                Err(_) => return base_dir.clone(),
            };

            let mut best: Option<(u64, PathBuf)> = None;
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let pb = path;
                if !Self::is_valid_asr_model_dir(&pb) {
                    continue;
                }
                let size = Self::dir_size(&pb);
                match &best {
                    Some((best_size, _)) if *best_size >= size => {}
                    _ => best = Some((size, pb)),
                }
            }

            best.map(|(_, p)| p).unwrap_or_else(|| base_dir.clone())
        }

        fn dir_size(path: &PathBuf) -> u64 {
            fn inner(p: &std::path::Path) -> u64 {
                let mut total = 0u64;
                let Ok(entries) = std::fs::read_dir(p) else {
                    return 0;
                };
                for entry in entries.flatten() {
                    let ep = entry.path();
                    if let Ok(meta) = entry.metadata() {
                        if meta.is_file() {
                            total += meta.len();
                        } else if meta.is_dir() {
                            total += inner(&ep);
                        }
                    }
                }
                total
            }
            inner(path.as_path())
        }

        /// Find model file with various naming conventions
        fn find_model_file(model_dir: &PathBuf, prefix: &str) -> PathBuf {
            let candidates = vec![
                model_dir.join(format!("{}-epoch-99-avg-1.int8.onnx", prefix)),
                model_dir.join(format!("{}-epoch-99-avg-1.onnx", prefix)),
                model_dir.join(format!("{}.int8.onnx", prefix)),
                model_dir.join(format!("{}.onnx", prefix)),
            ];

            for path in candidates {
                if path.exists() {
                    return path;
                }
            }

            model_dir.join(format!("{}.onnx", prefix))
        }
    }

    impl AsrEngine for SherpaAsrEngine {
        fn init(config: AsrConfig) -> Result<Self, AsrError> {
            let mut engine = Self::new(config)?;
            engine.init_components()?;
            Ok(engine)
        }

        fn transcribe(
            &mut self,
            audio: &[f32],
            sample_rate: u32,
        ) -> Result<Transcription, AsrError> {
            if !self.initialized {
                self.init_components()?;
            }

            let recognizer = self.recognizer.as_ref().ok_or_else(|| {
                AsrError::InitializationFailed("Recognizer not initialized".to_string())
            })?;

            // Resample if needed (sherpa expects 16kHz)
            let samples = if sample_rate != 16000 {
                // For now, just use as-is (proper resampling should be done by audio processor)
                audio.to_vec()
            } else {
                audio.to_vec()
            };

            // Decode using ZipFormer
            let start = Instant::now();
            let mut recognizer = recognizer.lock();
            let text = recognizer.decode(self.sample_rate, samples.clone());
            let processing_time_ms = start.elapsed().as_millis() as u64;

            let duration = audio.len() as f64 / sample_rate as f64;

            if text.is_empty() {
                return Ok(Transcription::default());
            }

            Ok(Transcription {
                text: text.clone(),
                segments: vec![Segment {
                    start: 0.0,
                    end: duration,
                    text,
                    confidence: 0.9,
                    token_confidences: None,
                    words: None,
                }],
                language: self.language.clone().unwrap_or_else(|| "auto".to_string()),
                confidence: 0.9,
                processing_time_ms,
                is_partial: false,
            })
        }

        fn transcribe_streaming(
            &mut self,
            audio: &[f32],
        ) -> Result<Option<PartialTranscription>, AsrError> {
            // The higher-level StreamingAsrEngine handles buffering + VAD.
            // Returning None here avoids double-buffering and optional VAD model
            // dependencies (silero_vad.onnx) from breaking partial results.
            let _ = audio;
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

        fn reset(&mut self) {
            self.audio_buffer.clear();
            if let Some(vad) = &self.vad {
                vad.lock().clear();
            }
        }

        fn is_gpu_enabled(&self) -> bool {
            false
        }

        fn get_model_info(&self) -> ModelInfo {
            ModelInfo {
                model_size: format!("{:?}", self.config.model_size),
                language_count: 99,
                is_multilingual: true,
                is_english_only: false,
                gpu_enabled: false,
                memory_usage_mb: self.config.estimated_memory_mb(),
            }
        }
    }
}

#[cfg(not(feature = "sherpa-backend"))]
mod sherpa_impl {
    use super::*;

    /// Mock Sherpa ASR Engine for development without sherpa-rs
    pub struct SherpaAsrEngine {
        language: Option<String>,
    }

    impl SherpaAsrEngine {
        pub fn new(config: AsrConfig) -> Result<Self, AsrError> {
            tracing::info!("Mock Sherpa ASR initialized");
            let _ = config;
            Ok(Self {
                language: None,
            })
        }
    }

    impl AsrEngine for SherpaAsrEngine {
        fn init(config: AsrConfig) -> Result<Self, AsrError> {
            Self::new(config)
        }

        fn transcribe(
            &mut self,
            audio: &[f32],
            _sample_rate: u32,
        ) -> Result<Transcription, AsrError> {
            let energy: f32 = if audio.is_empty() {
                0.0
            } else {
                (audio.iter().map(|x| x * x).sum::<f32>() / audio.len() as f32).sqrt()
            };

            let text = if energy > 0.01 {
                match self.language.as_deref() {
                    Some("zh") => "[检测到语音内容 - Mock Sherpa]".to_string(),
                    Some("ja") => "[音声が検出されました]".to_string(),
                    Some("ko") => "[음성이 감지되었습니다]".to_string(),
                    _ => "[Speech detected - Mock Sherpa ASR]".to_string(),
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

        fn transcribe_streaming(
            &mut self,
            _audio: &[f32],
        ) -> Result<Option<PartialTranscription>, AsrError> {
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
            ]
        }

        fn reset(&mut self) {}

        fn is_gpu_enabled(&self) -> bool {
            false
        }

        fn get_model_info(&self) -> ModelInfo {
            ModelInfo {
                model_size: "mock".to_string(),
                language_count: 4,
                is_multilingual: true,
                is_english_only: false,
                gpu_enabled: false,
                memory_usage_mb: 100,
            }
        }
    }
}

pub use sherpa_impl::SherpaAsrEngine;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sherpa_engine_new() {
        let config = AsrConfig::default();
        let engine = SherpaAsrEngine::new(config).unwrap();
        assert!(engine.get_supported_languages().contains(&"en".to_string()));
    }

    #[test]
    fn test_set_language() {
        let config = AsrConfig::default();
        let mut engine = SherpaAsrEngine::new(config).unwrap();
        engine.set_language("zh");
        assert_eq!(engine.get_language(), Some("zh"));
    }
}
