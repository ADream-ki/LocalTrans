//! TTS (Text-to-Speech) Commands

use anyhow::Result;
use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tauri::State;
use std::fs;
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
    /// Engine name: "sherpa-melo" | "edge-tts" | "piper" | "system" | "custom"
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

#[tauri::command]
pub async fn run_tts_system_doctor_playback(
    request: TtsSystemDoctorPlaybackRequest,
) -> Result<TtsSystemDoctorPlaybackResult, String> {
    let voice = request
        .voice
        .unwrap_or_else(|| "sherpa-melo-female".to_string());
    let lang_hint = request
        .language
        .unwrap_or_else(|| infer_lang_from_voice(&voice));

    let system_probe = synthesize_system_tts_audio("localtrans system tts doctor", 1.0);
    let (system_ok, system_detail) = match system_probe {
        Ok(a) => (
            true,
            format!("system tts ok, sample_rate={}, samples={}", a.sample_rate, a.samples.len()),
        ),
        Err(e) => (false, e),
    };

    let reason_text = if system_ok {
        if is_english_lang(&lang_hint) {
            "System TTS check passed.".to_string()
        } else {
            "系统 TTS 检查通过。".to_string()
        }
    } else {
        if is_english_lang(&lang_hint) {
            format!(
                "System TTS is currently unavailable. Reason: {}. Recommended: use Sherpa offline voice as default output.",
                summarize_reason(&system_detail, 180)
            )
        } else {
            format!(
                "系统 TTS 当前不可用。原因：{}。建议使用 Sherpa 离线音色作为默认输出。",
                summarize_reason(&system_detail, 180)
            )
        }
    };

    let (audio, reason_audio_engine) = match synthesize_sherpa_melo_audio(&reason_text, &voice, 1.0) {
        Ok(a) => (a, "sherpa-melo".to_string()),
        Err(sherpa_err) => {
            let edge_voice = if is_english_lang(&lang_hint) {
                "en-US-JennyNeural"
            } else {
                "zh-CN-XiaoxiaoNeural"
            };
            let edge_audio = synthesize_single_backend("edge", &reason_text, edge_voice, 1.0, 0)
                .await
                .map_err(|edge_err| {
                    format!(
                        "Failed to synthesize reason audio. sherpa={}, edge={}",
                        summarize_reason(&sherpa_err, 120),
                        summarize_reason(&edge_err, 120)
                    )
                })?;
            (edge_audio, "edge-tts".to_string())
        }
    };

    let reason_audio_path = if let Some(out_wav) = request.out_wav {
        let out_path = PathBuf::from(out_wav);
        if let Some(parent) = out_path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = fs::create_dir_all(parent);
            }
        }
        write_wav_f32(&out_path, &audio.samples, audio.sample_rate)?;
        Some(out_path.to_string_lossy().to_string())
    } else {
        None
    };

    let mut player = tts::playback::AudioPlayer::new(request.output_device.as_deref())
        .map_err(|e| format!("Failed to create audio player: {}", e))?;
    player.set_volume(1.0);
    player
        .play_async(audio)
        .await
        .map_err(|e| format!("Failed to play reason audio: {}", e))?;

    Ok(TtsSystemDoctorPlaybackResult {
        system_ok,
        system_detail,
        reason_text,
        reason_audio_path,
        reason_audio_engine,
        play_device: request.output_device,
    })
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

    let engine_name = request
        .engine
        .clone()
        .unwrap_or_else(|| "sherpa-melo".to_string());
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
        synthesize_with_backend_fallback(&engine_name, &text, &voice, rate, pitch).await?
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

async fn synthesize_with_backend_fallback(
    engine_name: &str,
    text: &str,
    voice: &str,
    rate: f32,
    pitch: i32,
) -> Result<tts::TtsAudio, String> {
    let order: Vec<&str> = match engine_name {
        "sherpa-melo" => vec!["sherpa", "edge", "system"],
        "edge-tts" => vec!["edge", "sherpa", "system"],
        "system" => vec!["system"],
        "piper" => {
            return Err("Piper TTS is temporarily disabled due to model compatibility issues on this build; use sherpa-melo/edge-tts/system".to_string())
        }
        other => return Err(format!("Unknown TTS engine: {}", other)),
    };
    let mut errors: Vec<String> = Vec::new();

    for backend in order {
        if !tts::tts_backend_allow_request(backend) {
            let snap = tts::tts_backend_snapshot(backend);
            errors.push(format!(
                "{} circuit {:?} remaining={}s",
                backend, snap.stage, snap.circuit_remaining_sec
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
                errors.push(format!("{} failed: {}", backend, e));
            }
        }
    }

    Err(format!("All TTS backends failed: {}", errors.join(" | ")))
}

async fn synthesize_single_backend(
    backend: &str,
    text: &str,
    voice: &str,
    rate: f32,
    pitch: i32,
) -> Result<tts::TtsAudio, String> {
    match backend {
        "sherpa" => synthesize_sherpa_melo_audio(text, voice, rate),
        "edge" => {
            let engine = tts::edge_tts::EdgeTtsEngine::new()
                .map_err(|e| format!("Failed to create Edge TTS engine: {}", e))?;
            match tokio::time::timeout(
                std::time::Duration::from_secs(4),
                engine.synthesize_with_prosody(text, voice, rate, pitch),
            )
            .await
            {
                Ok(Ok(audio)) => Ok(audio),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("timeout >4s".to_string()),
            }
        }
        "system" => synthesize_system_tts_audio(text, rate),
        other => Err(format!("Unsupported backend: {}", other)),
    }
}

fn synthesize_sherpa_melo_audio(text: &str, voice: &str, rate: f32) -> Result<tts::TtsAudio, String> {
    #[cfg(feature = "sherpa-backend")]
    {
        use sherpa_rs::tts::{VitsTts, VitsTtsConfig};
        use sherpa_rs::OnnxConfig;

        let base = dirs::data_local_dir()
            .ok_or_else(|| "Cannot determine local data dir".to_string())?
            .join("LocalTrans")
            .join("models")
            .join("tts")
            .join("sherpa")
            .join("vits-melo-tts-zh_en");
        let model_int8 = base.join("model.int8.onnx");
        let model_fp = base.join("model.onnx");
        let model = if model_int8.exists() { model_int8 } else { model_fp };
        let tokens = base.join("tokens.txt");
        if !model.exists() || !tokens.exists() {
            return Err(format!(
                "Sherpa Melo model files not found under {}",
                base.display()
            ));
        }

        let config = VitsTtsConfig {
            model: model.to_string_lossy().to_string(),
            tokens: tokens.to_string_lossy().to_string(),
            lexicon: if base.join("lexicon.txt").exists() {
                base.join("lexicon.txt").to_string_lossy().to_string()
            } else {
                String::new()
            },
            dict_dir: if base.join("dict").exists() {
                base.join("dict").to_string_lossy().to_string()
            } else {
                String::new()
            },
            onnx_config: OnnxConfig {
                provider: "cpu".to_string(),
                num_threads: 2,
                debug: false,
            },
            ..Default::default()
        };

        let mut tts = VitsTts::new(config);
        let sid = sherpa_speaker_id_from_voice(voice);
        let out = match tts.create(text, sid, rate.clamp(0.5, 2.0)) {
            Ok(v) => v,
            Err(e) => {
                if sid != 0 {
                    tracing::warn!(
                        "Sherpa speaker sid={} failed ({}), fallback to sid=0",
                        sid,
                        e
                    );
                    tts.create(text, 0, rate.clamp(0.5, 2.0))
                        .map_err(|e2| format!("Sherpa create failed (sid={}): {}; fallback sid=0 failed: {}", sid, e, e2))?
                } else {
                    return Err(format!("Sherpa create failed: {}", e));
                }
            }
        };
        let duration_secs = if out.sample_rate == 0 {
            0.0
        } else {
            out.samples.len() as f32 / out.sample_rate as f32
        };
        Ok(tts::TtsAudio {
            samples: out.samples,
            sample_rate: out.sample_rate,
            channels: 1,
            duration_secs,
        })
    }

    #[cfg(not(feature = "sherpa-backend"))]
    {
        let _ = (text, voice, rate);
        Err("Sherpa Melo TTS requires sherpa-backend feature".to_string())
    }
}

fn sherpa_speaker_id_from_voice(voice: &str) -> i32 {
    let v = voice.to_ascii_lowercase();
    if v.contains("sherpa-melo-male")
        || v.contains("yunxi")
        || v.contains("yunyang")
        || v.contains("guy")
        || v.contains("ryan")
        || v.contains("keita")
        || v.contains("injoon")
        || v.contains("male")
    {
        1
    } else {
        0
    }
}

fn synthesize_system_tts_audio(text: &str, rate: f32) -> Result<tts::TtsAudio, String> {
    #[cfg(target_os = "windows")]
    {
        if std::env::var("LOCALTRANS_DISABLE_SYSTEM_TTS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            return Err("System TTS is disabled by LOCALTRANS_DISABLE_SYSTEM_TTS".to_string());
        }
        if let Some(reason) = tts::system_tts_cached_skip_reason() {
            return Err(format!("System TTS skipped by cached host capability: {}", reason));
        }

        let mut wav_path = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
        wav_path.push("LocalTrans");
        wav_path.push("tts-tmp");
        if let Err(e) = std::fs::create_dir_all(&wav_path) {
            return Err(format!("Failed to create TTS temp dir: {}", e));
        }
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        wav_path.push(format!("localtrans_system_tts_{}_{}.wav", ts, std::process::id()));

        let escaped_text = text.replace('\'', "''").replace('\r', " ").replace('\n', " ");
        let escaped_wav = wav_path.to_string_lossy().replace('\'', "''");
        let ps_rate = ((rate - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let script_system_speech = format!(
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
        let script_sapi_file = format!(
            "$ErrorActionPreference='Stop'; \
             $voice=$null; $stream=$null; \
             try {{ \
               $voice=New-Object -ComObject SAPI.SpVoice; \
               if ($null -eq $voice) {{ throw 'SpVoice is null' }}; \
               try {{ $voice.Rate={} }} catch {{ }}; \
               $stream=New-Object -ComObject SAPI.SpFileStream; \
               if ($null -eq $stream) {{ throw 'SpFileStream is null' }}; \
               $stream.Open('{}', 3, $false); \
               $voice.AudioOutputStream=$stream; \
               [void]$voice.Speak('{}'); \
               $stream.Close(); \
             }} finally {{ \
               if ($null -ne $stream) {{ [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($stream) }}; \
               if ($null -ne $voice) {{ [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($voice) }}; \
             }}; \
             if (-not (Test-Path '{}')) {{ throw 'wav not created' }}",
            ps_rate, escaped_wav, escaped_text, escaped_wav
        );
        let strategies = [
            ("system_speech", script_system_speech.as_str()),
            ("sapi_file", script_sapi_file.as_str()),
        ];
        let mut last_errs: Vec<String> = Vec::new();
        for exe in windows_powershell_candidates() {
            for (name, script) in strategies {
                for sta in [true, false] {
                    match run_powershell_encoded(exe, sta, script) {
                        Ok(_) => {
                            if wav_path.exists() {
                                last_errs.clear();
                                break;
                            }
                            last_errs.push(format!(
                                "{} {} {}: wav not created",
                                exe,
                                if sta { "sta" } else { "mta" },
                                name
                            ));
                        }
                        Err(e) => last_errs.push(format!(
                            "{} {} {}: {}",
                            exe,
                            if sta { "sta" } else { "mta" },
                            name,
                            e
                        )),
                    }
                    if wav_path.exists() {
                        break;
                    }
                }
                if wav_path.exists() {
                    break;
                }
            }
            if wav_path.exists() {
                break;
            }
        }
        if !wav_path.exists() {
            let joined = last_errs.iter().take(4).cloned().collect::<Vec<_>>().join(" | ");
            let lower = joined.to_ascii_lowercase();
            let blocked_by_host = lower.contains("0x80070005")
                || lower.contains("access is denied")
                || lower.contains("object reference not set");
            if blocked_by_host {
                let msg = "System TTS is unavailable on this host (speech runtime/permissions). For production, keep sherpa-melo/edge as default and use `localtrans-cli tts-system-doctor` for host diagnostics.".to_string();
                tts::system_tts_mark_failure(&msg);
                return Err(msg);
            }
            let msg = format!("System TTS probe failed across strategies: {}", joined);
            tts::system_tts_mark_failure(&msg);
            return Err(msg);
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
        if samples.is_empty() || duration_secs <= 0.0 {
            let msg = "System TTS produced empty audio".to_string();
            tts::system_tts_mark_failure(&msg);
            return Err(msg);
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
fn windows_powershell_candidates() -> &'static [&'static str] {
    &[
        "powershell.exe",
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    ]
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
        return Err(format!(
            "status {:?} stderr={}",
            output.status.code(),
            stderr.trim()
        ));
    }
    Ok(())
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
    let l = lang.to_ascii_lowercase();
    l.starts_with("en")
}

fn write_wav_f32(path: &PathBuf, samples: &[f32], sample_rate: u32) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)
        .map_err(|e| format!("Failed to create WAV {}: {}", path.display(), e))?;
    for s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        writer
            .write_sample((clamped * 32767.0) as i16)
            .map_err(|e| format!("Failed to write WAV sample: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV {}: {}", path.display(), e))?;
    Ok(())
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
