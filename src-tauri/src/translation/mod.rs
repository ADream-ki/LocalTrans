mod loci_bridge;
mod nllb;

pub use loci_bridge::LociTranslator;
pub use nllb::NllbTranslator;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub confidence: f32,
}

pub trait Translator: Send + Sync {
    fn init(model_path: &Path) -> Result<Self> where Self: Sized;
    fn translate(&mut self, text: &str, source_lang: &str, target_lang: &str) -> Result<TranslationResult>;
    fn get_supported_languages(&self) -> Vec<(String, String)>;
}
