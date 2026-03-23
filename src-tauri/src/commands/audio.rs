use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use crate::audio::AudioCapture;
use crate::error::{AppError, AppResult};

#[derive(Debug, Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_input: bool,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
pub struct TtsOutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Serialize)]
pub struct VirtualAudioDriver {
    pub name: String,
    pub device_id: String,
    pub driver_type: String,
}

#[derive(Debug, Serialize)]
pub struct VirtualDriverCheckResult {
    pub has_virtual_driver: bool,
    pub detected_drivers: Vec<VirtualAudioDriver>,
    pub recommendation: String,
    pub download_url: Option<String>,
}

fn capture_state() -> &'static Mutex<Option<AudioCapture>> {
    static STATE: OnceLock<Mutex<Option<AudioCapture>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(None))
}

#[tauri::command]
pub fn get_audio_devices() -> AppResult<Vec<AudioDevice>> {
    let devices = AudioCapture::list_devices().map_err(|e| AppError::InvalidState(e.to_string()))?;
    Ok(devices
        .into_iter()
        .map(|d| AudioDevice {
            id: d.id,
            name: d.name,
            is_input: d.is_input,
            is_default: d.is_default,
        })
        .collect())
}

#[tauri::command]
pub fn get_tts_output_devices() -> AppResult<Vec<TtsOutputDevice>> {
    let devices = AudioCapture::list_devices().map_err(|e| AppError::InvalidState(e.to_string()))?;
    Ok(devices
        .into_iter()
        .filter(|d| !d.is_input)
        .map(|d| TtsOutputDevice {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        })
        .collect())
}

#[tauri::command]
pub fn check_virtual_audio_driver() -> AppResult<VirtualDriverCheckResult> {
    let outputs = get_tts_output_devices()?;
    let mut detected = Vec::new();
    for d in &outputs {
        let lname = d.name.to_ascii_lowercase();
        let driver_type = if lname.contains("vb-audio") || lname.contains("cable") {
            Some("vb-audio")
        } else if lname.contains("voicemeeter") {
            Some("voicemeeter")
        } else if lname.contains("virtual audio") || lname.contains("virtual") {
            Some("virtual")
        } else {
            None
        };
        if let Some(t) = driver_type {
            detected.push(VirtualAudioDriver {
                name: d.name.clone(),
                device_id: d.id.clone(),
                driver_type: t.to_string(),
            });
        }
    }

    let has_virtual_driver = !detected.is_empty();
    let (recommendation, download_url) = if has_virtual_driver {
        (
            format!(
                "已检测到虚拟音频设备: {}",
                detected
                    .iter()
                    .map(|d| d.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        )
    } else {
        (
            "未检测到虚拟音频设备，建议安装 VB-Audio Virtual Cable。".to_string(),
            Some("https://vb-audio.com/Cable/".to_string()),
        )
    };

    Ok(VirtualDriverCheckResult {
        has_virtual_driver,
        detected_drivers: detected,
        recommendation,
        download_url,
    })
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn start_capture(deviceId: Option<String>) -> AppResult<()> {
    let mut guard = capture_state()
        .lock()
        .map_err(|_| AppError::InvalidState("audio capture lock poisoned".to_string()))?;

    if let Some(existing) = guard.as_mut() {
        existing.stop_capture();
    }

    let mut capture =
        AudioCapture::new(deviceId.as_deref()).map_err(|e| AppError::InvalidState(e.to_string()))?;
    capture
        .start_capture()
        .map_err(|e| AppError::InvalidState(e.to_string()))?;
    *guard = Some(capture);
    Ok(())
}

#[tauri::command]
pub fn stop_capture() -> AppResult<()> {
    let mut guard = capture_state()
        .lock()
        .map_err(|_| AppError::InvalidState("audio capture lock poisoned".to_string()))?;
    if let Some(capture) = guard.as_mut() {
        capture.stop_capture();
    }
    *guard = None;
    Ok(())
}
