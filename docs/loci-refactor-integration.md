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
- `load_loci_plugins`
- `activate_loci_rewriter`

These can be reached from:

- Tauri invoke commands
- the existing CLI bridge via `localtrans.exe call --name ...`

Example:

```powershell
localtrans.exe call --name get_loci_runtime_snapshot --args-json "{}"
localtrans.exe call --name load_loci_plugins --args-json "{\"path\":\"D:\\\\plugins\\\\loci\",\"source_kind\":\"directory\"}"
localtrans.exe call --name activate_loci_rewriter --args-json "{\"component\":\"inference\",\"plugin_name\":\"my-inference-plugin\"}"
```

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

The external engine reference matrix for the next migration stages lives in:

- `docs/open-source-reference-matrix.md`

## Next migration steps

1. Move `audio`, `asr`, `translation`, `tts`, and `pipeline` slices into shared crates.
2. Replace the remaining ad hoc command bridge paths with the `Loci-refactor/projects/loci-trans` host-runtime pattern.
3. Re-home UI/runtime coordination behind plugin-governed surfaces instead of direct Tauri page coupling.
