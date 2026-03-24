use serde::Serialize;
use std::fs;
use std::path::PathBuf;

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
