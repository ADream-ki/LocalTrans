use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = env!("CARGO_PKG_NAME"),
    version,
    about = "Hybrid GUI + CLI desktop tool",
    long_about = None
)]
pub struct Cli {
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    Hello {
        #[arg(short, long, default_value = "world")]
        name: String,
    },
    Version,
    ProcessFile {
        #[arg(value_name = "INPUT")]
        input: PathBuf,
    },
    DownloadModel {
        #[arg(long)]
        model_id: String,
        #[arg(long)]
        model_type: String,
    },
    ListModels {
        #[arg(long)]
        model_type: String,
    },
    DeleteModel {
        #[arg(long)]
        model_id: String,
    },
    SessionStart {
        #[arg(long, default_value = "en")]
        source_lang: String,
        #[arg(long, default_value = "zh")]
        target_lang: String,
        #[arg(long)]
        asr_engine: Option<String>,
        #[arg(long)]
        translation_engine: Option<String>,
        #[arg(long)]
        tts_engine: Option<String>,
        #[arg(long, default_value_t = false)]
        bidirectional: bool,
        #[arg(long, default_value = "balanced")]
        latency_profile: String,
    },
    RuntimeAdapters,
    LociRuntimeSnapshot {
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    LociGovernanceSnapshot {
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    LociWorkflowPolicy {
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    LociRewriterInventory {
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    LociLoadPlugins {
        #[arg(long)]
        path: PathBuf,
        #[arg(long)]
        source_kind: Option<String>,
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    LociActivateRewriter {
        #[arg(long)]
        component: String,
        #[arg(long)]
        plugin_name: String,
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    SessionPause,
    SessionResume,
    SessionStop,
    SessionPreflight,
    SessionStatus,
    SessionStats,
    SessionHistory {
        #[arg(long, default_value_t = 20)]
        count: usize,
    },
    SessionClearHistory,
    SessionExportHistory {
        #[arg(long)]
        output: Option<PathBuf>,
    },
    SessionUpdateLanguages {
        #[arg(long)]
        source_lang: String,
        #[arg(long)]
        target_lang: String,
    },
    TranslateText {
        #[arg(long)]
        text: String,
        #[arg(long)]
        source_lang: String,
        #[arg(long)]
        target_lang: String,
        #[arg(long)]
        engine: Option<String>,
        #[arg(long)]
        model_path: Option<PathBuf>,
    },
    LogStatus,
    MtRuntimeCheck,
    TtsVoices {
        #[arg(long)]
        language: Option<String>,
    },
    TtsConfig,
    TtsDefaultVoice {
        #[arg(long)]
        language: String,
    },
    TtsCustomVoices {
        #[arg(long)]
        models_dir: Option<PathBuf>,
    },
    ConfigSet {
        #[arg(long)]
        key: String,
        #[arg(long)]
        value: String,
    },
    ConfigGet {
        #[arg(long)]
        key: String,
    },
    Call {
        #[arg(long)]
        name: String,
        #[arg(long, default_value = "{}")]
        args_json: String,
    },
}
