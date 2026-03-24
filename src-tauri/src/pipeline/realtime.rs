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
use tokio::sync::mpsc;
use parking_lot::Mutex as SyncMutex;
use tauri::AppHandle;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::audio::AudioCapture;
use crate::audio::resample_linear;
use crate::asr::{AsrConfig, StreamingConfig, StreamingAsrEngine, StreamingResult};
use crate::commands::tts::TtsRequest;
use crate::translation::{LociTranslator, NllbTranslator, Translator};
use super::events::{
    EventBroadcaster, HistoryManager, HistoryItem, PipelineEvent, PipelineState,
    ErrorCode, TranscriptionSegment, TranslationInfo,
};
use crate::session_bus;

/// Configuration for the realtime pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Source language code (e.g., "en", "zh")
    pub source_lang: String,
    /// Target language code (e.g., "zh", "en")
    pub target_lang: String,
    /// Audio input device ID (None for default)
    pub input_device: Option<String>,
    /// Peer audio input device ID for dual-route setups (optional, reserved)
    pub peer_input_device: Option<String>,
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
    /// Translation engine ("nllb" for deterministic MT, "loci" for LLM translation)
    pub translation_engine: String,
    /// Audio buffer chunk size in samples
    pub chunk_size: usize,
    /// Maximum history items to keep
    pub max_history: usize,
    /// Streaming partial translation emit interval
    pub stream_translation_interval_ms: u64,
    /// Minimum source text chars to emit a streaming translation
    pub stream_translation_min_chars: usize,
    /// Enable TTS playback in backend pipeline
    pub tts_enabled: bool,
    /// Auto play translated text
    pub tts_auto_play: bool,
    /// TTS engine name
    pub tts_engine: String,
    /// TTS voice id
    pub tts_voice: String,
    /// TTS speaking rate
    pub tts_rate: f32,
    /// TTS volume
    pub tts_volume: f32,
    /// TTS output device id
    pub tts_output_device: Option<String>,
    /// Streaming TTS emit interval
    pub stream_tts_interval_ms: u64,
    /// Minimum translated chars to trigger streaming TTS
    pub stream_tts_min_chars: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            source_lang: "en".to_string(),
            target_lang: "zh".to_string(),
            input_device: None,
            peer_input_device: None,
            bidirectional: false,
            vad_frame_ms: 30,
            vad_threshold: 0.01,
            min_speech_duration_ms: 260,
            max_segment_duration_ms: 30000,
            loci_enhanced: false,
            translation_engine: "nllb".to_string(),
            chunk_size: 480, // 30ms at 16kHz
            max_history: 500,
            stream_translation_interval_ms: 450,
            stream_translation_min_chars: 4,
            tts_enabled: true,
            tts_auto_play: true,
            tts_engine: "sherpa-melo".to_string(),
            tts_voice: "sherpa-melo-female".to_string(),
            tts_rate: 1.0,
            tts_volume: 1.0,
            tts_output_device: None,
            stream_tts_interval_ms: 900,
            stream_tts_min_chars: 8,
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
    /// Pipeline instance identifier for diagnostics
    pipeline_id: String,
    /// Pipeline configuration
    config: PipelineConfig,
    /// Current pipeline state
    state: Arc<tokio::sync::RwLock<PipelineState>>,
    /// Audio capture component
    audio_capture: Arc<AsyncMutex<Option<AudioCapture>>>,
    /// Peer audio capture component (for dual-route bidirectional mode)
    peer_audio_capture: Arc<AsyncMutex<Option<AudioCapture>>>,
    /// Streaming ASR engine
    asr_engine: Arc<AsyncMutex<Option<StreamingAsrEngine>>>,
    /// Peer streaming ASR engine
    peer_asr_engine: Arc<AsyncMutex<Option<StreamingAsrEngine>>>,
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
    /// Processing task handle
    processing_task: Arc<AsyncMutex<Option<JoinHandle<()>>>>,
    /// Sample rate for audio
    sample_rate: u32,
}

impl RealtimePipeline {
    /// Create a new realtime pipeline
    pub fn new(app_handle: AppHandle, config: PipelineConfig) -> Result<Self> {
        let max_history = config.max_history;
        // Create event broadcaster
        let broadcaster = EventBroadcaster::new(app_handle);
        
        let engine = config.translation_engine.to_ascii_lowercase();
        let translator: Box<dyn Translator> = match engine.as_str() {
            "loci" => {
                if !config.loci_enhanced {
                    return Err(anyhow!(
                        "Loci translation requested but loci_enhanced is disabled"
                    ));
                }
                let model_path = find_default_loci_model().ok_or_else(|| {
                    anyhow!(
                        "No Loci model found under {}",
                        default_loci_dir().display()
                    )
                })?;
                let translator = LociTranslator::init(&model_path).map_err(|e| {
                    let _ = broadcaster.emit_error(
                        ErrorCode::TranslationModelNotLoaded,
                        format!(
                            "Failed to load Loci model ({}): {}",
                            model_path.display(),
                            e
                        ),
                    );
                    anyhow!(
                        "failed to load Loci model ({}): {}",
                        model_path.display(),
                        e
                    )
                })?;
                Box::new(translator)
            }
            "nllb" | "argos" | "mt" => Box::new(NllbTranslator::new()),
            other => {
                return Err(anyhow!("Unsupported translation engine: {}", other));
            }
        };
        
        let pipeline_id = Uuid::new_v4().to_string();
        tracing::info!(
            pipeline_id = %pipeline_id,
            source_lang = %config.source_lang,
            target_lang = %config.target_lang,
            input_device = ?config.input_device,
            peer_input_device = ?config.peer_input_device,
            bidirectional = config.bidirectional,
            "creating realtime pipeline"
        );

        Ok(Self {
            pipeline_id,
            config,
            state: Arc::new(tokio::sync::RwLock::new(PipelineState::Idle)),
            audio_capture: Arc::new(AsyncMutex::new(None)),
            peer_audio_capture: Arc::new(AsyncMutex::new(None)),
            asr_engine: Arc::new(AsyncMutex::new(None)),
            peer_asr_engine: Arc::new(AsyncMutex::new(None)),
            translator: Arc::new(AsyncMutex::new(translator)),
            broadcaster: Arc::new(broadcaster),
            history: Arc::new(AsyncMutex::new(HistoryManager::new(max_history))),
            stats: Arc::new(SyncMutex::new(PipelineStats::default())),
            is_running: Arc::new(AtomicBool::new(false)),
            shutdown_tx: Arc::new(AsyncMutex::new(None)),
            processing_task: Arc::new(AsyncMutex::new(None)),
            sample_rate: 16000,
        })
    }
    
    /// Start the pipeline
    pub async fn start(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state == PipelineState::Running {
            return Err(anyhow!("Pipeline is already running"));
        }

        tracing::info!(
            pipeline_id = %self.pipeline_id,
            current_state = %current_state,
            source_lang = %self.config.source_lang,
            target_lang = %self.config.target_lang,
            input_device = ?self.config.input_device,
            "pipeline start requested"
        );
        
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

        tracing::info!(
            pipeline_id = %self.pipeline_id,
            input_sample_rate = audio_capture.sample_rate(),
            input_channels = audio_capture.channels(),
            "audio capture initialized for pipeline"
        );
        
        *self.audio_capture.lock().await = Some(audio_capture);
        
        // Initialize streaming ASR engine
        let asr_config = AsrConfig {
            language: Some(self.config.source_lang.clone()),
            ..Default::default()
        };
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

        tracing::info!(
            pipeline_id = %self.pipeline_id,
            asr_chunk_size,
            sample_rate = self.sample_rate,
            "asr engine initialized for pipeline"
        );
        
        *self.asr_engine.lock().await = Some(asr_engine);

        // Initialize peer route when bidirectional mode explicitly provides a second input device.
        if self.config.bidirectional {
            if let Some(peer_input_device) = self.config.peer_input_device.as_deref() {
                let mut peer_capture = AudioCapture::new(Some(peer_input_device))
                    .context("Failed to initialize peer audio capture")?;
                peer_capture
                    .start_capture()
                    .context("Failed to start peer audio capture")?;
                tracing::info!(
                    pipeline_id = %self.pipeline_id,
                    peer_input_device = %peer_input_device,
                    peer_input_sample_rate = peer_capture.sample_rate(),
                    peer_input_channels = peer_capture.channels(),
                    "peer audio capture initialized for pipeline"
                );
                *self.peer_audio_capture.lock().await = Some(peer_capture);

                let peer_asr_config = AsrConfig {
                    language: Some(self.config.target_lang.clone()),
                    ..Default::default()
                };
                let mut peer_streaming_config = StreamingConfig::new(self.sample_rate);
                peer_streaming_config.vad_energy_threshold = self.config.vad_threshold;
                peer_streaming_config.min_speech_duration_ms =
                    (self.config.min_speech_duration_ms.min(u32::MAX as u64)) as u32;
                peer_streaming_config.max_speech_duration_ms =
                    (self.config.max_segment_duration_ms.min(u32::MAX as u64)) as u32;
                peer_streaming_config.chunk_size = asr_chunk_size;
                let peer_asr_engine = StreamingAsrEngine::new(peer_asr_config, peer_streaming_config)
                    .map_err(|e| anyhow!("Failed to initialize peer ASR engine: {}", e))?;
                *self.peer_asr_engine.lock().await = Some(peer_asr_engine);
                tracing::info!(
                    pipeline_id = %self.pipeline_id,
                    peer_input_device = %peer_input_device,
                    "peer asr engine initialized for pipeline"
                );
            } else {
                tracing::info!(
                    pipeline_id = %self.pipeline_id,
                    "bidirectional enabled without peer_input_device; peer capture route disabled"
                );
            }
        }
        
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
        let peer_audio_capture = self.peer_audio_capture.clone();
        let asr_engine = self.asr_engine.clone();
        let peer_asr_engine = self.peer_asr_engine.clone();
        let translator = self.translator.clone();
        let broadcaster = self.broadcaster.clone();
        let history = self.history.clone();
        let stats = self.stats.clone();
        let is_running = self.is_running.clone();
        let state = self.state.clone();
        let config = self.config.clone();
        let sample_rate = self.sample_rate;
        // Spawn the main processing loop
        let pipeline_id = self.pipeline_id.clone();
        let handle = tokio::spawn(async move {
            let mut last_stats_emit = Instant::now();
            let mut last_translation_error_emit = Instant::now() - Duration::from_secs(60);
            let mut last_stream_translation_emit = Instant::now() - Duration::from_secs(60);
            let mut last_stream_tts_emit = Instant::now() - Duration::from_secs(60);
            let mut last_stream_source_text = String::new();
            let mut last_tts_text = String::new();
            let mut stream_seq: u64 = 0;
            let mut chunk_buffer: std::collections::VecDeque<f32> =
                std::collections::VecDeque::with_capacity(asr_chunk_size * 8);
            let mut peer_chunk_buffer: std::collections::VecDeque<f32> =
                std::collections::VecDeque::with_capacity(asr_chunk_size * 8);

            tracing::info!(pipeline_id = %pipeline_id, "pipeline processing loop started");
            
            loop {
                // Check for shutdown
                if shutdown_rx.try_recv().is_ok() || !is_running.load(Ordering::SeqCst) {
                    tracing::info!(pipeline_id = %pipeline_id, "pipeline loop shutdown signal received");
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
                                    tracing::error!(pipeline_id = %pipeline_id, error = %e, "asr process error");
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

                            let ptxt = partial.text.trim();
                            let should_stream_translate = ptxt.len() >= config.stream_translation_min_chars
                                && last_stream_translation_emit.elapsed()
                                    >= Duration::from_millis(config.stream_translation_interval_ms)
                                && ptxt != last_stream_source_text;
                            if should_stream_translate {
                                let translation_result: Result<_, String> = {
                                    let mut trans = translator.lock().await;
                                    trans.translate(
                                        ptxt,
                                        &config.source_lang,
                                        &config.target_lang,
                                    )
                                    .map_err(|e| e.to_string())
                                };
                                match translation_result {
                                    Ok(translation) => {
                                        stream_seq += 1;
                                        let sid = format!("stream-{}", stream_seq);
                                        let _ = broadcaster.emit_translation(
                                            &sid,
                                            ptxt,
                                            &translation.text,
                                            &translation.source_lang,
                                            &translation.target_lang,
                                            translation.confidence,
                                        );
                                        last_stream_source_text = ptxt.to_string();
                                        last_stream_translation_emit = Instant::now();

                                        if config.tts_enabled
                                            && config.tts_auto_play
                                            && translation.text.trim().len() >= config.stream_tts_min_chars
                                            && last_stream_tts_emit.elapsed()
                                                >= Duration::from_millis(config.stream_tts_interval_ms)
                                            && translation.text.trim() != last_tts_text
                                        {
                                            spawn_pipeline_tts(&config, translation.text.clone());
                                            last_stream_tts_emit = Instant::now();
                                            last_tts_text = translation.text.trim().to_string();
                                        }
                                    }
                                    Err(err) => {
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
                                    tracing::debug!(
                                        pipeline_id = %pipeline_id,
                                        source_len = transcription.text.len(),
                                        target_len = translation.text.len(),
                                        "translation completed"
                                    );
                                    
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
                                    let _ = session_bus::append_history(&session_bus::SessionHistoryItem {
                                        id,
                                        source_text: transcription.text.clone(),
                                        translated_text: translation.text.clone(),
                                        source_lang: config.source_lang.clone(),
                                        target_lang: config.target_lang.clone(),
                                        timestamp: chrono::Utc::now().to_rfc3339(),
                                        confidence: transcription.confidence.min(translation.confidence),
                                    });
                                    
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

                                    if config.tts_enabled
                                        && config.tts_auto_play
                                        && !translation.text.trim().is_empty()
                                        && translation.text.trim() != last_tts_text
                                    {
                                        spawn_pipeline_tts(&config, translation.text.clone());
                                        last_tts_text = translation.text.trim().to_string();
                                    }
                                    }
                                    Err(err) => {
                                        stats.lock().error_count += 1;
                                        tracing::error!(pipeline_id = %pipeline_id, error = %err, "translation failed");
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
                            tracing::error!(pipeline_id = %pipeline_id, error = %msg, "asr returned error result");
                            let _ = broadcaster.emit_error(
                                ErrorCode::AsrTranscriptionFailed,
                                msg,
                            );
                            stats.lock().error_count += 1;
                        }
                        }
                    }
                }

                // Peer route: independent capture + ASR in bidirectional mode.
                if config.bidirectional {
                    let (peer_samples, peer_input_sample_rate) = {
                        let mut capture_guard = peer_audio_capture.lock().await;
                        if let Some(ref mut capture) = *capture_guard {
                            (capture.get_samples(), capture.sample_rate())
                        } else {
                            (Vec::new(), sample_rate)
                        }
                    };

                    if !peer_samples.is_empty() {
                        let peer_resampled =
                            if peer_input_sample_rate > 0 && peer_input_sample_rate != sample_rate {
                                resample_linear(&peer_samples, peer_input_sample_rate, sample_rate)
                            } else {
                                peer_samples
                            };
                        peer_chunk_buffer.extend(peer_resampled);

                        while peer_chunk_buffer.len() >= asr_chunk_size {
                            let peer_chunk: Vec<f32> =
                                peer_chunk_buffer.drain(..asr_chunk_size).collect();
                            let peer_asr_result = {
                                let mut asr_guard = peer_asr_engine.lock().await;
                                if let Some(ref mut engine) = *asr_guard {
                                    match engine.process(&peer_chunk) {
                                        Ok(r) => r,
                                        Err(e) => {
                                            stats.lock().error_count += 1;
                                            tracing::error!(
                                                pipeline_id = %pipeline_id,
                                                error = %e,
                                                "peer asr process error"
                                            );
                                            None
                                        }
                                    }
                                } else {
                                    None
                                }
                            };

                            if let Some(StreamingResult::Final(peer_transcription)) = peer_asr_result
                            {
                                if !peer_transcription.text.trim().is_empty() {
                                    let reverse_result: Result<_, String> = {
                                        let mut trans = translator.lock().await;
                                        trans.translate(
                                            &peer_transcription.text,
                                            &config.target_lang,
                                            &config.source_lang,
                                        )
                                        .map_err(|e| e.to_string())
                                    };
                                    if let Ok(reverse) = reverse_result {
                                        let _ = broadcaster.emit(&PipelineEvent::BidirectionalTranslation {
                                            forward: None,
                                            backward: Some(TranslationInfo {
                                                source_text: peer_transcription.text.clone(),
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
                        }
                    }
                }
                
                // Emit stats periodically
                if last_stats_emit.elapsed().as_secs() >= 5 {
                    let stats_guard = stats.lock();
                    let _ = session_bus::write_metrics(&session_bus::SessionMetrics {
                        total_audio_duration_ms: stats_guard.total_audio_duration_ms,
                        speech_duration_ms: stats_guard.speech_duration_ms,
                        transcription_count: stats_guard.transcription_count,
                        translation_count: stats_guard.translation_count,
                        average_latency_ms: stats_guard.average_latency_ms(),
                        asr_average_latency_ms: stats_guard.average_latency_ms(),
                        translation_average_latency_ms: 0.0,
                        tts_average_latency_ms: 0.0,
                        last_updated_unix_ms: session_bus::now_unix_ms(),
                    });
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
            tracing::info!(pipeline_id = %pipeline_id, "pipeline processing loop ended");
        });

        *self.processing_task.lock().await = Some(handle);
        
        Ok(())
    }
    
    /// Stop the pipeline
    pub async fn stop(&self) -> Result<()> {
        let current_state = *self.state.read().await;
        if current_state != PipelineState::Running && current_state != PipelineState::Paused {
            tracing::info!(
                pipeline_id = %self.pipeline_id,
                current_state = %current_state,
                "pipeline stop requested but already idle"
            );
            return Ok(());
        }

        tracing::info!(
            pipeline_id = %self.pipeline_id,
            current_state = %current_state,
            "pipeline stop requested"
        );
        
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
        let mut peer_capture_guard = self.peer_audio_capture.lock().await;
        if let Some(ref mut capture) = *peer_capture_guard {
            capture.stop_capture();
        }
        *peer_capture_guard = None;
        tracing::info!(pipeline_id = %self.pipeline_id, "pipeline audio capture released");
        
        // Clear ASR engine
        *self.asr_engine.lock().await = None;
        *self.peer_asr_engine.lock().await = None;
        tracing::info!(pipeline_id = %self.pipeline_id, "pipeline asr engine released");

        if let Some(mut handle) = self.processing_task.lock().await.take() {
            match tokio::time::timeout(Duration::from_secs(2), &mut handle).await {
                Ok(joined) => {
                    if let Err(join_err) = joined {
                        tracing::error!(
                            pipeline_id = %self.pipeline_id,
                            error = %join_err,
                            "pipeline task join failed"
                        );
                    }
                }
                Err(_) => {
                    handle.abort();
                    tracing::warn!(
                        pipeline_id = %self.pipeline_id,
                        "pipeline task did not finish in time; aborted"
                    );
                }
            }
        }
        
        // Update state
        self.set_state(PipelineState::Idle).await;
        tracing::info!(pipeline_id = %self.pipeline_id, "pipeline stopped");
        
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

fn spawn_pipeline_tts(config: &PipelineConfig, text: String) {
    let request = TtsRequest {
        text,
        voice: config.tts_voice.clone(),
        engine: Some(config.tts_engine.clone()),
        rate: config.tts_rate,
        pitch: Some(0.0),
        volume: Some(config.tts_volume),
        output_device: config.tts_output_device.clone(),
        custom_voice: None,
    };

    tokio::task::spawn_blocking(move || {
        if let Err(e) = crate::commands::tts::speak_text(request) {
            tracing::warn!("pipeline tts playback failed: {}", e);
        }
    });
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
        let stats = PipelineStats {
            transcription_count: 10,
            total_latency_ms: 500,
            ..Default::default()
        };
        
        assert!((stats.average_latency_ms() - 50.0).abs() < 0.01);
    }
}

