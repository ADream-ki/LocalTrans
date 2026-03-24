#![allow(dead_code)]
//! Streaming ASR Processing Module
//!
//! Provides real-time speech recognition with VAD integration,
//! intelligent audio chunking, and partial result management.

use crate::asr::{AsrConfig, AsrEngine, AsrError, PartialTranscription, Transcription};
use crate::audio::{AudioBuffer, VadDetector, VadResult};
use std::path::{Path, PathBuf};

#[cfg(all(feature = "sherpa-backend", not(feature = "mock-asr")))]
use crate::asr::SherpaAsrEngine;
#[cfg(all(not(feature = "sherpa-backend"), not(feature = "mock-asr")))]
use crate::asr::WhisperEngine;

use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};

/// Streaming ASR state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StreamingState {
    /// Idle, waiting for speech
    #[default]
    Idle,
    /// Speech detected, accumulating audio
    SpeechDetected,
    /// Processing speech
    Processing,
    /// Speech ended, finalizing
    Finalizing,
    /// Error state
    Error,
}

impl std::fmt::Display for StreamingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamingState::Idle => write!(f, "idle"),
            StreamingState::SpeechDetected => write!(f, "speech_detected"),
            StreamingState::Processing => write!(f, "processing"),
            StreamingState::Finalizing => write!(f, "finalizing"),
            StreamingState::Error => write!(f, "error"),
        }
    }
}

use serde::{Deserialize, Serialize};

/// Streaming ASR configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// Sample rate for audio (default: 16000)
    pub sample_rate: u32,
    /// Minimum speech duration to trigger transcription (ms)
    pub min_speech_duration_ms: u32,
    /// Maximum speech duration before forced segmentation (ms)
    pub max_speech_duration_ms: u32,
    /// Silence duration to end speech segment (ms)
    pub silence_duration_ms: u32,
    /// Buffer size for audio chunks
    pub chunk_size: usize,
    /// Enable partial results
    pub enable_partial_results: bool,
    /// Interval for partial results (ms)
    pub partial_result_interval_ms: u32,
    /// VAD sensitivity (0.0 - 1.0)
    pub vad_sensitivity: f32,
    /// Energy threshold used by the built-in VAD (higher = less sensitive)
    pub vad_energy_threshold: f32,
    /// Auto-reset after long silence (ms)
    pub auto_reset_timeout_ms: u32,
    /// Maximum accumulated audio before forced flush (seconds)
    pub max_buffer_seconds: f32,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000,
            min_speech_duration_ms: 220,
            max_speech_duration_ms: 30000, // 30 seconds max
            silence_duration_ms: 350,
            chunk_size: 1600, // 100ms at 16kHz
            enable_partial_results: true,
            // Calling a full offline decode too frequently is expensive.
            // A slightly slower cadence keeps CPU usage stable while still
            // feeling "live" in the UI.
            partial_result_interval_ms: 220,
            vad_sensitivity: 0.5,
            vad_energy_threshold: 0.01,
            auto_reset_timeout_ms: 5000,
            max_buffer_seconds: 60.0,
        }
    }
}

impl StreamingConfig {
    /// Create a new configuration with the specified sample rate
    pub fn new(sample_rate: u32) -> Self {
        // 30ms chunks are a good compromise for low-latency VAD and stable
        // downstream buffering when targeting 16kHz audio.
        let chunk_size = ((sample_rate as f32) * 0.03) as usize;
        Self {
            sample_rate,
            chunk_size: chunk_size.max(1),
            ..Default::default()
        }
    }

    /// Calculate frame size for VAD based on frame duration
    pub fn vad_frame_size(&self, frame_duration_ms: u32) -> usize {
        (self.sample_rate as f32 * frame_duration_ms as f32 / 1000.0) as usize
    }
}

/// Audio segment for processing
#[derive(Debug, Clone)]
struct AudioSegment {
    samples: Vec<f32>,
    start_time: f64,
    end_time: f64,
    is_speech: bool,
}

/// Callback for transcription results
pub type TranscriptionCallback = Box<dyn Fn(Transcription) + Send + Sync>;

/// Callback for partial results
pub type PartialResultCallback = Box<dyn Fn(PartialTranscription) + Send + Sync>;

/// State change callback
pub type StateChangeCallback = Box<dyn Fn(StreamingState) + Send + Sync>;

/// The ASR engine type used by StreamingAsrEngine
#[cfg(all(feature = "sherpa-backend", not(feature = "mock-asr")))]
type AsrEngineImpl = SherpaAsrEngine;
#[cfg(all(not(feature = "sherpa-backend"), not(feature = "mock-asr")))]
type AsrEngineImpl = WhisperEngine;
#[cfg(feature = "mock-asr")]
type AsrEngineImpl = super::MockAsrEngine;

/// Streaming ASR Engine with VAD integration
pub struct StreamingAsrEngine {
    /// ASR engine (Sherpa or Whisper based on feature)
    asr: AsrEngineImpl,
    /// Configuration
    config: StreamingConfig,
    /// VAD detector
    vad: VadDetector,
    /// Audio buffer for accumulation
    audio_buffer: AudioBuffer,
    /// Speech segment buffer
    speech_buffer: Vec<f32>,
    /// Current state
    state: StreamingState,
    /// Speech start timestamp
    speech_start: Option<Instant>,
    /// Last activity timestamp
    last_activity: Instant,
    /// Last partial result time
    last_partial_time: Instant,
    /// Accumulated transcription
    accumulated_text: String,
    /// Last emitted partial text (avoid UI spam)
    last_partial_text: String,
    /// Speech buffer length at last partial decode
    last_partial_audio_len: usize,
    /// Time offset for segment timing
    time_offset: f64,
    /// Number of frames processed
    frames_processed: usize,
    /// Silence frames counter
    silence_frames: usize,
    /// Speech frames counter
    speech_frames: usize,
    /// Processing statistics
    stats: StreamingStats,
}

/// Processing statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StreamingStats {
    /// Total audio processed (seconds)
    pub total_audio_seconds: f64,
    /// Total speech detected (seconds)
    pub total_speech_seconds: f64,
    /// Number of utterances processed
    pub utterance_count: usize,
    /// Average processing latency (ms)
    pub avg_latency_ms: f64,
    /// Number of partial results
    pub partial_result_count: usize,
    /// Number of final results
    pub final_result_count: usize,
}

impl StreamingAsrEngine {
    /// Create a new streaming ASR engine
    pub fn new(asr_config: AsrConfig, streaming_config: StreamingConfig) -> Result<Self, AsrError> {
        #[cfg(all(feature = "sherpa-backend", not(feature = "mock-asr")))]
        let asr = {
            // Use model path from AsrConfig or default location
            let mut config = if asr_config.model_path.as_os_str().is_empty() {
                // Use default model path from environment or app data
                let model_path = std::env::var("SHERPA_MODEL_PATH")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| {
                        dirs::data_local_dir()
                            .unwrap_or_else(|| std::path::PathBuf::from("."))
                            .join("LocalTrans")
                            .join("models")
                            .join("asr")
                    });
                AsrConfig {
                    model_path,
                    ..asr_config
                }
            } else {
                asr_config
            };

            let preferred_lang = config.language.as_deref().unwrap_or("auto");
            let selected = pick_preferred_asr_model_dir(&config.model_path, preferred_lang);
            if selected != config.model_path {
                tracing::info!(
                    preferred_lang = %preferred_lang,
                    from = %config.model_path.display(),
                    to = %selected.display(),
                    "selected preferred ASR model directory"
                );
                config.model_path = selected;
            } else {
                tracing::info!(
                    preferred_lang = %preferred_lang,
                    selected = %config.model_path.display(),
                    "using ASR model directory"
                );
            }

            SherpaAsrEngine::init(config)?
        };

        #[cfg(all(not(feature = "sherpa-backend"), not(feature = "mock-asr")))]
        let asr = WhisperEngine::init(asr_config)?;

        #[cfg(feature = "mock-asr")]
        let asr = {
            tracing::info!("Using Mock ASR engine for development");
            crate::asr::MockAsrEngine::init(asr_config)?
        };

        let vad_frame_duration_ms = 30;
        let mut vad = VadDetector::new(streaming_config.sample_rate, vad_frame_duration_ms);
        vad.set_threshold(streaming_config.vad_energy_threshold);

        let chunk_size = streaming_config.chunk_size;
        let max_buffer_samples =
            (streaming_config.max_buffer_seconds * streaming_config.sample_rate as f32) as usize;

        let audio_buffer = AudioBuffer::new(max_buffer_samples, chunk_size);

        Ok(Self {
            asr,
            config: streaming_config,
            vad,
            audio_buffer,
            speech_buffer: Vec::with_capacity(16000 * 30), // 30 seconds
            state: StreamingState::Idle,
            speech_start: None,
            last_activity: Instant::now(),
            last_partial_time: Instant::now(),
            accumulated_text: String::new(),
            last_partial_text: String::new(),
            last_partial_audio_len: 0,
            time_offset: 0.0,
            frames_processed: 0,
            silence_frames: 0,
            speech_frames: 0,
            stats: StreamingStats::default(),
        })
    }

    /// Get current state
    pub fn state(&self) -> StreamingState {
        self.state
    }

    /// Process a chunk of audio
    pub fn process(&mut self, audio: &[f32]) -> Result<Option<StreamingResult>, AsrError> {
        self.last_activity = Instant::now();
        self.frames_processed += audio.len();
        self.stats.total_audio_seconds += audio.len() as f64 / self.config.sample_rate as f64;

        // Add to buffer
        self.audio_buffer.push(audio);

        // Process through VAD
        let vad_result = self.vad.process(audio);
        let state_changed = self.update_state(vad_result);

        // Handle state changes
        if state_changed {
            return self.handle_state_change();
        }

        // Accumulate speech audio
        if matches!(
            self.state,
            StreamingState::SpeechDetected | StreamingState::Processing
        ) {
            self.speech_buffer.extend_from_slice(audio);
            self.stats.total_speech_seconds += audio.len() as f64 / self.config.sample_rate as f64;
        }

        // Check for partial result timing
        // Require a minimum amount of accumulated speech before attempting a partial decode.
        let min_partial_samples =
            (self.config.sample_rate as usize / 2).max(self.config.chunk_size * 5);
        if self.config.enable_partial_results && self.speech_buffer.len() >= min_partial_samples {
            let elapsed = self.last_partial_time.elapsed().as_millis() as u32;
            if elapsed >= self.config.partial_result_interval_ms {
                return self.generate_partial_result();
            }
        }

        // Check for max speech duration
        if let Some(start) = self.speech_start {
            let speech_duration = start.elapsed().as_millis() as u32;
            if speech_duration >= self.config.max_speech_duration_ms {
                debug!("Max speech duration reached, forcing finalize");
                return self.finalize_speech();
            }
        }

        Ok(None)
    }

    /// Update internal state based on VAD result
    fn update_state(&mut self, vad_result: VadResult) -> bool {
        let previous_state = self.state;

        match vad_result {
            VadResult::SpeechStart => {
                self.state = StreamingState::SpeechDetected;
                self.speech_start = Some(Instant::now());
                self.speech_frames = 1;
                self.silence_frames = 0;
                info!("Speech started");
            }
            VadResult::Speech => {
                self.speech_frames += 1;
                if matches!(self.state, StreamingState::Idle) {
                    self.state = StreamingState::SpeechDetected;
                    self.speech_start = Some(Instant::now());
                }
            }
            VadResult::SpeechEnd => {
                // VadDetector already applies a silence window before emitting SpeechEnd.
                // Treat SpeechEnd as authoritative and finalize immediately.
                if matches!(
                    self.state,
                    StreamingState::SpeechDetected | StreamingState::Processing
                ) {
                    self.state = StreamingState::Finalizing;
                    info!(
                        "Speech ended after {}ms",
                        self.speech_start
                            .map(|s| s.elapsed().as_millis())
                            .unwrap_or(0)
                    );
                }
            }
            VadResult::Silence => {
                self.silence_frames += 1;
            }
        }

        self.state != previous_state
    }

    /// Handle state transitions
    fn handle_state_change(&mut self) -> Result<Option<StreamingResult>, AsrError> {
        match self.state {
            StreamingState::Finalizing => self.finalize_speech(),
            StreamingState::Error => {
                error!("Streaming ASR entered error state");
                Ok(Some(StreamingResult::Error("ASR error state".to_string())))
            }
            _ => Ok(None),
        }
    }

    /// Generate a partial result
    fn generate_partial_result(&mut self) -> Result<Option<StreamingResult>, AsrError> {
        if self.speech_buffer.is_empty() {
            return Ok(None);
        }

        // Avoid running a full decode when very little new audio arrived.
        let min_delta_samples = (self.config.sample_rate as usize / 4).max(self.config.chunk_size);
        if self
            .speech_buffer
            .len()
            .saturating_sub(self.last_partial_audio_len)
            < min_delta_samples
        {
            return Ok(None);
        }

        self.last_partial_time = Instant::now();
        self.state = StreamingState::Processing;

        // For maximum compatibility across ASR backends, generate partial text
        // by transcribing the currently accumulated speech buffer.
        //
        // This avoids relying on backend-specific streaming APIs and prevents
        // issues like double-feeding the same audio.
        let start = Instant::now();
        match self
            .asr
            .transcribe(&self.speech_buffer, self.config.sample_rate)
        {
            Ok(transcription) => {
                let latency = start.elapsed().as_millis() as f64;
                self.update_latency_stats(latency);

                // Mark that we've attempted to decode up to the current buffer length,
                // even if we decide not to emit an event (e.g. duplicate text).
                self.last_partial_audio_len = self.speech_buffer.len();

                let text = transcription.text.trim();
                if text.is_empty() {
                    return Ok(None);
                }

                if text == self.last_partial_text.as_str() {
                    return Ok(None);
                }
                self.last_partial_text = text.to_string();

                self.stats.partial_result_count += 1;
                debug!(
                    "Partial result: '{}' (confidence: {:.2})",
                    text, transcription.confidence
                );

                Ok(Some(StreamingResult::Partial(PartialTranscription {
                    text: text.to_string(),
                    is_final: false,
                    confidence: transcription.confidence,
                    detected_language: Some(transcription.language),
                    segment_count: transcription.segments.len(),
                })))
            }
            Err(e) => {
                // Partial failures should not kill the whole pipeline; keep going.
                warn!("Partial transcription failed (ignored): {}", e);
                Ok(None)
            }
        }
    }

    /// Finalize the current speech segment
    fn finalize_speech(&mut self) -> Result<Option<StreamingResult>, AsrError> {
        if self.speech_buffer.is_empty() {
            self.reset_speech_state();
            return Ok(None);
        }

        // Drop extremely short segments (often false positives)
        let duration_ms =
            (self.speech_buffer.len() as f64 / self.config.sample_rate as f64 * 1000.0) as u32;
        if duration_ms < self.config.min_speech_duration_ms {
            debug!(
                "Discarding short speech segment: {}ms < {}ms",
                duration_ms, self.config.min_speech_duration_ms
            );
            self.reset_speech_state();
            return Ok(None);
        }

        let start = Instant::now();
        self.state = StreamingState::Processing;

        // Run final transcription
        match self
            .asr
            .transcribe(&self.speech_buffer, self.config.sample_rate)
        {
            Ok(transcription) => {
                let latency = start.elapsed().as_millis() as f64;
                self.update_latency_stats(latency);

                // Update accumulated text
                if !transcription.text.is_empty() {
                    if !self.accumulated_text.is_empty() {
                        self.accumulated_text.push(' ');
                    }
                    self.accumulated_text.push_str(&transcription.text);
                }

                // Update time offset
                if let Some(last_segment) = transcription.segments.last() {
                    self.time_offset = last_segment.end;
                }

                self.stats.final_result_count += 1;
                self.stats.utterance_count += 1;

                info!(
                    "Final transcription: '{}' ({} segments, {}ms)",
                    transcription.text,
                    transcription.segments.len(),
                    latency
                );

                let result = StreamingResult::Final(transcription);
                self.reset_speech_state();

                Ok(Some(result))
            }
            Err(e) => {
                error!("Final transcription failed: {}", e);
                self.state = StreamingState::Error;
                Err(e)
            }
        }
    }

    /// Reset speech-related state
    fn reset_speech_state(&mut self) {
        self.speech_buffer.clear();
        self.speech_start = None;
        self.silence_frames = 0;
        self.speech_frames = 0;
        self.last_partial_text.clear();
        self.last_partial_audio_len = 0;
        self.state = StreamingState::Idle;
        self.asr.reset();
    }

    /// Update latency statistics
    fn update_latency_stats(&mut self, latency_ms: f64) {
        let total = self.stats.final_result_count + self.stats.partial_result_count;
        if total > 0 {
            self.stats.avg_latency_ms =
                (self.stats.avg_latency_ms * (total - 1) as f64 + latency_ms) / total as f64;
        } else {
            self.stats.avg_latency_ms = latency_ms;
        }
    }

    /// Force finalize current speech (e.g., on user action)
    pub fn force_finalize(&mut self) -> Result<Option<StreamingResult>, AsrError> {
        if self.speech_buffer.is_empty() {
            return Ok(None);
        }
        self.state = StreamingState::Finalizing;
        self.finalize_speech()
    }

    /// Reset the entire engine state
    pub fn reset(&mut self) {
        info!("Resetting streaming ASR engine");
        self.reset_speech_state();
        self.audio_buffer.clear();
        self.accumulated_text.clear();
        self.time_offset = 0.0;
        self.frames_processed = 0;
        self.last_activity = Instant::now();
    }

    /// Check for auto-reset timeout
    pub fn check_timeout(&mut self) -> bool {
        let elapsed = self.last_activity.elapsed().as_millis() as u32;
        if elapsed >= self.config.auto_reset_timeout_ms
            && !matches!(self.state, StreamingState::Idle)
        {
            info!("Auto-reset triggered after {}ms inactivity", elapsed);
            self.reset();
            return true;
        }
        false
    }

    /// Get accumulated transcription text
    pub fn accumulated_text(&self) -> &str {
        &self.accumulated_text
    }

    /// Get processing statistics
    pub fn stats(&self) -> &StreamingStats {
        &self.stats
    }

    /// Set the transcription language
    pub fn set_language(&mut self, lang: &str) {
        self.asr.set_language(lang);
    }

    /// Get the current language
    pub fn get_language(&self) -> Option<&str> {
        self.asr.get_language()
    }

    /// Check if GPU is enabled
    pub fn is_gpu_enabled(&self) -> bool {
        self.asr.is_gpu_enabled()
    }

    /// Get buffered audio duration
    pub fn buffered_duration(&self) -> f64 {
        self.speech_buffer.len() as f64 / self.config.sample_rate as f64
    }

    /// Get VAD state
    pub fn is_speech_detected(&self) -> bool {
        self.vad.is_speech()
    }
}

fn pick_preferred_asr_model_dir(base_dir: &Path, preferred_lang: &str) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if is_valid_asr_model_dir(base_dir) {
        candidates.push(base_dir.to_path_buf());
    }

    if let Ok(entries) = std::fs::read_dir(base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_valid_asr_model_dir(&path) {
                candidates.push(path);
            }
        }
    }

    if candidates.is_empty() {
        return base_dir.to_path_buf();
    }

    let lang = preferred_lang.to_ascii_lowercase();
    let prefer_zh = matches!(lang.as_str(), "zh" | "zh-cn" | "zh-tw" | "yue");
    let prefer_stable_for_en = matches!(lang.as_str(), "en" | "en-us" | "en-gb")
        && std::env::var("LOCALTRANS_PREFER_STABLE_ASR_FOR_EN")
            .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true);

    let mut best: Option<(i32, u64, PathBuf)> = None;
    for c in candidates {
        let name = c.to_string_lossy().to_ascii_lowercase();
        let has_paraformer = has_paraformer_files(&c);
        let has_zipformer = has_zipformer_files(&c);
        let mut score = 0i32;

        if prefer_zh {
            if has_paraformer {
                score += 8;
            }
            if name.contains("zh") || name.contains("trilingual") || name.contains("multi") {
                score += 4;
            }
            if name.contains("-en") || name.contains("_en") {
                score -= 4;
            }
        } else {
            if prefer_stable_for_en {
                // On some Windows hosts, specific English zipformer packs can hard-abort
                // in native runtime. Prefer paraformer/multilingual packs for robustness.
                if has_paraformer {
                    score += 8;
                }
                if name.contains("zh") || name.contains("multi") || name.contains("trilingual") {
                    score += 4;
                }
                if name.contains("-en") || name.contains("_en") {
                    score -= 4;
                }
            }
            if has_zipformer {
                score += 4;
            }
            if name.contains("-en") || name.contains("_en") {
                score += 2;
            }
        }

        let size = dir_size(&c);
        match &best {
            Some((best_score, best_size, _))
                if *best_score > score || (*best_score == score && *best_size >= size) => {}
            _ => best = Some((score, size, c)),
        }
    }

    best.map(|(_, _, p)| p).unwrap_or_else(|| base_dir.to_path_buf())
}

fn is_valid_asr_model_dir(dir: &Path) -> bool {
    has_zipformer_files(dir) || has_paraformer_files(dir)
}

fn has_zipformer_files(dir: &Path) -> bool {
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
        let p = entry.path();
        if !p.is_file() {
            return false;
        }
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
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

/// Result from streaming ASR processing
#[derive(Debug, Clone)]
pub enum StreamingResult {
    /// Partial transcription result
    Partial(PartialTranscription),
    /// Final transcription result
    Final(Transcription),
    /// Error occurred
    Error(String),
}

/// Thread-safe wrapper for StreamingAsrEngine
pub struct ThreadSafeStreamingEngine {
    inner: Arc<RwLock<StreamingAsrEngine>>,
}

impl ThreadSafeStreamingEngine {
    pub fn new(asr_config: AsrConfig, streaming_config: StreamingConfig) -> Result<Self, AsrError> {
        Ok(Self {
            inner: Arc::new(RwLock::new(StreamingAsrEngine::new(
                asr_config,
                streaming_config,
            )?)),
        })
    }

    pub fn process(&self, audio: &[f32]) -> Result<Option<StreamingResult>, AsrError> {
        let mut engine = self.inner.write();
        engine.process(audio)
    }

    pub fn force_finalize(&self) -> Result<Option<StreamingResult>, AsrError> {
        let mut engine = self.inner.write();
        engine.force_finalize()
    }

    pub fn reset(&self) {
        let mut engine = self.inner.write();
        engine.reset();
    }

    pub fn state(&self) -> StreamingState {
        let engine = self.inner.read();
        engine.state()
    }

    pub fn set_language(&self, lang: &str) {
        let mut engine = self.inner.write();
        engine.set_language(lang);
    }

    pub fn stats(&self) -> StreamingStats {
        let engine = self.inner.read();
        engine.stats().clone()
    }

    pub fn accumulated_text(&self) -> String {
        let engine = self.inner.read();
        engine.accumulated_text().to_string()
    }
}

impl Clone for ThreadSafeStreamingEngine {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

/// Audio chunker for breaking continuous audio into processable segments
pub struct AudioChunker {
    buffer: VecDeque<f32>,
    chunk_size: usize,
    overlap_size: usize,
}

impl AudioChunker {
    /// Create a new audio chunker
    pub fn new(chunk_size: usize, overlap_ms: u32, sample_rate: u32) -> Self {
        let overlap_size = (sample_rate as f32 * overlap_ms as f32 / 1000.0) as usize;
        Self {
            buffer: VecDeque::with_capacity(chunk_size * 2),
            chunk_size,
            overlap_size,
        }
    }

    /// Add audio samples to the buffer
    pub fn push(&mut self, samples: &[f32]) {
        self.buffer.extend(samples);
    }

    /// Get the next chunk if available
    pub fn get_chunk(&mut self) -> Option<Vec<f32>> {
        if self.buffer.len() >= self.chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.chunk_size).collect();
            // Put back overlap samples
            if self.overlap_size > 0 && self.buffer.len() < self.overlap_size {
                let overlap_start = chunk.len().saturating_sub(self.overlap_size);
                for sample in chunk.iter().skip(overlap_start).rev() {
                    self.buffer.push_front(*sample);
                }
            }
            Some(chunk)
        } else {
            None
        }
    }

    /// Get remaining samples
    pub fn drain_remaining(&mut self) -> Vec<f32> {
        self.buffer.drain(..).collect()
    }

    /// Check if buffer has enough samples
    pub fn has_chunk(&self) -> bool {
        self.buffer.len() >= self.chunk_size
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_config_default() {
        let config = StreamingConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.chunk_size, 1600);
        assert!(config.enable_partial_results);
    }

    #[test]
    fn test_streaming_state_display() {
        assert_eq!(StreamingState::Idle.to_string(), "idle");
        assert_eq!(
            StreamingState::SpeechDetected.to_string(),
            "speech_detected"
        );
        assert_eq!(StreamingState::Processing.to_string(), "processing");
    }

    #[test]
    fn test_audio_chunker() {
        let mut chunker = AudioChunker::new(100, 0, 16000);

        // Add samples
        chunker.push(&[1.0f32; 50]);
        assert!(!chunker.has_chunk());

        chunker.push(&[1.0f32; 100]);
        assert!(chunker.has_chunk());

        let chunk = chunker.get_chunk();
        assert!(chunk.is_some());
        assert_eq!(chunk.unwrap().len(), 100);
    }

    #[test]
    fn test_audio_chunker_overlap() {
        let mut chunker = AudioChunker::new(100, 10, 16000); // 10ms overlap = 160 samples

        chunker.push(&[1.0f32; 200]);

        let chunk1 = chunker.get_chunk();
        assert!(chunk1.is_some());

        // Check that overlap was preserved
        assert!(!chunker.is_empty());
    }

    #[test]
    fn test_streaming_stats() {
        let stats = StreamingStats::default();
        assert_eq!(stats.total_audio_seconds, 0.0);
        assert_eq!(stats.utterance_count, 0);
    }
}

