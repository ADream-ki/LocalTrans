# LocalTrans

LocalTrans 的定位：一款“本地优先、低延迟”的实时语音转译桌面工具，面向会议/直播/跨语种沟通场景。

- 本地处理优先：音频采集、ASR、翻译尽量在本机完成
- 会议场景友好：支持将 TTS 输出到虚拟声卡（如 VB-Audio Cable），方便送入会议软件
- 关注实时性：端到端目标是“边说边翻译”的体验（延迟可诊断）

## 功能概览

- `会话`：启动/暂停/停止实时转译；显示实时文本与历史记录；可一键朗读翻译结果
- `设置`：TTS 输出设备选择、虚拟音频驱动检测与推荐、ASR/性能参数
- `模型`：读取本机模型目录并展示已安装模型（下载/删除仍在接入中）
- `诊断`：TTS/翻译/麦克风采集测试与环境信息

## 数据流（核心架构）

Rust 后端实时管线（`src-tauri/src/pipeline`）：

1) 音频采集（CPAL）
2) 统一为 mono f32，按需重采样到 16kHz
3) Streaming ASR（默认走 sherpa 后端；需要模型）
4) 翻译（Loci/NLLB 等；目前未配置模型时会走占位实现）
5) 通过 Tauri events 向前端推送：
   - `pipeline:partial-transcription`
   - `pipeline:final-transcription`
   - `pipeline:translation`
   - `pipeline:state-changed`
   - `pipeline:error`

前端（`src/pages/SessionPage`）监听事件并更新 UI/历史记录，翻译到达后可自动触发 TTS。

## 开发运行

### 1) 安装依赖

```bash
npm install
```

### 2) 前端开发（可单独跑 UI）

```bash
npm run dev
```

### 3) Tauri 桌面开发（推荐）

```bash
npm run tauri dev
```

备注：`src-tauri/tauri.conf.json` 已配置 `beforeDevCommand`，通常只需要跑 `tauri dev`。

## Windows 构建注意：libclang（bindgen）

默认启用 `sherpa-backend`（见 `src-tauri/Cargo.toml` 的 features），其依赖 `bindgen` 生成绑定，需要能找到 `libclang.dll`。

如果你遇到：`Unable to find libclang ... set the LIBCLANG_PATH environment variable`，请按以下方式处理：

1) 安装 LLVM/Clang（或 Visual Studio Build Tools 自带的 LLVM）
2) 设置环境变量 `LIBCLANG_PATH` 为包含 `libclang.dll` 的目录

PowerShell 示例：

```powershell
$env:LIBCLANG_PATH = "D:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools\VC\Tools\Llvm\x64\bin"
cd src-tauri
cargo check
```

Git Bash 示例：

```bash
export LIBCLANG_PATH="/d/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/VC/Tools/Llvm/x64/bin"
cd src-tauri
cargo check
```

## 模型目录

后端会使用系统的 Local App Data 目录作为模型根目录（命令：`get_models_dir`）。常见位置：

- Windows：`%LOCALAPPDATA%\LocalTrans\models`

建议的子目录结构（逐步完善中）：

- `asr/`：ASR 模型（sherpa zipformer 等）
- `vad/`：VAD 模型（例如 `silero_vad.onnx`）
- `tts/piper/`：Piper/VITS ONNX 模型
- `translation/` 或 `mt/`：机器翻译模型（待接入）
- `loci/`：本地 LLM 模型（待接入）

## 隐私说明

- 目标是“音频不出本地”：ASR/翻译尽量本地完成
- 当前默认的 `Edge TTS` 属于在线服务：会将要合成的文本发送至微软服务以获得语音音频
- 若你对离线 TTS 有强需求，可切换到自定义音色/本地 TTS（需模型/本地服务，仍在完善）
