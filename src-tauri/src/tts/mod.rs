//! TTS (Text-to-Speech) Module
//! 
//! Supports multiple TTS engines for real-time speech synthesis
//! - Edge TTS: Microsoft's online TTS (free, high quality)
//! - Piper TTS: Local neural TTS (offline, fast)
//! - GPT-SoVITS: Custom voice cloning (local)
//! - RVC: Real-time Voice Conversion

pub mod edge_tts;
pub mod playback;
pub mod custom_voice;
pub mod piper_tts;

// Re-export main types
pub use edge_tts::EdgeTtsEngine;
pub use custom_voice::CustomVoiceEngine;
pub use piper_tts::PiperTtsEngine;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Voice information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub language: String,
    pub gender: String,
    pub style: Option<String>,
    /// Is this a custom voice?
    pub is_custom: bool,
    /// Path to custom voice model (if custom)
    pub model_path: Option<String>,
}

/// TTS audio result
#[derive(Debug, Clone)]
pub struct TtsAudio {
    /// Audio samples (f32, normalized to [-1.0, 1.0])
    pub samples: Vec<f32>,
    /// Sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u16,
    /// Duration in seconds
    pub duration_secs: f32,
}

impl TtsAudio {
    /// Get duration in milliseconds
    pub fn duration_ms(&self) -> u64 {
        (self.duration_secs * 1000.0) as u64
    }
}

/// TTS engine trait
#[async_trait]
pub trait TtsEngine: Send + Sync {
    /// Get engine name
    fn name(&self) -> &str;
    
    /// Check if engine is available
    fn is_ready(&self) -> bool;
    
    /// Get available voices
    fn get_voices(&self) -> Vec<VoiceInfo>;
    
    /// Synthesize text to audio
    async fn synthesize(&self, text: &str, voice: &str) -> Result<TtsAudio>;
    
    /// Get default voice for language
    fn default_voice_for_lang(&self, lang: &str) -> Option<&str>;
}

/// TTS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsConfig {
    /// Engine type
    pub engine: TtsEngineType,
    
    /// Selected voice ID
    pub voice: String,
    
    /// Speech rate (0.5 - 2.0)
    pub rate: f32,
    
    /// Pitch adjustment (-50 to 50)
    pub pitch: i32,
    
    /// Volume (0.0 - 1.0)
    pub volume: f32,
    
    /// Output sample rate
    pub sample_rate: u32,
    
    /// Enable real-time streaming
    pub streaming: bool,
    
    /// Output device name (None = default, Some = specific device like VB-Audio Cable)
    pub output_device: Option<String>,
    
    /// Custom voice settings
    pub custom_voice: Option<CustomVoiceConfig>,
}

/// Custom voice configuration for GPT-SoVITS, RVC, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomVoiceConfig {
    /// Voice model type
    pub model_type: CustomVoiceType,
    /// Path to model file
    pub model_path: PathBuf,
    /// Reference audio for voice cloning (optional)
    pub reference_audio: Option<PathBuf>,
    /// Reference text for voice cloning (optional)
    pub reference_text: Option<String>,
    /// Voice similarity threshold (0.0 - 1.0)
    pub similarity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomVoiceType {
    /// GPT-SoVITS voice cloning
    GptSoVits,
    /// RVC (Real-time Voice Conversion)
    Rvc,
    /// Piper TTS model
    Piper,
    /// VITS model
    Vits,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TtsEngineType {
    /// Microsoft Edge TTS (online, free, high quality)
    EdgeTts,
    /// Custom voice (GPT-SoVITS, RVC, etc.)
    Custom,
    /// Piper TTS (offline, fast)
    Piper,
    /// System TTS (platform native)
    System,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            engine: TtsEngineType::EdgeTts,
            voice: "zh-CN-XiaoxiaoNeural".to_string(),
            rate: 1.0,
            pitch: 0,
            volume: 1.0,
            sample_rate: 24000,
            streaming: true,
            output_device: None,
            custom_voice: None,
        }
    }
}

impl TtsConfig {
    pub fn new(voice: &str) -> Self {
        Self {
            voice: voice.to_string(),
            ..Default::default()
        }
    }
    
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = rate.clamp(0.5, 2.0);
        self
    }
    
    pub fn with_pitch(mut self, pitch: i32) -> Self {
        self.pitch = pitch.clamp(-50, 50);
        self
    }
    
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume.clamp(0.0, 1.0);
        self
    }
    
    /// Set output device (for virtual audio routing)
    pub fn with_output_device(mut self, device: Option<String>) -> Self {
        self.output_device = device;
        self
    }
    
    /// Set custom voice configuration
    pub fn with_custom_voice(mut self, config: CustomVoiceConfig) -> Self {
        self.custom_voice = Some(config);
        self.engine = TtsEngineType::Custom;
        self
    }
}

/// Default voices for common languages (Edge TTS)
pub const DEFAULT_VOICES: &[(&str, &str, &str)] = &[
    // Chinese (Simplified)
    ("zh-CN", "zh-CN-XiaoxiaoNeural", "晓晓 (女声)"),
    ("zh-CN", "zh-CN-YunxiNeural", "云希 (男声)"),
    ("zh-CN", "zh-CN-YunyangNeural", "云扬 (男声)"),
    ("zh-CN", "zh-CN-XiaoyiNeural", "晓伊 (女声)"),
    // Chinese (Traditional)
    ("zh-TW", "zh-TW-HsiaoChenNeural", "曉臻 (女聲)"),
    ("zh-TW", "zh-TW-YunJheNeural", "雲哲 (男聲)"),
    // English (US)
    ("en-US", "en-US-JennyNeural", "Jenny (Female)"),
    ("en-US", "en-US-GuyNeural", "Guy (Male)"),
    ("en-US", "en-US-AriaNeural", "Aria (Female)"),
    // English (GB)
    ("en-GB", "en-GB-SoniaNeural", "Sonia (Female)"),
    ("en-GB", "en-GB-RyanNeural", "Ryan (Male)"),
    // Japanese
    ("ja-JP", "ja-JP-NanamiNeural", "七海 (女声)"),
    ("ja-JP", "ja-JP-KeitaNeural", "圭太 (男声)"),
    // Korean
    ("ko-KR", "ko-KR-SunHiNeural", "선히 (여성)"),
    ("ko-KR", "ko-KR-InJoonNeural", "인준 (남성)"),
    // French
    ("fr-FR", "fr-FR-DeniseNeural", "Denise (Femme)"),
    ("fr-FR", "fr-FR-HenriNeural", "Henri (Homme)"),
    // German
    ("de-DE", "de-DE-KatjaNeural", "Katja"),
    ("de-DE", "de-DE-ConradNeural", "Conrad"),
    // Spanish
    ("es-ES", "es-ES-ElviraNeural", "Elvira"),
    ("es-ES", "es-ES-AlvaroNeural", "Álvaro"),
    // Russian
    ("ru-RU", "ru-RU-SvetlanaNeural", "Светлана"),
    ("ru-RU", "ru-RU-DmitryNeural", "Дмитрий"),
    // Portuguese
    ("pt-BR", "pt-BR-FranciscaNeural", "Francisca"),
    // Italian
    ("it-IT", "it-IT-ElsaNeural", "Elsa"),
    // Arabic
    ("ar-SA", "ar-SA-ZariyahNeural", "زارية"),
    // Hindi
    ("hi-IN", "hi-IN-SwaraNeural", "स्वरा"),
];

/// Get default voice for a language
pub fn get_default_voice(lang: &str) -> Option<&'static str> {
    DEFAULT_VOICES.iter()
        .find(|(l, _, _)| l == &lang)
        .map(|(_, voice, _)| *voice)
}

/// Get all voices for a language
pub fn get_voices_for_lang(lang: &str) -> Vec<VoiceInfo> {
    DEFAULT_VOICES.iter()
        .filter(|(l, _, _)| l == &lang)
        .map(|(l, voice, name)| VoiceInfo {
            id: voice.to_string(),
            name: name.to_string(),
            language: l.to_string(),
            gender: if name.contains("女") || name.contains("Female") || name.contains("Femme") { 
                "female".to_string() 
            } else if name.contains("男") || name.contains("Male") || name.contains("Homme") { 
                "male".to_string() 
            } else { 
                "neutral".to_string() 
            },
            style: None,
            is_custom: false,
            model_path: None,
        })
        .collect()
}

/// Get all available voices
pub fn get_all_voices() -> Vec<VoiceInfo> {
    DEFAULT_VOICES.iter()
        .map(|(l, voice, name)| VoiceInfo {
            id: voice.to_string(),
            name: name.to_string(),
            language: l.to_string(),
            gender: if name.contains("女") || name.contains("Female") || name.contains("Femme") { 
                "female".to_string() 
            } else if name.contains("男") || name.contains("Male") { 
                "male".to_string() 
            } else { 
                "neutral".to_string() 
            },
            style: None,
            is_custom: false,
            model_path: None,
        })
        .collect()
}

const DEFAULT_CIRCUIT_FAILURE_THRESHOLD: u32 = 2;
const DEFAULT_CIRCUIT_COOLDOWN_SECS: u64 = 600;
const DEFAULT_HALF_OPEN_PROBE_INTERVAL_SECS: u64 = 15;
const DEFAULT_SYSTEM_TTS_DISABLE_TTL_SECS: u64 = 24 * 60 * 60;

static CIRCUIT_IO_LOCK: Mutex<()> = Mutex::new(());
static CAPABILITY_IO_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TtsCircuitStage {
    #[default]
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsCircuitSnapshot {
    pub backend: String,
    pub stage: TtsCircuitStage,
    pub circuit_open: bool,
    pub circuit_remaining_sec: u64,
    pub consecutive_failures: u32,
    pub total_failures: u64,
    pub total_successes: u64,
    pub open_count: u64,
    pub last_failure_ms: Option<u64>,
    pub last_success_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TtsCircuitPolicy {
    pub failure_threshold: u32,
    pub cooldown_secs: u64,
    pub half_open_probe_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct BackendCircuitEntry {
    stage: TtsCircuitStage,
    consecutive_failures: u32,
    total_failures: u64,
    total_successes: u64,
    open_count: u64,
    disabled_until_ms: u64,
    half_open_last_probe_ms: u64,
    last_failure_ms: Option<u64>,
    last_success_ms: Option<u64>,
    last_error: Option<String>,
}

impl Default for BackendCircuitEntry {
    fn default() -> Self {
        Self {
            stage: TtsCircuitStage::Closed,
            consecutive_failures: 0,
            total_failures: 0,
            total_successes: 0,
            open_count: 0,
            disabled_until_ms: 0,
            half_open_last_probe_ms: 0,
            last_failure_ms: None,
            last_success_ms: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TtsCircuitState {
    version: u8,
    backends: HashMap<String, BackendCircuitEntry>,
    // Legacy schema compatibility (v1)
    disabled_until_ms: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SystemTtsCapability {
    checked_at_ms: u64,
    disabled_until_ms: u64,
    reason: Option<String>,
    consecutive_failures: u32,
    total_failures: u64,
    total_successes: u64,
}

impl Default for SystemTtsCapability {
    fn default() -> Self {
        Self {
            checked_at_ms: 0,
            disabled_until_ms: 0,
            reason: None,
            consecutive_failures: 0,
            total_failures: 0,
            total_successes: 0,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct TtsCapabilityState {
    version: u8,
    system_tts: SystemTtsCapability,
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn tts_circuit_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("tts-backend-circuit.json")
}

fn tts_capability_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("tts-capability.json")
}

fn read_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn tts_circuit_policy(_backend: &str) -> TtsCircuitPolicy {
    TtsCircuitPolicy {
        failure_threshold: read_env_u32(
            "LOCALTRANS_TTS_CB_FAILURE_THRESHOLD",
            DEFAULT_CIRCUIT_FAILURE_THRESHOLD,
        )
        .max(1),
        cooldown_secs: read_env_u64(
            "LOCALTRANS_TTS_CB_COOLDOWN_SECS",
            DEFAULT_CIRCUIT_COOLDOWN_SECS,
        )
        .max(1),
        half_open_probe_interval_secs: read_env_u64(
            "LOCALTRANS_TTS_CB_HALF_OPEN_PROBE_INTERVAL_SECS",
            DEFAULT_HALF_OPEN_PROBE_INTERVAL_SECS,
        )
        .max(1),
    }
}

fn system_tts_disable_ttl_secs() -> u64 {
    read_env_u64(
        "LOCALTRANS_SYSTEM_TTS_DISABLE_TTL_SECS",
        DEFAULT_SYSTEM_TTS_DISABLE_TTL_SECS,
    )
    .max(60)
}

fn load_tts_circuit_state() -> TtsCircuitState {
    let path = tts_circuit_path();
    let text = fs::read_to_string(path).ok();
    let mut state = text
        .and_then(|v| serde_json::from_str::<TtsCircuitState>(&v).ok())
        .unwrap_or_default();
    if state.version == 0 {
        state.version = 2;
    }
    // Migrate legacy schema where only disabled_until_ms was persisted.
    if !state.disabled_until_ms.is_empty() {
        for (backend, until) in state.disabled_until_ms.drain() {
            state.backends.insert(
                backend,
                BackendCircuitEntry {
                    stage: TtsCircuitStage::Open,
                    disabled_until_ms: until,
                    ..Default::default()
                },
            );
        }
    }
    state
}

fn save_tts_circuit_state(state: &TtsCircuitState) {
    let path = tts_circuit_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, text);
    }
}

fn load_tts_capability_state() -> TtsCapabilityState {
    let path = tts_capability_path();
    let text = fs::read_to_string(path).ok();
    let mut state = text
        .and_then(|v| serde_json::from_str::<TtsCapabilityState>(&v).ok())
        .unwrap_or_default();
    if state.version == 0 {
        state.version = 1;
    }
    state
}

fn save_tts_capability_state(state: &TtsCapabilityState) {
    let path = tts_capability_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, text);
    }
}

pub fn system_tts_cached_skip_reason() -> Option<String> {
    if std::env::var("LOCALTRANS_FORCE_SYSTEM_TTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return None;
    }
    let _guard = CAPABILITY_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_capability_state();
    let now = now_unix_ms();
    let cap = &mut state.system_tts;
    if cap.disabled_until_ms > now {
        return Some(cap.reason.clone().unwrap_or_else(|| "cached host capability block".to_string()));
    }
    if cap.disabled_until_ms != 0 {
        cap.disabled_until_ms = 0;
        cap.reason = None;
        save_tts_capability_state(&state);
    }
    None
}

pub fn system_tts_mark_success() {
    let _guard = CAPABILITY_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_capability_state();
    let now = now_unix_ms();
    let cap = &mut state.system_tts;
    cap.checked_at_ms = now;
    cap.disabled_until_ms = 0;
    cap.reason = None;
    cap.consecutive_failures = 0;
    cap.total_successes = cap.total_successes.saturating_add(1);
    save_tts_capability_state(&state);
}

pub fn system_tts_mark_failure(reason: &str) {
    let _guard = CAPABILITY_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_capability_state();
    let now = now_unix_ms();
    let cap = &mut state.system_tts;
    cap.checked_at_ms = now;
    cap.total_failures = cap.total_failures.saturating_add(1);
    cap.consecutive_failures = cap.consecutive_failures.saturating_add(1);
    cap.disabled_until_ms = now.saturating_add(system_tts_disable_ttl_secs().saturating_mul(1000));
    cap.reason = Some(reason.to_string());
    save_tts_capability_state(&state);
}

fn move_open_to_half_open_if_needed(entry: &mut BackendCircuitEntry, now: u64) -> bool {
    if entry.stage == TtsCircuitStage::Open && now >= entry.disabled_until_ms {
        entry.stage = TtsCircuitStage::HalfOpen;
        entry.disabled_until_ms = 0;
        entry.half_open_last_probe_ms = 0;
        return true;
    }
    false
}

fn open_circuit(entry: &mut BackendCircuitEntry, now: u64, cooldown_secs: u64, error: Option<&str>) {
    entry.stage = TtsCircuitStage::Open;
    entry.open_count = entry.open_count.saturating_add(1);
    entry.disabled_until_ms = now.saturating_add(cooldown_secs.saturating_mul(1000));
    entry.half_open_last_probe_ms = 0;
    entry.last_failure_ms = Some(now);
    if let Some(err) = error {
        entry.last_error = Some(err.to_string());
    }
}

pub fn tts_backend_allow_request(backend: &str) -> bool {
    let _guard = CIRCUIT_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_circuit_state();
    let now = now_unix_ms();
    let policy = tts_circuit_policy(backend);
    let entry = state.backends.entry(backend.to_string()).or_default();
    let mut changed = move_open_to_half_open_if_needed(entry, now);
    let allowed = match entry.stage {
        TtsCircuitStage::Closed => true,
        TtsCircuitStage::Open => false,
        TtsCircuitStage::HalfOpen => {
            let interval_ms = policy.half_open_probe_interval_secs.saturating_mul(1000);
            if now.saturating_sub(entry.half_open_last_probe_ms) >= interval_ms {
                entry.half_open_last_probe_ms = now;
                changed = true;
                true
            } else {
                false
            }
        }
    };
    if changed {
        save_tts_circuit_state(&state);
    }
    allowed
}

pub fn tts_backend_record_failure(backend: &str, error: Option<&str>) {
    let _guard = CIRCUIT_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_circuit_state();
    let now = now_unix_ms();
    let policy = tts_circuit_policy(backend);
    let entry = state.backends.entry(backend.to_string()).or_default();
    let _ = move_open_to_half_open_if_needed(entry, now);

    entry.total_failures = entry.total_failures.saturating_add(1);
    entry.last_failure_ms = Some(now);
    if let Some(err) = error {
        entry.last_error = Some(err.to_string());
    }

    match entry.stage {
        TtsCircuitStage::Closed => {
            entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
            if entry.consecutive_failures >= policy.failure_threshold {
                open_circuit(entry, now, policy.cooldown_secs, error);
            }
        }
        TtsCircuitStage::HalfOpen => {
            open_circuit(entry, now, policy.cooldown_secs, error);
        }
        TtsCircuitStage::Open => {
            // Keep existing open window; failures during open are already accounted for.
        }
    }

    save_tts_circuit_state(&state);
}

pub fn tts_backend_record_success(backend: &str) {
    let _guard = CIRCUIT_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_circuit_state();
    let now = now_unix_ms();
    let entry = state.backends.entry(backend.to_string()).or_default();
    let _ = move_open_to_half_open_if_needed(entry, now);
    entry.total_successes = entry.total_successes.saturating_add(1);
    entry.last_success_ms = Some(now);
    entry.last_error = None;
    entry.consecutive_failures = 0;
    if entry.stage != TtsCircuitStage::Closed {
        entry.stage = TtsCircuitStage::Closed;
        entry.disabled_until_ms = 0;
        entry.half_open_last_probe_ms = 0;
    }
    save_tts_circuit_state(&state);
}

pub fn tts_backend_snapshot(backend: &str) -> TtsCircuitSnapshot {
    let _guard = CIRCUIT_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_circuit_state();
    let now = now_unix_ms();
    let changed = {
        let entry = state.backends.entry(backend.to_string()).or_default();
        move_open_to_half_open_if_needed(entry, now)
    };
    if changed {
        save_tts_circuit_state(&state);
    }
    let entry = state.backends.entry(backend.to_string()).or_default().clone();

    let remaining_sec = if entry.stage == TtsCircuitStage::Open && entry.disabled_until_ms > now {
        entry.disabled_until_ms.saturating_sub(now) / 1000
    } else {
        0
    };

    TtsCircuitSnapshot {
        backend: backend.to_string(),
        stage: entry.stage,
        circuit_open: entry.stage == TtsCircuitStage::Open,
        circuit_remaining_sec: remaining_sec,
        consecutive_failures: entry.consecutive_failures,
        total_failures: entry.total_failures,
        total_successes: entry.total_successes,
        open_count: entry.open_count,
        last_failure_ms: entry.last_failure_ms,
        last_success_ms: entry.last_success_ms,
        last_error: entry.last_error.clone(),
    }
}

pub fn tts_backend_circuit_policy(backend: &str) -> TtsCircuitPolicy {
    tts_circuit_policy(backend)
}

pub fn tts_backend_is_disabled(backend: &str) -> bool {
    tts_backend_snapshot(backend).circuit_open
}

pub fn tts_backend_disabled_remaining_sec(backend: &str) -> u64 {
    tts_backend_snapshot(backend).circuit_remaining_sec
}

pub fn tts_backend_mark_failed(backend: &str, cooldown_secs: u64) {
    // Legacy behavior: mark failed immediately opens the circuit.
    let _guard = CIRCUIT_IO_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_tts_circuit_state();
    let now = now_unix_ms();
    let entry = state.backends.entry(backend.to_string()).or_default();
    entry.total_failures = entry.total_failures.saturating_add(1);
    entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
    open_circuit(entry, now, cooldown_secs.max(1), Some("legacy mark_failed"));
    save_tts_circuit_state(&state);
}

pub fn tts_backend_mark_success(backend: &str) {
    tts_backend_record_success(backend);
}
