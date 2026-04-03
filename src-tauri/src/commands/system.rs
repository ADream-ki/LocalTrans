use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::AppResult;
use crate::runtime_adapters::RuntimeAdapterInventory;

#[derive(Debug, Serialize)]
pub struct RuntimeComponentStatus {
    pub ready: bool,
    pub path: String,
    pub message: String,
    pub action: Option<String>,
    pub engine: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeStatus {
    pub models_dir: String,
    pub asr: RuntimeComponentStatus,
    pub translation: RuntimeComponentStatus,
    pub tts: RuntimeComponentStatus,
    pub vad: RuntimeComponentStatus,
    pub tts_engine: String,
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
pub fn get_runtime_adapter_inventory() -> AppResult<RuntimeAdapterInventory> {
    let asr_engine = super::session::resolved_asr_engine(None);
    let translation_engine = super::translation::resolved_translation_engine(None);
    let tts_engine = super::session::resolved_tts_engine(None);
    Ok(crate::runtime_adapters::runtime_adapter_inventory(
        &asr_engine,
        &translation_engine,
        &tts_engine,
    ))
}

#[tauri::command]
pub fn get_runtime_status() -> AppResult<RuntimeStatus> {
    let models_dir = super::model::models_dir()?;
    let asr_ready = super::model::has_ready_model("asr")?;
    let loci_ready = super::model::has_ready_model("loci")?;
    let bundled_tts_ready = super::model::has_ready_model("tts")?;
    let asr_engine = super::session::resolved_asr_engine(None);
    let translation_engine = super::translation::resolved_translation_engine(None);
    let tts_engine = super::session::resolved_tts_engine(None);
    let mt_ready = check_mt_runtime()?.ready;
    let custom_voice_enabled = crate::commands::config::get_value("customVoiceEnabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let custom_voice_profile_id = crate::commands::config::get_string("customVoiceProfileId");
    let custom_voice_model_path = crate::commands::config::get_string("customVoiceModelPath");

    let (translation_ready, translation_path, translation_message, translation_action) =
        if translation_engine == "loci" {
            (
                loci_ready,
                models_dir.join("loci").display().to_string(),
                if loci_ready {
                    "Loci enhanced translation ready".to_string()
                } else {
                    "Loci translation model not installed".to_string()
                },
                if loci_ready {
                    None
                } else {
                    Some("download_loci_model".to_string())
                },
            )
        } else {
            let mt_root = if let Some(path) = resolve_bundled_argos_packages() {
                path
            } else {
                models_dir.join("mt").display().to_string()
            };
            (
                mt_ready,
                mt_root,
                if mt_ready {
                    "Deterministic MT runtime ready".to_string()
                } else {
                    "Bundled MT runtime incomplete".to_string()
                },
                if mt_ready {
                    None
                } else {
                    Some("prepare_mt_runtime".to_string())
                },
            )
        };

    let (tts_ready, tts_path, tts_message, tts_action) = match tts_engine.as_str() {
        "edge-tts" => (
            true,
            "remote://edge-tts".to_string(),
            "Edge TTS is available as an online backend".to_string(),
            None,
        ),
        "system" => (
            true,
            "host://system-tts".to_string(),
            "System TTS route is available on the host".to_string(),
            None,
        ),
        "custom" => {
            let ready = custom_voice_enabled
                && (custom_voice_profile_id.is_some() || custom_voice_model_path.is_some());
            (
                ready,
                custom_voice_model_path
                    .or(custom_voice_profile_id.map(|id| format!("profile://{id}")))
                    .unwrap_or_else(|| models_dir.join("tts").display().to_string()),
                if ready {
                    "Custom voice route is configured".to_string()
                } else {
                    "Custom voice engine selected, but no profile/model is configured".to_string()
                },
                if ready {
                    None
                } else {
                    Some("open_settings_page".to_string())
                },
            )
        }
        "qwen3-tts" => (
            false,
            models_dir.join("tts").display().to_string(),
            "Qwen3-TTS adapter slot exists but this build does not yet include a concrete backend".to_string(),
            Some("build_or_plugin_required".to_string()),
        ),
        "sherpa-melo" | "piper" => (
            bundled_tts_ready,
            models_dir.join("tts").display().to_string(),
            if bundled_tts_ready {
                "Bundled local TTS assets are ready".to_string()
            } else {
                "Bundled local TTS assets are not installed".to_string()
            },
            if bundled_tts_ready {
                None
            } else {
                Some("download_tts_model".to_string())
            },
        ),
        other => (
            false,
            models_dir.join("tts").display().to_string(),
            format!("Unknown TTS engine: {other}"),
            None,
        ),
    };

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
            engine: Some(asr_engine),
        },
        translation: RuntimeComponentStatus {
            ready: translation_ready,
            path: translation_path,
            message: translation_message,
            action: translation_action,
            engine: Some(translation_engine),
        },
        tts: RuntimeComponentStatus {
            ready: tts_ready,
            path: tts_path,
            message: tts_message,
            action: tts_action,
            engine: Some(tts_engine.clone()),
        },
        vad: RuntimeComponentStatus {
            ready: false,
            path: models_dir.join("vad").display().to_string(),
            message: "Optional VAD model not installed".to_string(),
            action: Some("download_optional_vad".to_string()),
            engine: None,
        },
        tts_engine,
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
