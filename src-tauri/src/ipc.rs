use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

use crate::commands;
use crate::error::{AppError, AppResult};

const IPC_ADDR: &str = "127.0.0.1:38991";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "kebab-case")]
pub enum IpcCommand {
    Hello {
        name: String,
    },
    Version,
    ProcessFile {
        input: String,
    },
    DownloadModel {
        model_id: String,
        model_type: String,
    },
    ListModels {
        model_type: String,
    },
    DeleteModel {
        model_id: String,
    },
    SessionStart {
        source_lang: String,
        target_lang: String,
        bidirectional: bool,
    },
    SessionPause,
    SessionResume,
    SessionStop,
    SessionStatus,
    SessionStats,
    SessionHistory {
        count: usize,
    },
    SessionClearHistory,
    SessionExportHistory {
        output: Option<String>,
    },
    SessionUpdateLanguages {
        source_lang: String,
        target_lang: String,
    },
    TranslateText {
        text: String,
        source_lang: String,
        target_lang: String,
    },
    LogStatus,
    TtsVoices {
        language: Option<String>,
    },
    TtsConfig,
    TtsDefaultVoice {
        language: String,
    },
    TtsCustomVoices {
        models_dir: Option<String>,
    },
    ConfigSet {
        key: String,
        value: String,
    },
    ConfigGet {
        key: String,
    },
    Call {
        name: String,
        args: Value,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcRequest {
    command: IpcCommand,
}

#[derive(Debug, Serialize, Deserialize)]
struct IpcResponse {
    ok: bool,
    payload: Option<Value>,
    error: Option<String>,
}

pub fn start_gui_ipc_server(app: AppHandle) {
    thread::spawn(move || {
        let listener = match TcpListener::bind(IPC_ADDR) {
            Ok(l) => l,
            Err(_) => return,
        };

        for stream in listener.incoming().flatten() {
            let app = app.clone();
            thread::spawn(move || {
                let _ = handle_client(stream, app);
            });
        }
    });
}

pub fn try_send_command(command: IpcCommand) -> Result<Option<Value>, String> {
    let mut resolved = IPC_ADDR
        .to_socket_addrs()
        .map_err(|e| format!("resolve ipc addr failed: {e}"))?;
    let addr = match resolved.next() {
        Some(a) => a,
        None => return Ok(None),
    };

    let mut stream = match TcpStream::connect_timeout(&addr, Duration::from_millis(120)) {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let req = IpcRequest { command };
    let req_text = serde_json::to_string(&req).map_err(|e| format!("encode ipc request failed: {e}"))?;
    stream
        .write_all(format!("{req_text}\n").as_bytes())
        .map_err(|e| format!("send ipc request failed: {e}"))?;
    stream.flush().map_err(|e| format!("flush ipc request failed: {e}"))?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read ipc response failed: {e}"))?;
    if line.trim().is_empty() {
        return Err("empty ipc response".to_string());
    }

    let resp: IpcResponse =
        serde_json::from_str(&line).map_err(|e| format!("decode ipc response failed: {e}"))?;
    if resp.ok {
        Ok(Some(resp.payload.unwrap_or(Value::Null)))
    } else {
        Err(resp.error.unwrap_or_else(|| "unknown ipc error".to_string()))
    }
}

fn handle_client(mut stream: TcpStream, app: AppHandle) -> AppResult<()> {
    let mut line = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader
            .read_line(&mut line)
            .map_err(|e| AppError::Io(format!("ipc read failed: {e}")))?;
    }
    if line.trim().is_empty() {
        return Ok(());
    }

    let req: IpcRequest =
        serde_json::from_str(&line).map_err(|e| AppError::Io(format!("ipc parse failed: {e}")))?;
    let result = execute(req.command, app);
    let resp = match result {
        Ok(payload) => IpcResponse {
            ok: true,
            payload: Some(payload),
            error: None,
        },
        Err(err) => IpcResponse {
            ok: false,
            payload: None,
            error: Some(err.to_string()),
        },
    };
    let text =
        serde_json::to_string(&resp).map_err(|e| AppError::Io(format!("ipc encode failed: {e}")))?;
    stream
        .write_all(format!("{text}\n").as_bytes())
        .map_err(|e| AppError::Io(format!("ipc write failed: {e}")))?;
    stream
        .flush()
        .map_err(|e| AppError::Io(format!("ipc flush failed: {e}")))?;
    Ok(())
}

fn execute(command: IpcCommand, app: AppHandle) -> AppResult<Value> {
    match command {
        IpcCommand::Hello { name } => to_json(commands::hello::hello(name)),
        IpcCommand::Version => to_json(commands::version::version()),
        IpcCommand::ProcessFile { input } => {
            to_json(commands::process_file::process_file(PathBuf::from(input)))
        }
        IpcCommand::DownloadModel {
            model_id,
            model_type,
        } => to_json(commands::model::download_model(app, model_id, model_type)),
        IpcCommand::ListModels { model_type } => to_json(commands::model::list_models(model_type)),
        IpcCommand::DeleteModel { model_id } => to_json(commands::model::delete_model(model_id)),
        IpcCommand::SessionStart {
            source_lang,
            target_lang,
            bidirectional,
        } => {
            commands::session::start_session(
                app,
                commands::session::SessionConfig {
                    source_lang,
                    target_lang,
                    translation_engine: Some("nllb".to_string()),
                    input_device: None,
                    peer_input_device: None,
                    bidirectional,
                    loci_enhanced: false,
                    vad_frame_ms: None,
                    vad_threshold: None,
                    stream_translation_interval_ms: None,
                    stream_translation_min_chars: None,
                    tts_enabled: None,
                    tts_auto_play: None,
                    tts_engine: None,
                    tts_voice: None,
                    tts_rate: None,
                    tts_volume: None,
                    tts_output_device: None,
                    stream_tts_interval_ms: None,
                    stream_tts_min_chars: None,
                },
            )?;
            to_json(commands::session::get_session_status())
        }
        IpcCommand::SessionPause => {
            commands::session::pause_session(app)?;
            to_json(commands::session::get_session_status())
        }
        IpcCommand::SessionResume => {
            commands::session::resume_session(app)?;
            to_json(commands::session::get_session_status())
        }
        IpcCommand::SessionStop => {
            commands::session::stop_session(app)?;
            to_json(commands::session::get_session_status())
        }
        IpcCommand::SessionStatus => to_json(commands::session::get_session_status()),
        IpcCommand::SessionStats => to_json(commands::session::get_session_stats()),
        IpcCommand::SessionHistory { count } => {
            to_json(commands::session::get_session_history_cli(Some(count)))
        }
        IpcCommand::SessionClearHistory => to_json(commands::session::clear_session_history_cli()),
        IpcCommand::SessionExportHistory { output } => {
            to_json(commands::session::export_history_cli(output))
        }
        IpcCommand::SessionUpdateLanguages {
            source_lang,
            target_lang,
        } => to_json(commands::session::update_languages_cli(source_lang, target_lang)),
        IpcCommand::TranslateText {
            text,
            source_lang,
            target_lang,
        } => to_json(commands::translation::translate_text(
            commands::translation::TranslateRequest {
                text,
                source_lang,
                target_lang,
                engine: Some("nllb".to_string()),
                model_path: None,
            },
        )),
        IpcCommand::LogStatus => to_json(commands::system::get_log_status()),
        IpcCommand::TtsVoices { language } => to_json(commands::tts::get_tts_voices(language)),
        IpcCommand::TtsConfig => to_json(commands::tts::get_tts_config()),
        IpcCommand::TtsDefaultVoice { language } => {
            to_json(commands::tts::get_default_tts_voice(language))
        }
        IpcCommand::TtsCustomVoices { models_dir } => {
            to_json(commands::tts::list_custom_voice_models(models_dir))
        }
        IpcCommand::ConfigSet { key, value } => {
            let parsed = serde_json::from_str::<Value>(&value).unwrap_or(Value::String(value));
            to_json(commands::config::set_config_value(key, parsed))
        }
        IpcCommand::ConfigGet { key } => to_json(commands::config::get_config_value(key)),
        IpcCommand::Call { name, args } => commands::router::execute_named(&name, args, Some(app)),
    }
}

fn to_json<T: Serialize>(result: AppResult<T>) -> AppResult<Value> {
    let value = result?;
    serde_json::to_value(value).map_err(|e| AppError::Io(format!("json encode failed: {e}")))
}
