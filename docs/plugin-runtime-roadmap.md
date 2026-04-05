# Plugin Runtime Roadmap

Validated against the current `LocalTrans` codebase on 2026-04-05.

## Product stance

`LocalTrans` is the sellable desktop product shell.

`Loci-refactor` is the pinned inference-runtime anchor.

Everything that can change by customer tier, hardware target, privacy posture, latency target, or language strategy should remain replaceable behind plugin-governed seams instead of being hardwired into the GUI.

## Non-negotiables

- `Loci-refactor` stays pinned as a git submodule so every shipped `LocalTrans` revision is tied to an exact inference-runtime commit.
- GUI, CLI, IPC, and packaging must expose the same effective runtime state.
- ASR / MT / TTS engines must remain adapter-driven so we can ship compatibility backends now and higher-end engines later without re-breaking the product shell.
- Workflow policy must remain manifest-first so solution presets can be sold by scenario, not by asking end users to hand-edit low-level config.

## Runtime layering

### 1. Product shell

Owned by this repo:

- Tauri desktop host
- React GUI
- CLI
- IPC bridge
- release packaging
- readiness / diagnostics / support surfaces

### 2. Governance layer

Shared contract between product and inference runtime:

- effective engine selection
- workflow profile application
- latency posture
- voice-cloning posture
- privacy and local-only routing
- plugin inventory and activation

### 3. Engine adapters

Current production principle:

- keep the host independent from specific engine crates
- treat each engine as an implementation behind `ASR / MT / TTS` adapter factories

### 4. Model/runtime providers

External projects are referenced here only through bounded adapter seams.

## Open-source reference mapping

### Rust-first production references

- `qwen3-asr-rs`
  - Target role: multilingual ASR adapter
  - Why: Rust-local, low-latency direction, aligned with the product's local-first positioning

- `qwen3-tts-rs`
  - Target role: multilingual streaming TTS adapter with voice cloning
  - Why: closest Rust-side fit for sellable custom-voice scenarios

- Candle Whisper example
  - Target role: minimal pure-Rust ASR reference and fallback substrate
  - Why: validates inference boundaries without locking product design to a specific stack

- Candle Marian-MT example
  - Target role: Rust-native deterministic MT adapter
  - Why: long-term replacement path for the bundled Python MT runtime in stricter offline deployments

- `transcribe-rs`
  - Target role: API-shape reference for unified ASR adapter inventory
  - Why: useful abstraction reference even if not adopted directly

- `VoiRS`
  - Target role: secondary pure-Rust TTS research track
  - Why: keeps an alternative path open if Qwen3-TTS integration tradeoffs become unfavorable

### Python reference / benchmark tracks

- `facebookresearch/seamless_communication`
  - Target role: benchmark and architecture reference for end-to-end S2ST
  - Why: strongest open reference for multilingual speech-to-speech behavior, but not the first desktop mainline

- `huggingface/speech-to-speech`
  - Target role: prototype and service-boundary reference
  - Why: good cascaded reference for experimentation and comparison against our Rust-first product shell

## Current repo status

Already productized:

- pinned `Loci-refactor` submodule integration
- plugin-governed runtime and workflow policy surfaces
- built-in workflow bundles for sellable scenarios
- readiness center for launch gating
- release center for delivery/support handoff
- Windows packaging pipeline including portable bundle validation

Intentionally scaffolded but not yet wired to real third-party crates:

- `qwen3-asr`
- `qwen3-tts`

This is deliberate. The current code exposes the correct architectural seams without pretending the integration is production-ready before we can compile, package, and validate the real engine crates in this repository.

## Shipping strategy

### What ships now

- stable desktop shell
- deterministic offline MT baseline
- plugin-governed Loci translation path
- compatibility ASR/TTS backends
- portable Windows release with bundled runtime dependencies

### What unlocks premium SKU value next

1. Real `qwen3-asr-rs` adapter wiring
2. Real `qwen3-tts-rs` adapter wiring
3. customer-facing voice clone enrollment flow
4. language-pair capability matrix surfaced in GUI and CLI
5. benchmark suite comparing current compatibility engines vs next-gen Rust adapters

## Engineering rule for future integrations

When adding a new engine:

1. add or extend the adapter implementation
2. wire it into the factory and explicit engine identity flow
3. expose it through workflow declarations instead of direct page logic
4. add readiness diagnostics and release packaging checks
5. fail fast when the engine is selected but not compiled or not provisioned

If a candidate engine cannot meet those five conditions, it is not integrated deeply enough to be considered product-ready.
