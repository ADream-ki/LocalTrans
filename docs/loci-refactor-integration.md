# Loci-refactor Integration

This repository now consumes `Loci-refactor` as a pinned git submodule at `submodules/loci-refactor`.

## Why

- `LocalTrans` keeps its product shell and realtime pipeline locally.
- `Loci-refactor` is treated as the inference runtime source of truth.
- The superproject commit pins the exact `Loci-refactor` commit used by a given `LocalTrans` revision.

## Layout

```text
LocalTrans/
|-- submodules/
|   `-- loci-refactor/
`-- src-tauri/
    `-- Cargo.toml   # optional dependency alias: loci -> loci-core
```

`src-tauri/Cargo.toml` maps the dependency like this:

```toml
loci = { package = "loci-core", path = "../submodules/loci-refactor/crates/core", features = ["llama"], optional = true }
```

This keeps the existing `loci` crate name in `LocalTrans` code while binding it to the refactored `loci-core` crate.

## Bootstrap

Clone and initialize recursively:

```powershell
git clone <localtrans-repo>
cd LocalTrans
git submodule update --init --recursive
```

If the nested `llama.cpp` dependency inside `submodules/loci-refactor` is missing, initialize it too:

```powershell
git -C submodules/loci-refactor submodule update --init --recursive
```

## Current integration boundary

Current work only refactors the Loci translation bridge:

- `LocalTrans` uses `LociTranslator` as the adapter.
- The adapter now targets `loci-core`'s `InferenceEngine` API.
- The adapter explicitly requests the `llama.cpp` backend so builds fail fast instead of silently falling back to the mock backend.
- `LocalTrans` now exposes a plugin-governed Loci runtime facade instead of treating Loci as a direct one-off inference call.

## Plugin-governed runtime

The host now has backend commands for the Loci runtime:

- `get_loci_runtime_snapshot`
- `get_loci_rewriter_inventory`
- `get_loci_governance_snapshot`
- `get_loci_workflow_policy`
- `load_loci_plugins`
- `activate_loci_rewriter`

These can be reached from:

- Tauri invoke commands
- dedicated CLI subcommands
- the existing generic CLI bridge via `localtrans.exe call --name ...`

Example:

```powershell
localtrans.exe loci-runtime-snapshot
localtrans.exe loci-governance-snapshot
localtrans.exe loci-workflow-policy
localtrans.exe session-preflight
localtrans.exe loci-rewriter-inventory
localtrans.exe workflow-profiles
localtrans.exe workflow-apply --profile-id meeting-low-latency
localtrans.exe loci-load-plugins --path D:\plugins\loci --source-kind directory
localtrans.exe loci-activate-rewriter --component workflow --plugin-name my-workflow-plugin

localtrans.exe call --name get_loci_runtime_snapshot --args-json "{}"
localtrans.exe call --name get_loci_governance_snapshot --args-json "{}"
localtrans.exe call --name get_loci_workflow_policy --args-json "{}"
localtrans.exe call --name get_session_preflight --args-json "{}"
localtrans.exe call --name list_workflow_profiles --args-json "{}"
localtrans.exe call --name apply_workflow_profile --args-json "{\"profile_id\":\"privacy-local-only\"}"
localtrans.exe call --name load_loci_plugins --args-json "{\"path\":\"D:\\\\plugins\\\\loci\",\"source_kind\":\"directory\"}"
localtrans.exe call --name activate_loci_rewriter --args-json "{\"component\":\"inference\",\"plugin_name\":\"my-inference-plugin\"}"
```

The dedicated CLI commands are the preferred operational surface for Loci governance now that `LocalTrans` treats `Loci-refactor` as the core plugin-governed runtime.

`LocalTrans` now also ships a built-in manifest-first workflow bundle at `src-tauri/resources/loci-plugins/localtrans-speech-workflow`, and the Tauri bundle includes that resource directory. In a default `translationEngine=loci` setup, this gives the product an out-of-box workflow governance baseline even before the user adds external plugin directories.

Additional built-in workflow bundles are now shipped for product scenarios:

- `localtrans-meeting-low-latency`
- `localtrans-privacy-local-only`
- `localtrans-caption-high-accuracy`

These bundles are discovered from the same built-in plugin directory and can be activated by setting `lociWorkflowPlugin`.

## Shared config keys

The host runtime reads these keys from `.localtrans-config.json`:

- `translationEngine`
- `lociModelPath`
- `lociPluginDirs`
- `lociInferencePlugin`
- `lociModelPlugin`
- `lociHardwarePlugin`
- `lociWorkflowPlugin`
- `lociEventBusPlugin`
- `lociPluginManagerPlugin`
- `lociUiHostPlugin`

This keeps the runtime aligned with the `Loci-refactor` design where inference, model, hardware, workflow, event bus, plugin manager, and UI host are all governed seams.

## Realtime adapter status

The realtime host now carries explicit engine identity across settings, session IPC, and pipeline runtime:

- `asrEngine`
- `translationEngine`
- `ttsEngine`

Current behavior:

- translation uses explicit MT adapters (`LociMtAdapter`, `DeterministicMtAdapter`)
- ASR is instantiated through a runtime adapter factory instead of direct pipeline wiring
- TTS is instantiated through a runtime adapter factory instead of a hardcoded command bridge
- when `translationEngine=loci`, the host now resolves an effective runtime selection through a dedicated governance layer instead of reading ASR/TTS/latency solely from host config
- an active Loci `workflow` rewriter can now steer effective ASR route, TTS route, latency profile, bidirectional mode, and backend TTS autoplay/disable behavior
- backend runtime diagnostics now expose adapter inventory plus selected ASR/MT/TTS routes
- when backend TTS is disabled, the realtime pipeline now uses a no-op TTS adapter instead of failing on an unavailable concrete TTS backend
- diagnostics now expose a dedicated Loci governance snapshot: selected engine state, resolved model path, configured plugin dirs, configured core rewriters, and current active rewriter inventory
- session UI now exposes a lightweight Loci governance summary so workflow/plugin state is visible during live operation instead of only on the diagnostics page
- settings UI now exposes `lociPluginDirs` and `lociWorkflowPlugin`, so manifest-first workflow governance can be configured without editing `.localtrans-config.json` by hand
- session UI now exposes a startup preflight so users see blockers and corrective actions before they hit runtime errors
- app shell now exposes a global first-run onboarding modal instead of a model-page-only popup, so model readiness, workflow governance, device routing, and privacy posture are explained before the user starts a session
- app shell now exposes a dedicated Readiness Center tab that aggregates preflight, workflow governance, MT runtime health, and privacy risk in one launch checklist
- Readiness Center can apply built-in workflow presets in one click by syncing `translationEngine` / `asrEngine` / `ttsEngine` / `lociWorkflowPlugin`

Current workflow declaration conventions recognized by `LocalTrans`:

- `speech.asr.whisper`
- `speech.asr.sensevoice`
- `speech.asr.vosk`
- `speech.asr.qwen3-asr`
- `speech.tts.sherpa-melo`
- `speech.tts.edge-tts`
- `speech.tts.system`
- `speech.tts.piper`
- `speech.tts.custom`
- `speech.tts.qwen3-tts`
- `speech.tts.none`
- `speech.tts.autoplay`
- `speech.tts.manual`
- `speech.latency.low-latency`
- `speech.latency.balanced`
- `speech.latency.high-accuracy`
- `speech.mode.bidirectional`
- `speech.mode.unidirectional`
- `speech.translate.realtime`
- `speech.voice.clone`

These declarations are intentionally manifest-first and map cleanly onto the active OSS migration targets already tracked in `docs/open-source-reference-matrix.md`, especially the `qwen3-asr-rs` and `qwen3-tts-rs` adapter slots.

Scaffolded engine slots:

- `qwen3-asr`
- `qwen3-tts`

These slots are intentionally explicit placeholders for the next integration stage. In the current build they fail fast with a clear "adapter not compiled/wired" error instead of silently falling back to the legacy backend.

The external engine reference matrix for the next migration stages lives in:

- `docs/open-source-reference-matrix.md`

## Next migration steps

1. Move `audio`, `asr`, `translation`, `tts`, and `pipeline` slices into shared crates.
2. Replace the remaining ad hoc command bridge paths with the `Loci-refactor/projects/loci-trans` host-runtime pattern.
3. Re-home UI/runtime coordination behind plugin-governed surfaces instead of direct Tauri page coupling.
