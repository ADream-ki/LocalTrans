//! Piper TTS (VITS) local inference engine
//!
//! Uses sherpa-rs VitsTts for offline neural TTS synthesis.

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::{TtsAudio, TtsConfig, TtsEngine, VoiceInfo};

/// Piper TTS model info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiperModelInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub quality: String,
    pub model_path: PathBuf,
    pub json_path: PathBuf,
}

/// Piper TTS engine for offline neural speech synthesis
pub struct PiperTtsEngine {
    pub config: TtsConfig,
    model_infos: Vec<PiperModelInfo>,
    selected_model: Option<String>,
}

impl PiperTtsEngine {
    /// Create a new Piper TTS engine
    pub fn new() -> Self {
        Self {
            config: TtsConfig::default(),
            model_infos: Vec::new(),
            selected_model: None,
        }
    }

    /// Create with configuration
    pub fn with_config(config: TtsConfig) -> Self {
        Self {
            config,
            model_infos: Vec::new(),
            selected_model: None,
        }
    }

    fn models_root() -> Result<PathBuf> {
        let base =
            dirs::data_local_dir().ok_or_else(|| anyhow!("Cannot determine local data directory"))?;
        Ok(base.join("LocalTrans").join("models").join("tts").join("piper"))
    }

    fn detect_language(name: &str) -> String {
        if name.starts_with("zh_") || name.starts_with("zh-") || name.starts_with("zh") {
            return "zh".to_string();
        }
        if name.starts_with("en_") || name.starts_with("en-") || name.starts_with("en") {
            return "en".to_string();
        }
        "unknown".to_string()
    }

    fn detect_quality(name: &str) -> String {
        for q in ["x_low", "low", "medium", "high", "x_high"] {
            if name.contains(q) {
                return q.to_string();
            }
        }
        "medium".to_string()
    }

    #[cfg(feature = "sherpa-backend")]
    fn build_vits_config(model: &PiperModelInfo, model_root: &Path) -> sherpa_rs::tts::VitsTtsConfig {
        use sherpa_rs::{tts::VitsTtsConfig, OnnxConfig};

        let tokens_in_model_dir = model.model_path.with_file_name("tokens.txt");
        let tokens_in_root = model_root.join("tokens.txt");
        let tokens_path = if tokens_in_model_dir.exists() {
            tokens_in_model_dir
        } else {
            tokens_in_root
        };

        let lexicon_in_model_dir = model.model_path.with_file_name("lexicon.txt");
        let lexicon_path = if lexicon_in_model_dir.exists() {
            lexicon_in_model_dir
        } else {
            PathBuf::new()
        };

        let dict_in_model_dir = model.model_path.with_file_name("dict");
        let melo_dict = model_root
            .parent()
            .map(|p| p.join("sherpa").join("vits-melo-tts-zh_en").join("dict"))
            .unwrap_or_default();
        let espeak_data = model_root.join("espeak-ng-data");
        let dict_dir = if dict_in_model_dir.exists() {
            dict_in_model_dir
        } else if melo_dict.exists() {
            melo_dict
        } else if espeak_data.exists() {
            espeak_data
        } else {
            PathBuf::new()
        };

        VitsTtsConfig {
            model: model.model_path.to_string_lossy().to_string(),
            tokens: tokens_path.to_string_lossy().to_string(),
            lexicon: if lexicon_path.as_os_str().is_empty() {
                String::new()
            } else {
                lexicon_path.to_string_lossy().to_string()
            },
            dict_dir: if dict_dir.as_os_str().is_empty() {
                String::new()
            } else {
                dict_dir.to_string_lossy().to_string()
            },
            onnx_config: OnnxConfig {
                provider: "cpu".to_string(),
                num_threads: 2,
                debug: false,
            },
            ..Default::default()
        }
    }

    /// Scan for Piper models in the models directory
    pub fn scan_models(&mut self) -> Result<()> {
        let root = Self::models_root()?;
        if !root.exists() {
            self.model_infos.clear();
            self.selected_model = None;
            return Ok(());
        }

        let mut models = Vec::new();
        let entries = std::fs::read_dir(&root)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("onnx") {
                continue;
            }

            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            let json_path = path.with_extension("onnx.json");
            models.push(PiperModelInfo {
                id: stem.clone(),
                name: stem.clone(),
                language: Self::detect_language(&stem),
                quality: Self::detect_quality(&stem),
                model_path: path,
                json_path,
            });
        }

        models.sort_by(|a, b| a.name.cmp(&b.name));
        self.model_infos = models;

        if self.selected_model.is_none() {
            self.selected_model = self
                .model_infos
                .iter()
                .find(|m| m.language == "zh")
                .map(|m| m.id.clone())
                .or_else(|| self.model_infos.first().map(|m| m.id.clone()));
        }

        Ok(())
    }

    /// Load a specific model
    pub fn load_model(&mut self, model_id: &str) -> Result<()> {
        if self.model_infos.is_empty() {
            self.scan_models()?;
        }

        if self.model_infos.iter().any(|m| m.id == model_id) {
            self.selected_model = Some(model_id.to_string());
            return Ok(());
        }

        bail!("Piper model not found: {}", model_id);
    }

    /// Synthesize text to audio using a specific model
    pub fn synthesize_with_model(&self, text: &str, model_id: &str, speed: f32) -> Result<TtsAudio> {
        #[cfg(not(feature = "sherpa-backend"))]
        {
            let _ = (text, model_id, speed);
            bail!("Piper TTS requires sherpa-backend feature to be enabled");
        }

        #[cfg(feature = "sherpa-backend")]
        {
            use sherpa_rs::tts::VitsTts;

            let model = self
                .model_infos
                .iter()
                .find(|m| m.id == model_id)
                .ok_or_else(|| anyhow!("Piper model not found: {}", model_id))?;

            let root = Self::models_root()?;
            let config = Self::build_vits_config(model, &root);
            let mut tts = VitsTts::new(config);

            let audio = tts
                .create(text, 0, speed.clamp(0.5, 2.0))
                .map_err(|e| anyhow!("Piper synthesis failed: {}", e))?;

            let duration_secs = if audio.sample_rate == 0 {
                0.0
            } else {
                audio.samples.len() as f32 / audio.sample_rate as f32
            };

            Ok(TtsAudio {
                samples: audio.samples,
                sample_rate: audio.sample_rate,
                channels: 1,
                duration_secs,
            })
        }
    }

    /// Get available model infos
    pub fn get_model_infos(&self) -> &[PiperModelInfo] {
        &self.model_infos
    }

    /// Get default model ID
    pub fn get_default_model(&self) -> Option<&str> {
        self.selected_model.as_deref().or_else(|| self.model_infos.first().map(|m| m.id.as_str()))
    }
}

#[async_trait]
impl TtsEngine for PiperTtsEngine {
    fn name(&self) -> &str {
        "Piper TTS"
    }

    fn is_ready(&self) -> bool {
        !self.model_infos.is_empty()
    }

    fn get_voices(&self) -> Vec<VoiceInfo> {
        self.model_infos
            .iter()
            .map(|m| VoiceInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                language: m.language.clone(),
                gender: "neutral".to_string(),
                style: Some(m.quality.clone()),
                is_custom: true,
                model_path: Some(m.model_path.to_string_lossy().to_string()),
            })
            .collect()
    }

    async fn synthesize(&self, text: &str, voice: &str) -> Result<TtsAudio> {
        self.synthesize_with_model(text, voice, 1.0)
    }

    fn default_voice_for_lang(&self, _lang: &str) -> Option<&str> {
        self.get_default_model()
    }
}

impl Default for PiperTtsEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piper_engine_new() {
        let engine = PiperTtsEngine::new();
        assert_eq!(engine.name(), "Piper TTS");
    }
}
