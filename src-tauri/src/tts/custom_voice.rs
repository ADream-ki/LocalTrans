#![allow(dead_code)]
//! Custom Voice Support
//! 
//! Supports custom voice cloning engines:
//! - GPT-SoVITS: Few-shot voice cloning
//! - RVC (Retrieval-based Voice Conversion): Real-time voice conversion
//! - Piper: Fast local TTS

use anyhow::{Result, Context};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::{TtsEngine, TtsAudio, VoiceInfo, CustomVoiceConfig, CustomVoiceType};

/// Custom voice engine for voice cloning
pub struct CustomVoiceEngine {
    config: CustomVoiceConfig,
    cache: HashMap<String, TtsAudio>,
}

impl CustomVoiceEngine {
    /// Create a new custom voice engine
    pub fn new(config: CustomVoiceConfig) -> Result<Self> {
        // Validate model exists
        if !config.model_path.exists() {
            anyhow::bail!("Model file not found: {:?}", config.model_path);
        }

        Ok(Self {
            config,
            cache: HashMap::new(),
        })
    }

    /// List available custom voices from a directory
    pub fn list_custom_voices(models_dir: &Path) -> Result<Vec<CustomVoiceModel>> {
        if !models_dir.exists() {
            return Ok(Vec::new());
        }

        let mut models = Vec::new();

        for entry in std::fs::read_dir(models_dir)? {
            let entry = entry?;
            let path = entry.path();

            // Check for model files
            if let Some(ext) = path.extension() {
                match ext.to_str() {
                    Some("pth") | Some("pt") | Some("onnx") => {
                        let name = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        let model_type = detect_model_type(&path);

                        models.push(CustomVoiceModel {
                            name,
                            path: path.clone(),
                            model_type,
                        });
                    }
                    _ => {}
                }
            }
        }

        Ok(models)
    }
}

#[async_trait]
impl TtsEngine for CustomVoiceEngine {
    fn name(&self) -> &str {
        match self.config.model_type {
            CustomVoiceType::GptSoVits => "gpt-sovits",
            CustomVoiceType::Rvc => "rvc",
            CustomVoiceType::Piper => "piper",
            CustomVoiceType::Vits => "vits",
        }
    }

    fn is_ready(&self) -> bool {
        self.config.model_path.exists()
    }

    fn get_voices(&self) -> Vec<VoiceInfo> {
        vec![VoiceInfo {
            id: "custom".to_string(),
            name: format!("Custom Voice ({:?})", self.config.model_type),
            language: "multi".to_string(),
            gender: "custom".to_string(),
            style: None,
            is_custom: true,
            model_path: Some(self.config.model_path.to_string_lossy().to_string()),
        }]
    }

    async fn synthesize(&self, text: &str, _voice: &str) -> Result<TtsAudio> {
        match self.config.model_type {
            CustomVoiceType::GptSoVits => self.synthesize_gpt_sovits(text).await,
            CustomVoiceType::Rvc => self.synthesize_rvc(text).await,
            CustomVoiceType::Piper => self.synthesize_piper(text).await,
            CustomVoiceType::Vits => self.synthesize_vits(text).await,
        }
    }

    fn default_voice_for_lang(&self, _lang: &str) -> Option<&str> {
        Some("custom")
    }
}

impl CustomVoiceEngine {
    /// Synthesize using GPT-SoVITS
    async fn synthesize_gpt_sovits(&self, text: &str) -> Result<TtsAudio> {
        // GPT-SoVITS typically runs as a web API
        // For local inference, we can call the API or use Python bindings
        
        let api_url = std::env::var("GPT_SOVITS_API")
            .unwrap_or_else(|_| "http://127.0.0.1:9880".to_string());

        let client = reqwest::Client::new();
        
        let response = client
            .post(format!("{}/tts", api_url))
            .json(&serde_json::json!({
                "text": text,
                "text_lang": "zh",
                "ref_audio_path": self.config.reference_audio.as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                "prompt_text": self.config.reference_text.as_ref()
                    .unwrap_or(&String::new()),
                "prompt_lang": "zh",
                "top_k": 5,
                "top_p": 1.0,
                "temperature": 1.0,
            }))
            .send()
            .await
            .context("Failed to connect to GPT-SoVITS API")?;

        if !response.status().is_success() {
            anyhow::bail!("GPT-SoVITS API error: {}", response.status());
        }

        // Get audio data (WAV format)
        let audio_data = response.bytes().await?;
        let samples = decode_wav_to_f32(&audio_data)?;

        let duration_secs = samples.len() as f32 / 32000.0; // GPT-SoVITS typically uses 32kHz

        Ok(TtsAudio {
            samples,
            sample_rate: 32000,
            channels: 1,
            duration_secs,
        })
    }

    /// Synthesize using RVC (Real-time Voice Conversion)
    async fn synthesize_rvc(&self, text: &str) -> Result<TtsAudio> {
        // RVC requires a base TTS output to convert
        // First, generate base audio using Edge TTS
        let edge_engine = crate::tts::edge_tts::EdgeTtsEngine::new()?;
        let base_audio = edge_engine.synthesize(text, "zh-CN-XiaoxiaoNeural").await?;

        // Then apply RVC voice conversion
        // RVC typically runs as a local API
        let api_url = std::env::var("RVC_API")
            .unwrap_or_else(|_| "http://127.0.0.1:7865".to_string());

        let client = reqwest::Client::new();

        // Convert f32 samples to base64 for API
        let samples_bytes: Vec<u8> = base_audio.samples.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        let response = client
            .post(format!("{}/voice_conversion", api_url))
            .json(&serde_json::json!({
                "input_audio": BASE64.encode(&samples_bytes),
                "model_path": self.config.model_path.to_string_lossy(),
                "f0_up_key": 0,
                "f0_method": "pm",
                "index_rate": 0.75,
                "filter_radius": 3,
                "resample_sr": 0,
                "rms_mix_rate": 0.25,
                "protect": 0.33,
            }))
            .send()
            .await
            .context("Failed to connect to RVC API")?;

        if !response.status().is_success() {
            // Fall back to base audio if RVC fails
            tracing::warn!("RVC API error: {}, using base audio", response.status());
            return Ok(base_audio);
        }

        let result: RvcResponse = response.json().await?;
        let converted_samples = decode_base64_audio(&result.audio)?;

        Ok(TtsAudio {
            samples: converted_samples,
            sample_rate: base_audio.sample_rate,
            channels: base_audio.channels,
            duration_secs: base_audio.duration_secs,
        })
    }

    /// Synthesize using Piper (via sherpa-rs)
    async fn synthesize_piper(&self, text: &str) -> Result<TtsAudio> {
        // Use sherpa-rs PiperTtsEngine for local inference
        use super::piper_tts::PiperTtsEngine;
        use super::TtsEngine;
        
        let mut engine = PiperTtsEngine::new();
        engine.scan_models()?;
        
        // Get default model for the language or first available
        let model_id = engine
            .get_default_model()
            .ok_or_else(|| anyhow::anyhow!("No Piper TTS models found"))?
            .to_string();
        
        // Load the model
        engine.load_model(&model_id)?;
        
        // Synthesize
        engine.synthesize(text, &model_id).await
    }

    /// Synthesize using VITS
    async fn synthesize_vits(&self, text: &str) -> Result<TtsAudio> {
        // Similar to Piper, VITS models can be run via ONNX Runtime
        // For simplicity, fall back to Piper implementation
        self.synthesize_piper(text).await
    }
}

/// Custom voice model info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomVoiceModel {
    pub name: String,
    pub path: PathBuf,
    pub model_type: CustomVoiceType,
}

/// RVC API response
#[derive(Debug, Deserialize)]
struct RvcResponse {
    audio: String,
}

/// Detect model type from file
fn detect_model_type(path: &Path) -> CustomVoiceType {
    let name = path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    if name.contains("gpt") || name.contains("sovits") {
        CustomVoiceType::GptSoVits
    } else if name.contains("rvc") {
        CustomVoiceType::Rvc
    } else if name.contains("piper") || name.contains("onnx") {
        CustomVoiceType::Piper
    } else {
        CustomVoiceType::Vits
    }
}

/// Decode WAV audio to f32 samples
fn decode_wav_to_f32(data: &[u8]) -> Result<Vec<f32>> {
    use hound::WavReader;
    
    let cursor = std::io::Cursor::new(data);
    let reader = WavReader::new(cursor)?;
    
    let samples: Vec<f32> = reader.into_samples::<i16>()
        .filter_map(|s| s.ok())
        .map(|s| s as f32 / 32768.0)
        .collect();

    Ok(samples)
}

/// Decode base64 encoded audio
fn decode_base64_audio(data: &str) -> Result<Vec<f32>> {
    let bytes = BASE64.decode(data)?;
    decode_wav_to_f32(&bytes)
}

