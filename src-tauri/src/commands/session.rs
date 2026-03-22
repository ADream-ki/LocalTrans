//! Session and pipeline commands for Tauri
//! 
//! This module provides the Tauri command handlers for managing
//! real-time transcription/translation sessions.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use parking_lot::Mutex as SyncMutex;
use uuid::Uuid;

use crate::pipeline::{
    RealtimePipeline, PipelineConfig, PipelineState, PipelineStats,
    HistoryItem,
};
use crate::session_bus;

/// Information about session status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStatusInfo {
    pub is_running: bool,
    pub status: String,
    pub source_lang: String,
    pub target_lang: String,
    pub bidirectional: bool,
}

/// Session configuration for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub source_lang: String,
    pub target_lang: String,
    pub input_device: Option<String>,
    pub peer_input_device: Option<String>,
    pub bidirectional: bool,
    pub loci_enhanced: bool,
    /// VAD frame size in milliseconds (controls latency vs stability)
    pub vad_frame_ms: Option<u32>,
    pub vad_threshold: Option<f32>,
    /// Streaming partial translation interval
    pub stream_translation_interval_ms: Option<u64>,
    /// Streaming partial translation minimum chars
    pub stream_translation_min_chars: Option<usize>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            input_device: None,
            peer_input_device: None,
            bidirectional: false,
            // Lower default latency: use lightweight translator unless explicitly enabled.
            loci_enhanced: false,
            vad_frame_ms: None,
            vad_threshold: None,
            stream_translation_interval_ms: None,
            stream_translation_min_chars: None,
        }
    }
}

impl From<SessionConfig> for PipelineConfig {
    fn from(config: SessionConfig) -> Self {
        let mut pipeline_config = PipelineConfig {
            source_lang: config.source_lang,
            target_lang: config.target_lang,
            input_device: config.input_device,
            peer_input_device: config.peer_input_device,
            bidirectional: config.bidirectional,
            loci_enhanced: config.loci_enhanced,
            ..Default::default()
        };

        if let Some(vad_frame_ms) = config.vad_frame_ms {
            pipeline_config.vad_frame_ms = vad_frame_ms;
        }
        
        if let Some(threshold) = config.vad_threshold {
            pipeline_config.vad_threshold = threshold;
        }
        if let Some(v) = config.stream_translation_interval_ms {
            pipeline_config.stream_translation_interval_ms = v.clamp(300, 5000);
        }
        if let Some(v) = config.stream_translation_min_chars {
            pipeline_config.stream_translation_min_chars = v.clamp(2, 64);
        }
        
        pipeline_config
    }
}

/// Statistics response for the frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub total_audio_duration_ms: u64,
    pub speech_duration_ms: u64,
    pub transcription_count: u64,
    pub translation_count: u64,
    pub average_latency_ms: f32,
    pub asr_average_latency_ms: f32,
    pub translation_average_latency_ms: f32,
    pub tts_average_latency_ms: f32,
    pub error_count: u64,
}

impl From<PipelineStats> for SessionStats {
    fn from(stats: PipelineStats) -> Self {
        Self {
            total_audio_duration_ms: stats.total_audio_duration_ms,
            speech_duration_ms: stats.speech_duration_ms,
            transcription_count: stats.transcription_count,
            translation_count: stats.translation_count,
            average_latency_ms: stats.average_latency_ms(),
            asr_average_latency_ms: stats.average_latency_ms(),
            translation_average_latency_ms: 0.0,
            tts_average_latency_ms: 0.0,
            error_count: stats.error_count,
        }
    }
}

/// History item for frontend display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItemInfo {
    pub id: String,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub timestamp: String,
    pub confidence: f32,
    pub loci_enhanced: bool,
}

impl From<HistoryItem> for HistoryItemInfo {
    fn from(item: HistoryItem) -> Self {
        Self {
            id: item.id,
            source_text: item.source_text,
            translated_text: item.translated_text,
            source_lang: item.source_lang,
            target_lang: item.target_lang,
            timestamp: item.timestamp.to_rfc3339(),
            confidence: item.confidence,
            loci_enhanced: item.loci_enhanced,
        }
    }
}

/// Start a new transcription/translation session
#[tauri::command]
pub async fn start_session(
    config: SessionConfig,
    app_handle: AppHandle,
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let request_id = Uuid::new_v4().to_string();
    tracing::info!(
        request_id = %request_id,
        source_lang = %config.source_lang,
        target_lang = %config.target_lang,
        input_device = ?config.input_device,
        peer_input_device = ?config.peer_input_device,
        bidirectional = config.bidirectional,
        loci_enhanced = config.loci_enhanced,
        "start_session requested"
    );

    // Check if there's an existing running session
    {
        let existing = state.lock().clone();
        if let Some(ref pipeline) = existing {
            let current_state = pipeline.get_state().await;
            if current_state == PipelineState::Running {
                tracing::warn!(
                    request_id = %request_id,
                    state = %current_state,
                    "start_session rejected: already running"
                );
                return Err("A session is already running. Please stop it first.".to_string());
            }
        }
    }

    if is_english_only_asr_model() && config.source_lang != "en" {
        tracing::warn!(
            request_id = %request_id,
            source_lang = %config.source_lang,
            "english-only ASR model detected; session will continue and rely on runtime model resolution"
        );
    }

    // Create pipeline config
    let pipeline_config: PipelineConfig = config.into();

    // Create new pipeline
    let pipeline = Arc::new(
        RealtimePipeline::new(app_handle, pipeline_config.clone())
            .map_err(|e| format!("Failed to create pipeline: {}", e))?
    );

    // Start the pipeline
    pipeline.start().await
        .map_err(|e| format!("Failed to start pipeline: {}", e))?;

    // Store the pipeline
    *state.lock() = Some(pipeline);
    let _ = session_bus::write_state(&session_bus::SessionRuntimeState {
        pid: std::process::id(),
        status: "running".to_string(),
        source_lang: pipeline_config.source_lang.clone(),
        target_lang: pipeline_config.target_lang.clone(),
        bidirectional: pipeline_config.bidirectional,
        tts_enabled: true,
        start_unix_ms: session_bus::now_unix_ms(),
        last_heartbeat_unix_ms: session_bus::now_unix_ms(),
        utterance_count: 0,
        error_count: 0,
        last_error: None,
    });
    tracing::info!(request_id = %request_id, "start_session completed");

    Ok(())
}

/// Stop the current session
#[tauri::command]
pub async fn stop_session(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let request_id = Uuid::new_v4().to_string();
    tracing::info!(request_id = %request_id, "stop_session requested");

    let pipeline = {
        let guard = state.lock();
        guard.clone()
    };
    
    if let Some(pipeline) = pipeline {
        pipeline.stop().await
            .map_err(|e| format!("Failed to stop pipeline: {}", e))?;
        tracing::info!(request_id = %request_id, "stop_session stopped active pipeline");
    } else {
        let _ = session_bus::write_control("stop", None, None);
        tracing::info!(request_id = %request_id, "stop_session no active pipeline");
    }
    
    *state.lock() = None;
    let _ = session_bus::write_state(&session_bus::SessionRuntimeState {
        pid: std::process::id(),
        status: "idle".to_string(),
        source_lang: String::new(),
        target_lang: String::new(),
        bidirectional: false,
        tts_enabled: true,
        start_unix_ms: session_bus::now_unix_ms(),
        last_heartbeat_unix_ms: session_bus::now_unix_ms(),
        utterance_count: 0,
        error_count: 0,
        last_error: None,
    });
    tracing::info!(request_id = %request_id, "stop_session completed");
    
    Ok(())
}

/// Pause the current session
#[tauri::command]
pub async fn pause_session(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    tracing::info!("pause_session requested");
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.pause().await
            .map_err(|e| format!("Failed to pause pipeline: {}", e))?;
        if let Some(mut st) = session_bus::read_state() {
            st.status = "paused".to_string();
            st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
            let _ = session_bus::write_state(&st);
        }
    } else {
        let _ = session_bus::write_control("pause", None, None);
        return Ok(());
    }
    
    Ok(())
}

/// Resume a paused session
#[tauri::command]
pub async fn resume_session(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    tracing::info!("resume_session requested");
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.resume().await
            .map_err(|e| format!("Failed to resume pipeline: {}", e))?;
        if let Some(mut st) = session_bus::read_state() {
            st.status = "running".to_string();
            st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
            let _ = session_bus::write_state(&st);
        }
    } else {
        let _ = session_bus::write_control("resume", None, None);
        return Ok(());
    }
    
    Ok(())
}

/// Get current session status
#[tauri::command]
pub async fn get_session_status(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<SessionStatusInfo, String> {
    let pipeline = state.lock().clone();
    
    match pipeline {
        Some(pipeline) => {
            let state = pipeline.get_state().await;
            let mut source_lang = "unknown".to_string();
            let mut target_lang = "unknown".to_string();
            let mut bidirectional = false;
            if let Some(st) = session_bus::read_state() {
                source_lang = st.source_lang;
                target_lang = st.target_lang;
                bidirectional = st.bidirectional;
            }
            if let Some(mut st) = session_bus::read_state() {
                st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
                st.status = state.to_string().to_ascii_lowercase();
                let _ = session_bus::write_state(&st);
            }
            Ok(SessionStatusInfo {
                is_running: state == PipelineState::Running,
                status: state.to_string(),
                source_lang,
                target_lang,
                bidirectional,
            })
        }
        None => {
            if let Some(st) = session_bus::read_state() {
                if session_bus::is_session_alive(&st) {
                    return Ok(SessionStatusInfo {
                        is_running: st.status == "running" || st.status == "paused",
                        status: st.status,
                        source_lang: st.source_lang,
                        target_lang: st.target_lang,
                        bidirectional: st.bidirectional,
                    });
                }
            }
            Ok(SessionStatusInfo {
                is_running: false,
                status: "Idle".to_string(),
                source_lang: String::new(),
                target_lang: String::new(),
                bidirectional: false,
            })
        }
    }
}

/// Get session statistics
#[tauri::command]
pub async fn get_session_stats(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<SessionStats, String> {
    let pipeline = state.lock().clone();
    
    match pipeline {
        Some(pipeline) => {
            let stats = pipeline.get_stats();
            Ok(SessionStats::from(stats))
        }
        None => {
            if let Some(m) = session_bus::read_metrics() {
                let error_count = session_bus::read_state().map(|s| s.error_count).unwrap_or(0);
                Ok(SessionStats {
                    total_audio_duration_ms: m.total_audio_duration_ms,
                    speech_duration_ms: m.speech_duration_ms,
                    transcription_count: m.transcription_count,
                    translation_count: m.translation_count,
                    average_latency_ms: m.average_latency_ms,
                    asr_average_latency_ms: m.asr_average_latency_ms,
                    translation_average_latency_ms: m.translation_average_latency_ms,
                    tts_average_latency_ms: m.tts_average_latency_ms,
                    error_count,
                })
            } else {
                Ok(SessionStats::default())
            }
        }
    }
}

/// Get session history
#[tauri::command]
pub async fn get_session_history(
    count: Option<usize>,
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<Vec<HistoryItemInfo>, String> {
    let pipeline = state.lock().clone();
    let count = count.unwrap_or(50);
    
    match pipeline {
        Some(pipeline) => {
            let items = pipeline.get_recent_history(count).await;
            Ok(items.into_iter().map(HistoryItemInfo::from).collect())
        }
        None => {
            let items = session_bus::read_history_recent(count)
                .map_err(|e| format!("Failed to read session history bus: {}", e))?;
            Ok(items
                .into_iter()
                .map(|i| HistoryItemInfo {
                    id: i.id,
                    source_text: i.source_text,
                    translated_text: i.translated_text,
                    source_lang: i.source_lang,
                    target_lang: i.target_lang,
                    timestamp: i.timestamp,
                    confidence: i.confidence,
                    loci_enhanced: false,
                })
                .collect())
        }
    }
}

/// Clear session history
#[tauri::command]
pub async fn clear_session_history(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.clear_history().await;
    }
    let _ = session_bus::clear_history();
    let _ = session_bus::write_metrics(&session_bus::SessionMetrics::default());
    
    Ok(())
}

/// Export session history to JSON
#[tauri::command]
pub async fn export_history(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<String, String> {
    let pipeline = state.lock().clone();
    
    match pipeline {
        Some(pipeline) => {
            let items = pipeline.get_history().await;
            serde_json::to_string_pretty(&items)
                .map_err(|e| format!("Failed to export history: {}", e))
        }
        None => Ok("[]".to_string()),
    }
}

/// Update session language pair
#[tauri::command]
pub async fn update_languages(
    source_lang: String,
    target_lang: String,
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.set_source_lang(&source_lang).await
            .map_err(|e| format!("Failed to update language: {}", e))?;
    }
    let _ = session_bus::write_control("update_languages", Some(&source_lang), Some(&target_lang));
    if let Some(mut st) = session_bus::read_state() {
        st.source_lang = source_lang;
        st.target_lang = target_lang;
        st.last_heartbeat_unix_ms = session_bus::now_unix_ms();
        let _ = session_bus::write_state(&st);
    }
    
    Ok(())
}

impl Default for SessionStats {
    fn default() -> Self {
        Self {
            total_audio_duration_ms: 0,
            speech_duration_ms: 0,
            transcription_count: 0,
            translation_count: 0,
            average_latency_ms: 0.0,
            asr_average_latency_ms: 0.0,
            translation_average_latency_ms: 0.0,
            tts_average_latency_ms: 0.0,
            error_count: 0,
        }
    }
}

fn is_english_only_asr_model() -> bool {
    let Some(asr_dir) = resolve_asr_model_dir() else {
        return false;
    };

    let base = asr_dir.parent().map(|p| p.to_path_buf());
    if let Some(base) = base {
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let p = entry.path();
                if !p.is_dir() || !(has_required_asr_files(&p) || has_paraformer_files(&p)) {
                    continue;
                }
                let n = p.to_string_lossy().to_ascii_lowercase();
                if n.contains("zh")
                    || n.contains("multi")
                    || n.contains("trilingual")
                    || p.join("model.int8.onnx").exists()
                    || p.join("model.onnx").exists()
                {
                    return false;
                }
            }
        }
    }

    let dir_name = asr_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    if dir_name.contains("-en") || dir_name.contains("_en") || dir_name.ends_with("en") {
        return true;
    }

    let has_en_marked_model = std::fs::read_dir(&asr_dir)
        .ok()
        .map(|entries| {
            entries.flatten().any(|entry| {
                let name = entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                name.contains("-en-") || name.contains(".en.") || name.contains("_en_")
            })
        })
        .unwrap_or(false);

    has_en_marked_model
}

fn resolve_asr_model_dir() -> Option<PathBuf> {
    let base = dirs::data_local_dir()?.join("LocalTrans").join("models").join("asr");
    if has_required_asr_files(&base) {
        return Some(base);
    }

    let entries = std::fs::read_dir(&base).ok()?;
    let mut best: Option<(u64, PathBuf)> = None;
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

fn has_paraformer_files(dir: &Path) -> bool {
    dir.join("tokens.txt").exists()
        && (dir.join("model.int8.onnx").exists() || dir.join("model.onnx").exists())
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
