use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;

#[derive(Debug, Serialize)]
pub struct RuntimeComponentStatus {
    pub ready: bool,
    pub path: String,
    pub message: String,
    pub action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeStatus {
    pub models_dir: String,
    pub asr: RuntimeComponentStatus,
    pub translation: RuntimeComponentStatus,
    pub vad: RuntimeComponentStatus,
    pub loci_unhealthy: bool,
    pub loci_unhealthy_remaining_sec: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    pub log_dir: String,
    pub latest_log: Option<String>,
    pub exists: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MtRuntimeCheck {
    pub bundled_python: Option<String>,
    pub bundled_script: Option<String>,
    pub bundled_argos_packages: Option<String>,
    pub package_count: usize,
    pub language_pairs: Vec<String>,
    pub ready: bool,
    pub message: String,
}

#[tauri::command]
pub fn get_runtime_status() -> AppResult<RuntimeStatus> {
    let models_dir = super::model::models_dir()?;
    let asr_ready = super::model::has_ready_model("asr")?;
    let loci_ready = super::model::has_ready_model("loci")?;
    let tts_ready = super::model::has_ready_model("tts")?;

    Ok(RuntimeStatus {
        models_dir: models_dir.display().to_string(),
        asr: RuntimeComponentStatus {
            ready: asr_ready,
            path: models_dir.join("asr").display().to_string(),
            message: if asr_ready {
                "ASR model ready".to_string()
            } else {
                "ASR model not installed".to_string()
            },
            action: Some("open_model_page".to_string()),
        },
        translation: RuntimeComponentStatus {
            ready: loci_ready,
            path: if loci_ready {
                models_dir.join("loci").display().to_string()
            } else {
                models_dir.join("loci").display().to_string()
            },
            message: if loci_ready {
                "Loci enhanced translation ready".to_string()
            } else {
                "Loci translation model not installed".to_string()
            },
            action: if loci_ready {
                None
            } else {
                Some("download_loci_model".to_string())
            },
        },
        vad: RuntimeComponentStatus {
            ready: tts_ready,
            path: models_dir.join("vad").display().to_string(),
            message: if tts_ready {
                "Optional components available".to_string()
            } else {
                "Optional VAD model not installed".to_string()
            },
            action: Some("download_optional_vad".to_string()),
        },
        loci_unhealthy: false,
        loci_unhealthy_remaining_sec: 0,
    })
}

#[tauri::command]
pub fn open_url(url: String) -> AppResult<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| crate::error::AppError::Io(format!("Failed to open URL: {e}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| crate::error::AppError::Io(format!("Failed to open URL: {e}")))?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| crate::error::AppError::Io(format!("Failed to open URL: {e}")))?;
    }

    Ok(())
}

#[tauri::command]
pub fn get_log_status() -> AppResult<LogStatus> {
    let cwd = std::env::current_dir()?;
    let log_dir = cwd.join("logs");
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    if log_dir.exists() {
        for entry in fs::read_dir(&log_dir)? {
            let entry = match entry {
                Ok(v) => v,
                Err(_) => continue,
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            match &latest {
                Some((old, _)) if *old >= modified => {}
                _ => latest = Some((modified, path)),
            }
        }
    }
    Ok(LogStatus {
        log_dir: log_dir.display().to_string(),
        latest_log: latest.map(|(_, p)| p.display().to_string()),
        exists: log_dir.exists(),
    })
}

#[tauri::command]
pub fn check_mt_runtime() -> AppResult<MtRuntimeCheck> {
    let bundled_python = resolve_bundled_python();
    let bundled_script = resolve_bundled_mt_script();
    let bundled_argos_packages = resolve_bundled_argos_packages();
    let (package_count, language_pairs) = match bundled_argos_packages.as_deref() {
        Some(path) => scan_argos_packages(Path::new(path)),
        None => (0, Vec::new()),
    };
    let ready = bundled_python.is_some()
        && bundled_script.is_some()
        && bundled_argos_packages.is_some()
        && package_count > 0;

    let message = if ready {
        format!("Bundled MT runtime ready ({} packages).", package_count)
    } else {
        "Bundled MT runtime incomplete. Run tools/prepare_mt_runtime.ps1 before packaging."
            .to_string()
    };

    Ok(MtRuntimeCheck {
        bundled_python,
        bundled_script,
        bundled_argos_packages,
        package_count,
        language_pairs,
        ready,
        message,
    })
}

fn resolve_bundled_python() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir
            .join("resources")
            .join("mt-runtime")
            .join("python")
            .join("python.exe"),
        exe_dir.join("mt-runtime").join("python").join("python.exe"),
        exe_dir.join("python").join("python.exe"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| path_to_string(p.as_path()))
}

fn resolve_bundled_argos_packages() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir.join("resources").join("mt-runtime").join("argos-packages"),
        exe_dir.join("mt-runtime").join("argos-packages"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| path_to_string(p.as_path()))
}

fn resolve_bundled_mt_script() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    let candidates = [
        exe_dir
            .join("resources")
            .join("mt-runtime")
            .join("mt_translate.py"),
        exe_dir.join("mt-runtime").join("mt_translate.py"),
    ];
    candidates
        .iter()
        .find(|p| p.exists())
        .map(|p| path_to_string(p.as_path()))
}

fn scan_argos_packages(root: &Path) -> (usize, Vec<String>) {
    let mut count = 0usize;
    let mut pairs = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(v) => v,
        Err(_) => return (0, pairs),
    };
    for entry in entries.flatten() {
        let meta = entry.path().join("metadata.json");
        if !meta.exists() {
            continue;
        }
        let text = match fs::read_to_string(&meta) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let json: Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let from = json.get("from_code").and_then(Value::as_str).unwrap_or("");
        let to = json.get("to_code").and_then(Value::as_str).unwrap_or("");
        if !from.is_empty() && !to.is_empty() {
            pairs.push(format!("{from}->{to}"));
        }
        count += 1;
    }
    pairs.sort();
    pairs.dedup();
    (count, pairs)
}

fn path_to_string(path: &Path) -> String {
    path.display().to_string()
}
