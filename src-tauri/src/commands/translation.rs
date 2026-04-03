use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::translation::{LociTranslator, NllbTranslator, Translator, TranslationResult};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub engine: Option<String>,
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
struct TranslationService {
    loci: Option<LociTranslator>,
    loci_model_path: Option<PathBuf>,
    nllb: NllbTranslator,
}

impl TranslationService {
    fn translate(&mut self, request: &TranslateRequest) -> anyhow::Result<TranslationResult> {
        let engine = resolved_translation_engine(request.engine.as_deref());
        match engine.as_str() {
            "loci" => self.translate_loci(request),
            "nllb" => self
                .nllb
                .translate(&request.text, &request.source_lang, &request.target_lang),
            other => anyhow::bail!("Unsupported translation engine: {}", other),
        }
    }

    fn translate_loci(&mut self, request: &TranslateRequest) -> anyhow::Result<TranslationResult> {
        let model_path = resolve_loci_model_path(request.model_path.as_deref());

        let Some(model_path) = model_path else {
            anyhow::bail!(
                "Loci model not found under {}",
                default_loci_dir().display()
            );
        };

        let need_reload = match self.loci_model_path.as_ref() {
            Some(existing) => existing != &model_path,
            None => true,
        };

        if need_reload {
            let translator = LociTranslator::init(&model_path).map_err(|e| {
                anyhow::anyhow!(
                    "failed to initialize Loci model ({}): {}",
                    model_path.display(),
                    e
                )
            })?;
            self.loci = Some(translator);
            self.loci_model_path = Some(model_path);
        }

        match self.loci.as_mut() {
            Some(translator) => translator.translate(
                &request.text,
                &request.source_lang,
                &request.target_lang,
            ),
            None => anyhow::bail!("Loci translator not initialized"),
        }
    }
}

fn service() -> &'static Mutex<TranslationService> {
    static SERVICE: OnceLock<Mutex<TranslationService>> = OnceLock::new();
    SERVICE.get_or_init(|| Mutex::new(TranslationService::default()))
}

#[tauri::command]
pub fn translate_text(request: TranslateRequest) -> AppResult<TranslateResponse> {
    let mut svc = service()
        .lock()
        .map_err(|_| AppError::InvalidState("translation service lock poisoned".to_string()))?;
    let result = svc
        .translate(&request)
        .map_err(|e| AppError::InvalidState(e.to_string()))?;
    super::session::note_translation();
    Ok(TranslateResponse {
        text: result.text,
        source_lang: result.source_lang,
        target_lang: result.target_lang,
        confidence: result.confidence,
    })
}

pub fn translate_text_cli(
    text: String,
    source_lang: String,
    target_lang: String,
    engine: Option<String>,
    model_path: Option<String>,
) -> AppResult<TranslateResponse> {
    translate_text(TranslateRequest {
        text,
        source_lang,
        target_lang,
        engine,
        model_path,
    })
}

pub(crate) fn resolved_translation_engine(requested: Option<&str>) -> String {
    let selected = requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| super::config::get_string("translationEngine"))
        .unwrap_or_else(|| "nllb".to_string());

    match selected.to_ascii_lowercase().as_str() {
        "loci" => "loci".to_string(),
        "nllb" | "argos" | "mt" | "m2m" => "nllb".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn configured_loci_model_path() -> Option<PathBuf> {
    super::config::get_string("lociModelPath").map(PathBuf::from)
}

pub(crate) fn default_loci_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
        .join("loci")
}

pub(crate) fn find_default_loci_model() -> Option<PathBuf> {
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

fn resolve_loci_model_path(requested: Option<&str>) -> Option<PathBuf> {
    requested
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(configured_loci_model_path)
        .or_else(find_default_loci_model)
}
