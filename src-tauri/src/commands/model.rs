use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::logging::default_log_dir;

const URL_SHERPA_MODELS: &str = "https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html";
const URL_SHERPA_ASR: &str = "https://k2-fsa.github.io/sherpa/onnx/pretrained_models/offline-transducer/index.html";
const URL_SHERPA_TTS: &str = "https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/index.html";
const URL_LOCI_GGUF: &str = "https://huggingface.co/models?library=gguf&sort=downloads";
const URL_PIPER: &str = "https://github.com/rhasspy/piper";
const URL_NLLB: &str = "https://huggingface.co/facebook/nllb-200-distilled-600M";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeComponentStatus {
    pub ready: bool,
    pub path: String,
    pub message: String,
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub models_dir: String,
    pub asr: RuntimeComponentStatus,
    pub translation: RuntimeComponentStatus,
    pub vad: RuntimeComponentStatus,
    pub loci_unhealthy: bool,
    pub loci_unhealthy_remaining_sec: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub log_dir: String,
    pub exists: bool,
    pub files: Vec<String>,
}

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

#[tauri::command]
pub fn list_models(model_type: Option<String>) -> Result<Vec<ModelInfo>, String> {
    let models_dir = models_dir_path()?;

    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models directory: {}", e))?;
    }

    let mut models = Vec::new();
    models.extend(list_asr_models(&models_dir));
    models.extend(list_loci_models(&models_dir));
    models.extend(list_tts_models(&models_dir));
    models.extend(list_mt_models(&models_dir));

    let filtered = if let Some(mt) = model_type {
        models.into_iter().filter(|m| m.model_type == mt).collect()
    } else {
        models
    };

    tracing::info!(
        models_dir = %models_dir.display(),
        total = filtered.len(),
        "model list queried"
    );

    Ok(filtered)
}

/// Get the base models directory used by the app
#[tauri::command]
pub fn get_models_dir() -> Result<String, String> {
    let dir = models_dir_path()?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create models directory: {}", e))?;
    }
    let path = dir.to_string_lossy().to_string();
    tracing::info!(models_dir = %path, "models directory queried");
    Ok(path)
}

/// Check whether runtime-required models are present and where to put them.
///
/// This is used to drive product-level UX: show actionable, user-friendly
/// guidance instead of failing deep inside the pipeline.
#[tauri::command]
pub fn get_runtime_status() -> Result<RuntimeStatus, String> {
    let models_dir = models_dir_path()?;
    if !models_dir.exists() {
        std::fs::create_dir_all(&models_dir)
            .map_err(|e| format!("Failed to create models directory: {}", e))?;
    }

    // ASR (Sherpa ZipFormer / ParaFormer)
    let asr_root = models_dir.join("asr");
    let asr_dir = find_sherpa_asr_model_dir(&asr_root);
    let asr = match asr_dir {
        Some(dir) => RuntimeComponentStatus {
            ready: true,
            path: dir.to_string_lossy().to_string(),
            message: "ASR 模型已就绪 (sherpa zipformer/paraformer)".to_string(),
            action: None,
        },
        None => RuntimeComponentStatus {
            ready: false,
            path: asr_root.to_string_lossy().to_string(),
            message: "未找到 ASR 模型。请将 sherpa zipformer/paraformer 模型文件放入 asr/。".to_string(),
            action: Some("打开【模型】页下载推荐 ASR 模型并解压到 asr/".to_string()),
        },
    };

    // Translation (Loci GGUF)
    let loci_root = models_dir.join("loci");
    let loci_model = find_default_loci_model(&loci_root);
    let (loci_unhealthy, loci_unhealthy_remaining_sec) = loci_unhealthy_state();
    let translation = match loci_model {
        Some(path) => RuntimeComponentStatus {
            ready: true,
            path: path.to_string_lossy().to_string(),
            message: if loci_unhealthy {
                "检测到 Loci 最近多次失败，已临时降级为 NLLB（稍后自动恢复）".to_string()
            } else {
                "翻译模型已就绪 (Loci GGUF)".to_string()
            },
            action: if loci_unhealthy {
                Some("当前建议使用 NLLB；等待熔断窗口结束后可重试 Loci".to_string())
            } else {
                None
            },
        },
        None => RuntimeComponentStatus {
            ready: true,
            path: loci_root.to_string_lossy().to_string(),
            message: "未找到 Loci 模型，当前将使用内置翻译引擎（建议导入 .gguf 以提升质量）".to_string(),
            action: Some("可选：打开【模型】页导入 Loci GGUF 模型".to_string()),
        },
    };

    // VAD (optional)
    let vad_path = models_dir.join("vad").join("silero_vad.onnx");
    let vad = if vad_path.exists() {
        RuntimeComponentStatus {
            ready: true,
            path: vad_path.to_string_lossy().to_string(),
            message: "VAD 模型已就绪 (Silero)".to_string(),
            action: None,
        }
    } else {
        RuntimeComponentStatus {
            ready: false,
            path: vad_path.to_string_lossy().to_string(),
            message: "未找到 Silero VAD 模型 (可选)。将使用能量阈值 VAD。".to_string(),
            action: None,
        }
    };

    let status = RuntimeStatus {
        models_dir: models_dir.to_string_lossy().to_string(),
        asr,
        translation,
        vad,
        loci_unhealthy,
        loci_unhealthy_remaining_sec,
    };

    tracing::info!(
        models_dir = %status.models_dir,
        asr_ready = status.asr.ready,
        asr_path = %status.asr.path,
        translation_ready = status.translation.ready,
        translation_path = %status.translation.path,
        vad_ready = status.vad.ready,
        vad_path = %status.vad.path,
        loci_unhealthy = status.loci_unhealthy,
        loci_unhealthy_remaining_sec = status.loci_unhealthy_remaining_sec,
        "runtime status checked"
    );

    Ok(status)
}

/// Return current log directory and recent files for diagnostics upload.
#[tauri::command]
pub fn get_log_status() -> Result<LogStatus, String> {
    let dir = default_log_dir();
    let exists = dir.exists();
    let mut files: Vec<String> = Vec::new();

    if exists {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    files.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    files.sort();
    files.reverse();

    let status = LogStatus {
        log_dir: dir.to_string_lossy().to_string(),
        exists,
        files: files.into_iter().take(10).collect(),
    };

    tracing::info!(
        log_dir = %status.log_dir,
        file_count = status.files.len(),
        "log status checked"
    );

    Ok(status)
}

#[tauri::command]
pub async fn download_model(
    model_id: String,
    model_type: String,
) -> Result<String, String> {
    let _ = model_type;
    // For now we open the download page in the default browser.
    let url = match model_id.as_str() {
        "asr:sherpa" => Some(URL_SHERPA_ASR),
        "asr:sherpa-zh-paraformer" => Some(URL_SHERPA_MODELS),
        "asr:sherpa-en-zipformer" => Some(URL_SHERPA_MODELS),
        "asr:sherpa-multi-zipformer" => Some(URL_SHERPA_MODELS),
        "loci:gguf" => Some(URL_LOCI_GGUF),
        "loci:qwen2.5-0.5b" => Some("https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF"),
        "tts:sherpa-melo" => Some(URL_SHERPA_TTS),
        "tts:piper" => Some(URL_PIPER),
        "mt:nllb" => Some(URL_NLLB),
        _ => None,
    }
    .ok_or_else(|| format!("No download URL for model id: {}", model_id))?;

    open_external(url)?;
    Ok(format!("Opened download page: {}", url))
}

#[tauri::command]
pub fn delete_model(model_id: String) -> Result<(), String> {
    let models_dir = models_dir_path()?;
    let all = list_models(None)?;
    let info = all
        .into_iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| "Model not found".to_string())?;

    let target = info
        .path
        .ok_or_else(|| "Model path is not available".to_string())?;
    let target = PathBuf::from(target);

    // Safety: only allow deleting inside the models directory
    let base = models_dir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve base models dir: {}", e))?;

    let target_canon = target
        .canonicalize()
        .map_err(|e| format!("Failed to resolve target path: {}", e))?;

    if !target_canon.starts_with(&base) {
        return Err("Refusing to delete path outside models dir".to_string());
    }

    if target_canon.is_file() {
        std::fs::remove_file(&target_canon)
            .map_err(|e| format!("Failed to delete file: {}", e))?;
        return Ok(());
    }

    if target_canon.is_dir() {
        std::fs::remove_dir_all(&target_canon)
            .map_err(|e| format!("Failed to delete directory: {}", e))?;
        return Ok(());
    }

    Err("Target path does not exist".to_string())
}

fn models_dir_path() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| "Cannot determine data directory".to_string())?;

    Ok(base.join("LocalTrans").join("models"))
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

                let id = format!("loci:{}", name);
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
        models.push(ModelInfo {
            id: "loci:qwen2.5-0.5b".to_string(),
            name: "Qwen2.5-0.5B-Instruct-GGUF（轻量）".to_string(),
            model_type: "loci".to_string(),
            size: "~0.4-0.8 GB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some("https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string()),
        });
    }

    models
}

fn list_asr_models(models_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir.join("asr");
    let mut models = Vec::new();

    // Model directly under asr/
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

    // Scan subdirectories
    if dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if !has_sherpa_any_asr_files(&path) {
                    continue;
                }

                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("asr-model")
                    .to_string();

                models.push(ModelInfo {
                    id: format!("asr:{}", name),
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
            id: "asr:sherpa".to_string(),
            name: "Sherpa ASR 模型索引（推荐）".to_string(),
            model_type: "asr".to_string(),
            size: "-".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(URL_SHERPA_MODELS.to_string()),
        });
        models.push(ModelInfo {
            id: "asr:sherpa-zh-paraformer".to_string(),
            name: "中文 Paraformer（低延时）".to_string(),
            model_type: "asr".to_string(),
            size: "~200 MB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(URL_SHERPA_MODELS.to_string()),
        });
        models.push(ModelInfo {
            id: "asr:sherpa-en-zipformer".to_string(),
            name: "英文 Zipformer（高准确）".to_string(),
            model_type: "asr".to_string(),
            size: "~300 MB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(URL_SHERPA_MODELS.to_string()),
        });
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

fn find_sherpa_asr_model_dir(asr_root: &Path) -> Option<PathBuf> {
    if has_sherpa_any_asr_files(asr_root) {
        return Some(asr_root.to_path_buf());
    }

    let entries = std::fs::read_dir(asr_root).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !has_sherpa_any_asr_files(&path) {
            continue;
        }
        let size = dir_size(&path);
        match &best {
            Some((best_size, _)) if *best_size >= size => {}
            _ => best = Some((size, path)),
        }
    }

    best.map(|(_, p)| p)
}

fn find_default_loci_model(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
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
                if !path.is_file() {
                    continue;
                }
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
                let bytes = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);

                models.push(ModelInfo {
                    id: format!("tts:{}", name),
                    name,
                    model_type: "tts".to_string(),
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
            id: "tts:sherpa-melo".to_string(),
            name: "Sherpa Melo 离线 TTS（推荐）".to_string(),
            model_type: "tts".to_string(),
            size: "~100 MB".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(sherpa_dir.to_string_lossy().to_string()),
            download_url: Some(URL_SHERPA_TTS.to_string()),
        });
        models.push(ModelInfo {
            id: "tts:piper".to_string(),
            name: "Piper 离线 TTS (可选)".to_string(),
            model_type: "tts".to_string(),
            size: "-".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(piper_dir.to_string_lossy().to_string()),
            download_url: Some(URL_PIPER.to_string()),
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

fn has_sherpa_zipformer_asr_files(dir: &Path) -> bool {
    if !dir.exists() || !dir.is_dir() {
        return false;
    }

    if !dir.join("tokens.txt").exists() {
        return false;
    }

    has_prefix_onnx(dir, "encoder") && has_prefix_onnx(dir, "decoder") && has_prefix_onnx(dir, "joiner")
}

fn has_sherpa_paraformer_asr_files(dir: &Path) -> bool {
    if !dir.exists() || !dir.is_dir() {
        return false;
    }
    if !dir.join("tokens.txt").exists() {
        return false;
    }
    dir.join("model.int8.onnx").exists() || dir.join("model.onnx").exists()
}

fn has_sherpa_any_asr_files(dir: &Path) -> bool {
    has_sherpa_zipformer_asr_files(dir) || has_sherpa_paraformer_asr_files(dir)
}

fn has_prefix_onnx(dir: &Path, prefix: &str) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        if !file_name.starts_with(prefix) {
            continue;
        }
        if file_name.ends_with(".onnx") {
            return true;
        }
    }

    false
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
        format!("{} B", bytes)
    }
}

fn localtrans_data_dir() -> Result<PathBuf, String> {
    let base = dirs::data_local_dir()
        .ok_or_else(|| "Cannot determine data directory".to_string())?;
    Ok(base.join("LocalTrans"))
}

fn loci_unhealthy_marker_path() -> Result<PathBuf, String> {
    Ok(localtrans_data_dir()?.join("loci-unhealthy-until.txt"))
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as u64
}

fn loci_unhealthy_state() -> (bool, u64) {
    let Ok(path) = loci_unhealthy_marker_path() else {
        return (false, 0);
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return (false, 0);
    };
    let Ok(until_ms) = text.trim().parse::<u64>() else {
        return (false, 0);
    };
    let now = now_unix_ms();
    if now >= until_ms {
        return (false, 0);
    }
    let remain_sec = until_ms.saturating_sub(now) / 1000;
    (true, remain_sec)
}

fn open_external(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    Ok(())
}
