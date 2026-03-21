#![allow(dead_code)]
//! Real-time transcription and translation pipeline
//! 
//! This module implements the core pipeline that orchestrates:
//! Audio capture → VAD detection → ASR transcription → Translation → Event broadcasting
//!
//! The pipeline is fully async and uses channels for inter-component communication.

use anyhow::{Result, Context, anyhow};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use std::path::PathBuf;
use tokio::sync::Mutex as AsyncMutex;
use parking_lot::Mutex as SyncMutex;
use tauri::AppHandle;

use crate::audio::AudioCapture;
use crate::audio::resample_linear;
use crate::asr::{AsrConfig, StreamingConfig, StreamingAsrEngine, StreamingResult};
use crate::translation::{Translator, LociTranslator};
use super::events::{
    EventBroadcaster, HistoryManager, HistoryItem, PipelineEvent, PipelineState,
    ErrorCode, TranscriptionSegment, TranslationInfo,
};

/// Configuration for the realtime pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Source language code (e.g., "en", "zh")
    pub source_lang: String,
    /// Target language code (e.g., "zh", "en")
    pub target_lang: String,
    /// Audio input device ID (None for default)
    pub input_device: Option<String>,
    /// Enable bidirectional translation
    pub bidirectional: bool,
    /// VAD frame duration in milliseconds
    pub vad_frame_ms: u32,
    /// VAD energy threshold
    pub vad_threshold: f32,
    /// Minimum speech duration to trigger transcription (ms)
    pub min_speech_duration_ms: u64,
    /// Maximum speech segment duration (ms)
    pub max_segment_duration_ms: u64,
    /// Enable Loci enhancement for translations
    pub loci_enhanced: bool,
    /// Audio buffer chunk size in samples
    pub chunk_size: usize,
    /// Maximum history items to keep
    pub max_history: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            input_device: None,
            bidirectional: false,
            vad_frame_ms: 30,
            vad_threshold: 0.01,
            min_speech_duration_ms: 500,
            max_segment_duration_ms: 30000,
            loci_enhanced: true,
            chunk_size: 480, // 30ms at 16kHz
            max_history: 500,
        }
    }
}

impl PipelineConfig {
    /// Create config for Chinese to English translation
    pub fn zh_to_en() -> Self {
        Self {
            source_lang: "zh".to_string(),
            target_lang: "en".to_string(),
            ..Default::default()
        }
    }
    
    /// Create config for English to Chinese translation
    pub fn en_to_zh() -> Self {
        Self {
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            ..Default::default()
        }
    }
    
    /// Create config for bidirectional translation
    pub fn bidirectional(source: &str, target: &str) -> Self {
        Self {
            source_lang: source.to_string(),
            target_lang: target.to_string(),
            bidirectional: true,
            ..Default::default()
        }
    }
}

/// Statistics for pipeline operation
#[derive(Debug, Clone, Default)]
pub struct PipelineStats {
    pub total_audio_duration_ms: u64,
    pub speech_duration_ms: u64,
    pub transcription_count: u64,
    pub translation_count: u64,
    pub total_latency_ms: u64,
    pub error_count: u64,
    pub retry_count: u64,
    pub start_time: Option<Instant>,
}

impl PipelineStats {
    pub fn average_latency_ms(&self) -> f32 {
        if self.transcription_count == 0 {
            0.0
        } else {
            self.total_latency_ms as f32 / self.transcription_count as f32
        }
    }

    pub fn reset(&mut self) {
        *self = Self {
            start_time: Some(Instant::now()),
            ..Default::default()
        };
    }
}

/// The main realtime pipeline structure
pub struct RealtimePipeline {
    /// Pipeline configuration
    config: PipelineConfig,
    /// Current pipeline state
    state: Arc<tokio::sync::RwLock<PipelineState>>,
    /// Audio capture component
    audio_capture: Arc<AsyncMutex<Option<AudioCapture>>>,
    /// Streaming ASR engine
    asr_engine: Arc<AsyncMutex<Option<StreamingAsrEngine>>>,
    /// Translator
    translator: Arc<AsyncMutex<Box<dyn Translator>>>,
    /// Event broadcaster
    broadcaster: Arc<EventBroadcaster>,
    /// History manager
    history: Arc<AsyncMutex<HistoryManager>>,
    /// Statistics
    stats: Arc<SyncMutex<PipelineStats>>,
    /// Running flag
    is_running: Arc<AtomicBool>,
    /// Shutdown signal
    shutdown_tx: Arc<AsyncMutex<Option<mpsc::Sender<()>>>>,
    /// Sample rate for audio
    sample_rate: u32,
}

impl RealtimePipeline {
    /// Create a new realtime pipeline
    pub fn new(app_handle: AppHandle, config: PipelineConfig) -> Result<Self> {
        let max_history = config.max_history;
        // Create event broadcaster
        let broadcaster = EventBroadcaster::new(app_handle);
        
        // Initialize translator (prefer Loci GGUF model under app data dir)
        let translator: Box<dyn Translator> = match find_default_loci_model() {
            Some(model_path) => match LociTranslator::init(&model_path) {
                Ok(t) => Box::new(t),
                Err(e) => {
                    let _ = broadcaster.emit_error(
                        ErrorCode::TranslationModelNotLoaded,
                        format!("Failed to load Loci model ({}): {}", model_path.display(), e),
                    );
                    Box::new(LociTranslator::new())
                }
            },
            None => {
                let _ = broadcaster.emit_error(
                    ErrorCode::TranslationModelNotLoaded,
                    format!(
                        "No Loci model found. Put a .gguf file under: {}",
                        default_loci_dir().display()
                    ),
                );
                Box::new(LociTranslator::new())
            }
        };
        
        Ok(Self {
            config,
            state: Arc::new(tokio::sync::RwLock::new(PipelineState::Idle)),
            audio_capture: Arc::new(AsyncMutex::new(None)),
            asr_engine: Arc::new(AsyncMutex::new(None)),
            translator: Arc::new(AsyncMutex::new(translator)),
            broadcaster: Arc::new(broadcaster),
            history: Arc::new(AsyncMutex::new(HistoryManager::new(max_history))),
            stats: Arc::new(SyncMutex::new(PipelineStats::default())),
            is_running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: Arc::new(AsyncMutex::new(None)),
            sample_rate: 16000,
        })
    }
    
    /// Start the pipeline
    pub async fn start(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state == PipelineState::Running {
            return Err(anyhow!("Pipeline is already running"));
        }
        
        // Update state
        self.set_state(PipelineState::Initializing).await;
        self.broadcaster.emit_state_change(
            current_state,
            PipelineState::Initializing,
            Some("Starting pipeline"),
        ).map_err(|e| anyhow!(e))?;
        
        // Reset stats
        self.stats.lock().reset();
        
        // Initialize audio capture
        let mut audio_capture = AudioCapture::new(self.config.input_device.as_deref())
            .context("Failed to initialize audio capture")?;
        
        audio_capture.start_capture()
            .context("Failed to start audio capture")?;
        
        *self.audio_capture.lock().await = Some(audio_capture);
        
        // Initialize streaming ASR engine
        let asr_config = AsrConfig::default();
        let mut streaming_config = StreamingConfig::new(self.sample_rate);
        streaming_config.vad_energy_threshold = self.config.vad_threshold;
        streaming_config.min_speech_duration_ms =
            (self.config.min_speech_duration_ms.min(u32::MAX as u64)) as u32;
        streaming_config.max_speech_duration_ms =
            (self.config.max_segment_duration_ms.min(u32::MAX as u64)) as u32;

        // Use ms-based frame size as the primary control.
        // Clamp to a sane range to avoid extreme latency/CPU usage.
        let vad_frame_ms = self.config.vad_frame_ms.clamp(10, 200);
        let from_ms = (self.sample_rate as usize * vad_frame_ms as usize) / 1000;
        let chunk_size = if from_ms > 0 {
            from_ms
        } else {
            self.config.chunk_size
        };
        streaming_config.chunk_size = chunk_size.max(1);
        let asr_chunk_size = streaming_config.chunk_size;
        
        let asr_engine = StreamingAsrEngine::new(asr_config, streaming_config)
            .map_err(|e| anyhow!("Failed to initialize ASR engine: {}", e))?;
        
        *self.asr_engine.lock().await = Some(asr_engine);
        
        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        *self.shutdown_tx.lock().await = Some(shutdown_tx);
        
        // Mark as running
        self.is_running.store(true, Ordering::SeqCst);
        self.set_state(PipelineState::Running).await;
        
        self.broadcaster.emit_state_change(
            PipelineState::Initializing,
            PipelineState::Running,
            Some("Pipeline started successfully"),
        ).map_err(|e| anyhow!(e))?;
        
        // Clone references for the async task
        let audio_capture = self.audio_capture.clone();
        let asr_engine = self.asr_engine.clone();
        let translator = self.translator.clone();
        let broadcaster = self.broadcaster.clone();
        let history = self.history.clone();
        let stats = self.stats.clone();
        let is_running = self.is_running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let sample_rate = self.sample_rate;
        let asr_chunk_size = asr_chunk_size;
        
        // Spawn the main processing loop
        tokio::spawn(async move {
            let mut last_stats_emit = Instant::now();
            let mut last_translation_error_emit = Instant::now() - Duration::from_secs(60);
            let mut chunk_buffer: std::collections::VecDeque<f32> =
                std::collections::VecDeque::with_capacity(asr_chunk_size * 8);
            
            loop {
                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() || !is_running.load(Ordering::SeqCst) {
                    break;
                }
                
                // Check if paused
                {
                    let current_state = *state.read().await;
                    if current_state == PipelineState::Paused {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                }
                
                // Collect audio samples (mono f32 at device sample rate)
                let (samples, input_sample_rate) = {
                    let mut capture_guard = audio_capture.lock().await;
                    if let Some(ref mut capture) = *capture_guard {
                        (capture.get_samples(), capture.sample_rate())
                    } else {
                        (Vec::new(), sample_rate)
                    }
                };
                
                if samples.is_empty() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    continue;
                }
                
                // Resample to pipeline sample rate (16kHz) if needed
                let resampled = if input_sample_rate > 0 && input_sample_rate != sample_rate {
                    resample_linear(&samples, input_sample_rate, sample_rate)
                } else {
                    samples
                };

                // Update stats (use resampled duration for consistency)
                {
                    let mut stats_guard = stats.lock();
                    stats_guard.total_audio_duration_ms +=
                        (resampled.len() as f64 / sample_rate as f64 * 1000.0) as u64;
                }

                // Buffer and process in fixed-size chunks for stable VAD/ASR behavior
                chunk_buffer.extend(resampled);

                while chunk_buffer.len() >= asr_chunk_size {
                    let chunk: Vec<f32> = chunk_buffer.drain(..asr_chunk_size).collect();

                    // Process through ASR
                    let asr_result = {
                        let mut asr_guard = asr_engine.lock().await;
                        if let Some(ref mut engine) = *asr_guard {
                            match engine.process(&chunk) {
                                Ok(r) => r,
                                Err(e) => {
                                    stats.lock().error_count += 1;
                                    let _ = broadcaster.emit_error(
                                        ErrorCode::AsrTranscriptionFailed,
                                        e.to_string(),
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    };

                    // Handle ASR results
                    if let Some(result) = asr_result {
                        match result {
                        StreamingResult::Partial(partial) => {
                            let _ = broadcaster.emit_partial_transcription(
                                &partial.text,
                                partial.detected_language.as_deref().unwrap_or("auto"),
                                partial.confidence,
                            );
                        }
                        StreamingResult::Final(transcription) => {
                            // Emit final transcription
                            let segments: Vec<TranscriptionSegment> = transcription.segments.iter()
                                .map(|s| TranscriptionSegment {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    start_ms: (s.start * 1000.0) as u64,
                                    end_ms: (s.end * 1000.0) as u64,
                                    text: s.text.clone(),
                                    confidence: s.confidence,
                                    speaker_id: None,
                                })
                                .collect();
                            
                            let _ = broadcaster.emit_final_transcription(
                                &transcription.text,
                                segments.clone(),
                                &transcription.language,
                                transcription.confidence,
                            );
                            
                            // Update stats
                            {
                                let mut stats_guard = stats.lock();
                                stats_guard.transcription_count += 1;
                                stats_guard.total_latency_ms += transcription.processing_time_ms;
                            }
                            
                            // Translate if text is not empty
                            if !transcription.text.trim().is_empty() {
                                let translation_result: Result<_, String> = {
                                    let mut trans = translator.lock().await;
                                    trans.translate(
                                        &transcription.text,
                                        &config.source_lang,
                                        &config.target_lang,
                                    )
                                    .map_err(|e| e.to_string())
                                };

                                match translation_result {
                                    Ok(translation) => {
                                    // Update stats
                                    stats.lock().translation_count += 1;
                                    
                                    // Emit translation
                                    let id = uuid::Uuid::new_v4().to_string();
                                    let _ = broadcaster.emit_translation(
                                        &id,
                                        &transcription.text,
                                        &translation.text,
                                        &translation.source_lang,
                                        &translation.target_lang,
                                        translation.confidence,
                                    );
                                    
                                    // Add to history
                                    let history_item = HistoryItem::new(
                                        transcription.text.clone(),
                                        translation.text.clone(),
                                        translation.source_lang,
                                        translation.target_lang,
                                    )
                                    .with_confidence(transcription.confidence.min(translation.confidence))
                                    .with_segments(segments)
                                    .with_loci_enhanced(config.loci_enhanced);
                                    
                                    history.lock().await.add(history_item);
                                    
                                    // Bidirectional translation
                                    if config.bidirectional {
                                        let reverse_translation = {
                                            let mut trans = translator.lock().await;
                                            trans.translate(
                                                &translation.text,
                                                &config.target_lang,
                                                &config.source_lang,
                                            ).ok()
                                        };
                                        
                                        if let Some(reverse) = reverse_translation {
                                            let target_text = translation.text.clone();
                                            let _ = broadcaster.emit(&PipelineEvent::BidirectionalTranslation {
                                                forward: Some(TranslationInfo {
                                                    source_text: transcription.text.clone(),
                                                    target_text: target_text.clone(),
                                                    source_lang: config.source_lang.clone(),
                                                    target_lang: config.target_lang.clone(),
                                                    confidence: translation.confidence,
                                                }),
                                                backward: Some(TranslationInfo {
                                                    source_text: target_text,
                                                    target_text: reverse.text,
                                                    source_lang: config.target_lang.clone(),
                                                    target_lang: config.source_lang.clone(),
                                                    confidence: reverse.confidence,
                                                }),
                                                timestamp: chrono::Utc::now(),
                                            });
                                        }
                                    }
                                    }
                                    Err(err) => {
                                        stats.lock().error_count += 1;
                                        if last_translation_error_emit.elapsed() >= Duration::from_secs(5) {
                                            let _ = broadcaster.emit_error(
                                                ErrorCode::TranslationFailed,
                                                err,
                                            );
                                            last_translation_error_emit = Instant::now();
                                        }
                                    }
                                }
                            }
                        }
                        StreamingResult::Error(msg) => {
                            let _ = broadcaster.emit_error(
                                ErrorCode::AsrTranscriptionFailed,
                                msg,
                            );
                            stats.lock().error_count += 1;
                        }
                        }
                    }
                }
                
                // Emit stats periodically
                if last_stats_emit.elapsed().as_secs() >= 5 {
                    let stats_guard = stats.lock();
                    let _ = broadcaster.emit_stats(
                        stats_guard.total_audio_duration_ms,
                        stats_guard.speech_duration_ms,
                        stats_guard.transcription_count,
                        stats_guard.translation_count,
                        stats_guard.average_latency_ms(),
                    );
                    last_stats_emit = Instant::now();
                }
                
                // Small sleep to prevent CPU spinning
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            
            // Cleanup
            let _ = broadcaster.emit_state_change(
                PipelineState::Running,
                PipelineState::Idle,
                Some("Pipeline stopped"),
            );
        });
        
        Ok(())
    }
    
    /// Stop the pipeline
    pub async fn stop(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state != PipelineState::Running && current_state != PipelineState::Paused {
            return Ok(());
        }
        
        // Signal shutdown
        self.is_running.store(false, Ordering::SeqCst);
        
        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.lock().await.take() {
            let _ = tx.send(()).await;
        }
        
        // Stop audio capture
        let mut capture_guard = self.audio_capture.lock().await;
        if let Some(ref mut capture) = *capture_guard {
            capture.stop_capture();
        }
        *capture_guard = None;
        
        // Clear ASR engine
        *self.asr_engine.lock().await = None;
        
        // Update state
        self.set_state(PipelineState::Idle).await;
        
        Ok(())
    }
    
    /// Pause the pipeline
    pub async fn pause(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state != PipelineState::Running {
            return Err(anyhow!("Can only pause a running pipeline"));
        }
        
        self.set_state(PipelineState::Paused).await;
        self.broadcaster.emit_state_change(
            PipelineState::Running,
            PipelineState::Paused,
            Some("Pipeline paused"),
        ).map_err(|e| anyhow!(e))?;
        
        Ok(())
    }
    
    /// Resume the pipeline
    pub async fn resume(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state != PipelineState::Paused {
            return Err(anyhow!("Can only resume a paused pipeline"));
        }
        
        self.set_state(PipelineState::Running).await;
        self.broadcaster.emit_state_change(
            PipelineState::Paused,
            PipelineState::Running,
            Some("Pipeline resumed"),
        ).map_err(|e| anyhow!(e))?;
        
        Ok(())
    }
    
    /// Get current pipeline state
    pub async fn get_state(&self) -> PipelineState {
        *self.state.read().await
    }
    
    /// Get pipeline statistics
    pub fn get_stats(&self) -> PipelineStats {
        self.stats.lock().clone()
    }
    
    /// Get history items
    pub async fn get_history(&self) -> Vec<HistoryItem> {
        self.history.lock().await.get_all().to_vec()
    }
    
    /// Get recent history items
    pub async fn get_recent_history(&self, count: usize) -> Vec<HistoryItem> {
        self.history.lock().await.get_recent(count)
            .into_iter()
            .cloned()
            .collect()
    }
    
    /// Clear history
    pub async fn clear_history(&self) {
        self.history.lock().await.clear();
    }
    
    /// Update source language
    pub async fn set_source_lang(&self, lang: &str) -> Result<()> {
        let mut asr = self.asr_engine.lock().await;
        if let Some(ref mut engine) = *asr {
            engine.set_language(lang);
        }
        Ok(())
    }
    
    /// Update configuration
    pub async fn update_config(&self, _config: PipelineConfig) -> Result<()> {
        // Can only update config when not running
        let current_state = *self.state.read().await;
        if current_state == PipelineState::Running {
            return Err(anyhow!("Cannot update config while pipeline is running"));
        }
        
        // Configuration updates would go here
        Ok(())
    }
    
    /// Set state and emit event
    async fn set_state(&self, new_state: PipelineState) {
        let mut state = self.state.write().await;
        *state = new_state;
    }
}

fn default_loci_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("models")
        .join("loci")
}

fn find_default_loci_model() -> Option<PathBuf> {
    let dir = default_loci_dir();
    let entries = std::fs::read_dir(&dir).ok()?;

    let mut best: Option<(u64, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("gguf"))
            != Some(true)
        {
            continue;
        }

        let size = entry.metadata().ok().map(|m| m.len()).unwrap_or(0);
        match &best {
            Some((best_size, _)) if *best_size >= size => {}
            _ => best = Some((size, path)),
        }
    }

    best.map(|(_, p)| p)
}

/// Pipeline manager for handling multiple concurrent pipelines
pub struct PipelineManager {
    pipelines: Arc<AsyncMutex<Vec<Arc<RealtimePipeline>>>>,
}

impl PipelineManager {
    pub fn new() -> Self {
        Self {
            pipelines: Arc::new(AsyncMutex::new(Vec::new())),
        }
    }
    
    pub async fn add_pipeline(&self, pipeline: Arc<RealtimePipeline>) {
        self.pipelines.lock().await.push(pipeline);
    }
    
    pub async fn remove_pipeline(&self, _id: &str) -> bool {
        let pipelines = self.pipelines.lock().await;
        let len_before = pipelines.len();
        // Note: In a real implementation, you'd want to track pipeline IDs
        pipelines.len() != len_before
    }
    
    pub async fn stop_all(&self) {
        let pipelines = self.pipelines.lock().await;
        for pipeline in pipelines.iter() {
            let _ = pipeline.stop().await;
        }
    }
    
    pub async fn count(&self) -> usize {
        self.pipelines.lock().await.len()
    }
}

impl Default for PipelineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pipeline_config() {
        let config = PipelineConfig::en_to_zh();
        assert_eq!(config.source_lang, "en");
        assert_eq!(config.target_lang, "zh");
        assert!(!config.bidirectional);
        
        let config = PipelineConfig::bidirectional("en", "zh");
        assert!(config.bidirectional);
    }
    
    #[test]
    fn test_pipeline_stats() {
        let mut stats = PipelineStats::default();
        stats.transcription_count = 10;
        stats.total_latency_ms = 500;
        
        assert!((stats.average_latency_ms() - 50.0).abs() < 0.01);
    }
}

// Import mpsc for shutdown channel
use tokio::sync::mpsc;

