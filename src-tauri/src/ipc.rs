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
        asr_engine: Option<String>,
        translation_engine: Option<String>,
        tts_engine: Option<String>,
        bidirectional: bool,
        latency_profile: Option<String>,
    },
    SessionPause,
    SessionResume,
    SessionStop,
    SessionPreflight,
    LociRuntimeSnapshot {
        model_path: Option<String>,
    },
    LociGovernanceSnapshot {
        model_path: Option<String>,
    },
    LociWorkflowPolicy {
        model_path: Option<String>,
    },
    LociRewriterInventory {
        model_path: Option<String>,
    },
    LociLoadPlugins {
        path: String,
        source_kind: Option<String>,
        model_path: Option<String>,
    },
    LociActivateRewriter {
        component: String,
        plugin_name: String,
        model_path: Option<String>,
    },
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
        engine: Option<String>,
        model_path: Option<String>,
    },
    LogStatus,
    MtRuntimeCheck,
    SupportSnapshot,
    WorkflowProfiles,
    WorkflowApply {
        profile_id: String,
    },
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
    let req_text =
        serde_json::to_string(&req).map_err(|e| format!("encode ipc request failed: {e}"))?;
    stream
        .write_all(format!("{req_text}\n").as_bytes())
        .map_err(|e| format!("send ipc request failed: {e}"))?;
    stream
        .flush()
        .map_err(|e| format!("flush ipc request failed: {e}"))?;

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
        Err(resp
            .error
            .unwrap_or_else(|| "unknown ipc error".to_string()))
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
    let text = serde_json::to_string(&resp)
        .map_err(|e| AppError::Io(format!("ipc encode failed: {e}")))?;
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
            asr_engine,
            translation_engine,
            tts_engine,
            bidirectional,
            latency_profile,
        } => {
            commands::session::start_session(
                app,
                commands::session::SessionConfig {
                    source_lang,
                    target_lang,
                    asr_engine,
                    loci_enhanced: matches!(translation_engine.as_deref(), Some("loci")),
                    translation_engine,
                    input_device: None,
                    peer_input_device: None,
                    bidirectional,
                    vad_frame_ms: None,
                    vad_threshold: None,
                    stream_translation_interval_ms: None,
                    stream_translation_min_chars: None,
                    latency_profile,
                    tts_enabled: None,
                    tts_auto_play: None,
                    tts_engine,
                    tts_voice: None,
                    tts_rate: None,
                    tts_volume: None,
                    tts_output_device: None,
                    custom_voice_profile_id: None,
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
        IpcCommand::SessionPreflight => to_json(commands::system::get_session_preflight()),
        IpcCommand::LociRuntimeSnapshot { model_path } => {
            to_json(commands::loci_runtime::get_loci_runtime_snapshot(Some(
                commands::loci_runtime::LociSnapshotRequest { model_path },
            )))
        }
        IpcCommand::LociGovernanceSnapshot { model_path } => {
            to_json(commands::loci_runtime::get_loci_governance_snapshot(Some(
                commands::loci_runtime::LociSnapshotRequest { model_path },
            )))
        }
        IpcCommand::LociWorkflowPolicy { model_path } => {
            to_json(commands::loci_runtime::get_loci_workflow_policy(Some(
                commands::loci_runtime::LociSnapshotRequest { model_path },
            )))
        }
        IpcCommand::LociRewriterInventory { model_path } => {
            to_json(commands::loci_runtime::get_loci_rewriter_inventory(Some(
                commands::loci_runtime::LociSnapshotRequest { model_path },
            )))
        }
        IpcCommand::LociLoadPlugins {
            path,
            source_kind,
            model_path,
        } => to_json(commands::loci_runtime::load_loci_plugins(
            commands::loci_runtime::LoadLociPluginsRequest {
                model_path,
                path,
                source_kind,
            },
        )),
        IpcCommand::LociActivateRewriter {
            component,
            plugin_name,
            model_path,
        } => to_json(commands::loci_runtime::activate_loci_rewriter(
            commands::loci_runtime::ActivateLociRewriterRequest {
                model_path,
                component,
                plugin_name,
            },
        )),
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
        } => to_json(commands::session::update_languages_cli(
            source_lang,
            target_lang,
        )),
        IpcCommand::TranslateText {
            text,
            source_lang,
            target_lang,
            engine,
            model_path,
        } => to_json(commands::translation::translate_text(
            commands::translation::TranslateRequest {
                text,
                source_lang,
                target_lang,
                engine,
                model_path,
            },
        )),
        IpcCommand::LogStatus => to_json(commands::system::get_log_status()),
        IpcCommand::MtRuntimeCheck => to_json(commands::system::check_mt_runtime()),
        IpcCommand::SupportSnapshot => to_json(commands::system::get_support_snapshot()),
        IpcCommand::WorkflowProfiles => to_json(commands::system::list_workflow_profiles()),
        IpcCommand::WorkflowApply { profile_id } => {
            to_json(commands::system::apply_workflow_profile(
                commands::system::ApplyWorkflowProfileRequest { profile_id },
            ))
        }
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
