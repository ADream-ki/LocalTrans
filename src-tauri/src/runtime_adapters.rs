use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::PathBuf;

use crate::asr::{AsrConfig, StreamingAsrEngine, StreamingConfig, StreamingResult};
use crate::commands::tts::TtsRequest;
use crate::translation::{LociTranslator, NllbTranslator, TranslationResult, Translator};

pub trait RealtimeAsrAdapter: Send + Sync {
    fn adapter_name(&self) -> &'static str;
    fn process(&mut self, chunk: &[f32]) -> Result<Option<StreamingResult>>;
    fn set_language(&mut self, lang: &str);
}

pub trait MtAdapter: Send + Sync {
    fn adapter_name(&self) -> &'static str;
    fn translate(
        &mut self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResult>;
}

#[derive(Debug, Clone)]
pub struct TtsPlaybackRequest {
    pub text: String,
    pub voice: String,
    pub engine: Option<String>,
    pub rate: f32,
    pub volume: Option<f32>,
    pub output_device: Option<String>,
    pub custom_voice_profile_id: Option<String>,
}

pub trait TtsAdapter: Send + Sync {
    fn adapter_name(&self) -> &'static str;
    fn speak(&self, request: TtsPlaybackRequest) -> Result<()>;
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAdapterDescriptor {
    pub id: String,
    pub stage: String,
    pub label: String,
    pub available: bool,
    pub selected: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeAdapterInventory {
    pub asr: Vec<RuntimeAdapterDescriptor>,
    pub translation: Vec<RuntimeAdapterDescriptor>,
    pub tts: Vec<RuntimeAdapterDescriptor>,
}

pub struct StreamingAsrAdapter {
    engine: StreamingAsrEngine,
}

impl StreamingAsrAdapter {
    pub fn new(config: AsrConfig, streaming: StreamingConfig) -> Result<Self> {
        let engine = StreamingAsrEngine::new(config, streaming)
            .map_err(|e| anyhow!("failed to initialize streaming ASR adapter: {e}"))?;
        Ok(Self { engine })
    }
}

impl RealtimeAsrAdapter for StreamingAsrAdapter {
    fn adapter_name(&self) -> &'static str {
        "streaming-asr"
    }

    fn process(&mut self, chunk: &[f32]) -> Result<Option<StreamingResult>> {
        self.engine
            .process(chunk)
            .map_err(|e| anyhow!("streaming ASR adapter process failed: {e}"))
    }

    fn set_language(&mut self, lang: &str) {
        self.engine.set_language(lang);
    }
}

pub struct QwenAsrAdapter;

impl QwenAsrAdapter {
    pub fn new(_config: AsrConfig, _streaming: StreamingConfig) -> Result<Self> {
        Err(anyhow!(
            "qwen3-asr adapter slot is scaffolded but this build does not yet include qwen3-asr-rs; wire a concrete adapter or a Loci-governed plugin before selecting qwen3-asr"
        ))
    }
}

impl RealtimeAsrAdapter for QwenAsrAdapter {
    fn adapter_name(&self) -> &'static str {
        "qwen3-asr"
    }

    fn process(&mut self, _chunk: &[f32]) -> Result<Option<StreamingResult>> {
        Err(anyhow!("qwen3-asr adapter is not available in this build"))
    }

    fn set_language(&mut self, _lang: &str) {}
}

pub struct LociMtAdapter {
    inner: LociTranslator,
}

impl LociMtAdapter {
    pub fn new(model_path: PathBuf) -> Result<Self> {
        let inner = LociTranslator::init(&model_path).with_context(|| {
            format!(
                "failed to initialize Loci MT adapter ({})",
                model_path.display()
            )
        })?;
        Ok(Self { inner })
    }
}

impl MtAdapter for LociMtAdapter {
    fn adapter_name(&self) -> &'static str {
        "loci-mt"
    }

    fn translate(
        &mut self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResult> {
        self.inner.translate(text, source_lang, target_lang)
    }
}

pub struct DeterministicMtAdapter {
    inner: NllbTranslator,
}

impl DeterministicMtAdapter {
    pub fn new() -> Self {
        Self {
            inner: NllbTranslator::new(),
        }
    }
}

impl Default for DeterministicMtAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MtAdapter for DeterministicMtAdapter {
    fn adapter_name(&self) -> &'static str {
        "deterministic-mt"
    }

    fn translate(
        &mut self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResult> {
        self.inner.translate(text, source_lang, target_lang)
    }
}

pub struct CommandTtsAdapter;

impl CommandTtsAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CommandTtsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsAdapter for CommandTtsAdapter {
    fn adapter_name(&self) -> &'static str {
        "command-tts"
    }

    fn speak(&self, request: TtsPlaybackRequest) -> Result<()> {
        crate::commands::tts::speak_text(TtsRequest {
            text: request.text,
            voice: request.voice,
            engine: request.engine,
            rate: request.rate,
            pitch: Some(0.0),
            volume: request.volume,
            output_device: request.output_device,
            custom_voice: None,
            custom_voice_profile_id: request.custom_voice_profile_id,
        })
        .map(|_| ())
        .map_err(|e| anyhow!("command TTS adapter failed: {e}"))
    }
}

pub struct NoopTtsAdapter;

impl NoopTtsAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoopTtsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl TtsAdapter for NoopTtsAdapter {
    fn adapter_name(&self) -> &'static str {
        "noop-tts"
    }

    fn speak(&self, _request: TtsPlaybackRequest) -> Result<()> {
        Ok(())
    }
}

pub struct QwenTtsAdapter;

impl QwenTtsAdapter {
    pub fn new() -> Result<Self> {
        Err(anyhow!(
            "qwen3-tts adapter slot is scaffolded but this build does not yet include qwen3-tts-rs; wire a concrete adapter or a Loci-governed plugin before selecting qwen3-tts"
        ))
    }
}

impl TtsAdapter for QwenTtsAdapter {
    fn adapter_name(&self) -> &'static str {
        "qwen3-tts"
    }

    fn speak(&self, _request: TtsPlaybackRequest) -> Result<()> {
        Err(anyhow!("qwen3-tts adapter is not available in this build"))
    }
}

pub fn create_realtime_asr_adapter(
    engine: &str,
    config: AsrConfig,
    streaming: StreamingConfig,
) -> Result<Box<dyn RealtimeAsrAdapter>> {
    match engine.trim().to_ascii_lowercase().as_str() {
        // Keep legacy UI/config ids working while the runtime is still backed by the
        // existing streaming ASR stack. The adapter boundary now makes this explicit.
        "" | "streaming" | "streaming-asr" | "streaming-sherpa" | "sherpa" | "whisper"
        | "sensevoice" | "vosk" => Ok(Box::new(StreamingAsrAdapter::new(config, streaming)?)),
        "qwen3-asr" => Ok(Box::new(QwenAsrAdapter::new(config, streaming)?)),
        other => Err(anyhow!("Unsupported ASR engine: {other}")),
    }
}

pub fn create_tts_adapter(engine: &str) -> Result<Box<dyn TtsAdapter>> {
    match engine.trim().to_ascii_lowercase().as_str() {
        "" | "sherpa-melo" | "edge-tts" | "custom" | "piper" | "system" => {
            Ok(Box::new(CommandTtsAdapter::new()))
        }
        "qwen3-tts" => Ok(Box::new(QwenTtsAdapter::new()?)),
        other => Err(anyhow!("Unsupported TTS engine: {other}")),
    }
}

pub fn create_disabled_tts_adapter() -> Box<dyn TtsAdapter> {
    Box::new(NoopTtsAdapter::new())
}

pub fn runtime_adapter_inventory(
    selected_asr: &str,
    selected_translation: &str,
    selected_tts: &str,
) -> RuntimeAdapterInventory {
    let selected_asr = selected_asr.trim().to_ascii_lowercase();
    let selected_translation = selected_translation.trim().to_ascii_lowercase();
    let selected_tts = selected_tts.trim().to_ascii_lowercase();

    RuntimeAdapterInventory {
        asr: vec![
            RuntimeAdapterDescriptor {
                id: "whisper".to_string(),
                stage: "asr".to_string(),
                label: "Whisper compatibility route".to_string(),
                available: true,
                selected: selected_asr == "whisper",
                detail: "Currently mapped onto the existing local streaming ASR runtime.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "sensevoice".to_string(),
                stage: "asr".to_string(),
                label: "SenseVoice compatibility route".to_string(),
                available: true,
                selected: selected_asr == "sensevoice",
                detail: "Currently mapped onto the existing local streaming ASR runtime.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "vosk".to_string(),
                stage: "asr".to_string(),
                label: "Vosk compatibility route".to_string(),
                available: true,
                selected: selected_asr == "vosk",
                detail: "Currently mapped onto the existing local streaming ASR runtime.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "qwen3-asr".to_string(),
                stage: "asr".to_string(),
                label: "Qwen3-ASR scaffold".to_string(),
                available: false,
                selected: selected_asr == "qwen3-asr",
                detail: "Adapter slot exists, but this build does not yet include qwen3-asr-rs or an equivalent plugin-backed implementation.".to_string(),
            },
        ],
        translation: vec![
            RuntimeAdapterDescriptor {
                id: "loci".to_string(),
                stage: "translation".to_string(),
                label: "Loci plugin-governed MT".to_string(),
                available: cfg!(feature = "loci-backend"),
                selected: selected_translation == "loci",
                detail: if cfg!(feature = "loci-backend") {
                    "Backed by loci-core via the plugin-governed runtime facade.".to_string()
                } else {
                    "Requires the loci-backend feature in this build.".to_string()
                },
            },
            RuntimeAdapterDescriptor {
                id: "nllb".to_string(),
                stage: "translation".to_string(),
                label: "Deterministic MT runtime".to_string(),
                available: true,
                selected: selected_translation == "nllb",
                detail: "Current deterministic translation adapter.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "argos".to_string(),
                stage: "translation".to_string(),
                label: "Argos compatibility route".to_string(),
                available: true,
                selected: selected_translation == "argos",
                detail: "Normalized onto the deterministic MT adapter.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "m2m".to_string(),
                stage: "translation".to_string(),
                label: "M2M compatibility route".to_string(),
                available: true,
                selected: selected_translation == "m2m",
                detail: "Normalized onto the deterministic MT adapter.".to_string(),
            },
        ],
        tts: vec![
            RuntimeAdapterDescriptor {
                id: "sherpa-melo".to_string(),
                stage: "tts".to_string(),
                label: "Command TTS route: Sherpa Melo".to_string(),
                available: true,
                selected: selected_tts == "sherpa-melo",
                detail: "Handled by the host-side TTS command adapter.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "edge-tts".to_string(),
                stage: "tts".to_string(),
                label: "Command TTS route: Edge TTS".to_string(),
                available: true,
                selected: selected_tts == "edge-tts",
                detail: "Handled by the host-side TTS command adapter.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "custom".to_string(),
                stage: "tts".to_string(),
                label: "Command TTS route: Custom voice".to_string(),
                available: true,
                selected: selected_tts == "custom",
                detail: "Handled by the host-side TTS command adapter using custom voice profiles.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "piper".to_string(),
                stage: "tts".to_string(),
                label: "Command TTS route: Piper".to_string(),
                available: true,
                selected: selected_tts == "piper",
                detail: "Handled by the host-side TTS command adapter.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "system".to_string(),
                stage: "tts".to_string(),
                label: "Command TTS route: System voice".to_string(),
                available: true,
                selected: selected_tts == "system",
                detail: "Handled by the host-side TTS command adapter.".to_string(),
            },
            RuntimeAdapterDescriptor {
                id: "qwen3-tts".to_string(),
                stage: "tts".to_string(),
                label: "Qwen3-TTS scaffold".to_string(),
                available: false,
                selected: selected_tts == "qwen3-tts",
                detail: "Adapter slot exists, but this build does not yet include qwen3-tts-rs or an equivalent plugin-backed implementation.".to_string(),
            },
        ],
    }
}
