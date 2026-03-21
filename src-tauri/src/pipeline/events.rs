#![allow(dead_code)]
//! Pipeline events and Tauri event broadcasting
//! 
//! This module defines all events that can occur during the real-time
//! transcription/translation pipeline and provides utilities for
//! broadcasting them to the frontend via Tauri's event system.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

/// Main event type for pipeline events
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PipelineEvent {
    /// Speech detected by VAD
    SpeechDetected {
        timestamp: DateTime<Utc>,
    },
    
    /// Partial (intermediate) transcription result
    PartialTranscription {
        text: String,
        language: String,
        confidence: f32,
        timestamp: DateTime<Utc>,
    },
    
    /// Final transcription result with segments
    FinalTranscription {
        text: String,
        segments: Vec<TranscriptionSegment>,
        language: String,
        confidence: f32,
        timestamp: DateTime<Utc>,
    },
    
    /// Translation result
    Translation {
        id: String,
        source_text: String,
        target_text: String,
        source_lang: String,
        target_lang: String,
        confidence: f32,
        timestamp: DateTime<Utc>,
    },
    
    /// Bidirectional translation result (for dual-direction mode)
    BidirectionalTranslation {
        forward: Option<TranslationInfo>,
        backward: Option<TranslationInfo>,
        timestamp: DateTime<Utc>,
    },
    
    /// Error occurred
    Error {
        code: ErrorCode,
        message: String,
        recoverable: bool,
        timestamp: DateTime<Utc>,
    },
    
    /// Pipeline state changed
    StateChanged {
        old_state: PipelineState,
        new_state: PipelineState,
        reason: Option<String>,
        timestamp: DateTime<Utc>,
    },
    
    /// Audio level update for UI visualization
    AudioLevel {
        level: f32,
        is_speech: bool,
        timestamp: DateTime<Utc>,
    },
    
    /// Session statistics update
    Stats {
        total_audio_duration_ms: u64,
        speech_duration_ms: u64,
        transcription_count: u64,
        translation_count: u64,
        average_latency_ms: f32,
        timestamp: DateTime<Utc>,
    },
}

/// Transcription segment with timing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub id: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
    pub confidence: f32,
    pub speaker_id: Option<String>,
}

/// Translation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationInfo {
    pub source_text: String,
    pub target_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub confidence: f32,
}

/// Pipeline operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PipelineState {
    /// Pipeline is idle, not processing
    Idle,
    /// Pipeline is actively running
    Running,
    /// Pipeline is paused
    Paused,
    /// Pipeline encountered an error
    Error,
    /// Pipeline is initializing
    Initializing,
    /// Pipeline is stopping
    Stopping,
}

impl std::fmt::Display for PipelineState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Running => write!(f, "Running"),
            Self::Paused => write!(f, "Paused"),
            Self::Error => write!(f, "Error"),
            Self::Initializing => write!(f, "Initializing"),
            Self::Stopping => write!(f, "Stopping"),
        }
    }
}

/// Error codes for pipeline errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Audio capture failed
    AudioCaptureFailed,
    /// Audio device not found
    AudioDeviceNotFound,
    /// ASR model not loaded
    AsrModelNotLoaded,
    /// ASR transcription failed
    AsrTranscriptionFailed,
    /// Translation model not loaded
    TranslationModelNotLoaded,
    /// Translation failed
    TranslationFailed,
    /// VAD initialization failed
    VadInitFailed,
    /// Pipeline not initialized
    PipelineNotInitialized,
    /// Pipeline already running
    PipelineAlreadyRunning,
    /// Pipeline state invalid for operation
    InvalidState,
    /// Memory allocation failed
    MemoryError,
    /// Internal error
    InternalError,
    /// Timeout
    Timeout,
    /// Model download failed
    ModelDownloadFailed,
}

impl ErrorCode {
    /// Check if the error is recoverable without user intervention
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::AudioCaptureFailed |
            Self::AsrTranscriptionFailed |
            Self::TranslationFailed |
            Self::Timeout
        )
    }
}

/// Event broadcaster for sending events to frontend
pub struct EventBroadcaster {
    app_handle: AppHandle,
    event_prefix: String,
}

impl EventBroadcaster {
    /// Create a new event broadcaster
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            event_prefix: "pipeline:".to_string(),
        }
    }
    
    /// Create with custom event prefix
    pub fn with_prefix(app_handle: AppHandle, prefix: impl Into<String>) -> Self {
        Self {
            app_handle,
            event_prefix: prefix.into(),
        }
    }
    
    /// Emit a pipeline event to the frontend
    pub fn emit(&self, event: &PipelineEvent) -> Result<(), String> {
        let event_name = self.event_name_for(event);
        self.app_handle
            .emit(&event_name, event)
            .map_err(|e| format!("Failed to emit event: {}", e))
    }
    
    /// Emit event with additional payload
    pub fn emit_with_payload<T: Serialize>(
        &self, 
        event: &PipelineEvent,
        payload: &T,
    ) -> Result<(), String> {
        let event_name = self.event_name_for(event);
        let combined = serde_json::json!({
            "event": event,
            "payload": payload,
        });
        self.app_handle
            .emit(&event_name, combined)
            .map_err(|e| format!("Failed to emit event: {}", e))
    }
    
    /// Get the event name for a given event type
    fn event_name_for(&self, event: &PipelineEvent) -> String {
        let suffix = match event {
            PipelineEvent::SpeechDetected { .. } => "speech-detected",
            PipelineEvent::PartialTranscription { .. } => "partial-transcription",
            PipelineEvent::FinalTranscription { .. } => "final-transcription",
            PipelineEvent::Translation { .. } => "translation",
            PipelineEvent::BidirectionalTranslation { .. } => "bidirectional-translation",
            PipelineEvent::Error { .. } => "error",
            PipelineEvent::StateChanged { .. } => "state-changed",
            PipelineEvent::AudioLevel { .. } => "audio-level",
            PipelineEvent::Stats { .. } => "stats",
        };
        format!("{}{}", self.event_prefix, suffix)
    }
    
    /// Convenience method for emitting state change
    pub fn emit_state_change(
        &self, 
        old_state: PipelineState, 
        new_state: PipelineState,
        reason: Option<&str>,
    ) -> Result<(), String> {
        self.emit(&PipelineEvent::StateChanged {
            old_state,
            new_state,
            reason: reason.map(String::from),
            timestamp: Utc::now(),
        })
    }
    
    /// Convenience method for emitting error
    pub fn emit_error(
        &self, 
        code: ErrorCode, 
        message: impl Into<String>,
    ) -> Result<(), String> {
        let message = message.into();
        let recoverable = code.is_recoverable();
        self.emit(&PipelineEvent::Error {
            code,
            message,
            recoverable,
            timestamp: Utc::now(),
        })
    }
    
    /// Convenience method for emitting transcription
    pub fn emit_partial_transcription(
        &self,
        text: impl Into<String>,
        language: impl Into<String>,
        confidence: f32,
    ) -> Result<(), String> {
        self.emit(&PipelineEvent::PartialTranscription {
            text: text.into(),
            language: language.into(),
            confidence,
            timestamp: Utc::now(),
        })
    }
    
    /// Convenience method for emitting final transcription
    pub fn emit_final_transcription(
        &self,
        text: impl Into<String>,
        segments: Vec<TranscriptionSegment>,
        language: impl Into<String>,
        confidence: f32,
    ) -> Result<(), String> {
        self.emit(&PipelineEvent::FinalTranscription {
            text: text.into(),
            segments,
            language: language.into(),
            confidence,
            timestamp: Utc::now(),
        })
    }
    
    /// Convenience method for emitting translation
    pub fn emit_translation(
        &self,
        id: impl Into<String>,
        source_text: impl Into<String>,
        target_text: impl Into<String>,
        source_lang: impl Into<String>,
        target_lang: impl Into<String>,
        confidence: f32,
    ) -> Result<(), String> {
        self.emit(&PipelineEvent::Translation {
            id: id.into(),
            source_text: source_text.into(),
            target_text: target_text.into(),
            source_lang: source_lang.into(),
            target_lang: target_lang.into(),
            confidence,
            timestamp: Utc::now(),
        })
    }
    
    /// Convenience method for emitting audio level
    pub fn emit_audio_level(&self, level: f32, is_speech: bool) -> Result<(), String> {
        self.emit(&PipelineEvent::AudioLevel {
            level,
            is_speech,
            timestamp: Utc::now(),
        })
    }
    
    /// Emit speech detected event
    pub fn emit_speech_detected(&self) -> Result<(), String> {
        self.emit(&PipelineEvent::SpeechDetected {
            timestamp: Utc::now(),
        })
    }
    
    /// Emit stats update
    pub fn emit_stats(
        &self,
        total_audio_duration_ms: u64,
        speech_duration_ms: u64,
        transcription_count: u64,
        translation_count: u64,
        average_latency_ms: f32,
    ) -> Result<(), String> {
        self.emit(&PipelineEvent::Stats {
            total_audio_duration_ms,
            speech_duration_ms,
            transcription_count,
            translation_count,
            average_latency_ms,
            timestamp: Utc::now(),
        })
    }
}

/// Session history item for storing completed transcriptions/translations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: String,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub timestamp: DateTime<Utc>,
    pub confidence: f32,
    pub loci_enhanced: bool,
    pub segments: Vec<TranscriptionSegment>,
    pub metadata: HashMap<String, String>,
}

impl HistoryItem {
    pub fn new(
        source_text: String,
        translated_text: String,
        source_lang: String,
        target_lang: String,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            source_text,
            translated_text,
            source_lang,
            target_lang,
            timestamp: Utc::now(),
            confidence: 0.0,
            loci_enhanced: false,
            segments: Vec::new(),
            metadata: HashMap::new(),
        }
    }
    
    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence;
        self
    }
    
    pub fn with_segments(mut self, segments: Vec<TranscriptionSegment>) -> Self {
        self.segments = segments;
        self
    }
    
    pub fn with_loci_enhanced(mut self, enhanced: bool) -> Self {
        self.loci_enhanced = enhanced;
        self
    }
    
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// History manager for storing and retrieving past transcriptions
pub struct HistoryManager {
    items: Vec<HistoryItem>,
    max_items: usize,
}

impl HistoryManager {
    pub fn new(max_items: usize) -> Self {
        Self {
            items: Vec::with_capacity(max_items.min(1000)),
            max_items,
        }
    }
    
    pub fn add(&mut self, item: HistoryItem) {
        if self.items.len() >= self.max_items {
            self.items.remove(0);
        }
        self.items.push(item);
    }
    
    pub fn get_all(&self) -> &[HistoryItem] {
        &self.items
    }
    
    pub fn get_recent(&self, count: usize) -> Vec<&HistoryItem> {
        self.items.iter().rev().take(count).collect()
    }
    
    pub fn clear(&mut self) {
        self.items.clear();
    }
    
    pub fn len(&self) -> usize {
        self.items.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    
    /// Export history to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&self.items)
    }
    
    /// Import history from JSON string
    pub fn from_json(&mut self, json: &str) -> Result<(), serde_json::Error> {
        let items: Vec<HistoryItem> = serde_json::from_str(json)?;
        self.items = items;
        Ok(())
    }
}

impl Default for HistoryManager {
    fn default() -> Self {
        Self::new(500)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_history_manager() {
        let mut manager = HistoryManager::new(5);
        
        for i in 0..7 {
            manager.add(HistoryItem::new(
                format!("source {}", i),
                format!("target {}", i),
                "en".to_string(),
                "zh".to_string(),
            ));
        }
        
        assert_eq!(manager.len(), 5);
        assert_eq!(manager.get_all().first().unwrap().source_text, "source 2");
    }
    
    #[test]
    fn test_error_code_recoverable() {
        assert!(ErrorCode::AudioCaptureFailed.is_recoverable());
        assert!(!ErrorCode::AsrModelNotLoaded.is_recoverable());
    }
    
    #[test]
    fn test_history_item_builder() {
        let item = HistoryItem::new(
            "Hello".to_string(),
            "你好".to_string(),
            "en".to_string(),
            "zh".to_string(),
        )
        .with_confidence(0.95)
        .with_loci_enhanced(true);
        
        assert_eq!(item.confidence, 0.95);
        assert!(item.loci_enhanced);
    }
}

