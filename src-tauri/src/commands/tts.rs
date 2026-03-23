use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::tts::{self, TtsEngine};

fn playback_control() -> &'static StdMutex<tts::playback::PlaybackControl> {
    static CONTROL: OnceLock<StdMutex<tts::playback::PlaybackControl>> = OnceLock::new();
    CONTROL.get_or_init(|| StdMutex::new(tts::playback::PlaybackControl::default()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomVoiceRequest {
    pub model_type: String,
    pub model_path: String,
    pub reference_audio: Option<String>,
    pub reference_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsRequest {
    pub text: String,
    pub voice: String,
    pub engine: Option<String>,
    pub rate: f32,
    pub pitch: Option<f32>,
    pub volume: Option<f32>,
    pub output_device: Option<String>,
    pub custom_voice: Option<CustomVoiceRequest>,
}

#[derive(Debug, Serialize)]
pub struct TtsResult {
    pub duration_secs: f32,
    pub voice: String,
    pub success: bool,
    pub output_device: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsSystemDoctorPlaybackRequest {
    pub output_device: Option<String>,
    pub voice: Option<String>,
    pub out_wav: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsSystemDoctorPlaybackResult {
    pub system_ok: bool,
    pub system_detail: String,
    pub reason_text: String,
    pub reason_audio_path: Option<String>,
    pub reason_audio_engine: String,
    pub play_device: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomVoiceModelInfo {
    pub name: String,
    pub path: String,
    pub model_type: String,
}

fn to_app_error<E: std::fmt::Display>(e: E) -> AppError {
    AppError::InvalidState(e.to_string())
}

fn runtime() -> AppResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(to_app_error)
}

#[tauri::command]
pub fn get_tts_voices(language: Option<String>) -> AppResult<Vec<tts::VoiceInfo>> {
    let voices = if let Some(lang) = language {
        let mut voices = tts::get_voices_for_lang(&lang);
        if voices.is_empty() {
            let normalized = lang.to_ascii_lowercase();
            voices = tts::get_all_voices()
                .into_iter()
                .filter(|v| v.language.to_ascii_lowercase().starts_with(&normalized))
                .collect();
        }
        voices
    } else {
        tts::get_all_voices()
    };
    Ok(voices)
}

#[tauri::command]
pub fn get_tts_config() -> AppResult<tts::TtsConfig> {
    Ok(tts::TtsConfig::default())
}

#[tauri::command]
pub fn get_default_tts_voice(language: String) -> AppResult<Option<String>> {
    Ok(tts::get_default_voice(&language).map(ToString::to_string))
}

#[tauri::command]
pub fn list_custom_voice_models(models_dir: Option<String>) -> AppResult<Vec<CustomVoiceModelInfo>> {
    let path = match models_dir {
        Some(p) => PathBuf::from(p),
        None => {
            let mut p = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
            p.push("LocalTrans");
            p.push("models");
            p.push("tts");
            p.push("custom");
            p
        }
    };

    if !path.exists() {
        return Ok(Vec::new());
    }

    let models = tts::custom_voice::CustomVoiceEngine::list_custom_voices(&path).map_err(to_app_error)?;
    Ok(models
        .into_iter()
        .map(|m| CustomVoiceModelInfo {
            name: m.name,
            path: m.path.to_string_lossy().to_string(),
            model_type: format!("{:?}", m.model_type).to_lowercase(),
        })
        .collect())
}

#[tauri::command]
pub fn speak_text(request: TtsRequest) -> AppResult<TtsResult> {
    if request.text.trim().is_empty() {
        return Err(AppError::InvalidState("Text cannot be empty".to_string()));
    }

    let voice = request.voice.clone();
    let output_device = request.output_device.clone();
    let rate = request.rate.clamp(0.5, 2.0);
    let pitch = request.pitch.unwrap_or(0.0).round().clamp(-50.0, 50.0) as i32;
    let volume = request.volume.unwrap_or(1.0).clamp(0.0, 1.0);
    let engine_name = request
        .engine
        .clone()
        .unwrap_or_else(|| "sherpa-melo".to_string());

    let rt = runtime()?;
    let audio = if let Some(custom) = request.custom_voice {
        let model_type = match custom.model_type.as_str() {
            "gpt-sovits" => tts::CustomVoiceType::GptSoVits,
            "rvc" => tts::CustomVoiceType::Rvc,
            "piper" => tts::CustomVoiceType::Piper,
            "vits" => tts::CustomVoiceType::Vits,
            other => {
                return Err(AppError::InvalidState(format!(
                    "Unknown custom voice model type: {other}"
                )))
            }
        };
        let config = tts::CustomVoiceConfig {
            model_type,
            model_path: PathBuf::from(custom.model_path),
            reference_audio: custom.reference_audio.map(PathBuf::from),
            reference_text: custom.reference_text,
            similarity: 0.8,
        };
        let engine = tts::custom_voice::CustomVoiceEngine::new(config).map_err(to_app_error)?;
        rt.block_on(async { engine.synthesize(&request.text, &voice).await })
            .map_err(to_app_error)?
    } else {
        rt.block_on(async {
            synthesize_with_backend_fallback(&engine_name, &request.text, &voice, rate, pitch).await
        })?
    };

    let mut player = tts::playback::AudioPlayer::new(output_device.as_deref()).map_err(to_app_error)?;
    player.set_volume(volume);
    let cancel = {
        let mut guard = playback_control()
            .lock()
            .map_err(|_| AppError::InvalidState("playback lock poisoned".to_string()))?;
        guard.begin_new()
    };
    rt.block_on(async { player.play_async_cancellable(audio.clone(), cancel).await })
        .map_err(to_app_error)?;

    Ok(TtsResult {
        duration_secs: audio.duration_secs,
        voice,
        success: true,
        output_device: output_device.unwrap_or_else(|| "default".to_string()),
    })
}

#[tauri::command]
pub fn stop_tts() -> AppResult<()> {
    let guard = playback_control()
        .lock()
        .map_err(|_| AppError::InvalidState("playback lock poisoned".to_string()))?;
    guard.stop();
    Ok(())
}

#[tauri::command]
pub fn run_tts_system_doctor_playback(
    request: TtsSystemDoctorPlaybackRequest,
) -> AppResult<TtsSystemDoctorPlaybackResult> {
    let voice = request
        .voice
        .clone()
        .unwrap_or_else(|| "en-US-JennyNeural".to_string());
    let language = request
        .language
        .unwrap_or_else(|| infer_lang_from_voice(&voice));

    let system_probe = synthesize_system_tts_audio("localtrans system tts doctor", 1.0);
    let (system_ok, system_detail) = match system_probe {
        Ok(audio) => (
            true,
            format!("system tts ok, sample_rate={}, samples={}", audio.sample_rate, audio.samples.len()),
        ),
        Err(e) => (false, e),
    };

    let reason_text = if system_ok {
        if is_english_lang(&language) {
            "System TTS check passed.".to_string()
        } else {
            "系统 TTS 检查通过。".to_string()
        }
    } else if is_english_lang(&language) {
        format!(
            "System TTS is currently unavailable. Reason: {}. Recommended: use Edge voice fallback.",
            summarize_reason(&system_detail, 180)
        )
    } else {
        format!(
            "系统 TTS 当前不可用。原因：{}。建议使用 Edge 语音作为回退。",
            summarize_reason(&system_detail, 180)
        )
    };

    let rt = runtime()?;
    let (audio, reason_audio_engine) = if system_ok {
        (synthesize_system_tts_audio(&reason_text, 1.0).map_err(to_app_error)?, "system".to_string())
    } else {
        let a = rt.block_on(async {
            synthesize_with_backend_fallback("edge-tts", &reason_text, &voice, 1.0, 0).await
        })?;
        (a, "edge-tts".to_string())
    };

    let reason_audio_path = if let Some(out_wav) = request.out_wav {
        let out_path = PathBuf::from(out_wav);
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        write_wav_f32(&out_path, &audio.samples, audio.sample_rate).map_err(to_app_error)?;
        Some(out_path.to_string_lossy().to_string())
    } else {
        None
    };

    let mut player = tts::playback::AudioPlayer::new(request.output_device.as_deref()).map_err(to_app_error)?;
    player.set_volume(1.0);
    let cancel = {
        let mut guard = playback_control()
            .lock()
            .map_err(|_| AppError::InvalidState("playback lock poisoned".to_string()))?;
        guard.begin_new()
    };
    rt.block_on(async { player.play_async_cancellable(audio, cancel).await })
        .map_err(to_app_error)?;

    Ok(TtsSystemDoctorPlaybackResult {
        system_ok,
        system_detail,
        reason_text,
        reason_audio_path,
        reason_audio_engine,
        play_device: request.output_device,
    })
}

async fn synthesize_with_backend_fallback(
    engine_name: &str,
    text: &str,
    voice: &str,
    rate: f32,
    pitch: i32,
) -> AppResult<tts::TtsAudio> {
    let order: Vec<&str> = match engine_name {
        "sherpa-melo" => vec!["edge", "system"],
        "edge-tts" => vec!["edge", "system"],
        "system" => vec!["system", "edge"],
        "piper" => vec!["piper", "edge", "system"],
        other => return Err(AppError::InvalidState(format!("Unknown TTS engine: {other}"))),
    };

    let mut errors = Vec::new();
    for backend in order {
        if !tts::tts_backend_allow_request(backend) {
            let snap = tts::tts_backend_snapshot(backend);
            errors.push(format!(
                "{backend} circuit {:?} remaining={}s",
                snap.stage, snap.circuit_remaining_sec
            ));
            continue;
        }

        match synthesize_single_backend(backend, text, voice, rate, pitch).await {
            Ok(audio) => {
                tts::tts_backend_record_success(backend);
                return Ok(audio);
            }
            Err(e) => {
                tts::tts_backend_record_failure(backend, Some(&e));
                errors.push(format!("{backend} failed: {e}"));
            }
        }
    }

    Err(AppError::InvalidState(format!(
        "All TTS backends failed: {}",
        errors.join(" | ")
    )))
}

async fn synthesize_single_backend(
    backend: &str,
    text: &str,
    voice: &str,
    rate: f32,
    pitch: i32,
) -> Result<tts::TtsAudio, String> {
    match backend {
        "edge" => {
            let engine = tts::edge_tts::EdgeTtsEngine::new().map_err(|e| e.to_string())?;
            tokio::time::timeout(
                std::time::Duration::from_secs(6),
                engine.synthesize_with_prosody(text, voice, rate, pitch),
            )
            .await
            .map_err(|_| "edge timeout >6s".to_string())?
            .map_err(|e| e.to_string())
        }
        "system" => synthesize_system_tts_audio(text, rate),
        "piper" => {
            let _ = rate;
            let engine = tts::piper_tts::PiperTtsEngine::new();
            engine.synthesize(text, voice).await.map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported backend: {other}")),
    }
}

fn synthesize_system_tts_audio(text: &str, rate: f32) -> Result<tts::TtsAudio, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(reason) = tts::system_tts_cached_skip_reason() {
            return Err(format!("System TTS skipped by cached host capability: {reason}"));
        }

        let mut wav_path = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
        wav_path.push("LocalTrans");
        wav_path.push("tts-tmp");
        if let Err(e) = std::fs::create_dir_all(&wav_path) {
            return Err(format!("Failed to create TTS temp dir: {e}"));
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        wav_path.push(format!("localtrans_system_tts_{ts}_{}.wav", std::process::id()));

        let escaped_text = text.replace('\'', "''").replace('\r', " ").replace('\n', " ");
        let escaped_wav = wav_path.to_string_lossy().replace('\'', "''");
        let ps_rate = ((rate - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let script = format!(
            "$ErrorActionPreference='Stop'; \
             Add-Type -AssemblyName System.Speech | Out-Null; \
             $s=$null; \
             try {{ \
               $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; \
               if ($null -eq $s) {{ throw 'SpeechSynthesizer is null' }}; \
               try {{ $s.Rate={} }} catch {{ }}; \
               $s.SetOutputToWaveFile('{}'); \
               $s.Speak('{}'); \
             }} finally {{ if ($null -ne $s) {{ $s.Dispose() }} }}; \
             if (-not (Test-Path '{}')) {{ throw 'wav not created' }}",
            ps_rate, escaped_wav, escaped_text, escaped_wav
        );

        let mut last_err = String::new();
        for exe in ["powershell.exe", "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"] {
            for sta in [true, false] {
                match run_powershell_encoded(exe, sta, &script) {
                    Ok(_) => {
                        last_err.clear();
                        break;
                    }
                    Err(e) => last_err = e,
                }
            }
            if wav_path.exists() {
                break;
            }
        }

        if !wav_path.exists() {
            tts::system_tts_mark_failure(&last_err);
            return Err(format!("System TTS failed: {last_err}"));
        }

        let mut reader = hound::WavReader::open(&wav_path).map_err(|e| format!("Failed to read WAV: {e}"))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels.max(1);
        let mut pcm = Vec::<f32>::new();
        match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Int, 16) => {
                for s in reader.samples::<i16>() {
                    pcm.push(s.map_err(|e| e.to_string())? as f32 / 32768.0);
                }
            }
            (hound::SampleFormat::Int, 32) => {
                for s in reader.samples::<i32>() {
                    pcm.push(s.map_err(|e| e.to_string())? as f32 / i32::MAX as f32);
                }
            }
            (hound::SampleFormat::Float, 32) => {
                for s in reader.samples::<f32>() {
                    pcm.push(s.map_err(|e| e.to_string())?);
                }
            }
            _ => {
                return Err(format!(
                    "Unsupported WAV format: {:?}/{}bit",
                    spec.sample_format, spec.bits_per_sample
                ))
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
        if samples.is_empty() || duration_secs <= 0.0 {
            tts::system_tts_mark_failure("System TTS produced empty audio");
            return Err("System TTS produced empty audio".to_string());
        }

        tts::system_tts_mark_success();
        Ok(tts::TtsAudio {
            samples,
            sample_rate,
            channels: 1,
            duration_secs,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (text, rate);
        Err("System TTS is currently implemented for Windows only".to_string())
    }
}

#[cfg(target_os = "windows")]
fn run_powershell_encoded(exe: &str, sta: bool, script: &str) -> Result<(), String> {
    let wrapped = format!(
        "$ProgressPreference='SilentlyContinue'; \
         $ErrorActionPreference='Stop'; \
         try {{ {} }} catch {{ \
           $msg=$_.Exception.Message; \
           if ($null -ne $_.Exception.InnerException) {{ $msg=$msg + ' | inner: ' + $_.Exception.InnerException.Message }}; \
           Write-Output ('__LTERR__ ' + $msg); \
           exit 87; \
         }}",
        script
    );
    let mut utf16 = Vec::<u8>::with_capacity(wrapped.len() * 2);
    for u in wrapped.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-OutputFormat")
        .arg("Text");
    if sta {
        cmd.arg("-Sta");
    }
    let output = cmd
        .arg("-EncodedCommand")
        .arg(encoded)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Some(idx) = stdout.find("__LTERR__ ") {
        return Err(stdout[idx + "__LTERR__ ".len()..].trim().to_string());
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("status {:?} stderr={}", output.status.code(), stderr.trim()));
    }
    Ok(())
}

fn infer_lang_from_voice(voice: &str) -> String {
    let v = voice.to_ascii_lowercase();
    if v.contains("en-") || v.contains("jenny") || v.contains("english") {
        "en".to_string()
    } else if v.contains("zh-") || v.contains("xiaoxiao") || v.contains("melo") {
        "zh".to_string()
    } else {
        "zh".to_string()
    }
}

fn is_english_lang(lang: &str) -> bool {
    lang.to_ascii_lowercase().starts_with("en")
}

fn summarize_reason(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn write_wav_f32(path: &PathBuf, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("Failed to create WAV {}: {}", path.display(), e))?;
    for s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        writer
            .write_sample((clamped * 32767.0) as i16)
            .map_err(|e| format!("Failed to write WAV sample: {e}"))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV {}: {}", path.display(), e))?;
    Ok(())
}
