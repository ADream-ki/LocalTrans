//! Session and pipeline commands for Tauri
//! 
//! This module provides the Tauri command handlers for managing
//! real-time transcription/translation sessions.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use std::sync::Arc;
use parking_lot::Mutex as SyncMutex;

use crate::pipeline::{
    RealtimePipeline, PipelineConfig, PipelineState, PipelineStats,
    HistoryItem,
};

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
    pub bidirectional: bool,
    pub loci_enhanced: bool,
    /// VAD frame size in milliseconds (controls latency vs stability)
    pub vad_frame_ms: Option<u32>,
    pub vad_threshold: Option<f32>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            input_device: None,
            bidirectional: false,
            loci_enhanced: true,
            vad_frame_ms: None,
            vad_threshold: None,
        }
    }
}

impl From<SessionConfig> for PipelineConfig {
    fn from(config: SessionConfig) -> Self {
        let mut pipeline_config = PipelineConfig {
            source_lang: config.source_lang,
            target_lang: config.target_lang,
            input_device: config.input_device,
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
    // Check if there's an existing running session
    {
        let existing = state.lock().clone();
        if let Some(ref pipeline) = existing {
            let current_state = pipeline.get_state().await;
            if current_state == PipelineState::Running {
                return Err("A session is already running. Please stop it first.".to_string());
            }
        }
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

    Ok(())
}

/// Stop the current session
#[tauri::command]
pub async fn stop_session(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let pipeline = {
        let guard = state.lock();
        guard.clone()
    };
    
    if let Some(pipeline) = pipeline {
        pipeline.stop().await
            .map_err(|e| format!("Failed to stop pipeline: {}", e))?;
    }
    
    *state.lock() = None;
    
    Ok(())
}

/// Pause the current session
#[tauri::command]
pub async fn pause_session(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.pause().await
            .map_err(|e| format!("Failed to pause pipeline: {}", e))?;
    } else {
        return Err("No active session to pause".to_string());
    }
    
    Ok(())
}

/// Resume a paused session
#[tauri::command]
pub async fn resume_session(
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.resume().await
            .map_err(|e| format!("Failed to resume pipeline: {}", e))?;
    } else {
        return Err("No active session to resume".to_string());
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
            // We need to get config from the pipeline, but we don't expose it
            // For now, return basic status
            Ok(SessionStatusInfo {
                is_running: state == PipelineState::Running,
                status: state.to_string(),
                source_lang: "unknown".to_string(), // Would need to store this
                target_lang: "unknown".to_string(),
                bidirectional: false,
            })
        }
        None => Ok(SessionStatusInfo {
            is_running: false,
            status: "Idle".to_string(),
            source_lang: String::new(),
            target_lang: String::new(),
            bidirectional: false,
        }),
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
        None => Ok(SessionStats::default()),
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
        None => Ok(Vec::new()),
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
    _target_lang: String,
    state: State<'_, SyncMutex<Option<Arc<RealtimePipeline>>>>,
) -> Result<(), String> {
    let pipeline = state.lock().clone();
    
    if let Some(pipeline) = pipeline {
        pipeline.set_source_lang(&source_lang).await
            .map_err(|e| format!("Failed to update language: {}", e))?;
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
            error_count: 0,
        }
    }
}
