# LocalTrans

LocalTrans is a Tauri v2 desktop app with a unified GUI + CLI runtime.
This repository is the refactored codebase (migrated from `LocalTrans-old`).

Chinese doc: [README_CN.md](./README_CN.md)

## Repository Structure

- `src/`: frontend (React + Vite)
- `src-tauri/`: backend/runtime (Rust + Tauri)
- `src-tauri/src/commands/`: shared command layer for GUI invoke and CLI
- `src-tauri/src/cli/parser.rs`: CLI command definitions
- `.github/workflows/build-windows.yml`: CI build workflow

## Local Prerequisites (Windows)

- Node.js 20+
- Rust stable
- A valid `libclang.dll` path for real ASR build

Recommended (conda):

```powershell
D:\miniconda3\Scripts\conda.exe run -n localtrans python -m pip install -U libclang
```

Then set:

```powershell
$env:LIBCLANG_PATH="C:\Users\<you>\.conda\envs\localtrans\Lib\site-packages\clang\native"
```

## Run & Build

```powershell
npm ci
npm run tauri dev
```

```powershell
$env:LIBCLANG_PATH="C:\Users\<you>\.conda\envs\localtrans\Lib\site-packages\clang\native"
npm run tauri build
```

Portable release binary:

```text
src-tauri/target/release/localtrans.exe
```

## CLI Examples

```powershell
.\src-tauri\target\release\localtrans.exe --help
.\src-tauri\target\release\localtrans.exe version
.\src-tauri\target\release\localtrans.exe process-file .\README.md
```

```powershell
.\src-tauri\target\release\localtrans.exe session-start --source-lang en --target-lang zh
.\src-tauri\target\release\localtrans.exe session-status
.\src-tauri\target\release\localtrans.exe session-pause
.\src-tauri\target\release\localtrans.exe session-resume
.\src-tauri\target\release\localtrans.exe session-stop
```

## GitHub Actions

Workflow file:

- `.github/workflows/build-windows.yml`

Behavior:

- triggers on `push`, `pull_request`, and `workflow_dispatch`
- uses `windows-latest`
- installs LLVM and sets `LIBCLANG_PATH`
- runs `npm ci` and `npm run tauri build`
- uploads:
  - `src-tauri/target/release/localtrans.exe`
  - `src-tauri/target/release/bundle/**`

## Development Guide

### Install dependencies

```powershell
npm ci
```

### Run frontend only

```powershell
npm run dev
```

### Run desktop app in dev mode

```powershell
npm run tauri dev
```

### Build release locally

```powershell
$env:LIBCLANG_PATH="C:\Users\<you>\.conda\envs\localtrans\Lib\site-packages\clang\native"
npm run tauri build
```

### Backend check

```powershell
$env:LIBCLANG_PATH="C:\Users\<you>\.conda\envs\localtrans\Lib\site-packages\clang\native"
cd src-tauri
cargo check -q
```

### Typical troubleshooting

- `Unable to find libclang`:
  - verify `libclang.dll` exists
  - ensure `LIBCLANG_PATH` points to the directory containing that file
- CLI/GUI state mismatch:
  - test with release binary:
    - `session-start -> session-status -> session-pause -> session-resume -> session-stop`

## Output Convention

- success: JSON to stdout
- failure: JSON to stderr, exit code `1`
