use std::path::PathBuf;

use serde_json::{json, Value};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

fn arg_str<'a>(args: &'a Value, key: &str) -> AppResult<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidState(format!("missing string arg: {key}")))
}

fn arg_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn arg_usize(args: &Value, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(default)
}

pub fn execute_named(
    name: &str,
    args: Value,
    app: Option<AppHandle>,
) -> AppResult<Value> {
    match name {
        "hello" => Ok(serde_json::to_value(super::hello::hello(arg_str(&args, "name")?.to_string())?)
            .map_err(|e| AppError::Io(e.to_string()))?),
        "version" => Ok(serde_json::to_value(super::version::version()?)
            .map_err(|e| AppError::Io(e.to_string()))?),
        "process_file" => Ok(serde_json::to_value(super::process_file::process_file(PathBuf::from(
            arg_str(&args, "input")?,
        ))?)
        .map_err(|e| AppError::Io(e.to_string()))?),

        "download_model" => Ok(serde_json::to_value(super::model::download_model_cli(
            arg_str(&args, "model_id")?.to_string().replace('\\', ""),
            arg_str(&args, "model_type")?.to_string().replace('\\', ""),
        )?)
        .map_err(|e| AppError::Io(e.to_string()))?),
        "list_models" => Ok(serde_json::to_value(super::model::list_models(
            arg_str(&args, "model_type")?.to_string(),
        )?)
        .map_err(|e| AppError::Io(e.to_string()))?),
        "delete_model" => Ok(serde_json::to_value(super::model::delete_model(
            arg_str(&args, "model_id")?.to_string(),
        )?)
        .map_err(|e| AppError::Io(e.to_string()))?),
        "get_models_dir" => Ok(serde_json::to_value(super::model::get_models_dir()?)
            .map_err(|e| AppError::Io(e.to_string()))?),

        "session_start" => {
            if let Some(app) = app {
                super::session::start_session(
                    app,
                    super::session::SessionConfig {
                        source_lang: arg_str(&args, "source_lang")?.to_string(),
                        target_lang: arg_str(&args, "target_lang")?.to_string(),
                        input_device: None,
                        peer_input_device: None,
                        bidirectional: arg_bool(&args, "bidirectional", false),
                        loci_enhanced: true,
                        vad_frame_ms: None,
                        vad_threshold: None,
                        stream_translation_interval_ms: None,
                        stream_translation_min_chars: None,
                    },
                )?;
                Ok(serde_json::to_value(super::session::get_session_status()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            } else {
                Ok(serde_json::to_value(super::session::start_session_cli(
                    arg_str(&args, "source_lang")?.to_string(),
                    arg_str(&args, "target_lang")?.to_string(),
                    arg_bool(&args, "bidirectional", false),
                )?)
                .map_err(|e| AppError::Io(e.to_string()))?)
            }
        }
        "session_pause" => {
            if let Some(app) = app {
                super::session::pause_session(app)?;
                Ok(serde_json::to_value(super::session::get_session_status()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            } else {
                Ok(serde_json::to_value(super::session::pause_session_cli()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            }
        }
        "session_resume" => {
            if let Some(app) = app {
                super::session::resume_session(app)?;
                Ok(serde_json::to_value(super::session::get_session_status()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            } else {
                Ok(serde_json::to_value(super::session::resume_session_cli()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            }
        }
        "session_stop" => {
            if let Some(app) = app {
                super::session::stop_session(app)?;
                Ok(serde_json::to_value(super::session::get_session_status()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            } else {
                Ok(serde_json::to_value(super::session::stop_session_cli()?)
                    .map_err(|e| AppError::Io(e.to_string()))?)
            }
        }
        "session_status" | "get_session_status" => Ok(
            serde_json::to_value(super::session::get_session_status()?)
                .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "session_stats" | "get_session_stats" => Ok(
            serde_json::to_value(super::session::get_session_stats()?)
                .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "session_history" | "get_session_history" => Ok(
            serde_json::to_value(super::session::get_session_history_cli(Some(arg_usize(
                &args, "count", 20,
            )))?)
            .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "session_clear_history" | "clear_session_history" => {
            super::session::clear_session_history_cli()?;
            Ok(Value::Null)
        }
        "session_export_history" | "export_history" => Ok(
            serde_json::to_value(super::session::export_history_cli(
                args.get("output")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            )?)
            .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "session_update_languages" | "update_languages" => Ok(
            serde_json::to_value(super::session::update_languages_cli(
                arg_str(&args, "source_lang")?.to_string(),
                arg_str(&args, "target_lang")?.to_string(),
            )?)
            .map_err(|e| AppError::Io(e.to_string()))?,
        ),

        "translate_text" => Ok(serde_json::to_value(super::translation::translate_text(
            super::translation::TranslateRequest {
                text: arg_str(&args, "text")?.to_string(),
                source_lang: arg_str(&args, "source_lang")?.to_string(),
                target_lang: arg_str(&args, "target_lang")?.to_string(),
                engine: Some("loci".to_string()),
                model_path: None,
            },
        )?)
        .map_err(|e| AppError::Io(e.to_string()))?),

        "get_runtime_status" => Ok(serde_json::to_value(super::system::get_runtime_status()?)
            .map_err(|e| AppError::Io(e.to_string()))?),
        "get_log_status" => Ok(serde_json::to_value(super::system::get_log_status()?)
            .map_err(|e| AppError::Io(e.to_string()))?),
        "get_audio_devices" => Ok(serde_json::to_value(super::audio::get_audio_devices()?)
            .map_err(|e| AppError::Io(e.to_string()))?),
        "get_tts_output_devices" => Ok(
            serde_json::to_value(super::audio::get_tts_output_devices()?)
                .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "get_tts_voices" => Ok(
            serde_json::to_value(super::tts::get_tts_voices(
                args.get("language").and_then(Value::as_str).map(ToString::to_string),
            )?)
            .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "get_tts_config" => Ok(serde_json::to_value(super::tts::get_tts_config()?)
            .map_err(|e| AppError::Io(e.to_string()))?),
        "get_default_tts_voice" => Ok(
            serde_json::to_value(super::tts::get_default_tts_voice(
                arg_str(&args, "language")?.to_string(),
            )?)
            .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "list_custom_voice_models" => Ok(
            serde_json::to_value(super::tts::list_custom_voice_models(
                args.get("models_dir")
                    .and_then(Value::as_str)
                    .map(ToString::to_string),
            )?)
            .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "check_virtual_audio_driver" => Ok(
            serde_json::to_value(super::audio::check_virtual_audio_driver()?)
                .map_err(|e| AppError::Io(e.to_string()))?,
        ),
        "start_capture" => {
            super::audio::start_capture(args.get("deviceId").and_then(Value::as_str).map(|s| s.to_string()))?;
            Ok(Value::Null)
        }
        "stop_capture" => {
            super::audio::stop_capture()?;
            Ok(Value::Null)
        }

        "speak_text" => {
            let req = args
                .get("request")
                .cloned()
                .ok_or_else(|| AppError::InvalidState("missing request".to_string()))?;
            let parsed: super::tts::TtsRequest =
                serde_json::from_value(req).map_err(|e| AppError::InvalidState(e.to_string()))?;
            Ok(serde_json::to_value(super::tts::speak_text(parsed)?)
                .map_err(|e| AppError::Io(e.to_string()))?)
        }
        "stop_tts" => {
            super::tts::stop_tts()?;
            Ok(Value::Null)
        }
        "run_tts_system_doctor_playback" => {
            let req = args
                .get("request")
                .cloned()
                .ok_or_else(|| AppError::InvalidState("missing request".to_string()))?;
            let parsed: super::tts::TtsSystemDoctorPlaybackRequest =
                serde_json::from_value(req).map_err(|e| AppError::InvalidState(e.to_string()))?;
            Ok(serde_json::to_value(super::tts::run_tts_system_doctor_playback(parsed)?)
                .map_err(|e| AppError::Io(e.to_string()))?)
        }

        "set_app_config" => {
            super::config::set_app_config(args.get("config").cloned().unwrap_or_else(|| json!({})))?;
            Ok(Value::Null)
        }
        "get_app_config" => Ok(super::config::get_app_config()?),
        "set_config_value" => {
            super::config::set_config_value(
                arg_str(&args, "key")?.to_string(),
                args.get("value").cloned().unwrap_or(Value::Null),
            )?;
            Ok(Value::Null)
        }
        "get_config_value" => Ok(super::config::get_config_value(arg_str(&args, "key")?.to_string())?),

        _ => Err(AppError::InvalidState(format!("unsupported command: {name}"))),
    }
}
