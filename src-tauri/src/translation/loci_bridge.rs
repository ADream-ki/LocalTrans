#![allow(dead_code)]
use super::{TranslationResult, Translator};
use anyhow::Result;
use std::path::Path;

#[cfg(feature = "loci-backend")]
use anyhow::Context;

#[cfg(feature = "loci-backend")]
use loci::inference::{GenerationParams, InferenceEngine};

/// Loci-based translator that uses local LLM for translation
pub struct LociTranslator {
    #[cfg(feature = "loci-backend")]
    engine: Option<InferenceEngine>,
    model_path: Option<String>,
    initialized: bool,
}

impl LociTranslator {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "loci-backend")]
            engine: None,
            model_path: None,
            initialized: false,
        }
    }

    fn build_translation_prompt(text: &str, source_lang: &str, target_lang: &str) -> String {
        format!(
            "Translate the following text from {} to {}. Only output the translation, nothing else.\n\nText: {}\n\nTranslation:",
            source_lang, target_lang, text
        )
    }

    fn clean_translation_output(raw: &str) -> String {
        let trimmed = raw.trim();
        let first_line = trimmed.lines().next().unwrap_or("").trim();
        first_line
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string()
    }
}

impl Translator for LociTranslator {
    fn init(model_path: &Path) -> Result<Self> {
        #[cfg(feature = "loci-backend")]
        {
            tracing::info!("Initializing Loci translator with model: {:?}", model_path);

            if !model_path.exists() {
                anyhow::bail!("Loci model not found: {}", model_path.display());
            }

            let engine = InferenceEngine::builder()
                .model_path(model_path.to_str().context("Invalid model path")?)
                .build()
                .context("Failed to initialize Loci engine")?;

            tracing::info!("Loci engine initialized successfully");

            return Ok(Self {
                engine: Some(engine),
                model_path: Some(model_path.to_string_lossy().to_string()),
                initialized: true,
            });
        }

        #[cfg(not(feature = "loci-backend"))]
        {
            let _ = model_path;
            anyhow::bail!("loci-backend feature is not enabled")
        }
    }

    fn translate(
        &mut self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResult> {
        #[cfg(feature = "loci-backend")]
        {
            if let Some(ref mut engine) = self.engine {
                let prompt = Self::build_translation_prompt(text, source_lang, target_lang);

                let params = GenerationParams {
                    max_tokens: 512,
                    temperature: 0.3,
                    top_p: 0.9,
                    ..Default::default()
                };

                match engine.generate(&prompt, params) {
                    Ok(result) => {
                        let translated = Self::clean_translation_output(&result);
                        tracing::debug!("Translation: '{}' -> '{}'", text, translated);

                        return Ok(TranslationResult {
                            text: translated,
                            source_lang: source_lang.to_string(),
                            target_lang: target_lang.to_string(),
                            confidence: 0.95,
                        });
                    }
                    Err(e) => {
                        tracing::error!("Translation failed: {}", e);
                        anyhow::bail!("Loci translation failed: {}", e);
                    }
                }
            }
        }

        #[cfg(not(feature = "loci-backend"))]
        let _ = text;

        anyhow::bail!(
            "Loci translator not initialized (missing model). source={} target={}",
            source_lang,
            target_lang
        )
    }

    fn get_supported_languages(&self) -> Vec<(String, String)> {
        vec![
            ("en".to_string(), "English".to_string()),
            ("zh".to_string(), "Chinese".to_string()),
            ("ja".to_string(), "Japanese".to_string()),
            ("ko".to_string(), "Korean".to_string()),
            ("fr".to_string(), "French".to_string()),
            ("de".to_string(), "German".to_string()),
            ("es".to_string(), "Spanish".to_string()),
            ("ru".to_string(), "Russian".to_string()),
        ]
    }
}

impl Default for LociTranslator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt() {
        let prompt = LociTranslator::build_translation_prompt("Hello", "English", "Chinese");
        assert!(prompt.contains("Hello"));
        assert!(prompt.contains("English"));
        assert!(prompt.contains("Chinese"));
    }
}

