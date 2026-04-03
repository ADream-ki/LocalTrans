use anyhow::{anyhow, Context, Result};
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
    fn translate(&mut self, text: &str, source_lang: &str, target_lang: &str)
        -> Result<TranslationResult>;
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

pub struct LociMtAdapter {
    inner: LociTranslator,
}

impl LociMtAdapter {
    pub fn new(model_path: PathBuf) -> Result<Self> {
        let inner = LociTranslator::init(&model_path)
            .with_context(|| format!("failed to initialize Loci MT adapter ({})", model_path.display()))?;
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
