use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRuntimeState {
    pub pid: u32,
    pub status: String,
    pub source_lang: String,
    pub target_lang: String,
    pub bidirectional: bool,
    pub tts_enabled: bool,
    pub start_unix_ms: u64,
    pub last_heartbeat_unix_ms: u64,
    pub utterance_count: u64,
    pub error_count: u64,
    pub last_error: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionControl {
    pub command: String,
    pub source_lang: Option<String>,
    pub target_lang: Option<String>,
    pub ts_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetrics {
    pub total_audio_duration_ms: u64,
    pub speech_duration_ms: u64,
    pub transcription_count: u64,
    pub translation_count: u64,
    pub average_latency_ms: f32,
    pub asr_average_latency_ms: f32,
    pub translation_average_latency_ms: f32,
    pub tts_average_latency_ms: f32,
    pub last_updated_unix_ms: u64,
}

impl Default for SessionMetrics {
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
            last_updated_unix_ms: now_unix_ms(),
        }
    }
}

pub fn localtrans_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
}

pub fn session_state_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-state.json")
}

pub fn session_control_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-control.json")
}

pub fn session_history_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-history.jsonl")
}

pub fn session_metrics_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-metrics.json")
}

pub fn write_state(state: &SessionRuntimeState) -> Result<()> {
    let path = session_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

pub fn read_state() -> Option<SessionRuntimeState> {
    let path = session_state_path();
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_control(command: &str, source_lang: Option<&str>, target_lang: Option<&str>) -> Result<()> {
    let ctl = SessionControl {
        command: command.to_string(),
        source_lang: source_lang.map(ToString::to_string),
        target_lang: target_lang.map(ToString::to_string),
        ts_unix_ms: now_unix_ms(),
    };
    let path = session_control_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(&ctl)?)?;
    Ok(())
}

pub fn read_control() -> Option<SessionControl> {
    let path = session_control_path();
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn clear_control() {
    let path = session_control_path();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub fn append_history(item: &SessionHistoryItem) -> Result<()> {
    let path = session_history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(item)?)?;
    Ok(())
}

pub fn read_history_recent(count: usize) -> Result<Vec<SessionHistoryItem>> {
    let path = session_history_path();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut out: Vec<SessionHistoryItem> = Vec::new();
    for line in text.lines().rev().take(count) {
        if let Ok(item) = serde_json::from_str::<SessionHistoryItem>(line) {
            out.push(item);
        }
    }
    Ok(out)
}

pub fn clear_history() -> Result<()> {
    let path = session_history_path();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub fn write_metrics(metrics: &SessionMetrics) -> Result<()> {
    let path = session_metrics_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(metrics)?)?;
    Ok(())
}

pub fn read_metrics() -> Option<SessionMetrics> {
    let path = session_metrics_path();
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as u64
}

pub fn is_session_alive(state: &SessionRuntimeState) -> bool {
    let now = now_unix_ms();
    now.saturating_sub(state.last_heartbeat_unix_ms) <= 5_000
        && (state.status == "starting" || state.status == "running" || state.status == "paused")
}
