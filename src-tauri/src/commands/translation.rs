//! Translation commands for diagnostics and tooling

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex as SyncMutex;
use tauri::State;

use crate::translation::{LociTranslator, NllbTranslator, Translator, TranslationResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
    /// Engine name: "loci" (default)
    pub engine: Option<String>,
    /// Optional model path override (for Loci: path to .gguf)
    pub model_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResponse {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub confidence: f32,
}

#[derive(Default)]
pub struct TranslationService {
    loci: Option<LociTranslator>,
    loci_model_path: Option<PathBuf>,
    nllb: NllbTranslator,
}

impl TranslationService {
    pub fn translate(&mut self, request: &TranslateRequest) -> anyhow::Result<TranslationResult> {
        let engine = request.engine.as_deref().unwrap_or("loci");
        match engine {
            "loci" => self.translate_loci(request),
            "nllb" => self.nllb.translate(&request.text, &request.source_lang, &request.target_lang),
            other => anyhow::bail!("Unsupported translation engine: {}", other),
        }
    }

    fn translate_loci(&mut self, request: &TranslateRequest) -> anyhow::Result<TranslationResult> {
        let model_path = if let Some(p) = request.model_path.as_deref() {
            Some(PathBuf::from(p))
        } else {
            find_default_loci_model()
        };

        let Some(model_path) = model_path else {
            tracing::warn!(
                "No Loci model found under {}. Falling back to built-in translator.",
                default_loci_dir().display()
            );
            return self.nllb.translate(&request.text, &request.source_lang, &request.target_lang);
        };

        let need_reload = match self.loci_model_path.as_ref() {
            Some(existing) => existing != &model_path,
            None => true,
        };

        if need_reload {
            let translator = LociTranslator::init(&model_path)?;
            self.loci = Some(translator);
            self.loci_model_path = Some(model_path);
        }

        match self.loci.as_mut() {
            Some(translator) => match translator.translate(&request.text, &request.source_lang, &request.target_lang) {
                Ok(v) => Ok(v),
                Err(e) => {
                    tracing::warn!("Loci translation failed, fallback to NLLB: {}", e);
                    self.nllb.translate(&request.text, &request.source_lang, &request.target_lang)
                }
            },
            None => self.nllb.translate(&request.text, &request.source_lang, &request.target_lang),
        }
    }
}

#[tauri::command]
pub async fn translate_text(
    request: TranslateRequest,
    service: State<'_, Arc<SyncMutex<TranslationService>>>,
) -> Result<TranslateResponse, String> {
    let service = service.inner().clone();

    tokio::task::spawn_blocking(move || {
        let mut svc = service.lock();
        let result = svc.translate(&request).map_err(|e| e.to_string())?;

        Ok(TranslateResponse {
            text: result.text,
            source_lang: result.source_lang,
            target_lang: result.target_lang,
            confidence: result.confidence,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn default_loci_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
        .join("loci")
}

fn find_default_loci_model() -> Option<PathBuf> {
    let dir = default_loci_dir();
    let entries = std::fs::read_dir(&dir).ok()?;

    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gguf"))
            != Some(true)
        {
            continue;
        }

        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
        match &best {
            Some((best_size, _)) if *best_size >= size => {}
            _ => best = Some((size, path)),
        }
    }

    best.map(|(_, p)| p)
}
