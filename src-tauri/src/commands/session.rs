use std::path::Path;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};
use crate::pipeline::{HistoryItem, PipelineConfig, PipelineState, PipelineStats, RealtimePipeline};
use crate::session_bus;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub source_lang: String,
    pub target_lang: String,
    pub input_device: Option<String>,
    pub peer_input_device: Option<String>,
    pub bidirectional: bool,
    pub loci_enhanced: bool,
    pub vad_frame_ms: Option<u32>,
    pub vad_threshold: Option<f32>,
    pub stream_translation_interval_ms: Option<u32>,
    pub stream_translation_min_chars: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub total_audio_duration_ms: u64,
    pub speech_duration_ms: u64,
    pub transcription_count: u64,
    pub translation_count: u64,
    pub average_latency_ms: f32,
    pub asr_average_latency_ms: f32,
    pub translation_average_latency_ms: f32,
    pub tts_average_latency_ms: f32,
    pub timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct SessionStatus {
    pub state: String,
    pub label: String,
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHistoryItem {
    pub id: String,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub timestamp: String,
    pub confidence: f32,
}

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("failed to init tokio runtime for session")
    })
}

fn pipeline_state() -> &'static Mutex<Option<Arc<RealtimePipeline>>> {
    static PIPE: OnceLock<Mutex<Option<Arc<RealtimePipeline>>>> = OnceLock::new();
    PIPE.get_or_init(|| Mutex::new(None))
}

fn to_status(state: PipelineState) -> SessionStatus {
    match state {
        PipelineState::Idle => SessionStatus {
            state: "idle".to_string(),
            label: "已停止".to_string(),
            is_running: false,
        },
        PipelineState::Initializing => SessionStatus {
            state: "initializing".to_string(),
            label: "启动中".to_string(),
            is_running: true,
        },
        PipelineState::Running => SessionStatus {
            state: "running".to_string(),
            label: "实时转译中".to_string(),
            is_running: true,
        },
        PipelineState::Paused => SessionStatus {
            state: "paused".to_string(),
            label: "已暂停".to_string(),
            is_running: true,
        },
        PipelineState::Stopping => SessionStatus {
            state: "stopping".to_string(),
            label: "停止中".to_string(),
            is_running: true,
        },
        PipelineState::Error => SessionStatus {
            state: "error".to_string(),
            label: "错误".to_string(),
            is_running: false,
        },
    }
}

fn to_session_stats(stats: PipelineStats) -> SessionStats {
    SessionStats {
        total_audio_duration_ms: stats.total_audio_duration_ms,
        speech_duration_ms: stats.speech_duration_ms,
        transcription_count: stats.transcription_count,
        translation_count: stats.translation_count,
        average_latency_ms: stats.average_latency_ms(),
        asr_average_latency_ms: stats.average_latency_ms(),
        translation_average_latency_ms: 0.0,
        tts_average_latency_ms: 0.0,
        timestamp: chrono_like_timestamp(),
    }
}

fn map_history_item(item: HistoryItem) -> SessionHistoryItem {
    SessionHistoryItem {
        id: item.id,
        source_text: item.source_text,
        translated_text: item.translated_text,
        source_lang: item.source_lang,
        target_lang: item.target_lang,
        timestamp: item.timestamp.to_rfc3339(),
        confidence: item.confidence,
    }
}

fn ensure_not_running() -> AppResult<()> {
    let current = pipeline_state().lock();
    if let Some(pipe) = current.as_ref() {
        let state = rt().block_on(pipe.get_state());
        if matches!(state, PipelineState::Running | PipelineState::Initializing) {
            return Err(AppError::InvalidState("session is already running".to_string()));
        }
    }
    Ok(())
}

fn cfg_to_pipeline(config: SessionConfig) -> PipelineConfig {
    let mut pipeline = PipelineConfig {
        source_lang: config.source_lang,
        target_lang: config.target_lang,
        input_device: config.input_device,
        peer_input_device: config.peer_input_device,
        bidirectional: config.bidirectional,
        loci_enhanced: config.loci_enhanced,
        ..Default::default()
    };
    if let Some(v) = config.vad_frame_ms {
        pipeline.vad_frame_ms = v;
    }
    if let Some(v) = config.vad_threshold {
        pipeline.vad_threshold = v;
    }
    if let Some(v) = config.stream_translation_interval_ms {
        pipeline.stream_translation_interval_ms = u64::from(v).clamp(300, 5000);
    }
    if let Some(v) = config.stream_translation_min_chars {
        pipeline.stream_translation_min_chars = usize::try_from(v).unwrap_or(8).clamp(2, 64);
    }
    pipeline
}

fn write_runtime_state(
    status: &str,
    source_lang: String,
    target_lang: String,
    bidirectional: bool,
) {
    let _ = session_bus::write_state(&session_bus::SessionRuntimeState {
        pid: std::process::id(),
        status: status.to_string(),
        source_lang,
        target_lang,
        bidirectional,
        tts_enabled: true,
        start_unix_ms: session_bus::now_unix_ms(),
        last_heartbeat_unix_ms: session_bus::now_unix_ms(),
        utterance_count: 0,
        error_count: 0,
        last_error: None,
    });
}

fn update_runtime_status(status: &str) {
    if let Some(mut st) = session_bus::read_state() {
        st.status = status.to_string();
        st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
        let _ = session_bus::write_state(&st);
    }
}

fn apply_pending_control(pipe: Arc<RealtimePipeline>) {
    let Some(control) = session_bus::read_control() else {
        return;
    };

    match control.command.as_str() {
        "pause" => {
            if rt().block_on(pipe.pause()).is_ok() {
                update_runtime_status("paused");
            }
        }
        "resume" => {
            if rt().block_on(pipe.resume()).is_ok() {
                update_runtime_status("running");
            }
        }
        "stop" => {
            if rt().block_on(pipe.stop()).is_ok() {
                *pipeline_state().lock() = None;
                write_runtime_state("idle", String::new(), String::new(), false);
            }
        }
        "update_languages" => {
            if let Some(src) = control.source_lang.as_deref() {
                let _ = rt().block_on(pipe.set_source_lang(src));
            }
            if let Some(mut st) = session_bus::read_state() {
                if let Some(src) = control.source_lang {
                    st.source_lang = src;
                }
                if let Some(dst) = control.target_lang {
                    st.target_lang = dst;
                }
                st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
                let _ = session_bus::write_state(&st);
            }
        }
        _ => {}
    }

    session_bus::clear_control();
}

#[tauri::command]
pub fn start_session(app: AppHandle, config: SessionConfig) -> AppResult<()> {
    ensure_not_running()?;
    if !super::model::has_ready_model("asr")? {
        return Err(AppError::InvalidState(
            "ASR model is required before starting session".to_string(),
        ));
    }

    if is_english_only_asr_model() && config.source_lang != "en" {
        tracing::warn!(
            source_lang = %config.source_lang,
            "english-only ASR model detected; session may degrade to english-only"
        );
    }

    let pipeline_cfg = cfg_to_pipeline(config);
    let pipeline = Arc::new(
        RealtimePipeline::new(app, pipeline_cfg.clone())
            .map_err(|e| AppError::InvalidState(format!("create pipeline failed: {e}")))?,
    );
    rt().block_on(pipeline.start())
        .map_err(|e| AppError::InvalidState(format!("start pipeline failed: {e}")))?;
    *pipeline_state().lock() = Some(pipeline);

    write_runtime_state(
        "running",
        pipeline_cfg.source_lang,
        pipeline_cfg.target_lang,
        pipeline_cfg.bidirectional,
    );
    Ok(())
}

#[tauri::command]
pub fn stop_session(_app: AppHandle) -> AppResult<()> {
    let pipe = pipeline_state().lock().clone();
    if let Some(pipe) = pipe {
        rt().block_on(pipe.stop())
            .map_err(|e| AppError::InvalidState(format!("stop pipeline failed: {e}")))?;
    } else {
        let _ = session_bus::write_control("stop", None, None);
    }
    *pipeline_state().lock() = None;
    write_runtime_state("idle", String::new(), String::new(), false);
    Ok(())
}

#[tauri::command]
pub fn pause_session(_app: AppHandle) -> AppResult<()> {
    let pipe = pipeline_state().lock().clone();
    if let Some(pipe) = pipe {
        rt().block_on(pipe.pause())
            .map_err(|e| AppError::InvalidState(format!("pause pipeline failed: {e}")))?;
        if let Some(mut st) = session_bus::read_state() {
            st.status = "paused".to_string();
            st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
            let _ = session_bus::write_state(&st);
        }
    } else {
        let _ = session_bus::write_control("pause", None, None);
    }
    Ok(())
}

#[tauri::command]
pub fn resume_session(_app: AppHandle) -> AppResult<()> {
    let pipe = pipeline_state().lock().clone();
    if let Some(pipe) = pipe {
        rt().block_on(pipe.resume())
            .map_err(|e| AppError::InvalidState(format!("resume pipeline failed: {e}")))?;
        if let Some(mut st) = session_bus::read_state() {
            st.status = "running".to_string();
            st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
            let _ = session_bus::write_state(&st);
        }
    } else {
        let _ = session_bus::write_control("resume", None, None);
    }
    Ok(())
}

pub fn start_session_cli(
    source_lang: String,
    target_lang: String,
    bidirectional: bool,
) -> AppResult<SessionStatus> {
    write_runtime_state("running", source_lang, target_lang, bidirectional);
    Ok(SessionStatus {
        state: "running".to_string(),
        label: "实时转译中".to_string(),
        is_running: true,
    })
}

pub fn stop_session_cli() -> AppResult<SessionStatus> {
    let _ = session_bus::write_control("stop", None, None);
    write_runtime_state("idle", String::new(), String::new(), false);
    Ok(SessionStatus {
        state: "idle".to_string(),
        label: "已停止".to_string(),
        is_running: false,
    })
}

pub fn pause_session_cli() -> AppResult<SessionStatus> {
    let _ = session_bus::write_control("pause", None, None);
    update_runtime_status("paused");
    Ok(SessionStatus {
        state: "paused".to_string(),
        label: "已暂停".to_string(),
        is_running: true,
    })
}

pub fn resume_session_cli() -> AppResult<SessionStatus> {
    let _ = session_bus::write_control("resume", None, None);
    update_runtime_status("running");
    Ok(SessionStatus {
        state: "running".to_string(),
        label: "实时转译中".to_string(),
        is_running: true,
    })
}

pub fn session_status_cli() -> AppResult<SessionStatus> {
    let pipe = pipeline_state().lock().clone();
    if let Some(pipe) = pipe {
        apply_pending_control(pipe.clone());
        let st = rt().block_on(pipe.get_state());
        if matches!(st, PipelineState::Running | PipelineState::Paused | PipelineState::Initializing) {
            update_runtime_status(match st {
                PipelineState::Running => "running",
                PipelineState::Paused => "paused",
                PipelineState::Initializing => "starting",
                _ => "running",
            });
        }
        return Ok(to_status(st));
    }
    if let Some(st) = session_bus::read_state() {
        if session_bus::is_session_alive(&st) {
            return Ok(match st.status.as_str() {
                "running" => SessionStatus {
                    state: "running".to_string(),
                    label: "实时转译中".to_string(),
                    is_running: true,
                },
                "paused" => SessionStatus {
                    state: "paused".to_string(),
                    label: "已暂停".to_string(),
                    is_running: true,
                },
                _ => SessionStatus {
                    state: "idle".to_string(),
                    label: "已停止".to_string(),
                    is_running: false,
                },
            });
        }
    }
    Ok(to_status(PipelineState::Idle))
}

#[tauri::command]
pub fn get_session_status() -> AppResult<SessionStatus> {
    session_status_cli()
}

#[tauri::command]
pub fn get_session_stats() -> AppResult<SessionStats> {
    let pipe = pipeline_state().lock().clone();
    if let Some(pipe) = pipe {
        return Ok(to_session_stats(pipe.get_stats()));
    }
    if let Some(m) = session_bus::read_metrics() {
        return Ok(SessionStats {
            total_audio_duration_ms: m.total_audio_duration_ms,
            speech_duration_ms: m.speech_duration_ms,
            transcription_count: m.transcription_count,
            translation_count: m.translation_count,
            average_latency_ms: m.average_latency_ms,
            asr_average_latency_ms: m.asr_average_latency_ms,
            translation_average_latency_ms: m.translation_average_latency_ms,
            tts_average_latency_ms: m.tts_average_latency_ms,
            timestamp: chrono_like_timestamp(),
        });
    }
    Ok(SessionStats {
        total_audio_duration_ms: 0,
        speech_duration_ms: 0,
        transcription_count: 0,
        translation_count: 0,
        average_latency_ms: 0.0,
        asr_average_latency_ms: 0.0,
        translation_average_latency_ms: 0.0,
        tts_average_latency_ms: 0.0,
        timestamp: chrono_like_timestamp(),
    })
}

pub fn get_session_history_cli(count: Option<usize>) -> AppResult<Vec<SessionHistoryItem>> {
    let count = count.unwrap_or(20);
    let pipe = pipeline_state().lock().clone();
    if let Some(pipe) = pipe {
        let items = rt().block_on(pipe.get_recent_history(count));
        return Ok(items.into_iter().map(map_history_item).collect());
    }
    let items = session_bus::read_history_recent(count)
        .map_err(|e| AppError::InvalidState(format!("read history failed: {e}")))?;
    Ok(items
        .into_iter()
        .map(|h| SessionHistoryItem {
            id: h.id,
            source_text: h.source_text,
            translated_text: h.translated_text,
            source_lang: h.source_lang,
            target_lang: h.target_lang,
            timestamp: h.timestamp,
            confidence: h.confidence,
        })
        .collect())
}

pub fn clear_session_history_cli() -> AppResult<()> {
    if let Some(pipe) = pipeline_state().lock().clone() {
        rt().block_on(pipe.clear_history());
    }
    session_bus::clear_history()
        .map_err(|e| AppError::InvalidState(format!("clear history failed: {e}")))?;
    session_bus::write_metrics(&session_bus::SessionMetrics::default())
        .map_err(|e| AppError::InvalidState(format!("clear metrics failed: {e}")))?;
    Ok(())
}

pub fn export_history_cli(output: Option<String>) -> AppResult<String> {
    let items = get_session_history_cli(Some(100_000))?;
    let json = serde_json::to_string_pretty(&items).map_err(|e| AppError::Io(e.to_string()))?;
    let out = output
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("session-history-export.json"));
    std::fs::write(&out, json)?;
    Ok(out.display().to_string())
}

pub fn update_languages_cli(source_lang: String, target_lang: String) -> AppResult<SessionStatus> {
    if let Some(pipe) = pipeline_state().lock().clone() {
        rt().block_on(pipe.set_source_lang(&source_lang))
            .map_err(|e| AppError::InvalidState(format!("update language failed: {e}")))?;
    }
    let _ = session_bus::write_control("update_languages", Some(&source_lang), Some(&target_lang));
    if let Some(mut st) = session_bus::read_state() {
        st.source_lang = source_lang;
        st.target_lang = target_lang;
        st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
        let _ = session_bus::write_state(&st);
    }
    session_status_cli()
}

#[tauri::command(rename_all = "snake_case")]
pub fn update_languages(source_lang: String, target_lang: String) -> AppResult<SessionStatus> {
    update_languages_cli(source_lang, target_lang)
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_session_history(count: Option<usize>) -> AppResult<Vec<SessionHistoryItem>> {
    get_session_history_cli(count)
}

#[tauri::command]
pub fn clear_session_history() -> AppResult<()> {
    clear_session_history_cli()
}

#[tauri::command]
pub fn export_history(output: Option<String>) -> AppResult<String> {
    export_history_cli(output)
}

pub(crate) fn note_translation() {
    if let Some(mut metrics) = session_bus::read_metrics() {
        metrics.translation_count = metrics.translation_count.saturating_add(1);
        metrics.last_updated_unix_ms = session_bus::now_unix_ms();
        let _ = session_bus::write_metrics(&metrics);
    } else {
        let mut metrics = session_bus::SessionMetrics::default();
        metrics.translation_count = 1;
        let _ = session_bus::write_metrics(&metrics);
    }
}

fn chrono_like_timestamp() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn is_english_only_asr_model() -> bool {
    let Some(asr_dir) = resolve_asr_model_dir() else {
        return false;
    };

    let dir_name = asr_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if dir_name.contains("-en") || dir_name.contains("_en") || dir_name.ends_with("en") {
        return true;
    }

    std::fs::read_dir(&asr_dir)
        .ok()
        .map(|entries| {
            entries.flatten().any(|entry| {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                name.contains("-en-") || name.contains(".en.") || name.contains("_en_")
            })
        })
        .unwrap_or(false)
}

fn resolve_asr_model_dir() -> Option<std::path::PathBuf> {
    let base = dirs::data_local_dir()?.join("LocalTrans").join("models").join("asr");
    if has_required_asr_files(&base) {
        return Some(base);
    }
    let entries = std::fs::read_dir(&base).ok()?;
    let mut best: Option<(u64, std::path::PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !has_required_asr_files(&path) {
            continue;
        }
        let size = dir_size(&path);
        match &best {
            Some((best_size, _)) if *best_size >= size => {}
            _ => best = Some((size, path)),
        }
    }
    best.map(|(_, p)| p)
}

fn has_required_asr_files(dir: &Path) -> bool {
    dir.join("tokens.txt").exists()
        && has_prefix_onnx(dir, "encoder")
        && has_prefix_onnx(dir, "decoder")
        && has_prefix_onnx(dir, "joiner")
}

fn has_prefix_onnx(dir: &Path, prefix: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        if !path.is_file() {
            return false;
        }
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        name.starts_with(prefix) && name.ends_with(".onnx")
    })
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total += meta.len();
            } else if meta.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}
