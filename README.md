# LocalTrans

LocalTrans 是一个端侧本地部署的 AI 实时翻译工具，支持从音频输入到 ASR、MT、TTS 的完整链路，提供 CLI 和 GUI 两种使用方式。

## 1. 核心能力

- 本地离线处理，默认不依赖云端接口。
- 实时语音识别 + 翻译 + 语音合成。
- 支持虚拟声卡接入会议软件（腾讯会议、Zoom 等）。
- 支持模型下载管理，并在 GUI 中自动下载与自动应用。
- 支持打包为 Windows 可执行文件（`localtrans.exe` / `localtrans-gui.exe`）。
- 支持双向会话编排（`我->对方` 与 `对方->我`）。
- 双向场景已内置跨方向防回授抑制（减少回声自激循环）。
- 双向场景已支持方向级自动恢复（单方向异常时自动拉起，不拖垮全会话）。

## 2. 当前默认稳定链路

- ASR: `vosk`
- MT: `argos-ct2`
- TTS: `pyttsx3`

可选真实模型能力（已接入）：

- ASR: `whisper-*`, `faster-whisper-*`, `funasr`, `sherpa-onnx`（可选）
- MT: `nllb-*`, `argos-*`
- TTS: `piper-*`

## 3. 环境要求

- Python: `3.10` 或 `3.11`（推荐 `3.11`）
- 系统: Windows 10/11（项目当前主要验证于 Windows）
- 虚拟声卡: VB-Cable（会议路由推荐）

## 4. 快速开始（Conda）

### 4.1 创建环境

```powershell
conda create -n localtrans python=3.11 -y
conda activate localtrans
```

### 4.2 安装项目

```powershell
pip install -U pip
pip install -e ".[gui,download]"
```

说明：

- 基础依赖已包含 `vosk`、`argostranslate`、`ctranslate2`、`pyttsx3`。
- 若需要 Whisper 模型下载/推理，请额外安装：

```powershell
pip install openai-whisper
```

- 若环境可安装 FunASR（中文流式识别候选）：

```powershell
pip install funasr
```

- 若要启用 Qwen3-ASR（依赖 FunASR）：

```powershell
pip install funasr
```

- 若要启用 sherpa-onnx（中文流式识别候选）：

```powershell
pip install sherpa-onnx
```

- 若需要 Piper 本地 TTS，请额外安装：

```powershell
pip install piper-tts
```

## 5. 模型下载

### 5.1 查看模型状态

```powershell
localtrans models
```

### 5.2 下载单个模型

```powershell
localtrans download vosk-model-small-en-us-0.15
localtrans download argos-en-zh
localtrans download argos-zh-en
localtrans download piper-zh_CN-huayan
localtrans download qwen3-asr-0.6b
localtrans download wenet-u2pp-cn
```

### 5.3 可下载模型（内置）

- ASR: `whisper-tiny`, `whisper-base`, `whisper-small`, `whisper-turbo`, `faster-whisper-base`, `faster-whisper-small`, `faster-whisper-medium`, `faster-whisper-large-v3`, `faster-whisper-distil-large-v3`, `funasr-sensevoice-small`, `qwen3-asr-0.6b`, `qwen3-asr-1.7b`, `wenet-u2pp-cn`, `sherpa-onnx-zh-en-zipformer`, `vosk-model-small-en-us-0.15`, `vosk-model-small-cn-0.22`
- MT: `nllb-200-distilled-600M`, `nllb-200-1.3B`, `argos-en-zh`, `argos-zh-en`
- TTS: `piper-zh_CN-huayan`, `piper-en_US-lessac`

模型默认目录：

- `C:\Users\<你的用户名>\.localtrans\models`

## 6. 运行方式

### 6.1 CLI

```powershell
localtrans --help
localtrans check
localtrans devices
localtrans run -s zh -t en
localtrans run -s zh -t en --profile realtime
localtrans run -s zh -t en --profile quality --asr-direct-translate
```

禁用 TTS：

```powershell
localtrans run -s zh -t en --no-tts
```

双向会话：

```powershell
localtrans run --bidirectional --input-device 3 --reverse-input-device 7 -s zh -t en
```

说明：

- 双向模式下 `--input-device` 必填（本地麦克风）。
- `--reverse-input-device` 可选；不填则自动探测会议回录设备（Stereo Mix / VB-CABLE 等）。
- CLI 会输出“会话摘要”，包含每方向设备、模型与输出模式。

### 6.2 GUI

```powershell
localtrans-gui
```

GUI 关键功能：

- 在“运行配置”页可选择 ASR/MT/TTS 后端。
- 可选“Whisper中译英直出（跳过MT）”实验模式（仅 whisper/faster-whisper + 目标英文）。
- 在“模型下载与自动配置”中可选择要下载的模型。
- 提供“极速(<1s)预设”一键切到中译英低延迟组合。
- 新增“运行模式”档位：`实时优先 / 平衡 / 质量优先`。
- 勾选“应用/启动时自动下载并配置所选模型”后，点击“应用到当前会话”或“开始翻译”会自动处理模型。
- 实时链路已启用增量流式输出（短句分段即译），并将 TTS 与 MT 解耦，减少排队延迟。
- 双向模式下 GUI 顶部会显示“当前会话”摘要（每方向设备、模型、输出模式、重启信息）。
- 双向模式启用编排层，包含跨方向防回授抑制与方向级自动恢复。

### 6.3 低延迟（<1s）推荐参数

在 GUI「运行配置 -> 音频与延迟」中建议：

- `ASR缓冲时长(s) = 0.6`
- `ASR重叠时长(s) = 0.05`
- `最大翻译队列 = 2`

推荐组合（中译英）：

- ASR: `faster-whisper-small`（实时优先）或 `faster-whisper-distil-large-v3`（精度优先，GPU推荐）
- MT: `argos-ct2` + `argos-zh-en`
- TTS: `piper-en_US-lessac`（若先追求最低延迟，可先关闭 TTS 验证文本链路）

### 6.4 延迟基准测试（P50/P95）

CLI（源码环境或 exe）：

```powershell
localtrans benchmark -s zh -t en --iterations 20 --warmup 3
localtrans benchmark -s zh -t en --iterations 20 --warmup 3 --no-tts
localtrans benchmark -s zh -t en --audio-file .\sample.wav --iterations 10
```

脚本方式（源码仓库）：

```powershell
python scripts/benchmark_latency.py --iterations 20 --warmup 3
```

输出会包含 `MT/TTS/TOTAL`（或含音频文件时的 `ASR/MT/TTS/TOTAL`）的 `P50/P95/AVG`。

说明：

- 实时管线已加入低置信度片段过滤、中文字符占比过滤、常见幻听短语清洗（如“感谢观看/字幕by”），以减少误翻译噪声。

### 6.5 全链路长语音回归

```powershell
python -u scripts/fullchain_realtime_eval.py --asr-model-type faster-whisper --asr-model-size small
```

输出包含：

- 首次输出时间（是否在音频结束前实时输出）
- 分段 `MT/TTS/E2E` 延迟 `P50/P95`
- 近似准确度（ASR/翻译文本相似度，便于回归对比）

## 7. 腾讯会议接入（你说话 -> 会议其他人听到翻译语音）

目标链路：你麦克风输入 -> LocalTrans 翻译与合成 -> 输出到虚拟声卡 -> 腾讯会议把虚拟声卡当麦克风发送。

### 7.1 推荐配置

1. LocalTrans 中选择输入设备为你的真实麦克风。
2. LocalTrans 勾选“输出到虚拟设备”。
3. 腾讯会议中将“麦克风”设为 `CABLE Output (VB-Audio Virtual Cable)`。
4. 讲话后，会议中其他人会听到翻译后的 TTS 语音。

### 7.2 避免回声/环路

- 不要把“会议扬声器输出捕获”和“翻译语音回灌会议”混在同一条设备链路。
- 建议将“听会场景”和“发言转译场景”分开配置，必要时使用双虚拟线缆。

## 8. Piper TTS 使用说明

项目已兼容 `piper-tts` 新版接口（`1.4.x`）。

在 GUI 中可直接：

1. 选择 `TTS引擎 = piper`
2. 在“模型下载与自动配置”选择 `piper-zh_CN-huayan` 或 `piper-en_US-lessac`
3. 点击“下载并应用所选模型”

CLI 也可通过环境变量覆盖：

```powershell
$env:LOCALTRANS_TTS__ENGINE="piper"
$env:LOCALTRANS_TTS__MODEL_PATH="C:\Users\<你的用户名>\.localtrans\models\tts\piper-zh_CN-huayan\zh\zh_CN\huayan\medium\zh_CN-huayan-medium.onnx"
$env:LOCALTRANS_TTS__LANGUAGE="en"
localtrans run -s zh -t en
```

## 9. CUDA 说明

- 当前默认流程下，MT 使用 `ctranslate2`，可独立利用 CUDA（不依赖 torch CUDA 版）。
- `whisper`/`transformers` 等 torch 路线若要上 GPU，需要自行安装对应 CUDA 版 torch。
- 可用以下命令检查：

```powershell
python -c "import torch;print(torch.__version__, torch.cuda.is_available(), torch.version.cuda)"
python -c "import ctranslate2;print(ctranslate2.get_cuda_device_count())"
```

## 10. 打包 EXE

在 `localtrans` 环境执行：

```powershell
$env:PYTHONPATH="src"
pyinstaller localtrans.spec --noconfirm --clean
pyinstaller localtrans-gui.spec --noconfirm --clean
```

输出目录：

- `dist\localtrans.exe`
- `dist\localtrans-gui.exe`

## 11. 产物可用性验证（建议发布前执行）

```powershell
.\dist\localtrans.exe --version
.\dist\localtrans.exe check
.\dist\localtrans.exe models
.\dist\localtrans.exe devices
```

运行时日志目录：

- `C:\Users\<你的用户名>\.localtrans\logs`

## 12. 常见问题

### 12.1 模型下载慢或超时

可使用 Hugging Face 镜像：

```powershell
$env:HF_ENDPOINT="https://hf-mirror.com"
```

### 12.2 GUI 启动后 TTS 无声

1. 检查是否启用 TTS（`启用语音合成`）。
2. 检查所选 TTS 引擎依赖是否安装（如 `piper-tts`）。
3. 在 GUI 中点击“刷新模型状态”，确认目标 TTS 模型显示 `[OK]`。

### 12.3 CLI 下载命令显示成功但模型不完整

以 `localtrans models` 或模型目录实际文件为准进行复核。

---

如需扩展模型或定制下载源，请修改：

- `src/localtrans/utils/model_downloader.py`
