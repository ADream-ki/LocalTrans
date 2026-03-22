#![allow(dead_code)]
use super::{TranslationResult, Translator};
use anyhow::Result;
use std::path::Path;
use std::time::{Duration, Instant};

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
    consecutive_failures: u32,
    circuit_open_until: Option<Instant>,
}

impl LociTranslator {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "loci-backend")]
            engine: None,
            model_path: None,
            initialized: false,
            consecutive_failures: 0,
            circuit_open_until: None,
        }
    }

    fn on_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= 3 {
            self.circuit_open_until = Some(Instant::now() + Duration::from_secs(300));
            self.consecutive_failures = 0;
        }
    }

    fn on_success(&mut self) {
        self.consecutive_failures = 0;
        self.circuit_open_until = None;
    }

    fn normalize_lang_name(lang: &str) -> &str {
        match lang.to_ascii_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" => "Chinese (Simplified)",
            "zh-tw" | "zh-hant" => "Chinese (Traditional)",
            "en" | "en-us" | "en-gb" => "English",
            "ja" => "Japanese",
            "ko" => "Korean",
            "fr" => "French",
            "de" => "German",
            "es" => "Spanish",
            "ru" => "Russian",
            _ => "the target language",
        }
    }

    fn build_translation_prompt(text: &str, source_lang: &str, target_lang: &str) -> String {
        let src = Self::normalize_lang_name(source_lang);
        let tgt = Self::normalize_lang_name(target_lang);
        format!(
            "<|im_start|>system\nYou are a professional translation engine.\nReturn translation text only.\n<|im_end|>\n<|im_start|>user\nTranslate from {src} to {tgt}.\nRules:\n1) Output only the final translation.\n2) No explanations.\n3) Preserve names and numbers.\nText:\n{text}\n<|im_end|>\n<|im_start|>assistant\n"
        )
    }

    fn clean_translation_output(raw: &str) -> String {
        let mut cleaned = raw.trim().to_string();
        for prefix in [
            "TRANSLATION:",
            "Translation:",
            "译文：",
            "译文:",
            "答案：",
            "Answer:",
        ] {
            if let Some(rest) = cleaned.strip_prefix(prefix) {
                cleaned = rest.trim().to_string();
            }
        }
        if cleaned.starts_with("```") {
            cleaned = cleaned
                .trim_start_matches('`')
                .trim()
                .trim_end_matches('`')
                .trim()
                .to_string();
        }
        let first_line = cleaned.lines().next().unwrap_or("").trim();
        first_line
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string()
    }

    fn looks_corrupted(text: &str, target_lang: &str) -> bool {
        let t = text.trim();
        if t.is_empty() {
            return true;
        }
        if t.len() < 2 {
            return true;
        }
        let noisy = ['{', '}', '|', '<', '>', '/', '\\', '*', '_'];
        let chars: Vec<char> = t.chars().collect();
        let n = chars.len().max(1);
        let noisy_count = chars.iter().filter(|c| noisy.contains(c)).count();
        if noisy_count * 4 > n {
            return true;
        }

        let target = target_lang.to_ascii_lowercase();
        let cjk_count = chars
            .iter()
            .filter(|c| {
                let u = **c as u32;
                (0x4E00..=0x9FFF).contains(&u)
                    || (0x3400..=0x4DBF).contains(&u)
                    || (0x3000..=0x303F).contains(&u)
            })
            .count();
        let alpha_count = chars.iter().filter(|c| c.is_ascii_alphabetic()).count();
        let punct_count = chars
            .iter()
            .filter(|c| c.is_ascii_punctuation() && !",.!?;:'\"()[]-".contains(**c))
            .count();

        if matches!(target.as_str(), "zh" | "zh-cn" | "zh-hans" | "zh-tw" | "zh-hant")
            && cjk_count * 5 < n
        {
            return true;
        }
        if target.starts_with("en") && alpha_count * 2 < n {
            return true;
        }
        punct_count * 5 > n
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

            Ok(Self {
                engine: Some(engine),
                model_path: Some(model_path.to_string_lossy().to_string()),
                initialized: true,
                consecutive_failures: 0,
                circuit_open_until: None,
            })
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
            if let Some(until) = self.circuit_open_until {
                if Instant::now() < until {
                    anyhow::bail!("Loci circuit breaker is open due to repeated failures");
                }
            }
            if let Some(ref mut engine) = self.engine {
                let prompt = Self::build_translation_prompt(text, source_lang, target_lang);

                let mut params = GenerationParams {
                    max_tokens: 256,
                    temperature: 0.2,
                    top_p: 0.85,
                    top_k: 20,
                    repeat_penalty: 1.02,
                    ..Default::default()
                };

                for attempt in 0..2 {
                    match engine.generate(&prompt, params.clone()) {
                        Ok(result) => {
                            let translated = Self::clean_translation_output(&result);
                            if Self::looks_corrupted(&translated, target_lang) && attempt == 0 {
                                params.temperature = 0.05;
                                params.top_p = 0.4;
                                params.top_k = 8;
                                continue;
                            }
                            if Self::looks_corrupted(&translated, target_lang) {
                                self.on_failure();
                                anyhow::bail!("Loci translation output looks corrupted");
                            }
                            self.on_success();
                            tracing::debug!("Translation: '{}' -> '{}'", text, translated);
                            return Ok(TranslationResult {
                                text: translated,
                                source_lang: source_lang.to_string(),
                                target_lang: target_lang.to_string(),
                                confidence: if attempt == 0 { 0.9 } else { 0.75 },
                            });
                        }
                        Err(e) => {
                            if attempt == 0 {
                                params.temperature = 0.05;
                                params.top_p = 0.4;
                                params.top_k = 8;
                                tracing::warn!("Loci translate attempt 1 failed, retrying: {}", e);
                                continue;
                            }
                            self.on_failure();
                            tracing::error!("Translation failed: {}", e);
                            anyhow::bail!("Loci translation failed: {}", e);
                        }
                    }
                }
                anyhow::bail!("Loci translation returned no valid output");
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
        assert!(prompt.contains("professional translation engine"));
    }

    #[test]
    fn test_clean_output() {
        let out = LociTranslator::clean_translation_output("Translation: 你好世界");
        assert_eq!(out, "你好世界");
    }

    #[test]
    fn test_corrupted_detection() {
        assert!(LociTranslator::looks_corrupted("{|<__*/", "zh"));
        assert!(!LociTranslator::looks_corrupted("你好，世界", "zh"));
        assert!(LociTranslator::looks_corrupted("abc{||//__", "en"));
    }
}

