use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use clap::{Parser, Subcommand};
use cpal::traits::{DeviceTrait, HostTrait};
use localtrans_lib::asr::{AsrConfig, AsrEngine, SherpaAsrEngine};
use localtrans_lib::commands::{audio as audio_cmd, model as model_cmd, tts as tts_cmd};
use localtrans_lib::audio::AudioCapture;
use localtrans_lib::tts::{EdgeTtsEngine, TtsAudio, TtsEngine};
use localtrans_lib::translation::{LociTranslator, NllbTranslator, Translator};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "localtrans-cli")]
#[command(version)]
#[command(about = "LocalTrans command line tool (works with GUI running in parallel)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Check model/runtime status
    Check,
    /// List system audio devices
    Devices,
    /// List audio devices (GUI schema)
    AudioDevices,
    /// Get persisted CLI config
    ConfigGet,
    /// Update persisted CLI config
    ConfigSet {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        bidirectional: Option<bool>,
        #[arg(long)]
        loci: Option<bool>,
        #[arg(long)]
        input_device: Option<String>,
        #[arg(long)]
        peer_input_device: Option<String>,
        #[arg(long)]
        vad_frame_ms: Option<u32>,
        #[arg(long)]
        vad_threshold: Option<f32>,
        #[arg(long)]
        tts_voice: Option<String>,
        #[arg(long)]
        translation_engine: Option<String>,
        #[arg(long)]
        tts_engine: Option<String>,
        #[arg(long)]
        tts_enabled: Option<bool>,
        #[arg(long)]
        tts_rate: Option<f32>,
        #[arg(long)]
        tts_pitch: Option<i32>,
        #[arg(long)]
        tts_volume: Option<f32>,
        #[arg(long)]
        tts_output_device: Option<String>,
        #[arg(long)]
        peer_tts_output_device: Option<String>,
        #[arg(long)]
        tts_autoplay: Option<bool>,
    },
    /// List available models under LocalTrans/models
    ModelList {
        #[arg(long)]
        model_type: Option<String>,
    },
    /// Show base models directory
    ModelsDir,
    /// Show GUI-equivalent runtime status
    RuntimeStatus,
    /// Show current log status
    LogStatus,
    /// Open model download URL (same mapping as GUI)
    ModelDownload {
        #[arg(long)]
        model_id: String,
        #[arg(long)]
        model_type: String,
    },
    /// Delete model by model id (same behavior as GUI)
    ModelDelete {
        #[arg(long)]
        model_id: String,
    },
    /// Switch model preference used by CLI
    ModelUse {
        #[arg(long)]
        asr_dir: Option<String>,
        #[arg(long)]
        loci_model: Option<String>,
        #[arg(long)]
        tts_voice: Option<String>,
    },
    /// List TTS voices
    TtsVoices {
        #[arg(long)]
        language: Option<String>,
    },
    /// List output audio devices for TTS
    TtsDevices,
    /// Show default TTS config from backend
    TtsConfig,
    /// Diagnose TTS backends (edge/sherpa/system)
    TtsHealth,
    /// Deep diagnose Windows System TTS runtime and permissions
    TtsSystemDoctor {
        /// Optional output WAV path for spoken diagnosis summary.
        #[arg(long)]
        out_wav: Option<String>,
        /// Optional output device for speaking diagnosis summary.
        #[arg(long)]
        play_device: Option<String>,
        /// Voice used for spoken diagnosis (default: sherpa local female).
        #[arg(long, default_value = "sherpa-melo-female")]
        voice: String,
    },
    /// Get backend default voice for a language
    TtsDefaultVoice {
        #[arg(long)]
        language: String,
    },
    /// Check virtual audio driver status
    TtsDriverCheck,
    /// List custom voice models from a directory
    TtsCustomVoices {
        #[arg(long)]
        models_dir: Option<String>,
    },
    /// Open URL or directory in system shell
    OpenUrl {
        #[arg(long)]
        url: String,
    },
    /// Start background realtime CLI session worker
    SessionStart {
        /// Do not auto-launch GUI app.
        #[arg(long, default_value_t = false)]
        no_gui: bool,
    },
    /// Stop background realtime CLI session worker
    SessionStop,
    /// Pause background realtime CLI session worker
    SessionPause,
    /// Resume background realtime CLI session worker
    SessionResume,
    /// Query worker status
    SessionStatus,
    /// Query session metrics (latency/stats)
    SessionStats,
    /// Read recent session history
    SessionHistory {
        #[arg(long, default_value_t = 20)]
        count: usize,
    },
    /// Clear session history
    SessionClearHistory,
    /// Update languages of running worker
    SessionUpdateLanguages {
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
    },
    #[command(hide = true)]
    AsrWorker {
        #[arg(long)]
        file: String,
        #[arg(long)]
        lang: String,
    },
    #[command(hide = true)]
    SherpaTtsWorker {
        #[arg(long)]
        text: String,
        #[arg(long, default_value_t = 1.0)]
        speed: f32,
        #[arg(long, default_value_t = 0)]
        sid: i32,
        #[arg(long)]
        out_wav: String,
    },
    #[command(hide = true)]
    LociWorker {
        #[arg(long)]
        text: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        model: String,
    },
    #[command(hide = true)]
    SessionWorker,
    /// Translate a text snippet
    Translate {
        #[arg(long)]
        text: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long, default_value_t = false)]
        loci: bool,
        #[arg(long, default_value_t = false)]
        bidirectional: bool,
        #[arg(long)]
        model: Option<String>,
    },
    /// Transcribe a wav file (16kHz mono recommended)
    AsrWav {
        #[arg(long)]
        file: String,
        #[arg(long, default_value = "zh")]
        lang: String,
    },
    /// End-to-end file test: ASR -> Translate -> TTS with latency metrics
    E2e {
        #[arg(long)]
        file: String,
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        target: Option<String>,
        #[arg(long)]
        reference: Option<String>,
        #[arg(long, default_value_t = false)]
        loci: bool,
        #[arg(long)]
        bidirectional: bool,
        #[arg(long)]
        voice: Option<String>,
        #[arg(long)]
        out_wav: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct CliConfig {
    source_lang: String,
    target_lang: String,
    bidirectional: bool,
    loci_enhanced: bool,
    input_device: Option<String>,
    peer_input_device: Option<String>,
    vad_frame_ms: u32,
    vad_threshold: f32,
    preferred_asr_dir: Option<String>,
    preferred_loci_model: Option<String>,
    preferred_tts_voice: String,
    translation_engine: String,
    tts_engine: String,
    tts_enabled: bool,
    tts_rate: f32,
    tts_pitch: i32,
    tts_volume: f32,
    tts_output_device: Option<String>,
    peer_tts_output_device: Option<String>,
    tts_autoplay: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            source_lang: "zh".to_string(),
            target_lang: "en".to_string(),
            bidirectional: false,
            loci_enhanced: false,
            input_device: None,
            peer_input_device: None,
            vad_frame_ms: 30,
            vad_threshold: 0.01,
            preferred_asr_dir: None,
            preferred_loci_model: None,
            preferred_tts_voice: "en-US-JennyNeural".to_string(),
            translation_engine: "loci".to_string(),
            tts_engine: "edge-tts".to_string(),
            tts_enabled: true,
            tts_rate: 1.0,
            tts_pitch: 0,
            tts_volume: 1.0,
            tts_output_device: None,
            peer_tts_output_device: None,
            tts_autoplay: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct RuntimeCheck {
    models_dir: String,
    asr_selected_dir: String,
    asr_ready: bool,
    loci_ready: bool,
    loci_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRuntimeState {
    pid: u32,
    status: String,
    source_lang: String,
    target_lang: String,
    bidirectional: bool,
    tts_enabled: bool,
    start_unix_ms: u64,
    last_heartbeat_unix_ms: u64,
    utterance_count: u64,
    error_count: u64,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionHistoryItem {
    id: String,
    source_text: String,
    translated_text: String,
    source_lang: String,
    target_lang: String,
    timestamp: String,
    confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionControl {
    command: String,
    source_lang: Option<String>,
    target_lang: Option<String>,
    ts_unix_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct UtteranceTiming {
    asr_ms: u128,
    translation_ms: u128,
    tts_ms: u128,
}

#[derive(Debug, Serialize, Deserialize)]
struct AsrWorkerOutput {
    text: String,
    confidence: f32,
    asr_ms: u128,
}

fn main() -> Result<()> {
    localtrans_lib::logging::init_logging_with_stderr(false);
    let cli = Cli::parse();
    tracing::info!(args = ?std::env::args().collect::<Vec<_>>(), "localtrans-cli command start");
    let mut config = load_cli_config().unwrap_or_default();
    match cli.command {
        Commands::Check => cmd_check(),
        Commands::Devices => cmd_devices(),
        Commands::AudioDevices => cmd_audio_devices(),
        Commands::ConfigGet => cmd_config_get(&config),
        Commands::ConfigSet {
            source,
            target,
            bidirectional,
            loci,
            input_device,
            peer_input_device,
            vad_frame_ms,
            vad_threshold,
            tts_voice,
            translation_engine,
            tts_engine,
            tts_enabled,
            tts_rate,
            tts_pitch,
            tts_volume,
            tts_output_device,
            peer_tts_output_device,
            tts_autoplay,
        } => cmd_config_set(
            &mut config,
            source,
            target,
            bidirectional,
            loci,
            input_device,
            peer_input_device,
            vad_frame_ms,
            vad_threshold,
            tts_voice,
            translation_engine,
            tts_engine,
            tts_enabled,
            tts_rate,
            tts_pitch,
            tts_volume,
            tts_output_device,
            peer_tts_output_device,
            tts_autoplay,
        ),
        Commands::ModelList { model_type } => cmd_model_list(model_type.as_deref()),
        Commands::ModelsDir => cmd_models_dir(),
        Commands::RuntimeStatus => cmd_runtime_status(),
        Commands::LogStatus => cmd_log_status(),
        Commands::ModelDownload {
            model_id,
            model_type,
        } => cmd_model_download(&model_id, &model_type),
        Commands::ModelDelete { model_id } => cmd_model_delete(&model_id),
        Commands::ModelUse {
            asr_dir,
            loci_model,
            tts_voice,
        } => cmd_model_use(&mut config, asr_dir, loci_model, tts_voice),
        Commands::TtsVoices { language } => cmd_tts_voices(language.as_deref()),
        Commands::TtsDevices => cmd_tts_devices(),
        Commands::TtsConfig => cmd_tts_config(),
        Commands::TtsHealth => cmd_tts_health(),
        Commands::TtsSystemDoctor {
            out_wav,
            play_device,
            voice,
        } => {
            cmd_tts_system_doctor(out_wav.as_deref(), play_device.as_deref(), &voice)
        }
        Commands::TtsDefaultVoice { language } => cmd_tts_default_voice(&language),
        Commands::TtsDriverCheck => cmd_tts_driver_check(),
        Commands::TtsCustomVoices { models_dir } => cmd_tts_custom_voices(models_dir.as_deref()),
        Commands::OpenUrl { url } => cmd_open_url(&url),
        Commands::SessionStart { no_gui } => cmd_session_start(&config, !no_gui),
        Commands::SessionStop => cmd_session_stop(),
        Commands::SessionPause => cmd_session_pause(),
        Commands::SessionResume => cmd_session_resume(),
        Commands::SessionStatus => cmd_session_status(),
        Commands::SessionStats => cmd_session_stats(),
        Commands::SessionHistory { count } => cmd_session_history(count),
        Commands::SessionClearHistory => cmd_session_clear_history(),
        Commands::SessionUpdateLanguages { source, target } => {
            cmd_session_update_languages(&source, &target)
        }
        Commands::AsrWorker { file, lang } => cmd_asr_worker(&file, &lang),
        Commands::SherpaTtsWorker {
            text,
            speed,
            sid,
            out_wav,
        } => cmd_sherpa_tts_worker(&text, speed, sid, &out_wav),
        Commands::LociWorker {
            text,
            source,
            target,
            model,
        } => cmd_loci_worker(&text, &source, &target, &model),
        Commands::SessionWorker => cmd_session_worker_entry(),
        Commands::Translate {
            text,
            source,
            target,
            loci,
            bidirectional,
            model,
        } => cmd_translate(
            &config,
            &text,
            source.as_deref(),
            target.as_deref(),
            loci,
            bidirectional,
            model.as_deref(),
        ),
        Commands::AsrWav { file, lang } => cmd_asr_wav(&file, &lang),
        Commands::E2e {
            file,
            source,
            target,
            reference,
            loci,
            bidirectional,
            voice,
            out_wav,
        } => cmd_e2e(
            &config,
            &file,
            source.as_deref(),
            target.as_deref(),
            reference.as_deref(),
            loci,
            bidirectional,
            voice.as_deref(),
            out_wav.as_deref(),
        ),
    }
}

fn cmd_check() -> Result<()> {
    let models_dir = default_models_dir();
    let asr_root = models_dir.join("asr");
    let asr_selected = pick_preferred_asr_model_dir(&asr_root, "zh");
    let asr_ready = is_valid_asr_model_dir(&asr_selected);

    let loci_model = find_default_loci_model();
    let loci_ready = loci_model.is_some();

    let result = RuntimeCheck {
        models_dir: models_dir.display().to_string(),
        asr_selected_dir: asr_selected.display().to_string(),
        asr_ready,
        loci_ready,
        loci_model: loci_model.map(|p| p.display().to_string()),
    };

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn cmd_config_get(config: &CliConfig) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(config)?);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_config_set(
    config: &mut CliConfig,
    source: Option<String>,
    target: Option<String>,
    bidirectional: Option<bool>,
    loci: Option<bool>,
    input_device: Option<String>,
    peer_input_device: Option<String>,
    vad_frame_ms: Option<u32>,
    vad_threshold: Option<f32>,
    tts_voice: Option<String>,
    translation_engine: Option<String>,
    tts_engine: Option<String>,
    tts_enabled: Option<bool>,
    tts_rate: Option<f32>,
    tts_pitch: Option<i32>,
    tts_volume: Option<f32>,
    tts_output_device: Option<String>,
    peer_tts_output_device: Option<String>,
    tts_autoplay: Option<bool>,
) -> Result<()> {
    if let Some(v) = source {
        config.source_lang = v;
    }
    if let Some(v) = target {
        config.target_lang = v;
    }
    if let Some(v) = bidirectional {
        config.bidirectional = v;
    }
    if let Some(v) = loci {
        config.loci_enhanced = v;
    }
    if let Some(v) = input_device {
        config.input_device = Some(v);
    }
    if let Some(v) = peer_input_device {
        config.peer_input_device = Some(v);
    }
    if let Some(v) = vad_frame_ms {
        config.vad_frame_ms = v.clamp(10, 200);
    }
    if let Some(v) = vad_threshold {
        config.vad_threshold = v.clamp(0.0, 1.0);
    }
    if let Some(v) = tts_voice {
        config.preferred_tts_voice = v;
    }
    if let Some(v) = translation_engine {
        config.translation_engine = v;
    }
    if let Some(v) = tts_engine {
        config.tts_engine = v;
    }
    if let Some(v) = tts_enabled {
        config.tts_enabled = v;
    }
    if let Some(v) = tts_rate {
        config.tts_rate = v.clamp(0.5, 2.0);
    }
    if let Some(v) = tts_pitch {
        config.tts_pitch = v.clamp(-50, 50);
    }
    if let Some(v) = tts_volume {
        config.tts_volume = v.clamp(0.0, 1.0);
    }
    if let Some(v) = tts_output_device {
        config.tts_output_device = Some(v);
    }
    if let Some(v) = peer_tts_output_device {
        config.peer_tts_output_device = Some(v);
    }
    if let Some(v) = tts_autoplay {
        config.tts_autoplay = v;
    }
    save_cli_config(config)?;
    cmd_config_get(config)
}

fn cmd_model_list(model_type: Option<&str>) -> Result<()> {
    let models = model_cmd::list_models(model_type.map(ToString::to_string)).map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&models)?);
    Ok(())
}

fn cmd_models_dir() -> Result<()> {
    let dir = model_cmd::get_models_dir().map_err(|e| anyhow!(e))?;
    println!("{}", dir);
    Ok(())
}

fn cmd_runtime_status() -> Result<()> {
    let status = model_cmd::get_runtime_status().map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

fn cmd_log_status() -> Result<()> {
    let status = model_cmd::get_log_status().map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&status)?);
    Ok(())
}

fn cmd_model_download(model_id: &str, model_type: &str) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt
        .block_on(model_cmd::download_model(
            model_id.to_string(),
            model_type.to_string(),
        ))
        .map_err(|e| anyhow!(e))?;
    println!("{}", result);
    Ok(())
}

fn cmd_model_delete(model_id: &str) -> Result<()> {
    model_cmd::delete_model(model_id.to_string()).map_err(|e| anyhow!(e))?;
    println!("deleted: {}", model_id);
    Ok(())
}

fn cmd_model_use(
    config: &mut CliConfig,
    asr_dir: Option<String>,
    loci_model: Option<String>,
    tts_voice: Option<String>,
) -> Result<()> {
    if let Some(v) = asr_dir {
        config.preferred_asr_dir = Some(v);
    }
    if let Some(v) = loci_model {
        config.preferred_loci_model = Some(v);
    }
    if let Some(v) = tts_voice {
        config.preferred_tts_voice = v;
    }
    save_cli_config(config)?;
    cmd_config_get(config)
}

fn cmd_devices() -> Result<()> {
    let host = cpal::default_host();
    let default_input = host.default_input_device().and_then(|d| d.name().ok());
    let default_output = host.default_output_device().and_then(|d| d.name().ok());

    println!("Input devices:");
    for d in host.input_devices()? {
        let name = d.name().unwrap_or_else(|_| "<unknown>".to_string());
        let marker = if default_input.as_ref() == Some(&name) {
            " (default)"
        } else {
            ""
        };
        println!("- {}{}", name, marker);
    }

    println!("Output devices:");
    for d in host.output_devices()? {
        let name = d.name().unwrap_or_else(|_| "<unknown>".to_string());
        let marker = if default_output.as_ref() == Some(&name) {
            " (default)"
        } else {
            ""
        };
        println!("- {}{}", name, marker);
    }

    Ok(())
}

fn cmd_audio_devices() -> Result<()> {
    let devices = audio_cmd::get_audio_devices().map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&devices)?);
    Ok(())
}

fn cmd_tts_voices(language: Option<&str>) -> Result<()> {
    let voices = tts_cmd::get_tts_voices(language.map(ToString::to_string)).map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&voices)?);
    Ok(())
}

fn cmd_tts_devices() -> Result<()> {
    let devices = tts_cmd::get_tts_output_devices().map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&devices)?);
    Ok(())
}

fn cmd_tts_config() -> Result<()> {
    let cfg = tts_cmd::get_tts_config().map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&cfg)?);
    Ok(())
}

#[derive(Serialize)]
struct TtsHealthItem {
    ok: bool,
    detail: String,
    stage: String,
    circuit_open: bool,
    circuit_remaining_sec: u64,
    consecutive_failures: u32,
    total_failures: u64,
    total_successes: u64,
    open_count: u64,
    last_error: Option<String>,
    failure_threshold: u32,
    cooldown_secs: u64,
    half_open_probe_interval_secs: u64,
}

#[derive(Serialize)]
struct TtsHealthReport {
    edge: TtsHealthItem,
    sherpa: TtsHealthItem,
    system: TtsHealthItem,
}

#[derive(Serialize)]
struct TtsDoctorCheck {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Serialize)]
struct TtsSystemDoctorReport {
    platform: String,
    overall_ok: bool,
    checks: Vec<TtsDoctorCheck>,
    recommendations: Vec<String>,
    reason_text: Option<String>,
    reason_audio_path: Option<String>,
    reason_audio_engine: Option<String>,
    reason_audio_error: Option<String>,
    reason_play_device: Option<String>,
    reason_play_error: Option<String>,
}

fn build_tts_health_item(ok: bool, detail: String, backend: &str) -> TtsHealthItem {
    let snap = localtrans_lib::tts::tts_backend_snapshot(backend);
    let policy = localtrans_lib::tts::tts_backend_circuit_policy(backend);
    TtsHealthItem {
        ok,
        detail,
        stage: format!("{:?}", snap.stage).to_ascii_lowercase(),
        circuit_open: snap.circuit_open,
        circuit_remaining_sec: snap.circuit_remaining_sec,
        consecutive_failures: snap.consecutive_failures,
        total_failures: snap.total_failures,
        total_successes: snap.total_successes,
        open_count: snap.open_count,
        last_error: snap.last_error,
        failure_threshold: policy.failure_threshold,
        cooldown_secs: policy.cooldown_secs,
        half_open_probe_interval_secs: policy.half_open_probe_interval_secs,
    }
}

fn cmd_tts_health() -> Result<()> {
    let edge = {
        let rt = tokio::runtime::Runtime::new()?;
        let result = rt.block_on(async {
            let engine = EdgeTtsEngine::new()?;
            tokio::time::timeout(
                Duration::from_secs(8),
                engine.synthesize("localtrans tts health check", "en-US-JennyNeural"),
            )
            .await
            .map_err(|_| anyhow!("timeout"))?
            .map(|a| a.samples.len())
        });
        match result {
            Ok(n) => build_tts_health_item(true, format!("ok, samples={}", n), "edge"),
            Err(e) => build_tts_health_item(false, summarize_text(&e.to_string(), 300), "edge"),
        }
    };

    let sherpa = match synthesize_sherpa_melo_tts_cli("健康检查", "sherpa-melo-female", 1.0) {
        Ok(a) => build_tts_health_item(
            true,
            format!("ok, sample_rate={}, samples={}", a.sample_rate, a.samples.len()),
            "sherpa",
        ),
        Err(e) => build_tts_health_item(false, summarize_text(&e.to_string(), 300), "sherpa"),
    };

    let system = match synthesize_system_tts_audio_cli("health check", 1.0) {
        Ok(a) => build_tts_health_item(
            true,
            format!("ok, sample_rate={}, samples={}", a.sample_rate, a.samples.len()),
            "system",
        ),
        Err(e) => build_tts_health_item(false, summarize_text(&e.to_string(), 300), "system"),
    };

    let report = TtsHealthReport {
        edge,
        sherpa,
        system,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn cmd_tts_system_doctor(
    out_wav: Option<&str>,
    play_device: Option<&str>,
    voice: &str,
) -> Result<()> {
    #[cfg(not(target_os = "windows"))]
    {
        let report = TtsSystemDoctorReport {
            platform: std::env::consts::OS.to_string(),
            overall_ok: false,
            checks: vec![TtsDoctorCheck {
                name: "platform".to_string(),
                ok: false,
                detail: "system TTS doctor is only implemented for Windows".to_string(),
            }],
            recommendations: vec!["Use sherpa-melo or edge-tts on non-Windows platforms.".to_string()],
            reason_text: None,
            reason_audio_path: None,
            reason_audio_engine: None,
            reason_audio_error: None,
            reason_play_device: None,
            reason_play_error: None,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        let mut checks: Vec<TtsDoctorCheck> = Vec::new();
        let mut recs: Vec<String> = Vec::new();
        let mut overall_ok = true;

        let out_dir = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("LocalTrans")
            .join("tts-tmp");

        let fs_check = (|| -> Result<String> {
            fs::create_dir_all(&out_dir)?;
            let p = out_dir.join(format!("doctor_fs_probe_{}_{}.tmp", now_unix_ms(), std::process::id()));
            fs::write(&p, b"ok")?;
            fs::remove_file(&p)?;
            Ok(format!("writable: {}", out_dir.display()))
        })();
        push_doctor_check(&mut checks, "tts_tmp_dir_write", fs_check, &mut overall_ok);

        let sys_init = run_powershell_script(
            "$ErrorActionPreference='Stop'; \
             Add-Type -AssemblyName System.Speech | Out-Null; \
             $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             if ($null -eq $s) { throw 'SpeechSynthesizer is null' }; \
             $cnt=$s.GetInstalledVoices().Count; \
             $s.Dispose(); \
             Write-Output ('voices=' + $cnt)",
        );
        push_doctor_check(&mut checks, "system_speech_init", sys_init, &mut overall_ok);

        let wav_sys = out_dir.join(format!("doctor_system_speech_{}_{}.wav", now_unix_ms(), std::process::id()));
        let wav_sys_s = wav_sys.to_string_lossy().replace('\'', "''");
        let sys_wav = run_powershell_script(&format!(
            "$ErrorActionPreference='Stop'; \
             Add-Type -AssemblyName System.Speech | Out-Null; \
             $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; \
             if ($null -eq $s) {{ throw 'SpeechSynthesizer is null' }}; \
             $s.SetOutputToWaveFile('{}'); \
             $s.Speak('localtrans system speech doctor'); \
             $s.Dispose(); \
             if (-not (Test-Path '{}')) {{ throw 'wav not created' }}; \
             Write-Output 'wav_created'",
            wav_sys_s, wav_sys_s
        ));
        push_doctor_check(&mut checks, "system_speech_wav", sys_wav, &mut overall_ok);
        let _ = fs::remove_file(&wav_sys);

        let sapi_direct = run_powershell_script(
            "$ErrorActionPreference='Stop'; \
             $v=New-Object -ComObject SAPI.SpVoice; \
             if ($null -eq $v) { throw 'SpVoice is null' }; \
             [void]$v.Speak('localtrans sapi direct doctor'); \
             Write-Output 'spoken'",
        );
        push_doctor_check(&mut checks, "sapi_direct_speak", sapi_direct, &mut overall_ok);

        let wav_sapi = out_dir.join(format!("doctor_sapi_{}_{}.wav", now_unix_ms(), std::process::id()));
        let wav_sapi_s = wav_sapi.to_string_lossy().replace('\'', "''");
        let sapi_wav = run_powershell_script(&format!(
            "$ErrorActionPreference='Stop'; \
             $v=New-Object -ComObject SAPI.SpVoice; \
             $f=New-Object -ComObject SAPI.SpFileStream; \
             if ($null -eq $v -or $null -eq $f) {{ throw 'SAPI object null' }}; \
             $f.Open('{}', 3, $false); \
             $v.AudioOutputStream=$f; \
             [void]$v.Speak('localtrans sapi wav doctor'); \
             $f.Close(); \
             if (-not (Test-Path '{}')) {{ throw 'wav not created' }}; \
             Write-Output 'wav_created'",
            wav_sapi_s, wav_sapi_s
        ));
        push_doctor_check(&mut checks, "sapi_wav_output", sapi_wav, &mut overall_ok);
        let _ = fs::remove_file(&wav_sapi);

        let any_access_denied = checks.iter().any(|c| {
            let d = c.detail.to_ascii_lowercase();
            d.contains("0x80070005") || d.contains("access is denied")
        });
        let any_null_ref = checks.iter().any(|c| {
            let d = c.detail.to_ascii_lowercase();
            d.contains("object reference not set")
        });
        if any_access_denied {
            recs.push(
                "Detected E_ACCESSDENIED from Windows speech runtime. Run terminal/app in an interactive user session (not service/session 0).".to_string(),
            );
            recs.push(
                "Check Group Policy / endpoint security controls that block SAPI or COM speech objects.".to_string(),
            );
        }
        if any_null_ref {
            recs.push(
                "Detected NullReference in Windows speech APIs. Reinstall Windows voice packs and Speech runtime, then reboot.".to_string(),
            );
        }
        if checks.iter().any(|c| c.name == "tts_tmp_dir_write" && !c.ok) {
            recs.push("Grant write permission to %LOCALAPPDATA%\\LocalTrans\\tts-tmp.".to_string());
        }
        if !checks.iter().any(|c| c.name == "system_speech_init" && c.ok) {
            recs.push("Install/repair Windows speech components and at least one system voice package.".to_string());
        }
        if recs.is_empty() {
            recs.push("No obvious environment blocker detected by doctor.".to_string());
        }

        if overall_ok {
            localtrans_lib::tts::system_tts_mark_success();
        } else {
            let summary = checks
                .iter()
                .filter(|c| !c.ok)
                .take(2)
                .map(|c| format!("{}: {}", c.name, summarize_text(&c.detail, 100)))
                .collect::<Vec<_>>()
                .join(" | ");
            localtrans_lib::tts::system_tts_mark_failure(&format!(
                "system-doctor failed: {}",
                summary
            ));
        }

        let need_reason_audio = out_wav.is_some() || play_device.is_some();
        let reason_text = if need_reason_audio {
            Some(build_tts_doctor_reason_text(overall_ok, &checks, &recs))
        } else {
            None
        };
        let mut reason_audio_path: Option<String> = None;
        let mut reason_audio_engine: Option<String> = None;
        let mut reason_audio_error: Option<String> = None;
        let mut reason_play_device: Option<String> = None;
        let mut reason_play_error: Option<String> = None;
        if let Some(text) = reason_text.as_deref() {
            match synthesize_reason_audio(text, voice) {
                Ok((audio, engine)) => {
                    reason_audio_engine = Some(engine);

                    if let Some(path) = out_wav {
                        let out_path = PathBuf::from(path);
                        if let Some(parent) = out_path.parent() {
                            if !parent.as_os_str().is_empty() {
                                let _ = fs::create_dir_all(parent);
                            }
                        }
                        if let Err(e) = write_wav_f32(&out_path, &audio.samples, audio.sample_rate) {
                            reason_audio_error = Some(format!(
                                "failed to write wav: {}",
                                summarize_text(&e.to_string(), 180)
                            ));
                        } else {
                            reason_audio_path = Some(out_path.to_string_lossy().to_string());
                        }
                    }

                    if let Some(device) = play_device {
                        reason_play_device = Some(device.to_string());
                        match localtrans_lib::tts::playback::AudioPlayer::new(Some(device)) {
                            Ok(mut player) => {
                                player.set_volume(1.0);
                                if let Err(e) = player.play(&audio) {
                                    reason_play_error = Some(format!(
                                        "failed to play on device {}: {}",
                                        device,
                                        summarize_text(&e.to_string(), 180)
                                    ));
                                }
                            }
                            Err(e) => {
                                reason_play_error = Some(format!(
                                    "failed to create audio player for {}: {}",
                                    device,
                                    summarize_text(&e.to_string(), 180)
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    reason_audio_error = Some(format!(
                        "failed to synthesize reason audio: {}",
                        summarize_text(&e.to_string(), 180)
                    ));
                }
            }
        }

        let report = TtsSystemDoctorReport {
            platform: "windows".to_string(),
            overall_ok,
            checks,
            recommendations: recs,
            reason_text,
            reason_audio_path,
            reason_audio_engine,
            reason_audio_error,
            reason_play_device,
            reason_play_error,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
}

fn push_doctor_check(
    checks: &mut Vec<TtsDoctorCheck>,
    name: &str,
    result: Result<String>,
    overall_ok: &mut bool,
) {
    match result {
        Ok(detail) => checks.push(TtsDoctorCheck {
            name: name.to_string(),
            ok: true,
            detail,
        }),
        Err(e) => {
            *overall_ok = false;
            checks.push(TtsDoctorCheck {
                name: name.to_string(),
                ok: false,
                detail: summarize_text(&e.to_string(), 360),
            });
        }
    }
}

fn build_tts_doctor_reason_text(
    overall_ok: bool,
    checks: &[TtsDoctorCheck],
    recommendations: &[String],
) -> String {
    if overall_ok {
        return "System TTS doctor result: all checks passed. Windows system speech runtime appears healthy.".to_string();
    }

    let failed_checks = checks
        .iter()
        .filter(|c| !c.ok)
        .take(3)
        .map(|c| format!("{}: {}", c.name, summarize_text(&c.detail, 96)))
        .collect::<Vec<_>>()
        .join("; ");

    let rec = recommendations
        .iter()
        .take(2)
        .map(|s| summarize_text(s, 100))
        .collect::<Vec<_>>()
        .join("; ");

    format!(
        "System TTS doctor detected host issues. Failed checks: {}. Recommended actions: {}.",
        failed_checks, rec
    )
}

fn synthesize_reason_audio(text: &str, voice: &str) -> Result<(TtsAudio, String)> {
    match synthesize_sherpa_melo_tts_cli(text, voice, 1.0) {
        Ok(audio) => Ok((audio, "sherpa-melo".to_string())),
        Err(sherpa_err) => {
            let rt = tokio::runtime::Runtime::new()?;
            match rt.block_on(async { synthesize_tts_with_fallback(text, voice, "system").await }) {
                Ok((audio, engine)) => Ok((audio, engine)),
                Err(fallback_err) => Err(anyhow!(
                    "sherpa failed: {}; fallback failed: {}",
                    summarize_text(&sherpa_err.to_string(), 120),
                    summarize_text(&fallback_err.to_string(), 120)
                )),
            }
        }
    }
}

fn windows_powershell_candidates() -> &'static [&'static str] {
    &[
        "powershell.exe",
        "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    ]
}

fn run_powershell_script(script: &str) -> Result<String> {
    run_powershell_script_with_mode(script, false, "powershell.exe")
}

fn run_powershell_script_with_mode(script: &str, sta: bool, exe: &str) -> Result<String> {
    let wrapped = format!(
        "$ProgressPreference='SilentlyContinue'; \
         $ErrorActionPreference='Stop'; \
         try {{ {} }} catch {{ \
           $msg = $_.Exception.Message; \
           if ($null -ne $_.Exception.InnerException) {{ $msg = $msg + ' | inner: ' + $_.Exception.InnerException.Message }}; \
           Write-Output ('__LTERR__ ' + $msg); \
           exit 87; \
         }}",
        script
    );
    let mut script_utf16le: Vec<u8> = Vec::with_capacity(wrapped.len() * 2);
    for u in wrapped.encode_utf16() {
        script_utf16le.extend_from_slice(&u.to_le_bytes());
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(script_utf16le);
    let mut cmd = Command::new(exe);
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-OutputFormat",
        "Text",
    ]);
    if sta {
        cmd.arg("-Sta");
    }
    let output = cmd
        .args(["-EncodedCommand", &encoded])
        .output()
        .context("failed to run powershell probe")?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if let Some(idx) = stdout.find("__LTERR__ ") {
        let detail = stdout[idx + "__LTERR__ ".len()..].trim();
        return Err(anyhow!("{}", detail));
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "powershell probe failed (status {:?}): {} {}",
            output.status.code(),
            summarize_text(&stderr, 220),
            summarize_text(&stdout, 180)
        ));
    }
    Ok(summarize_text(&stdout, 220))
}

fn cmd_tts_default_voice(language: &str) -> Result<()> {
    let voice = tts_cmd::get_default_tts_voice(language.to_string()).map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&voice)?);
    Ok(())
}

fn cmd_tts_driver_check() -> Result<()> {
    let info = tts_cmd::check_virtual_audio_driver().map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}

fn cmd_tts_custom_voices(models_dir: Option<&str>) -> Result<()> {
    let models_dir = models_dir
        .map(ToString::to_string)
        .unwrap_or_else(|| default_models_dir().join("tts").to_string_lossy().to_string());
    let voices = tts_cmd::list_custom_voice_models(models_dir).map_err(|e| anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&voices)?);
    Ok(())
}

fn cmd_open_url(url: &str) -> Result<()> {
    tts_cmd::open_url(url.to_string()).map_err(|e| anyhow!(e))?;
    println!("opened: {}", url);
    Ok(())
}

fn maybe_launch_gui_near_cli() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let gui = exe.with_file_name("localtrans.exe");
    if !gui.exists() {
        return;
    }
    let workdir = gui.parent().unwrap_or_else(|| std::path::Path::new("."));
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "Start-Process -FilePath '{}' -WorkingDirectory '{}'",
                    gui.to_string_lossy().replace('\'', "''"),
                    workdir.to_string_lossy().replace('\'', "''")
                ),
            ])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new(gui).current_dir(workdir).spawn();
    }
}

fn cmd_session_start(config: &CliConfig, with_gui: bool) -> Result<()> {
    if with_gui {
        maybe_launch_gui_near_cli();
    }
    if let Some(state) = read_session_state() {
        if is_session_alive(&state) && (state.status == "running" || state.status == "paused") {
            println!("{}", serde_json::to_string_pretty(&state)?);
            return Err(anyhow!("session worker is already running"));
        }
    }
    clear_session_control();

    let exe = std::env::current_exe().context("failed to locate current executable")?;
    #[cfg(target_os = "windows")]
    {
        let exe_s = exe.to_string_lossy().to_string();
        let workdir = exe
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let ps = format!(
            "Start-Process -FilePath '{}' -ArgumentList 'session-worker' -WorkingDirectory '{}' -WindowStyle Hidden",
            exe_s.replace('\'', "''"),
            workdir.replace('\'', "''")
        );
        Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .spawn()
            .context("failed to spawn session worker")?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new(exe)
            .arg("session-worker")
            .spawn()
            .context("failed to spawn session worker")?;
    }

    let start_wait = Instant::now();
    while start_wait.elapsed() < Duration::from_secs(15) {
        if let Some(state) = read_session_state() {
            if is_session_alive(&state)
                && (state.status == "starting"
                    || state.status == "running"
                    || state.status == "paused")
            {
                println!("{}", serde_json::to_string_pretty(&state)?);
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    tracing::error!("session-start timeout: worker did not become alive within 15s");

    let cfg = serde_json::to_string_pretty(config)?;
    Err(anyhow!(
        "session worker did not report ready in time. current cli config: {}",
        cfg
    ))
}

fn cmd_session_stop() -> Result<()> {
    write_session_control("stop", None, None)?;
    println!("stop command sent");
    Ok(())
}

fn cmd_session_pause() -> Result<()> {
    write_session_control("pause", None, None)?;
    println!("pause command sent");
    Ok(())
}

fn cmd_session_resume() -> Result<()> {
    write_session_control("resume", None, None)?;
    println!("resume command sent");
    Ok(())
}

fn cmd_session_status() -> Result<()> {
    if let Some(state) = read_session_state() {
        if is_session_alive(&state) {
            println!("{}", serde_json::to_string_pretty(&state)?);
        } else {
            println!(
                "{}",
                serde_json::json!({
                    "status": "idle",
                    "lastKnown": state,
                    "note": "session worker heartbeat expired"
                })
            );
        }
    } else {
        println!("{}", serde_json::json!({"status": "idle"}));
    }
    Ok(())
}

fn cmd_session_stats() -> Result<()> {
    if let Some(metrics) = localtrans_lib::session_bus::read_metrics() {
        println!("{}", serde_json::to_string_pretty(&metrics)?);
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&localtrans_lib::session_bus::SessionMetrics::default())?
        );
    }
    Ok(())
}

fn cmd_session_history(count: usize) -> Result<()> {
    let path = session_history_path();
    if !path.exists() {
        println!("[]");
        return Ok(());
    }
    let text = fs::read_to_string(&path)?;
    let mut out: Vec<SessionHistoryItem> = Vec::new();
    for line in text.lines().rev().take(count) {
        if let Ok(item) = serde_json::from_str::<SessionHistoryItem>(line) {
            out.push(item);
        }
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn cmd_session_clear_history() -> Result<()> {
    let path = session_history_path();
    if path.exists() {
        fs::remove_file(&path)?;
    }
    println!("session history cleared");
    Ok(())
}

fn cmd_session_update_languages(source: &str, target: &str) -> Result<()> {
    write_session_control("update_languages", Some(source), Some(target))?;
    println!("update-languages command sent: {} -> {}", source, target);
    Ok(())
}

fn cmd_session_worker_entry() -> Result<()> {
    let result = cmd_session_worker();
    if let Err(ref e) = result {
        let cfg = load_cli_config().unwrap_or_default();
        let fail_state = SessionRuntimeState {
            pid: std::process::id(),
            status: "error".to_string(),
            source_lang: cfg.source_lang,
            target_lang: cfg.target_lang,
            bidirectional: cfg.bidirectional,
            tts_enabled: cfg.tts_enabled,
            start_unix_ms: now_unix_ms(),
            last_heartbeat_unix_ms: now_unix_ms(),
            utterance_count: 0,
            error_count: 1,
            last_error: Some(e.to_string()),
        };
        let _ = write_session_state(&fail_state);
    }
    result
}

fn cmd_asr_worker(file: &str, lang: &str) -> Result<()> {
    let file_path = PathBuf::from(file);
    let samples = read_wav_to_f32(&file_path)?;
    let asr_root = default_models_dir().join("asr");
    let selected = pick_preferred_asr_model_dir(&asr_root, lang);

    let config = AsrConfig {
        model_path: selected.clone(),
        language: Some(lang.to_string()),
        ..Default::default()
    };

    let mut asr = SherpaAsrEngine::init(config)
        .with_context(|| format!("Failed to init ASR model at {}", selected.display()))?;
    let t_asr = Instant::now();
    let result = asr.transcribe(&samples, 16000)?;
    let out = AsrWorkerOutput {
        text: result.text,
        confidence: result.confidence,
        asr_ms: t_asr.elapsed().as_millis(),
    };
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

fn cmd_sherpa_tts_worker(text: &str, speed: f32, sid: i32, out_wav: &str) -> Result<()> {
    let audio = synthesize_sherpa_melo_tts_inproc(text, sid, speed)?;
    let out_path = PathBuf::from(out_wav);
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_wav_f32(&out_path, &audio.samples, audio.sample_rate)?;
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "ok": true,
            "outWav": out_path.to_string_lossy().to_string(),
            "sampleRate": audio.sample_rate
        }))?
    );
    Ok(())
}

fn cmd_loci_worker(text: &str, source: &str, target: &str, model: &str) -> Result<()> {
    let model_path = PathBuf::from(model);
    let mut t = LociTranslator::init(&model_path)
        .with_context(|| format!("Failed to init Loci model: {}", model_path.display()))?;
    let result = t.translate(text, source, target)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

fn cmd_session_worker() -> Result<()> {
    let mut config = load_cli_config().unwrap_or_default();
    tracing::info!(
        source = %config.source_lang,
        target = %config.target_lang,
        bidirectional = config.bidirectional,
        tts_enabled = config.tts_enabled,
        "session-worker boot"
    );
    let mut metrics = localtrans_lib::session_bus::SessionMetrics::default();
    let _ = localtrans_lib::session_bus::write_metrics(&metrics);
    let mut state = SessionRuntimeState {
        pid: std::process::id(),
        status: "starting".to_string(),
        source_lang: config.source_lang.clone(),
        target_lang: config.target_lang.clone(),
        bidirectional: config.bidirectional,
        tts_enabled: config.tts_enabled,
        start_unix_ms: now_unix_ms(),
        last_heartbeat_unix_ms: now_unix_ms(),
        utterance_count: 0,
        error_count: 0,
        last_error: None,
    };
    write_session_state(&state)?;
    tracing::info!(pid = state.pid, "session-worker state -> starting");

    let mut capture = AudioCapture::new(config.input_device.as_deref())?;
    capture.start_capture()?;
    tracing::info!("session-worker capture started");

    let asr_root = default_models_dir().join("asr");
    let asr_config = AsrConfig {
        model_path: config
            .preferred_asr_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| pick_preferred_asr_model_dir(&asr_root, &config.source_lang)),
        language: Some(config.source_lang.clone()),
        ..Default::default()
    };
    let mut asr = SherpaAsrEngine::init(asr_config)?;
    tracing::info!("session-worker primary ASR initialized");

    let mut peer_capture: Option<AudioCapture> = None;
    let mut peer_asr: Option<SherpaAsrEngine> = None;
    if config.bidirectional {
        if let Some(peer_dev) = config.peer_input_device.clone() {
            match AudioCapture::new(Some(&peer_dev)).and_then(|mut c| {
                c.start_capture()?;
                Ok(c)
            }) {
                Ok(c) => {
                    let peer_asr_config = AsrConfig {
                        model_path: config
                            .preferred_asr_dir
                            .as_ref()
                            .map(PathBuf::from)
                            .unwrap_or_else(|| pick_preferred_asr_model_dir(&asr_root, &config.target_lang)),
                        language: Some(config.target_lang.clone()),
                        ..Default::default()
                    };
                    match SherpaAsrEngine::init(peer_asr_config) {
                        Ok(engine) => {
                            tracing::info!(
                                peer_device = %peer_dev,
                                "peer capture/asr initialized for bidirectional session"
                            );
                            peer_capture = Some(c);
                            peer_asr = Some(engine);
                        }
                        Err(e) => {
                            tracing::warn!("peer asr init failed, disabling peer route: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("peer capture init failed, disabling peer route: {}", e);
                }
            }
        }
    }

    state = SessionRuntimeState {
        pid: std::process::id(),
        status: "running".to_string(),
        source_lang: config.source_lang.clone(),
        target_lang: config.target_lang.clone(),
        bidirectional: config.bidirectional,
        tts_enabled: config.tts_enabled,
        start_unix_ms: now_unix_ms(),
        last_heartbeat_unix_ms: now_unix_ms(),
        utterance_count: 0,
        error_count: 0,
        last_error: None,
    };
    write_session_state(&state)?;
    tracing::info!(pid = state.pid, "session-worker state -> running");

    let mut speech_buf: Vec<f32> = Vec::new();
    let mut silence_ms: u64 = 0;
    let mut in_speech = false;
    let mut peer_speech_buf: Vec<f32> = Vec::new();
    let mut peer_silence_ms: u64 = 0;
    let mut peer_in_speech = false;
    let min_speech_samples = 16000usize / 4;
    let stop_reason = loop {
        if let Some(ctrl) = read_session_control() {
            match ctrl.command.as_str() {
                "stop" => break "control:stop".to_string(),
                "pause" => {
                    state.status = "paused".to_string();
                    let _ = write_session_state(&state);
                    tracing::info!("session-worker paused");
                    clear_session_control();
                }
                "resume" => {
                    state.status = "running".to_string();
                    let _ = write_session_state(&state);
                    tracing::info!("session-worker resumed");
                    clear_session_control();
                }
                "update_languages" => {
                    if let Some(src) = ctrl.source_lang.clone() {
                        config.source_lang = src;
                        asr.set_language(&config.source_lang);
                    }
                    if let Some(tgt) = ctrl.target_lang.clone() {
                        config.target_lang = tgt;
                        if let Some(peer_engine) = peer_asr.as_mut() {
                            peer_engine.set_language(&config.target_lang);
                        }
                    }
                    state.source_lang = config.source_lang.clone();
                    state.target_lang = config.target_lang.clone();
                    clear_session_control();
                    let _ = write_session_state(&state);
                    tracing::info!(
                        source = %state.source_lang,
                        target = %state.target_lang,
                        "session-worker languages updated"
                    );
                }
                _ => {}
            }
        }

        if state.status == "paused" {
            state.last_heartbeat_unix_ms = now_unix_ms();
            let _ = write_session_state(&state);
            thread::sleep(Duration::from_millis(150));
            continue;
        }

        thread::sleep(Duration::from_millis(120));
        let input_samples = capture.get_samples();
        let peer_input_samples = if let Some(pc) = peer_capture.as_mut() {
            pc.get_samples()
        } else {
            Vec::new()
        };
        if input_samples.is_empty() && peer_input_samples.is_empty() {
            state.last_heartbeat_unix_ms = now_unix_ms();
            let _ = write_session_state(&state);
            continue;
        }

        let sample_rate = capture.sample_rate();
        let mono_16k = if sample_rate == 16000 {
            input_samples
        } else {
            linear_resample(&input_samples, sample_rate, 16000)
        };
        if !mono_16k.is_empty() {
            metrics.total_audio_duration_ms += (mono_16k.len() as u64 * 1000) / 16000;
        }

        if !mono_16k.is_empty() {
            let energy = avg_abs_energy(&mono_16k);
            if energy >= config.vad_threshold.max(0.005) {
                in_speech = true;
                silence_ms = 0;
                speech_buf.extend_from_slice(&mono_16k);
            } else if in_speech {
                speech_buf.extend_from_slice(&mono_16k);
                silence_ms += 120;
                if silence_ms >= 360 {
                    if speech_buf.len() >= min_speech_samples {
                        metrics.speech_duration_ms += (speech_buf.len() as u64 * 1000) / 16000;
                        let local_output =
                            config.peer_tts_output_device.as_deref().or(config.tts_output_device.as_deref());
                        let local_enable_back_translate =
                            config.bidirectional && peer_asr.is_none();
                        match process_one_utterance(
                            &mut asr,
                            &config,
                            &speech_buf,
                            &mut state,
                            &config.source_lang,
                            &config.target_lang,
                            local_output,
                            local_enable_back_translate,
                        ) {
                            Ok(timing) => {
                                if timing.translation_ms > 0 {
                                    metrics.transcription_count += 1;
                                    metrics.translation_count += 1;
                                    metrics.asr_average_latency_ms = update_running_avg(
                                        metrics.asr_average_latency_ms,
                                        metrics.transcription_count,
                                        timing.asr_ms as f32,
                                    );
                                    metrics.translation_average_latency_ms = update_running_avg(
                                        metrics.translation_average_latency_ms,
                                        metrics.translation_count,
                                        timing.translation_ms as f32,
                                    );
                                    if timing.tts_ms > 0 {
                                        metrics.tts_average_latency_ms = update_running_avg(
                                            metrics.tts_average_latency_ms,
                                            metrics.translation_count,
                                            timing.tts_ms as f32,
                                        );
                                    }
                                    let total = timing.asr_ms + timing.translation_ms + timing.tts_ms;
                                    metrics.average_latency_ms = update_running_avg(
                                        metrics.average_latency_ms,
                                        metrics.translation_count,
                                        total as f32,
                                    );
                                }
                            }
                            Err(e) => {
                                state.error_count += 1;
                                state.last_error = Some(e.to_string());
                            }
                        }
                    }
                    speech_buf.clear();
                    silence_ms = 0;
                    in_speech = false;
                }
            }
        }

        if let (Some(pc), Some(pasr)) = (peer_capture.as_ref(), peer_asr.as_mut()) {
            let peer_sample_rate = pc.sample_rate();
            let peer_mono_16k = if peer_sample_rate == 16000 {
                peer_input_samples
            } else {
                linear_resample(&peer_input_samples, peer_sample_rate, 16000)
            };
            if !peer_mono_16k.is_empty() {
                metrics.total_audio_duration_ms += (peer_mono_16k.len() as u64 * 1000) / 16000;
                let peer_energy = avg_abs_energy(&peer_mono_16k);
                if peer_energy >= config.vad_threshold.max(0.005) {
                    peer_in_speech = true;
                    peer_silence_ms = 0;
                    peer_speech_buf.extend_from_slice(&peer_mono_16k);
                } else if peer_in_speech {
                    peer_speech_buf.extend_from_slice(&peer_mono_16k);
                    peer_silence_ms += 120;
                    if peer_silence_ms >= 360 {
                        if peer_speech_buf.len() >= min_speech_samples {
                            metrics.speech_duration_ms +=
                                (peer_speech_buf.len() as u64 * 1000) / 16000;
                            match process_one_utterance(
                                pasr,
                                &config,
                                &peer_speech_buf,
                                &mut state,
                                &config.target_lang,
                                &config.source_lang,
                                config.tts_output_device.as_deref(),
                                false,
                            ) {
                                Ok(timing) => {
                                    if timing.translation_ms > 0 {
                                        metrics.transcription_count += 1;
                                        metrics.translation_count += 1;
                                        metrics.asr_average_latency_ms = update_running_avg(
                                            metrics.asr_average_latency_ms,
                                            metrics.transcription_count,
                                            timing.asr_ms as f32,
                                        );
                                        metrics.translation_average_latency_ms = update_running_avg(
                                            metrics.translation_average_latency_ms,
                                            metrics.translation_count,
                                            timing.translation_ms as f32,
                                        );
                                        if timing.tts_ms > 0 {
                                            metrics.tts_average_latency_ms = update_running_avg(
                                                metrics.tts_average_latency_ms,
                                                metrics.translation_count,
                                                timing.tts_ms as f32,
                                            );
                                        }
                                        let total =
                                            timing.asr_ms + timing.translation_ms + timing.tts_ms;
                                        metrics.average_latency_ms = update_running_avg(
                                            metrics.average_latency_ms,
                                            metrics.translation_count,
                                            total as f32,
                                        );
                                    }
                                }
                                Err(e) => {
                                    state.error_count += 1;
                                    state.last_error = Some(e.to_string());
                                }
                            }
                        }
                        peer_speech_buf.clear();
                        peer_silence_ms = 0;
                        peer_in_speech = false;
                    }
                }
            }
        }

        state.last_heartbeat_unix_ms = now_unix_ms();
        metrics.last_updated_unix_ms = now_unix_ms();
        let _ = write_session_state(&state);
        let _ = localtrans_lib::session_bus::write_metrics(&metrics);
    };

    capture.stop_capture();
    if let Some(pc) = peer_capture.as_mut() {
        pc.stop_capture();
    }
    state.status = "idle".to_string();
    state.last_error = Some(stop_reason.clone());
    state.last_heartbeat_unix_ms = now_unix_ms();
    let _ = write_session_state(&state);
    metrics.last_updated_unix_ms = now_unix_ms();
    let _ = localtrans_lib::session_bus::write_metrics(&metrics);
    clear_session_control();
    tracing::info!(reason = %stop_reason, "session-worker exit");
    Ok(())
}

fn cmd_translate(
    config: &CliConfig,
    text: &str,
    source: Option<&str>,
    target: Option<&str>,
    loci: bool,
    bidirectional: bool,
    model: Option<&str>,
) -> Result<()> {
    let source = source.unwrap_or(&config.source_lang);
    let target = target.unwrap_or(&config.target_lang);
    let use_loci = loci || config.translation_engine.eq_ignore_ascii_case("loci");
    if use_loci {
        let model_path = model
            .map(PathBuf::from)
            .or_else(|| config.preferred_loci_model.as_ref().map(PathBuf::from))
            .or_else(find_default_loci_model)
            .ok_or_else(|| anyhow!("No Loci model found under models/loci"))?;
        let result = run_loci_with_isolation(text, source, target, &model_path).or_else(|e| {
            eprintln!(
                "Loci unavailable, fallback to NLLB: {}",
                summarize_text(&e.to_string(), 240)
            );
            let mut t = NllbTranslator::new();
            t.translate(text, source, target)
        })?;
        println!("forward: {}", result.text);
        if bidirectional || config.bidirectional {
            let mut t = NllbTranslator::new();
            let back = t.translate(&result.text, target, source)?;
            println!("backward: {}", back.text);
        }
        return Ok(());
    }

    let mut t = NllbTranslator::new();
    let result = t.translate(text, source, target)?;
    println!("forward: {}", result.text);
    if bidirectional || config.bidirectional {
        let back = t.translate(&result.text, target, source)?;
        println!("backward: {}", back.text);
    }
    Ok(())
}

fn cmd_asr_wav(file: &str, lang: &str) -> Result<()> {
    let result = run_asr_with_isolation(file, lang)?;
    println!("{}", result.text);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_e2e(
    config: &CliConfig,
    file: &str,
    source: Option<&str>,
    target: Option<&str>,
    reference: Option<&str>,
    use_loci: bool,
    bidirectional: bool,
    voice: Option<&str>,
    out_wav: Option<&str>,
) -> Result<()> {
    let source = source.unwrap_or(&config.source_lang);
    let target = target.unwrap_or(&config.target_lang);
    let voice = voice.unwrap_or(&config.preferred_tts_voice);
    let t0 = Instant::now();
    let asr_out = run_asr_with_isolation(file, source)?;
    let asr_ms = asr_out.asr_ms;

    let t_tr0 = Instant::now();
    let translated = if use_loci || config.loci_enhanced || config.translation_engine.eq_ignore_ascii_case("loci") {
        let model = find_default_loci_model()
            .ok_or_else(|| anyhow!("No Loci model found under models/loci"))?;
        run_loci_with_isolation(&asr_out.text, source, target, &model).or_else(|e| {
            eprintln!(
                "Loci unavailable, fallback to NLLB: {}",
                summarize_text(&e.to_string(), 240)
            );
            let mut t = NllbTranslator::new();
            t.translate(&asr_out.text, source, target)
        })?
    } else {
        let mut t = NllbTranslator::new();
        t.translate(&asr_out.text, source, target)?
    };
    let tr_ms = t_tr0.elapsed().as_millis();

    let rt = tokio::runtime::Runtime::new()?;
    let t_tts0 = Instant::now();
    println!("ASR text: {}", asr_out.text);
    println!("Translated: {}", translated.text);
    if bidirectional || config.bidirectional {
        let mut t = NllbTranslator::new();
        let back = t.translate(&translated.text, target, source)?;
        println!("Back translated: {}", back.text);
    }
    let tts_result = if config.tts_enabled {
        rt.block_on(async { synthesize_tts_with_fallback(&translated.text, voice, &config.tts_engine).await })
    } else {
        Err(anyhow!("TTS disabled by config"))
    };
    let tts_ms = t_tts0.elapsed().as_millis();

    match tts_result {
        Ok((audio, tts_engine)) => {
            let out_path = out_wav
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("localtrans_e2e_tts.wav"));
            write_wav_f32(&out_path, &audio.samples, audio.sample_rate)?;
            println!("TTS engine: {}", tts_engine);
            if tts_engine == "silence-fallback" {
                println!(
                    "TTS diagnostics hint: run `localtrans-cli tts-health` to inspect edge/sherpa/system failures."
                );
            }
            println!("TTS wav: {}", out_path.display());
            println!(
                "Latency(ms): asr={} translate={} tts={} total={}",
                asr_ms,
                tr_ms,
                tts_ms,
                t0.elapsed().as_millis()
            );
        }
        Err(e) => {
            println!("TTS failed: {}", e);
            println!(
                "Latency(ms): asr={} translate={} tts=failed total={}",
                asr_ms,
                tr_ms,
                t0.elapsed().as_millis()
            );
        }
    }

    if let Some(ref_text) = reference {
        let acc = word_accuracy(ref_text, &asr_out.text);
        println!("ASR word accuracy: {:.2}%", acc * 100.0);
    }

    Ok(())
}

async fn synthesize_tts_with_fallback(text: &str, voice: &str, tts_engine: &str) -> Result<(TtsAudio, String)> {
    if tts_engine.eq_ignore_ascii_case("piper") {
        tracing::warn!("Piper TTS is temporarily disabled in this build due to model compatibility; falling back to system");
        let audio = synthesize_system_tts_audio_cli(text, 1.0)?;
        return Ok((audio, "system".to_string()));
    }
    if tts_engine.eq_ignore_ascii_case("system") {
        if !localtrans_lib::tts::tts_backend_allow_request("system") {
            anyhow::bail!("System TTS is temporarily disabled by circuit breaker");
        }
        let audio = synthesize_system_tts_audio_cli(text, 1.0)?;
        localtrans_lib::tts::tts_backend_record_success("system");
        return Ok((audio, "system".to_string()));
    }

    if localtrans_lib::tts::tts_backend_allow_request("edge") {
        if let Ok(edge) = EdgeTtsEngine::new() {
            match tokio::time::timeout(Duration::from_secs(4), edge.synthesize(text, voice)).await {
                Ok(res) => match res {
                    Ok(audio) => {
                        localtrans_lib::tts::tts_backend_record_success("edge");
                        return Ok((audio, "edge-tts".to_string()));
                    }
                    Err(e) => {
                        localtrans_lib::tts::tts_backend_record_failure("edge", Some(&e.to_string()));
                        tracing::warn!("Edge TTS fallback to system: {}", e);
                    }
                },
                Err(_) => {
                    localtrans_lib::tts::tts_backend_record_failure("edge", Some("timeout >4s"));
                    tracing::warn!("Edge TTS timeout (>4s), fallback to system");
                }
            }
        }
    } else {
        let snap = localtrans_lib::tts::tts_backend_snapshot("edge");
        tracing::warn!(
            "Edge TTS circuit {:?} ({}s remaining), skip",
            snap.stage,
            snap.circuit_remaining_sec
        );
    }

    if localtrans_lib::tts::tts_backend_allow_request("sherpa") {
        match synthesize_sherpa_melo_tts_cli(text, voice, 1.0) {
            Ok(audio) => {
                localtrans_lib::tts::tts_backend_record_success("sherpa");
                return Ok((audio, "sherpa-melo".to_string()));
            }
            Err(e) => {
                localtrans_lib::tts::tts_backend_record_failure("sherpa", Some(&e.to_string()));
                tracing::warn!("Sherpa Melo TTS fallback failed: {}", e);
            }
        }
    } else {
        let snap = localtrans_lib::tts::tts_backend_snapshot("sherpa");
        tracing::warn!(
            "Sherpa TTS circuit {:?} ({}s remaining), skip",
            snap.stage,
            snap.circuit_remaining_sec
        );
    }

    if localtrans_lib::tts::tts_backend_allow_request("system") {
        match synthesize_system_tts_audio_cli(text, 1.0) {
            Ok(audio) => {
                localtrans_lib::tts::tts_backend_record_success("system");
                return Ok((audio, "system".to_string()));
            }
            Err(system_err) => {
                localtrans_lib::tts::tts_backend_record_failure("system", Some(&system_err.to_string()));
                tracing::warn!("System TTS fallback failed: {}", system_err);
            }
        }
    } else {
        let snap = localtrans_lib::tts::tts_backend_snapshot("system");
        tracing::warn!(
            "System TTS circuit {:?} ({}s remaining), skip",
            snap.stage,
            snap.circuit_remaining_sec
        );
    }
    Ok((synthesize_silence_fallback(text), "silence-fallback".to_string()))
}

fn synthesize_sherpa_melo_tts_cli(text: &str, voice: &str, speed: f32) -> Result<TtsAudio> {
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let out_wav = std::env::temp_dir().join(format!("localtrans_sherpa_tts_{}.wav", now_unix_ms()));
    let sid = sherpa_speaker_id_from_voice(voice);
    let output = Command::new(exe)
        .arg("sherpa-tts-worker")
        .arg("--text")
        .arg(text)
        .arg("--speed")
        .arg(format!("{:.3}", speed))
        .arg("--sid")
        .arg(sid.to_string())
        .arg("--out-wav")
        .arg(out_wav.as_os_str())
        .output()
        .context("failed to spawn sherpa-tts-worker")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Sherpa TTS worker failed (status {:?}): {}",
            output.status.code(),
            stderr.trim()
        );
    }
    if !out_wav.exists() {
        anyhow::bail!("Sherpa TTS worker did not produce output wav");
    }
    let audio = load_wav_as_tts_audio(&out_wav)?;
    let _ = fs::remove_file(&out_wav);
    Ok(audio)
}

fn synthesize_sherpa_melo_tts_inproc(text: &str, sid: i32, speed: f32) -> Result<TtsAudio> {
    #[cfg(feature = "sherpa-backend")]
    {
        use sherpa_rs::tts::{VitsTts, VitsTtsConfig};
        use sherpa_rs::OnnxConfig;

        let base = dirs::data_local_dir()
            .ok_or_else(|| anyhow!("Cannot determine local data dir"))?
            .join("LocalTrans")
            .join("models")
            .join("tts")
            .join("sherpa")
            .join("vits-melo-tts-zh_en");
        let model_int8 = base.join("model.int8.onnx");
        let model_fp = base.join("model.onnx");
        let model = if model_int8.exists() {
            model_int8
        } else {
            model_fp
        };
        let tokens = base.join("tokens.txt");
        if !model.exists() || !tokens.exists() {
            anyhow::bail!("Sherpa Melo model files not found under {}", base.display());
        }
        let config = VitsTtsConfig {
            model: model.to_string_lossy().to_string(),
            tokens: tokens.to_string_lossy().to_string(),
            lexicon: if base.join("lexicon.txt").exists() {
                base.join("lexicon.txt").to_string_lossy().to_string()
            } else {
                String::new()
            },
            dict_dir: if base.join("dict").exists() {
                base.join("dict").to_string_lossy().to_string()
            } else {
                String::new()
            },
            onnx_config: OnnxConfig {
                provider: "cpu".to_string(),
                num_threads: 2,
                debug: false,
            },
            ..Default::default()
        };
        let mut tts = VitsTts::new(config);
        let out = match tts.create(text, sid, speed.clamp(0.5, 2.0)) {
            Ok(v) => v,
            Err(e) => {
                if sid != 0 {
                    tracing::warn!(
                        "Sherpa speaker sid={} failed ({}), fallback to sid=0",
                        sid,
                        e
                    );
                    tts.create(text, 0, speed.clamp(0.5, 2.0))
                        .map_err(|e2| anyhow!("Sherpa create failed (sid={}): {}; fallback sid=0 failed: {}", sid, e, e2))?
                } else {
                    return Err(anyhow!("Sherpa create failed: {}", e));
                }
            }
        };
        let duration_secs = if out.sample_rate == 0 {
            0.0
        } else {
            out.samples.len() as f32 / out.sample_rate as f32
        };
        Ok(TtsAudio {
            samples: out.samples,
            sample_rate: out.sample_rate,
            channels: 1,
            duration_secs,
        })
    }

    #[cfg(not(feature = "sherpa-backend"))]
    {
        let _ = (text, sid, speed);
        anyhow::bail!("Sherpa Melo TTS requires sherpa-backend feature");
    }
}

fn sherpa_speaker_id_from_voice(voice: &str) -> i32 {
    let v = voice.to_ascii_lowercase();
    if v.contains("sherpa-melo-male")
        || v.contains("yunxi")
        || v.contains("yunyang")
        || v.contains("guy")
        || v.contains("ryan")
        || v.contains("keita")
        || v.contains("injoon")
        || v.contains("male")
    {
        1
    } else {
        0
    }
}

fn load_wav_as_tts_audio(path: &Path) -> Result<TtsAudio> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("failed to open wav {}", path.display()))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let pcm: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 16) => reader
            .samples::<i16>()
            .map(|s| s.map(|v| v as f32 / 32768.0))
            .collect::<Result<Vec<_>, _>>()
            .context("failed to decode wav i16")?,
        (hound::SampleFormat::Int, 32) => reader
            .samples::<i32>()
            .map(|s| s.map(|v| v as f32 / i32::MAX as f32))
            .collect::<Result<Vec<_>, _>>()
            .context("failed to decode wav i32")?,
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .context("failed to decode wav f32")?,
        _ => anyhow::bail!(
            "unsupported wav format {:?}/{}bit",
            spec.sample_format,
            spec.bits_per_sample
        ),
    };
    let channels = spec.channels.max(1) as usize;
    let samples = if channels == 1 {
        pcm
    } else {
        let mut mono = Vec::with_capacity(pcm.len() / channels + 1);
        for frame in pcm.chunks(channels) {
            let sum: f32 = frame.iter().copied().sum();
            mono.push(sum / frame.len() as f32);
        }
        mono
    };
    let duration_secs = if sample_rate == 0 {
        0.0
    } else {
        samples.len() as f32 / sample_rate as f32
    };
    Ok(TtsAudio {
        samples,
        sample_rate,
        channels: 1,
        duration_secs,
    })
}

fn synthesize_system_tts_audio_cli(text: &str, rate: f32) -> Result<TtsAudio> {
    #[cfg(target_os = "windows")]
    {
        if std::env::var("LOCALTRANS_DISABLE_SYSTEM_TTS")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
        {
            anyhow::bail!("System TTS is disabled by LOCALTRANS_DISABLE_SYSTEM_TTS");
        }
        if let Some(reason) = localtrans_lib::tts::system_tts_cached_skip_reason() {
            anyhow::bail!("System TTS skipped by cached host capability: {}", reason);
        }

        let mut wav_path = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir);
        wav_path.push("LocalTrans");
        wav_path.push("tts-tmp");
        fs::create_dir_all(&wav_path)
            .with_context(|| format!("failed to create tts temp dir {}", wav_path.display()))?;
        let ts = now_unix_ms();
        wav_path.push(format!("localtrans_cli_tts_{}_{}.wav", ts, std::process::id()));

        let escaped_text = text.replace('\'', "''").replace('\r', " ").replace('\n', " ");
        let escaped_wav = wav_path.to_string_lossy().replace('\'', "''");
        let ps_rate = ((rate - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let script_system_speech = format!(
            "$ErrorActionPreference='Stop'; \
             Add-Type -AssemblyName System.Speech | Out-Null; \
             $s=$null; \
             try {{ \
               $s=New-Object System.Speech.Synthesis.SpeechSynthesizer; \
               if ($null -eq $s) {{ throw 'SpeechSynthesizer is null' }}; \
               try {{ $s.Rate={} }} catch {{ }}; \
               $s.SetOutputToWaveFile('{}'); \
               $s.Speak('{}'); \
             }} finally {{ if ($null -ne $s) {{ $s.Dispose() }} }}; \
             if (-not (Test-Path '{}')) {{ throw 'wav not created' }}",
            ps_rate, escaped_wav, escaped_text, escaped_wav
        );
        let script_sapi_file = format!(
            "$ErrorActionPreference='Stop'; \
             $voice=$null; $stream=$null; \
             try {{ \
               $voice=New-Object -ComObject SAPI.SpVoice; \
               if ($null -eq $voice) {{ throw 'SpVoice is null' }}; \
               try {{ $voice.Rate={} }} catch {{ }}; \
               $stream=New-Object -ComObject SAPI.SpFileStream; \
               if ($null -eq $stream) {{ throw 'SpFileStream is null' }}; \
               $stream.Open('{}', 3, $false); \
               $voice.AudioOutputStream=$stream; \
               [void]$voice.Speak('{}'); \
               $stream.Close(); \
             }} finally {{ \
               if ($null -ne $stream) {{ [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($stream) }}; \
               if ($null -ne $voice) {{ [void][System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($voice) }}; \
             }}; \
             if (-not (Test-Path '{}')) {{ throw 'wav not created' }}",
            ps_rate, escaped_wav, escaped_text, escaped_wav
        );
        let strategies = [
            ("system_speech", script_system_speech.as_str()),
            ("sapi_file", script_sapi_file.as_str()),
        ];
        let mut last_errs: Vec<String> = Vec::new();
        for exe in windows_powershell_candidates() {
            for (name, script) in strategies {
                for sta in [true, false] {
                    let probe = run_powershell_script_with_mode(script, sta, exe);
                    match probe {
                        Ok(_) => {
                            if wav_path.exists() {
                                last_errs.clear();
                                break;
                            }
                            last_errs.push(format!(
                                "{} {} {}: wav not created",
                                exe,
                                if sta { "sta" } else { "mta" },
                                name
                            ));
                        }
                        Err(e) => last_errs.push(format!(
                            "{} {} {}: {}",
                            exe,
                            if sta { "sta" } else { "mta" },
                            name,
                            summarize_text(&e.to_string(), 140)
                        )),
                    }
                    if wav_path.exists() {
                        break;
                    }
                }
                if wav_path.exists() {
                    break;
                }
            }
            if wav_path.exists() {
                break;
            }
        }
        if !wav_path.exists() {
            let joined = last_errs.iter().take(4).cloned().collect::<Vec<_>>().join(" | ");
            let lower = joined.to_ascii_lowercase();
            let blocked_by_host = lower.contains("0x80070005")
                || lower.contains("access is denied")
                || lower.contains("object reference not set");
            if blocked_by_host {
                localtrans_lib::tts::system_tts_mark_failure(
                    "System TTS is unavailable on this host (speech runtime/permissions).",
                );
                anyhow::bail!(
                    "System TTS is unavailable on this host (speech runtime/permissions). \
Keep sherpa-melo/edge as default and run `localtrans-cli tts-system-doctor` on this host."
                );
            }
            localtrans_lib::tts::system_tts_mark_failure(&format!(
                "System TTS probe failed across strategies: {}",
                joined
            ));
            anyhow::bail!("System TTS probe failed across strategies: {}", joined);
        }

        let mut reader = hound::WavReader::open(&wav_path)
            .with_context(|| format!("failed to open generated wav {}", wav_path.display()))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;

        let pcm: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
            (hound::SampleFormat::Int, 16) => reader
                .samples::<i16>()
                .map(|s| s.map(|v| v as f32 / 32768.0))
                .collect::<Result<Vec<_>, _>>()
                .context("failed to decode system wav i16")?,
            (hound::SampleFormat::Int, 32) => reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / i32::MAX as f32))
                .collect::<Result<Vec<_>, _>>()
                .context("failed to decode system wav i32")?,
            (hound::SampleFormat::Float, 32) => reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .context("failed to decode system wav f32")?,
            _ => anyhow::bail!(
                "unsupported system wav format {:?}/{}bit",
                spec.sample_format,
                spec.bits_per_sample
            ),
        };

        let channels = spec.channels.max(1) as usize;
        let samples = if channels == 1 {
            pcm
        } else {
            let mut mono = Vec::with_capacity(pcm.len() / channels + 1);
            for frame in pcm.chunks(channels) {
                let sum: f32 = frame.iter().copied().sum();
                mono.push(sum / frame.len() as f32);
            }
            mono
        };

        let _ = fs::remove_file(&wav_path);
        let duration_secs = if sample_rate == 0 {
            0.0
        } else {
            samples.len() as f32 / sample_rate as f32
        };
        if samples.is_empty() || duration_secs <= 0.0 {
            localtrans_lib::tts::system_tts_mark_failure("System TTS produced empty audio");
            anyhow::bail!("System TTS produced empty audio");
        }
        localtrans_lib::tts::system_tts_mark_success();
        Ok(TtsAudio {
            samples,
            sample_rate,
            channels: 1,
            duration_secs,
        })
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (text, rate);
        anyhow::bail!("system TTS fallback is only implemented for Windows");
    }
}

fn synthesize_silence_fallback(text: &str) -> TtsAudio {
    let sample_rate = 16_000u32;
    let seconds = ((text.chars().count() as f32 / 12.0).clamp(1.0, 5.0)) as usize;
    let sample_count = sample_rate as usize * seconds;
    TtsAudio {
        samples: vec![0.0; sample_count],
        sample_rate,
        channels: 1,
        duration_secs: sample_count as f32 / sample_rate as f32,
    }
}

fn read_wav_to_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("Failed to open wav: {}", path.display()))?;
    let spec = reader.spec();

    let samples = match spec.sample_format {
        hound::SampleFormat::Int => {
            if spec.bits_per_sample == 16 {
                reader
                    .samples::<i16>()
                    .map(|s| s.map(|x| x as f32 / 32768.0))
                    .collect::<Result<Vec<_>, _>>()?
            } else if spec.bits_per_sample == 32 {
                reader
                    .samples::<i32>()
                    .map(|s| s.map(|x| x as f32 / i32::MAX as f32))
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                return Err(anyhow!(
                    "Unsupported int PCM bit depth: {}",
                    spec.bits_per_sample
                ));
            }
        }
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
    };

    let mono = if spec.channels <= 1 {
        samples
    } else {
        let ch = spec.channels as usize;
        let mut out = Vec::with_capacity(samples.len() / ch + 1);
        for frame in samples.chunks(ch) {
            let sum: f32 = frame.iter().copied().sum();
            out.push(sum / frame.len() as f32);
        }
        out
    };

    let out = if spec.sample_rate == 16000 {
        mono
    } else {
        linear_resample(&mono, spec.sample_rate, 16000)
    };
    Ok(out)
}

fn run_asr_with_isolation(file: &str, lang: &str) -> Result<AsrWorkerOutput> {
    if lang.eq_ignore_ascii_case("en") && is_en_asr_marked_unstable() {
        return run_asr_worker_once(file, "zh");
    }
    match run_asr_worker_once(file, lang) {
        Ok(v) => Ok(v),
        Err(primary_err) => {
            // Some Zipformer English packages crash in native runtime on Windows.
            // Retry with zh/paraformer path as a pragmatic fallback.
            if !lang.eq_ignore_ascii_case("zh") {
                if lang.eq_ignore_ascii_case("en") {
                    mark_en_asr_unstable();
                }
                if let Ok(v) = run_asr_worker_once(file, "zh") {
                    eprintln!(
                        "ASR {} model is unstable on this machine, switched to stable zh model fallback",
                        lang
                    );
                    return Ok(v);
                }
            }
            Err(primary_err)
        }
    }
}

fn run_asr_worker_once(file: &str, lang: &str) -> Result<AsrWorkerOutput> {
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let output = Command::new(exe)
        .arg("asr-worker")
        .arg("--file")
        .arg(file)
        .arg("--lang")
        .arg(lang)
        .output()
        .context("failed to spawn asr-worker")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ASR worker failed (status {:?}): {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow!("ASR worker returned empty output"))?;
    let parsed: AsrWorkerOutput =
        serde_json::from_str(line).context("failed to parse asr-worker output")?;
    Ok(parsed)
}

fn run_loci_with_isolation(
    text: &str,
    source: &str,
    target: &str,
    model_path: &Path,
) -> Result<localtrans_lib::translation::TranslationResult> {
    if let Some(until_ms) = read_loci_unhealthy_until_ms() {
        let now = now_unix_ms();
        if now < until_ms {
            let remain_sec = (until_ms.saturating_sub(now)) / 1000;
            return Err(anyhow!(
                "Loci temporarily disabled after repeated failures ({}s remaining)",
                remain_sec
            ));
        }
        clear_loci_unhealthy_marker();
    }

    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let output = Command::new(exe)
        .arg("loci-worker")
        .arg("--text")
        .arg(text)
        .arg("--source")
        .arg(source)
        .arg("--target")
        .arg(target)
        .arg("--model")
        .arg(model_path.as_os_str())
        .stderr(Stdio::piped())
        .output()
        .context("failed to spawn loci-worker")?;

    if !output.status.success() {
        let stderr = summarize_text(&String::from_utf8_lossy(&output.stderr), 320);
        mark_loci_unhealthy_marker();
        return Err(anyhow!(
            "Loci worker failed (status {:?}): {}",
            output.status.code(),
            stderr
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .ok_or_else(|| anyhow!("Loci worker returned empty output"))?;
    let parsed: localtrans_lib::translation::TranslationResult =
        serde_json::from_str(line).context("failed to parse loci-worker output")?;
    clear_loci_unhealthy_marker();
    Ok(parsed)
}

fn write_wav_f32(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * 32767.0) as i16;
        writer.write_sample(v)?;
    }
    writer.finalize()?;
    Ok(())
}

fn word_accuracy(reference: &str, hyp: &str) -> f64 {
    let r = normalize_text(reference);
    let h = normalize_text(hyp);
    let r_words: Vec<&str> = r.split_whitespace().collect();
    let h_words: Vec<&str> = h.split_whitespace().collect();
    if r_words.is_empty() {
        return if h_words.is_empty() { 1.0 } else { 0.0 };
    }
    let dist = levenshtein_words(&r_words, &h_words);
    let wer = dist as f64 / r_words.len() as f64;
    (1.0 - wer).clamp(0.0, 1.0)
}

fn normalize_text(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() { c } else { ' ' })
        .collect::<String>()
}

fn levenshtein_words(a: &[&str], b: &[&str]) -> usize {
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in dp.iter_mut().enumerate().take(a.len() + 1) {
        row[0] = i;
    }
    for (j, v) in dp[0].iter_mut().enumerate().take(b.len() + 1) {
        *v = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

#[allow(clippy::too_many_arguments)]
fn process_one_utterance(
    asr: &mut SherpaAsrEngine,
    config: &CliConfig,
    samples: &[f32],
    state: &mut SessionRuntimeState,
    source_lang: &str,
    target_lang: &str,
    tts_output_device: Option<&str>,
    enable_back_translation: bool,
) -> Result<UtteranceTiming> {
    let t_asr = Instant::now();
    let (source_text, confidence) = if should_isolate_session_asr(source_lang) {
        let out = transcribe_samples_with_isolation(samples, source_lang)?;
        (out.text.trim().to_string(), out.confidence)
    } else {
        let out = asr.transcribe(samples, 16000)?;
        (out.text.trim().to_string(), out.confidence)
    };
    let asr_ms = t_asr.elapsed().as_millis();
    if source_text.is_empty() {
        return Ok(UtteranceTiming {
            asr_ms,
            translation_ms: 0,
            tts_ms: 0,
        });
    }

    let t_tr = Instant::now();
    let translated = if config.loci_enhanced || config.translation_engine.eq_ignore_ascii_case("loci")
    {
        let model = config
            .preferred_loci_model
            .as_ref()
            .map(PathBuf::from)
            .or_else(find_default_loci_model)
            .ok_or_else(|| anyhow!("No Loci model found under models/loci"))?;
        run_loci_with_isolation(&source_text, source_lang, target_lang, &model)
            .or_else(|e| {
                eprintln!(
                    "Loci unavailable, fallback to NLLB: {}",
                    summarize_text(&e.to_string(), 240)
                );
                let mut t = NllbTranslator::new();
                t.translate(&source_text, source_lang, target_lang)
            })?
    } else {
        let mut t = NllbTranslator::new();
        t.translate(&source_text, source_lang, target_lang)?
    };
    let translation_ms = t_tr.elapsed().as_millis();

    println!("{} -> {}", source_text, translated.text);

    let mut tts_ms: u128 = 0;
    if config.tts_enabled {
        let t_tts = Instant::now();
        let rt = tokio::runtime::Runtime::new()?;
        if let Ok((audio, _engine)) =
            rt.block_on(async { synthesize_tts_with_fallback(&translated.text, &config.preferred_tts_voice, &config.tts_engine).await })
        {
            let mut player = localtrans_lib::tts::playback::AudioPlayer::new(
                tts_output_device.or(config.tts_output_device.as_deref()),
            )?;
            player.set_volume(config.tts_volume);
            let _ = player.play(&audio);
        }
        tts_ms = t_tts.elapsed().as_millis();
    }

    let item = SessionHistoryItem {
        id: uuid::Uuid::new_v4().to_string(),
        source_text,
        translated_text: translated.text,
        source_lang: source_lang.to_string(),
        target_lang: target_lang.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        confidence,
    };
    append_session_history(&item)?;
    state.utterance_count += 1;

    if enable_back_translation {
        let mut t = NllbTranslator::new();
        let back = t.translate(&item.translated_text, target_lang, source_lang)?;
        println!("backward: {}", back.text);
    }
    Ok(UtteranceTiming {
        asr_ms,
        translation_ms,
        tts_ms,
    })
}

fn should_isolate_session_asr(source_lang: &str) -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }
    let env = std::env::var("LOCALTRANS_SESSION_ASR_ISOLATION")
        .unwrap_or_else(|_| "auto".to_string())
        .to_ascii_lowercase();
    if env == "1" || env == "true" || env == "on" {
        return true;
    }
    if env == "0" || env == "false" || env == "off" {
        return false;
    }
    source_lang.eq_ignore_ascii_case("en")
}

fn transcribe_samples_with_isolation(samples: &[f32], lang: &str) -> Result<AsrWorkerOutput> {
    let tmp_dir = localtrans_data_dir().join("tmp-asr");
    fs::create_dir_all(&tmp_dir)?;
    let wav_path = tmp_dir.join(format!(
        "session_asr_{}_{}_{}.wav",
        std::process::id(),
        now_unix_ms(),
        uuid::Uuid::new_v4()
    ));
    write_wav_f32(&wav_path, samples, 16000)?;
    let result = run_asr_with_isolation(&wav_path.to_string_lossy(), lang);
    let _ = fs::remove_file(&wav_path);
    result
}

fn avg_abs_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|s| s.abs()).sum::<f32>() / samples.len() as f32
}

fn update_running_avg(current_avg: f32, count: u64, sample: f32) -> f32 {
    if count <= 1 {
        return sample;
    }
    ((current_avg * (count as f32 - 1.0)) + sample) / count as f32
}

fn linear_resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        if idx + 1 >= samples.len() {
            out.push(*samples.last().unwrap_or(&0.0));
            continue;
        }
        let frac = (pos - idx as f64) as f32;
        out.push(samples[idx] * (1.0 - frac) + samples[idx + 1] * frac);
    }
    out
}

fn localtrans_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
}

fn session_state_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-state.json")
}

fn session_control_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-control.json")
}

fn session_history_path() -> PathBuf {
    localtrans_data_dir().join("cli-session-history.jsonl")
}

fn write_session_state(state: &SessionRuntimeState) -> Result<()> {
    let path = session_state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(state)?)?;
    Ok(())
}

fn read_session_state() -> Option<SessionRuntimeState> {
    let path = session_state_path();
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_session_control(command: &str, source_lang: Option<&str>, target_lang: Option<&str>) -> Result<()> {
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

fn read_session_control() -> Option<SessionControl> {
    let path = session_control_path();
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn clear_session_control() {
    let path = session_control_path();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn append_session_history(item: &SessionHistoryItem) -> Result<()> {
    let path = session_history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", serde_json::to_string(item)?)?;
    Ok(())
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as u64
}

fn is_session_alive(state: &SessionRuntimeState) -> bool {
    let now = now_unix_ms();
    now.saturating_sub(state.last_heartbeat_unix_ms) <= 5_000
        && (state.status == "starting" || state.status == "running" || state.status == "paused")
}

fn asr_unstable_flag_path() -> PathBuf {
    localtrans_data_dir().join("asr-en-unstable.flag")
}

fn is_en_asr_marked_unstable() -> bool {
    asr_unstable_flag_path().exists()
}

fn mark_en_asr_unstable() {
    let path = asr_unstable_flag_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, b"1");
}

fn loci_unhealthy_marker_path() -> PathBuf {
    localtrans_data_dir().join("loci-unhealthy-until.txt")
}

fn read_loci_unhealthy_until_ms() -> Option<u64> {
    let text = fs::read_to_string(loci_unhealthy_marker_path()).ok()?;
    text.trim().parse::<u64>().ok()
}

fn mark_loci_unhealthy_marker() {
    let path = loci_unhealthy_marker_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let until = now_unix_ms().saturating_add(5 * 60 * 1000);
    let _ = fs::write(path, until.to_string());
}

fn clear_loci_unhealthy_marker() {
    let path = loci_unhealthy_marker_path();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn summarize_text(input: &str, max_chars: usize) -> String {
    let compact = input
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join(" | ");
    if compact.len() <= max_chars {
        return compact;
    }
    let mut out = compact.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn default_models_dir() -> PathBuf {
    localtrans_data_dir().join("models")
}

fn cli_config_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("LocalTrans")
        .join("cli-config.json")
}

fn load_cli_config() -> Result<CliConfig> {
    let path = cli_config_path();
    if !path.exists() {
        return Ok(CliConfig::default());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let cfg: CliConfig = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse config: {}", path.display()))?;
    Ok(cfg)
}

fn save_cli_config(cfg: &CliConfig) -> Result<()> {
    let path = cli_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(cfg)?;
    fs::write(&path, text).with_context(|| format!("Failed to write config: {}", path.display()))
}

fn find_default_loci_model() -> Option<PathBuf> {
    let dir = default_models_dir().join("loci");
    let entries = fs::read_dir(&dir).ok()?;
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

fn pick_preferred_asr_model_dir(base_dir: &Path, preferred_lang: &str) -> PathBuf {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if is_valid_asr_model_dir(base_dir) {
        candidates.push(base_dir.to_path_buf());
    }
    if let Ok(entries) = fs::read_dir(base_dir) {
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
    let Ok(entries) = fs::read_dir(dir) else {
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
    let Ok(entries) = fs::read_dir(path) else {
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
