//! TTS (Text-to-Speech) Commands

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri::State;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tts::{self, TtsConfig, VoiceInfo, get_all_voices, get_voices_for_lang, CustomVoiceConfig, CustomVoiceType};
use crate::tts::playback::PlaybackControl;

/// Get all available TTS voices
#[tauri::command]
pub fn get_tts_voices(language: Option<String>) -> Result<Vec<VoiceInfo>, String> {
    let voices = if let Some(lang) = language {
        get_voices_for_lang(&lang)
    } else {
        get_all_voices()
    };
    Ok(voices)
}

/// Get all output audio devices (including virtual devices like VB-Audio Cable)
#[tauri::command]
pub fn get_tts_output_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    tts::playback::AudioPlayer::list_devices()
        .map(|devices| devices.into_iter().map(|d| AudioDeviceInfo {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        }).collect())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// TTS synthesis request
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsRequest {
    /// Text to synthesize
    pub text: String,
    /// Voice ID
    pub voice: String,
    /// Engine name: "edge-tts" | "piper" | "system" | "custom"
    pub engine: Option<String>,
    /// Speech rate (0.5 - 2.0)
    #[serde(default = "default_rate")]
    pub rate: f32,
    /// Pitch adjustment (-50 to 50, percent)
    #[serde(default = "default_pitch")]
    pub pitch: i32,
    /// Output volume (0.0 - 1.0)
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Output device (None = default, Some = specific device)
    pub output_device: Option<String>,
    /// Custom voice configuration (optional)
    pub custom_voice: Option<CustomVoiceRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomVoiceRequest {
    /// Model type: "gpt-sovits", "rvc", "piper", "vits"
    pub model_type: String,
    /// Path to model file
    pub model_path: String,
    /// Reference audio path (for voice cloning)
    pub reference_audio: Option<String>,
    /// Reference text (for voice cloning)
    pub reference_text: Option<String>,
}

fn default_rate() -> f32 { 1.0 }
fn default_pitch() -> i32 { 0 }
fn default_volume() -> f32 { 1.0 }

/// TTS synthesis result
#[derive(Debug, Serialize)]
pub struct TtsResult {
    /// Duration in seconds
    pub duration_secs: f32,
    /// Voice used
    pub voice: String,
    /// Whether playback succeeded
    pub success: bool,
    /// Output device used
    pub output_device: String,
}

/// Synthesize and play text
#[tauri::command]
pub async fn speak_text(
    app: AppHandle,
    request: TtsRequest,
    playback: State<'_, StdMutex<PlaybackControl>>,
) -> Result<TtsResult, String> {
    let text = request.text;
    let voice = request.voice.clone();
    let output_device = request.output_device.clone();
    
    if text.trim().is_empty() {
        return Err("Text cannot be empty".to_string());
    }

    tracing::info!("TTS: Speaking '{}' with voice '{}' to device {:?}", 
        text.chars().take(50).collect::<String>(), voice, output_device);

    let engine_name = request.engine.clone().unwrap_or_else(|| "edge-tts".to_string());
    let rate = request.rate.clamp(0.5, 2.0);
    let pitch = request.pitch.clamp(-50, 50);
    let volume = request.volume.clamp(0.0, 1.0);

    // Determine engine type and create appropriate engine
    let audio = if let Some(custom) = &request.custom_voice {
        // Use custom voice engine
        let model_type = match custom.model_type.as_str() {
            "gpt-sovits" => CustomVoiceType::GptSoVits,
            "rvc" => CustomVoiceType::Rvc,
            "piper" => CustomVoiceType::Piper,
            "vits" => CustomVoiceType::Vits,
            _ => return Err(format!("Unknown model type: {}", custom.model_type)),
        };

        let config = CustomVoiceConfig {
            model_type,
            model_path: PathBuf::from(&custom.model_path),
            reference_audio: custom.reference_audio.as_ref().map(PathBuf::from),
            reference_text: custom.reference_text.clone(),
            similarity: 0.8,
        };

        let engine = tts::custom_voice::CustomVoiceEngine::new(config)
            .map_err(|e| format!("Failed to create custom voice engine: {}", e))?;

        tts::TtsEngine::synthesize(&engine, &text, &voice)
            .await
            .map_err(|e| format!("Custom voice synthesis failed: {}", e))?
    } else {
        match engine_name.as_str() {
            "edge-tts" => {
                let engine = tts::edge_tts::EdgeTtsEngine::new()
                    .map_err(|e| format!("Failed to create TTS engine: {}", e))?;
                match engine
                    .synthesize_with_prosody(&text, &voice, rate, pitch)
                    .await
                {
                    Ok(audio) => audio,
                    Err(edge_err) => {
                        tracing::warn!(
                            "Edge TTS failed ({}). Falling back to system TTS.",
                            edge_err
                        );
                        synthesize_system_tts_audio(&text, rate)
                            .map_err(|e| format!("Edge failed and system fallback failed: {}", e))?
                    }
                }
            }
            "piper" => {
                let voice_id = voice.clone();
                let text = text.clone();
                tokio::task::spawn_blocking(move || {
                    let mut engine = tts::piper_tts::PiperTtsEngine::new();
                    engine.scan_models()?;

                    let model_id = engine
                        .get_model_infos()
                        .iter()
                        .find(|m| m.id == voice_id)
                        .map(|m| m.id.clone())
                        .or_else(|| engine.get_default_model().map(|s| s.to_string()))
                        .ok_or_else(|| anyhow::anyhow!("No Piper TTS models found"))?;

                    engine.load_model(&model_id)?;
                    engine.synthesize_with_model(&text, &model_id, rate)
                })
                .await
                .map_err(|e| format!("Piper synthesis task failed: {}", e))?
                .map_err(|e| format!("Piper TTS failed: {}", e))?
            }
            "system" => {
                synthesize_system_tts_audio(&text, rate)
                    .map_err(|e| format!("System TTS failed: {}", e))?
            }
            other => {
                return Err(format!("Unknown TTS engine: {}", other));
            }
        }
    };

    let duration_secs = audio.duration_secs;

    // Create audio player with specified output device
    let mut player = tts::playback::AudioPlayer::new(output_device.as_deref())
        .map_err(|e| format!("Failed to create audio player: {}", e))?;
    player.set_volume(volume);

    // Cancel any previous playback and get a new cancellation token
    let cancel_token = {
        let mut guard = playback.lock().map_err(|e| e.to_string())?;
        guard.begin_new()
    };

    // Emit event to frontend
    app.emit("tts:started", &text).ok();

    // Play audio
    player.play_async_cancellable(audio, cancel_token).await
        .map_err(|e| format!("Audio playback failed: {}", e))?;

    // Emit completion event
    app.emit("tts:finished", &text).ok();

    let device_name = output_device.unwrap_or_else(|| "default".to_string());

    Ok(TtsResult {
        duration_secs,
        voice,
        success: true,
        output_device: device_name,
    })
}

fn synthesize_system_tts_audio(text: &str, rate: f32) -> Result<tts::TtsAudio, String> {
    #[cfg(target_os = "windows")]
    {
        let mut wav_path = std::env::temp_dir();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        wav_path.push(format!("localtrans_system_tts_{}.wav", ts));

        let escaped_text = text.replace('\'', "''");
        let escaped_wav = wav_path.to_string_lossy().replace('\'', "''");
        let ps_rate = ((rate - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let script = format!(
            "Add-Type -AssemblyName System.Speech; \
             $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             $s.Rate={}; \
             $s.SetOutputToWaveFile('{}'); \
             $s.Speak('{}'); \
             $s.Dispose();",
            ps_rate, escaped_wav, escaped_text
        );

        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .map_err(|e| format!("Failed to start PowerShell TTS: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("PowerShell TTS failed: {}", stderr));
        }

        let mut reader =
            hound::WavReader::open(&wav_path).map_err(|e| format!("Failed to read WAV: {}", e))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels.max(1);

        let mut pcm = Vec::<f32>::new();
        match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Int, 16) => {
                for s in reader.samples::<i16>() {
                    let v = s.map_err(|e| format!("WAV decode error: {}", e))?;
                    pcm.push(v as f32 / 32768.0);
                }
            }
            (hound::SampleFormat::Int, 32) => {
                for s in reader.samples::<i32>() {
                    let v = s.map_err(|e| format!("WAV decode error: {}", e))?;
                    pcm.push(v as f32 / i32::MAX as f32);
                }
            }
            (hound::SampleFormat::Float, 32) => {
                for s in reader.samples::<f32>() {
                    pcm.push(s.map_err(|e| format!("WAV decode error: {}", e))?);
                }
            }
            _ => {
                return Err(format!(
                    "Unsupported WAV format: {:?}/{}bit",
                    spec.sample_format, spec.bits_per_sample
                ));
            }
        }

        let samples = if channels == 1 {
            pcm
        } else {
            let c = channels as usize;
            let mut mono = Vec::with_capacity(pcm.len() / c + 1);
            for frame in pcm.chunks(c) {
                let sum: f32 = frame.iter().copied().sum();
                mono.push(sum / frame.len() as f32);
            }
            mono
        };

        let _ = std::fs::remove_file(&wav_path);
        let duration_secs = if sample_rate == 0 {
            0.0
        } else {
            samples.len() as f32 / sample_rate as f32
        };

        return Ok(tts::TtsAudio {
            samples,
            sample_rate,
            channels: 1,
            duration_secs,
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (text, rate);
        Err("System TTS is currently implemented for Windows only".to_string())
    }
}

/// Stop TTS playback
#[tauri::command]
pub fn stop_tts(playback: State<'_, StdMutex<PlaybackControl>>) -> Result<(), String> {
    tracing::info!("TTS playback stop requested");
    let guard = playback.lock().map_err(|e| e.to_string())?;
    guard.stop();
    Ok(())
}

/// Get TTS configuration
#[tauri::command]
pub fn get_tts_config() -> Result<TtsConfig, String> {
    Ok(TtsConfig::default())
}

/// Get default voice for language
#[tauri::command]
pub fn get_default_tts_voice(language: String) -> Result<Option<String>, String> {
    Ok(tts::get_default_voice(&language).map(|s| s.to_string()))
}

/// List custom voice models in a directory
#[tauri::command]
pub fn list_custom_voice_models(models_dir: String) -> Result<Vec<CustomVoiceModelInfo>, String> {
    let path = PathBuf::from(models_dir);
    
    tts::custom_voice::CustomVoiceEngine::list_custom_voices(&path)
        .map(|models| models.into_iter().map(|m| CustomVoiceModelInfo {
            name: m.name,
            path: m.path.to_string_lossy().to_string(),
            model_type: format!("{:?}", m.model_type).to_lowercase(),
        }).collect())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct CustomVoiceModelInfo {
    pub name: String,
    pub path: String,
    pub model_type: String,
}

/// Virtual audio driver information
#[derive(Debug, Serialize, Clone)]
pub struct VirtualAudioDriver {
    pub name: String,
    pub device_id: String,
    pub driver_type: VirtualDriverType,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub enum VirtualDriverType {
    VbAudioCable,
    Voicemeeter,
    VirtualAudioCable,
    HiFiCable,
    Other,
}

/// Virtual audio driver check result
#[derive(Debug, Serialize)]
pub struct VirtualDriverCheckResult {
    /// Whether any virtual audio driver is detected
    pub has_virtual_driver: bool,
    /// List of detected virtual audio drivers
    pub detected_drivers: Vec<VirtualAudioDriver>,
    /// Recommended action
    pub recommendation: String,
    /// Download URL for recommended driver
    pub download_url: Option<String>,
}

/// Check for virtual audio drivers
#[tauri::command]
pub fn check_virtual_audio_driver() -> Result<VirtualDriverCheckResult, String> {
    let devices = get_tts_output_devices()?;
    
    // Known virtual audio driver patterns
    let virtual_driver_patterns = [
        ("VB-Audio", VirtualDriverType::VbAudioCable, "https://vb-audio.com/Cable/"),
        ("CABLE", VirtualDriverType::VbAudioCable, "https://vb-audio.com/Cable/"),
        ("Voicemeeter", VirtualDriverType::Voicemeeter, "https://vb-audio.com/Voicemeeter/"),
        ("Virtual Audio Cable", VirtualDriverType::VirtualAudioCable, "https://software.muzychenko.net/trial.htm"),
        ("Hi-Fi Cable", VirtualDriverType::HiFiCable, "https://vb-audio.com/Cable/"),
        ("Virtual", VirtualDriverType::Other, "https://vb-audio.com/Cable/"),
    ];

    let mut detected_drivers: Vec<VirtualAudioDriver> = Vec::new();

    for device in &devices {
        for (pattern, driver_type, _url) in &virtual_driver_patterns {
            if device.name.to_lowercase().contains(&pattern.to_lowercase()) {
                // Check if already detected
                if !detected_drivers.iter().any(|d| d.device_id == device.id) {
                    detected_drivers.push(VirtualAudioDriver {
                        name: device.name.clone(),
                        device_id: device.id.clone(),
                        driver_type: driver_type.clone(),
                    });
                }
                break;
            }
        }
    }

    let has_virtual_driver = !detected_drivers.is_empty();
    
    let (recommendation, download_url) = if has_virtual_driver {
        let driver_names: Vec<&str> = detected_drivers.iter()
            .map(|d| d.name.as_str())
            .collect();
        (
            format!("已检测到虚拟音频驱动: {}。请在设置中选择该设备作为TTS输出。", driver_names.join(", ")),
            None,
        )
    } else {
        (
            "未检测到虚拟音频驱动。建议安装 VB-Audio Virtual Cable（免费）以将翻译语音输出到会议软件。".to_string(),
            Some("https://vb-audio.com/Cable/".to_string()),
        )
    };

    Ok(VirtualDriverCheckResult {
        has_virtual_driver,
        detected_drivers,
        recommendation,
        download_url,
    })
}

/// Open URL in default browser
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            // `start` treats the first quoted argument as a window title.
            // Provide an explicit empty title so paths/URLs with spaces work.
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }

    Ok(())
}
