# LocalTrans（中文）

LocalTrans 是基于 Tauri v2 的桌面实时转写/翻译工具，GUI 与 CLI 共用同一套运行时。  
当前仓库是 `LocalTrans-old` 的重构版本。

English doc: [README.md](./README.md)

## 仓库结构

- `src/`: 前端（React + Vite）
- `src-tauri/`: 后端与运行时（Rust + Tauri）
- `src-tauri/src/commands/`: GUI/CLI 共享命令层
- `src-tauri/src/cli/parser.rs`: CLI 参数定义
- `.github/workflows/build-windows.yml`: 自动构建工作流

## 本地环境要求（Windows）

- Node.js 20+
- Rust stable
- 真实 ASR 构建需要可用的 `libclang.dll`

推荐方式（conda）：

```powershell
D:\miniconda3\Scripts\conda.exe run -n localtrans python -m pip install -U libclang
```

设置：

```powershell
$env:LIBCLANG_PATH="C:\Users\<你>\.conda\envs\localtrans\Lib\site-packages\clang\native"
```

## 运行与打包

```powershell
npm ci
npm run tauri dev
```

```powershell
$env:LIBCLANG_PATH="C:\Users\<你>\.conda\envs\localtrans\Lib\site-packages\clang\native"
npm run tauri build
```

便携版产物：

```text
src-tauri/target/release/localtrans.exe
```

## CLI 示例

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

## GitHub 自动构建

工作流文件：

- `.github/workflows/build-windows.yml`

触发与行为：

- 在 `push`、`pull_request`、`workflow_dispatch` 触发
- 使用 `windows-latest`
- 安装 LLVM 并设置 `LIBCLANG_PATH`
- 执行 `npm ci` 与 `npm run tauri build`
- 上传产物：
  - `src-tauri/target/release/localtrans.exe`
  - `src-tauri/target/release/bundle/**`

## 开发说明

### 安装依赖

```powershell
npm ci
```

### 仅运行前端

```powershell
npm run dev
```

### 桌面调试运行

```powershell
npm run tauri dev
```

### 本地 release 打包

```powershell
$env:LIBCLANG_PATH="C:\Users\<你>\.conda\envs\localtrans\Lib\site-packages\clang\native"
npm run tauri build
```

### 后端快速检查

```powershell
$env:LIBCLANG_PATH="C:\Users\<你>\.conda\envs\localtrans\Lib\site-packages\clang\native"
cd src-tauri
cargo check -q
```

### 常见问题

- `Unable to find libclang`：
  - 确认存在 `libclang.dll`
  - 确认 `LIBCLANG_PATH` 指向该文件所在目录
- CLI 与 GUI 状态不同步：
  - 用 release 可执行文件按顺序验证：
    - `session-start -> session-status -> session-pause -> session-resume -> session-stop`

## 输出约定

- 成功：JSON 输出到 stdout
- 失败：JSON 输出到 stderr，返回码 `1`
