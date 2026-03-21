use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    Ok(dir.to_string_lossy().to_string())
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

    // ASR (Sherpa ZipFormer)
    let asr_root = models_dir.join("asr");
    let asr_dir = find_sherpa_asr_model_dir(&asr_root);
    let asr = match asr_dir {
        Some(dir) => RuntimeComponentStatus {
            ready: true,
            path: dir.to_string_lossy().to_string(),
            message: "ASR 模型已就绪 (sherpa zipformer)".to_string(),
            action: None,
        },
        None => RuntimeComponentStatus {
            ready: false,
            path: asr_root.to_string_lossy().to_string(),
            message: "未找到 ASR 模型。请将 sherpa zipformer 模型文件放入 asr/ (包含 tokens.txt + encoder/decoder/joiner*.onnx)".to_string(),
            action: Some("打开【模型】页并导入 ASR 模型".to_string()),
        },
    };

    // Translation (Loci GGUF)
    let loci_root = models_dir.join("loci");
    let loci_model = find_default_loci_model(&loci_root);
    let translation = match loci_model {
        Some(path) => RuntimeComponentStatus {
            ready: true,
            path: path.to_string_lossy().to_string(),
            message: "翻译模型已就绪 (Loci GGUF)".to_string(),
            action: None,
        },
        None => RuntimeComponentStatus {
            ready: false,
            path: loci_root.to_string_lossy().to_string(),
            message: "未找到翻译模型。请将 .gguf 文件放入 loci/ (本地 LLM)".to_string(),
            action: Some("打开【模型】页并导入 Loci GGUF 模型".to_string()),
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

    Ok(RuntimeStatus {
        models_dir: models_dir.to_string_lossy().to_string(),
        asr,
        translation,
        vad,
    })
}

#[tauri::command]
pub async fn download_model(
    model_id: String,
    model_type: String,
) -> Result<String, String> {
    let _ = model_type;
    // For now we open the download page in the default browser.
    let url = match model_id.as_str() {
        "asr:sherpa" => Some("https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html"),
        "loci:gguf" => Some("https://huggingface.co/models?library=gguf&sort=downloads"),
        "tts:piper" => Some("https://github.com/rhasspy/piper"),
        "mt:nllb" => Some("https://huggingface.co/facebook/nllb-200-distilled-600M"),
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
            name: "Loci GGUF 模型 (导入)".to_string(),
            model_type: "loci".to_string(),
            size: "-".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some("https://huggingface.co/models?library=gguf&sort=downloads".to_string()),
        });
    }

    models
}

fn list_asr_models(models_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir.join("asr");
    let mut models = Vec::new();

    // Model directly under asr/
    if has_sherpa_asr_files(&dir) {
        models.push(ModelInfo {
            id: "asr:default".to_string(),
            name: "ASR (sherpa)".to_string(),
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
                if !has_sherpa_asr_files(&path) {
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
            name: "Sherpa ASR 模型 (导入)".to_string(),
            model_type: "asr".to_string(),
            size: "-".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some(
                "https://k2-fsa.github.io/sherpa/onnx/pretrained_models/index.html".to_string(),
            ),
        });
    }

    models
}

fn find_sherpa_asr_model_dir(asr_root: &Path) -> Option<PathBuf> {
    if has_sherpa_asr_files(asr_root) {
        return Some(asr_root.to_path_buf());
    }

    let entries = std::fs::read_dir(asr_root).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !has_sherpa_asr_files(&path) {
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
    let dir = models_dir.join("tts").join("piper");
    let mut models = Vec::new();

    if dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
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
            id: "tts:piper".to_string(),
            name: "Piper 离线 TTS (可选)".to_string(),
            model_type: "tts".to_string(),
            size: "-".to_string(),
            status: "not_downloaded".to_string(),
            path: Some(dir.to_string_lossy().to_string()),
            download_url: Some("https://github.com/rhasspy/piper".to_string()),
        });
    }

    models
}

fn list_mt_models(models_dir: &Path) -> Vec<ModelInfo> {
    let dir = models_dir.join("mt");
    // Currently not implemented; provide a placeholder.
    vec![ModelInfo {
        id: "mt:nllb".to_string(),
        name: "NLLB 翻译模型 (未接入)".to_string(),
        model_type: "mt".to_string(),
        size: "-".to_string(),
        status: "not_downloaded".to_string(),
        path: Some(dir.to_string_lossy().to_string()),
        download_url: Some("https://huggingface.co/facebook/nllb-200-distilled-600M".to_string()),
    }]
}

fn has_sherpa_asr_files(dir: &Path) -> bool {
    if !dir.exists() || !dir.is_dir() {
        return false;
    }

    if !dir.join("tokens.txt").exists() {
        return false;
    }

    has_prefix_onnx(dir, "encoder") && has_prefix_onnx(dir, "decoder") && has_prefix_onnx(dir, "joiner")
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
