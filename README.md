# LocalTrans

LocalTrans is a **real-time desktop speech translation product** for Windows.
It provides one runtime with two interfaces:
- GUI for daily operation
- CLI for automation and integration

This repository is the refactored codebase migrated from `LocalTrans-old`.
Chinese doc: [README_CN.md](./README_CN.md)

## Product Positioning

LocalTrans focuses on local-first realtime workflow:
- realtime ASR -> machine translation -> TTS playback
- GUI and CLI sharing the same command/runtime layer
- cross-process state sync (CLI can drive running GUI session)
- portable release (`localtrans.exe`) for direct distribution
- workflow-governed runtime powered by pinned `Loci-refactor`
- built-in Readiness Center and Release Center for delivery/support handoff

## Architecture (Refactor)

- `src/`: React + Vite frontend
- `src-tauri/`: Rust + Tauri runtime
- `src-tauri/src/commands/`: shared command layer (GUI invoke + CLI + IPC)
- `src-tauri/src/ipc.rs`: GUI/CLI cross-process command bridge
- `tools/prepare_mt_runtime.ps1`: prepare bundled MT runtime
- `.github/workflows/build-windows.yml`: Windows CI build

## Build Prerequisites (Windows)

- Node.js 20+
- Rust stable
- LLVM / `libclang.dll` (for ASR-related build path)

Example:

```powershell
$env:LIBCLANG_PATH="C:\Program Files\LLVM\bin"
```

The repo also includes a portable LLVM copy you can use for host builds:

```powershell
$env:LIBCLANG_PATH="$PWD\tools\llvm-portable\clang+llvm-22.1.1-x86_64-pc-windows-msvc\bin"
```

## Docker Build

This repo uses a two-layer production build flow:

1. Docker for reproducible frontend build and Rust validation
2. Windows host packaging for the final Tauri release

Why:
- the product target is a Windows desktop app
- when Docker runs with a `desktop-linux` daemon, it cannot emit a full Windows Tauri bundle directly
- Docker still helps lock down the frontend and validation environments

Run the Docker preparation step:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\docker-build.ps1
```

Run the full release flow:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\build-release.ps1
```

What the scripts do:
- `docker-build.ps1` runs `npm ci && npm run build` inside Docker
- `docker-build.ps1` runs `cargo check -q --no-default-features` inside Docker
- `build-release.ps1` sets `LIBCLANG_PATH` to the bundled LLVM and then runs `npm run tauri build` on the host

## Bundled MT Runtime (No user Python required)

`translate-text` and realtime MT can run with bundled runtime in release package:

- bundled Python runtime
- bundled `mt_translate.py`
- bundled Argos language packages (CT2 model format)

Runtime search order in app:
1. `resources/mt-runtime` next to executable (preferred)
2. `LOCALTRANS_MT_PYTHON`
3. conda fallback (`LOCALTRANS_MT_CONDA_ENV`, default `localtrans`)

### Prepare bundled MT runtime

Run before release packaging:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\prepare_mt_runtime.ps1
```

Optional custom Python source:

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\prepare_mt_runtime.ps1 -SourcePython "D:\miniconda3\envs\localtrans\python.exe"
```

The script generates:
- `src-tauri/resources/mt-runtime/python`
- `src-tauri/resources/mt-runtime/argos-packages`
- `src-tauri/resources/mt-runtime/mt_translate.py`

## Run and Package

Install dependencies:

```powershell
npm ci
```

Dev mode:

```powershell
npm run tauri dev
```

Release build:

```powershell
$env:LIBCLANG_PATH="C:\Program Files\LLVM\bin"
npm run tauri build
```

Portable release binary:

```text
src-tauri/target/release/localtrans.exe
```

## CLI Command Matrix

| Command | Purpose | Example |
|---|---|---|
| `hello` | Connectivity smoke test | `localtrans.exe hello --name Alice` |
| `version` | Print app version | `localtrans.exe version` |
| `process-file` | Process input file | `localtrans.exe process-file .\README.md` |
| `download-model` | Download model payload | `localtrans.exe download-model --model-type asr --model-id asr:sherpa-multi-zipformer` |
| `list-models` | List installed models by type | `localtrans.exe list-models --model-type asr` |
| `delete-model` | Delete model by id | `localtrans.exe delete-model --model-id asr:sherpa-multi-zipformer` |
| `session-start` | Start realtime session | `localtrans.exe session-start --source-lang zh --target-lang en --latency-profile low-latency` |
| `session-pause` | Pause realtime session | `localtrans.exe session-pause` |
| `session-resume` | Resume realtime session | `localtrans.exe session-resume` |
| `session-stop` | Stop realtime session | `localtrans.exe session-stop` |
| `session-status` | Query session state | `localtrans.exe session-status` |
| `session-stats` | Query session metrics | `localtrans.exe session-stats` |
| `session-history` | Query session history | `localtrans.exe session-history --count 20` |
| `session-clear-history` | Clear history and metrics | `localtrans.exe session-clear-history` |
| `session-export-history` | Export history to file | `localtrans.exe session-export-history --output .\history.json` |
| `session-update-languages` | Update language direction at runtime | `localtrans.exe session-update-languages --source-lang zh --target-lang en` |
| `translate-text` | One-shot text translation | `localtrans.exe translate-text --text "你好" --source-lang zh --target-lang en` |
| `session-preflight` | Query launch blockers and effective runtime | `localtrans.exe session-preflight` |
| `log-status` | Query log status | `localtrans.exe log-status` |
| `mt-runtime-check` | Verify bundled MT runtime integrity | `localtrans.exe mt-runtime-check` |
| `workflow-profiles` | List built-in sellable workflow profiles | `localtrans.exe workflow-profiles` |
| `workflow-apply` | Apply a workflow profile and sync config | `localtrans.exe workflow-apply --profile-id meeting-low-latency` |
| `loci-runtime-snapshot` | Inspect plugin-governed runtime snapshot | `localtrans.exe loci-runtime-snapshot` |
| `loci-governance-snapshot` | Inspect configured plugin governance | `localtrans.exe loci-governance-snapshot` |
| `loci-workflow-policy` | Inspect effective workflow policy | `localtrans.exe loci-workflow-policy` |
| `tts-voices` | List TTS voices | `localtrans.exe tts-voices --language zh` |
| `tts-config` | Query TTS config | `localtrans.exe tts-config` |
| `tts-default-voice` | Query default voice by language | `localtrans.exe tts-default-voice --language zh` |
| `tts-custom-voices` | Scan custom/local voice models | `localtrans.exe tts-custom-voices --models-dir .\models` |
| `config-set` | Set config value | `localtrans.exe config-set --key translationEngine --value nllb` |
| `config-get` | Get config value | `localtrans.exe config-get --key translationEngine` |
| `call` | Generic command bridge | `localtrans.exe call --name check_mt_runtime --args-json "{}"` |

`session-start --latency-profile` supports `low-latency`, `balanced`, `high-accuracy`.

The GUI now includes:
- `Readiness` tab for sellability/startup checks
- `Release` tab for version, support snapshot, workflow posture, and delivery packaging context

## Test Checklist (Portable Release)

1. Build release (`npm run tauri build`).
2. Start GUI (`localtrans.exe` with no args).
3. In another terminal, run CLI and verify GUI sync:
   - `session-start`, `session-pause`, `session-resume`, `session-stop`
4. Verify MT runtime:
   - `localtrans.exe mt-runtime-check`
   - `localtrans.exe translate-text --text "你好，我们下午三点开会。" --source-lang zh --target-lang en`
   - `localtrans.exe translate-text --text "Could you share the latest deployment checklist?" --source-lang en --target-lang zh`

## CI (GitHub Actions)

Workflow: `.github/workflows/build-windows.yml`

Current CI builds Tauri release artifacts. If you require bundled MT runtime in CI artifacts, ensure `src-tauri/resources/mt-runtime` is available before `npm run tauri build` (for example: checked in, or generated in a prior step using an available Python environment).

## Output Convention

- Success: JSON to `stdout`
- Failure: JSON to `stderr`, exit code `1`

