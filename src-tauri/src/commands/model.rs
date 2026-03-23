use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};

const URL_SHERPA_MODELS: &str = "https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html";
const URL_SHERPA_TTS: &str = "https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/index.html";
const URL_LOCI_GGUF: &str = "https://huggingface.co/models?library=gguf&sort=downloads";
const URL_PIPER: &str = "https://github.com/rhasspy/piper";
const URL_NLLB: &str = "https://huggingface.co/facebook/nllb-200-distilled-600M";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub model_type: String,
    pub size: String,
    pub status: String,
    pub path: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub model_type: String,
    pub progress: u8,
    pub status: String,
}

pub fn models_dir() -> AppResult<PathBuf> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| AppError::InvalidState("Cannot determine data directory".to_string()))?;
    let dir = base.join("LocalTrans").join("models");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

pub fn has_ready_model(model_type: &str) -> AppResult<bool> {
    let models = list_models(model_type.to_string())?;
    Ok(models.iter().any(|m| m.status == "ready"))
}

#[tauri::command]
pub fn get_models_dir() -> AppResult<String> {
    Ok(models_dir()?.to_string_lossy().to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub fn list_models(model_type: String) -> AppResult<Vec<ModelInfo>> {
    let root = models_dir()?;
    let mut models = Vec::new();
    models.extend(list_asr_models(&root));
    models.extend(list_loci_models(&root));
    models.extend(list_tts_models(&root));
    models.extend(list_mt_models(&root));
    Ok(models
        .into_iter()
        .filter(|m| m.model_type == model_type)
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub fn download_model(app: AppHandle, model_id: String, model_type: String) -> AppResult<String> {
    let _ = app.emit(
        "model-download-progress",
        ModelDownloadProgress {
            model_id: model_id.clone(),
            model_type: model_type.clone(),
            progress: 5,
            status: "opening".to_string(),
        },
    );
    let result = download_model_cli(model_id.clone(), model_type.clone())?;
    let _ = app.emit(
        "model-download-progress",
        ModelDownloadProgress {
            model_id,
            model_type,
            progress: 100,
            status: "completed".to_string(),
        },
    );
    Ok(result)
}

pub fn download_model_cli(model_id: String, _model_type: String) -> AppResult<String> {
    let url = match model_id.as_str() {
        "asr:sherpa" | "asr:sherpa-zh-paraformer" | "asr:sherpa-en-zipformer"
        | "asr:sherpa-multi-zipformer" => Some(URL_SHERPA_MODELS),
        "loci:gguf" | "loci:qwen2.5-0.5b" => Some(URL_LOCI_GGUF),
        "tts:sherpa-melo" | "tts:sherpa-melo-local" => Some(URL_SHERPA_TTS),
        "tts:piper" => Some(URL_PIPER),
        "mt:nllb" | "mt:nllb-distilled" => Some(URL_NLLB),
        _ => None,
    }
    .ok_or_else(|| AppError::NotFound(format!("No download URL for model id: {model_id}")))?;

    open_external(url)?;
    Ok(format!("Opened download page: {url}"))
}

#[tauri::command(rename_all = "snake_case")]
pub fn delete_model(model_id: String) -> AppResult<()> {
    let all = list_models("asr".to_string())?
        .into_iter()
        .chain(list_models("loci".to_string())?.into_iter())
        .chain(list_models("tts".to_string())?.into_iter())
        .chain(list_models("mt".to_string())?.into_iter())
        .collect::<Vec<_>>();
    let info = all
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| AppError::NotFound("Model not found".to_string()))?;
    let Some(path) = info.path else {
        return Err(AppError::InvalidPath("Model path is not available".to_string()));
    };
    let target = PathBuf::from(path);
    let base = models_dir()?.canonicalize()?;
    let target_canon = target.canonicalize()?;
    if !target_canon.starts_with(&base) {
        return Err(AppError::InvalidPath(
            "Refusing to delete path outside models dir".to_string(),
        ));
    }
    if target_canon.is_file() {
        std::fs::remove_file(target_canon)?;
        return Ok(());
    }
    if target_canon.is_dir() {
        std::fs::remove_dir_all(target_canon)?;
        return Ok(());
    }
    Err(AppError::NotFound("Target path does not exist".to_string()))
}

fn list_loci_models(models_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir.join("loci");
    let mut models = Vec::new();
    if dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let is_gguf = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("gguf"))
                    .unwrap_or(false);
                if !is_gguf {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("model.gguf")
                    .to_string();
                let id = format!("loci:{name}");
                let bytes = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
                models.push(ModelInfo {
                    id,
                    name,
                    model_type: "loci".to_string(),
                    size: format_bytes(bytes),
                    status: "ready".to_string(),
                    path: Some(path.to_string_lossy().to_string()),
                    download_url: None,
                });
            }
        }
    }
    if models.is_empty() {
        models.push(ModelInfo {
            id: "loci:gguf".to_string(),
            name: "Loci GGUF 模型索引（推荐）".to_string(),
            model_type: "loci".to_string(),
            size: "-".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(URL_LOCI_GGUF.to_string()),
        });
    }
    models
}

fn list_asr_models(models_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir.join("asr");
    let mut models = Vec::new();
    if has_sherpa_any_asr_files(&dir) {
        models.push(ModelInfo {
            id: "asr:default".to_string(),
            name: "ASR (sherpa，本地已就绪)".to_string(),
            model_type: "asr".to_string(),
            size: format_bytes(dir_size(&dir)),
            status: "ready".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: None,
        });
    }
    if dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() || !has_sherpa_any_asr_files(&path) {
                    continue;
                }
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("asr-model")
                    .to_string();
                models.push(ModelInfo {
                    id: format!("asr:{name}"),
                    name,
                    model_type: "asr".to_string(),
                    size: format_bytes(dir_size(&path)),
                    status: "ready".to_string(),
                    path: Some(path.to_string_lossy().to_string()),
                    download_url: None,
                });
            }
        }
    }
    if models.is_empty() {
        models.push(ModelInfo {
            id: "asr:sherpa-multi-zipformer".to_string(),
            name: "多语 Zipformer（通用）".to_string(),
            model_type: "asr".to_string(),
            size: "~300-600 MB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(URL_SHERPA_MODELS.to_string()),
        });
    }
    models
}

fn list_tts_models(models_dir: &Path) -> Vec<ModelInfo> {
    let piper_dir = models_dir.join("tts").join("piper");
    let sherpa_dir = models_dir.join("tts").join("sherpa").join("vits-melo-tts-zh_en");
    let mut models = Vec::new();
    if sherpa_dir.exists()
        && (sherpa_dir.join("model.int8.onnx").exists() || sherpa_dir.join("model.onnx").exists())
        && sherpa_dir.join("tokens.txt").exists()
    {
        models.push(ModelInfo {
            id: "tts:sherpa-melo-local".to_string(),
            name: "Sherpa Melo 离线音色（本地已就绪）".to_string(),
            model_type: "tts".to_string(),
            size: format_bytes(dir_size(&sherpa_dir)),
            status: "ready".to_string(),
            path: Some(sherpa_dir.to_string_lossy().to_string()),
            download_url: None,
        });
    }
    if piper_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&piper_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_onnx = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("onnx"))
                    .unwrap_or(false);
                if !is_onnx {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("piper-voice")
                    .to_string();
                models.push(ModelInfo {
                    id: format!("tts:{name}"),
                    name,
                    model_type: "tts".to_string(),
                    size: format_bytes(entry.metadata().ok().map(|m| m.len()).unwrap_or(0)),
                    status: "ready".to_string(),
                    path: Some(path.to_string_lossy().to_string()),
                    download_url: None,
                });
            }
        }
    }
    if models.is_empty() {
        models.push(ModelInfo {
            id: "tts:sherpa-melo".to_string(),
            name: "Sherpa Melo 离线 TTS（推荐）".to_string(),
            model_type: "tts".to_string(),
            size: "~100 MB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(sherpa_dir.to_string_lossy().to_string()),
            download_url: Some(URL_SHERPA_TTS.to_string()),
        });
    }
    models
}

fn list_mt_models(models_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir.join("mt");
    vec![
        ModelInfo {
            id: "mt:builtin".to_string(),
            name: "内置翻译引擎（开箱即用）".to_string(),
            model_type: "mt".to_string(),
            size: "-".to_string(),
            status: "ready".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: None,
        },
        ModelInfo {
            id: "mt:nllb".to_string(),
            name: "NLLB-200 600M（可选）".to_string(),
            model_type: "mt".to_string(),
            size: "~1.3 GB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(URL_NLLB.to_string()),
        },
    ]
}

fn has_sherpa_any_asr_files(dir: &Path) -> bool {
    has_sherpa_zipformer_asr_files(dir) || has_sherpa_paraformer_asr_files(dir)
}

fn has_sherpa_zipformer_asr_files(dir: &Path) -> bool {
    if !dir.exists() || !dir.is_dir() || !dir.join("tokens.txt").exists() {
        return false;
    }
    has_prefix_onnx(dir, "encoder") && has_prefix_onnx(dir, "decoder") && has_prefix_onnx(dir, "joiner")
}

fn has_sherpa_paraformer_asr_files(dir: &Path) -> bool {
    if !dir.exists() || !dir.is_dir() || !dir.join("tokens.txt").exists() {
        return false;
    }
    dir.join("model.int8.onnx").exists() || dir.join("model.onnx").exists()
}

fn has_prefix_onnx(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if !path.is_file() {
            return false;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        name.starts_with(prefix) && name.ends_with(".onnx")
    })
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn open_external(url: &str) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open URL: {e}")))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open URL: {e}")))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| AppError::Io(format!("Failed to open URL: {e}")))?;
    }
    Ok(())
}
