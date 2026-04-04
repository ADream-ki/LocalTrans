# Open Source Reference Matrix

Validated on 2026-04-03 against primary project pages and official repositories.

## Goal

This document turns the previously collected project list into a concrete adoption matrix for `LocalTrans` and `Loci-refactor`.

`Loci-refactor` remains the host runtime and inference-governance core. The projects below are referenced as model/runtime providers for specific pipeline stages.

## Decision frame

- Core runtime and orchestration: `Loci-refactor`
- Realtime desktop product shell: `LocalTrans`
- Stage-specific engines should be pluggable behind adapters
- Prefer Rust-first paths for production desktop
- Keep Python-only paths as prototype, benchmark, or server-side fallback

## Tiering

### Tier A: immediate reference and integration targets

- `qwen3-asr-rs`
  - Why: Rust-local ASR with explicit Qwen3-ASR focus, CLI/API-server shape, and GPU-capable local inference path.
  - Use in this repo: next-gen ASR adapter candidate for replacing or complementing current Sherpa/Whisper path.
  - Notes: docs.rs describes Rust-local Qwen3-ASR inference with Metal/CUDA support and streaming/batch orientation.

- `qwen3-tts-rs`
  - Why: Rust-side Qwen3-TTS integration path with voice cloning and API-server shape close to desktop-product needs.
  - Use in this repo: primary custom-voice and multilingual TTS adapter candidate.
  - Notes: current docs.rs package metadata points to the actively maintained `second-state/qwen3_tts_rs` line, while official model capability comes from `QwenLM/Qwen3-TTS`.

- `huggingface/candle` Whisper + Marian-MT examples
  - Why: minimal Rust-native examples for ASR and MT, strong fallback/reference value, no product lock-in.
  - Use in this repo: reference implementation for pure-Rust translation and ASR adapter boundaries.
  - Notes: Candle itself is not the product engine; it is the Rust inference substrate/reference path.

### Tier B: product-shaping references

- `facebookresearch/seamless_communication`
  - Why: strongest open-source reference for multilingual speech-to-speech, speech-to-text, and streaming translation behavior.
  - Use in this repo: benchmark target and architecture reference for end-to-end S2ST mode, not the desktop default.
  - Notes: Python-first stack with platform/dependency constraints.

- `huggingface/speech-to-speech`
  - Why: modular cascaded speech pipeline reference covering STT/TTS/VAD/LLM composition.
  - Use in this repo: reference for adapter decomposition, websocket/service boundaries, and rapid prototyping.

- `transcribe-rs`
  - Why: unified Rust transcription abstraction across Whisper and Parakeet.
  - Use in this repo: API-shape reference for a future `AsrAdapter` trait and engine registry.

### Tier C: experimental / secondary references

- `VoiRS`
  - Why: pure-Rust TTS framework with multilingual ambitions and future voice adaptation direction.
  - Use in this repo: secondary TTS research track, not current primary production path.

## Per-stage adoption guidance

### ASR

Primary production direction:

- `qwen3-asr-rs` behind a LocalTrans adapter trait
- keep current Sherpa path as compatibility fallback during migration

Secondary references:

- Candle Whisper example for pure-Rust minimal path
- `transcribe-rs` for engine-abstraction shape

Recommended adapter boundary:

```rust
pub trait AsrAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn supported_languages(&self) -> Vec<String>;
    fn transcribe_chunk(&mut self, pcm16: &[i16], sample_rate: u32) -> anyhow::Result<AsrChunk>;
}
```

### MT

Primary production direction:

- keep deterministic MT runtime for stable offline baseline
- keep `loci-core` as the LLM-enhanced translation path
- add a Rust-native MT adapter boundary so `Marian-MT` can replace Python runtime incrementally

Recommended split:

- `DeterministicMtAdapter`: current bundled MT runtime
- `LociMtAdapter`: current `loci-core` prompt-based translation
- `RustMarianMtAdapter`: future Candle-based deterministic MT

### TTS

Primary production direction:

- keep existing TTS chain for compatibility
- make `qwen3-tts-rs` the first-class candidate for multilingual cloning and low-latency streaming output

Secondary direction:

- evaluate `VoiRS` as a pure-Rust long-term alternative where model maturity becomes sufficient

Recommended adapter boundary:

```rust
pub trait TtsAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports_voice_clone(&self) -> bool;
    fn synthesize_stream(
        &mut self,
        text: &str,
        language: &str,
        voice_profile: Option<&VoiceProfile>,
    ) -> anyhow::Result<Box<dyn Iterator<Item = anyhow::Result<AudioChunk>>>>;
}
```

### End-to-end S2ST

Primary reference only:

- `facebookresearch/seamless_communication`

Reason:

- strongest benchmark and architecture reference
- not suitable as the first desktop mainline because the current repo is Rust/Tauri-first and already has cascaded pipeline slices

## What this means for the refactor

### Now

- `Loci-refactor` is the core runtime/inference engine anchor
- `LocalTrans` should move to adapter traits, not direct crate-specific calls scattered across pipeline code
- command, IPC, and session layers should carry explicit engine identity end-to-end

### Next code migrations

1. Extract `ASR / MT / TTS` adapter traits into shared Rust modules or crates.
2. Move `LociTranslator` from an ad hoc bridge into a proper `LociMtAdapter`.
3. Introduce a `QwenAsrAdapter` placeholder contract even before full implementation lands.
4. Introduce a `QwenTtsAdapter` placeholder contract aligned with cloning and streaming output.
5. Keep Python runtime only as a bounded deterministic-MT fallback.

## Current repo status

As of 2026-04-03, the host code now reflects this matrix directly:

- realtime ASR selection is carried through settings, session config, IPC, and pipeline runtime
- realtime TTS selection is carried through settings and pipeline runtime
- `QwenAsrAdapter` and `QwenTtsAdapter` exist as explicit scaffold slots
- the scaffolded `qwen3-*` engines fail fast with a clear unsupported/build-missing message
- built-in workflow bundles now expose sellable product profiles:
  - `localtrans-meeting-low-latency`
  - `localtrans-privacy-local-only`
  - `localtrans-caption-high-accuracy`

This is deliberate: the repo now has the right pluggable seams for `qwen3-asr-rs` and `qwen3-tts-rs`, without pretending the integration is complete before a concrete buildable adapter is wired in.

## Source links

- `qwen3-asr-rs` docs.rs: https://docs.rs/crate/qwen3-asr-rs/0.2.0
- `qwen3-tts-rs` docs.rs: https://docs.rs/qwen3-tts-rs
- Official Qwen3-TTS: https://github.com/QwenLM/Qwen3-TTS
- Candle: https://github.com/huggingface/candle
- Seamless Communication: https://github.com/facebookresearch/seamless_communication
- Hugging Face speech-to-speech: https://github.com/huggingface/speech-to-speech
- transcribe-rs: https://github.com/cjpais/transcribe-rs
- VoiRS: https://github.com/cool-japan/voirs
