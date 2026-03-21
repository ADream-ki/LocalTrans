use serde::{Deserialize, Serialize};
use tauri::State;
use std::sync::Mutex;
use crate::audio::AudioCapture;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_input: bool,
    pub is_default: bool,
}

#[tauri::command]
pub fn get_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    let devices = AudioCapture::list_devices()
        .map_err(|e| e.to_string())?;

    Ok(devices.into_iter().map(|d| AudioDeviceInfo {
        id: d.id,
        name: d.name,
        is_input: d.is_input,
        is_default: d.is_default,
    }).collect())
}

#[tauri::command]
pub async fn start_capture(
    device_id: Option<String>,
    state: State<'_, Mutex<Option<AudioCapture>>>,
) -> Result<(), String> {
    let mut capture = state.lock().map_err(|e| e.to_string())?;

    // Stop any existing capture
    if let Some(ref mut existing) = *capture {
        existing.stop_capture();
    }

    let mut audio_capture = AudioCapture::new(device_id.as_deref())
        .map_err(|e| e.to_string())?;

    audio_capture.start_capture()
        .map_err(|e| e.to_string())?;

    *capture = Some(audio_capture);
    Ok(())
}

#[tauri::command]
pub async fn stop_capture(
    state: State<'_, Mutex<Option<AudioCapture>>>,
) -> Result<(), String> {
    let mut capture = state.lock().map_err(|e| e.to_string())?;

    if let Some(ref mut audio_capture) = *capture {
        audio_capture.stop_capture();
    }
    *capture = None;
    Ok(())
}
