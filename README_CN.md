# LocalTrans（中文）

LocalTrans 是面向 Windows 的**实时语音翻译产品**。
一个运行时同时提供两种入口：
- GUI：日常操作
- CLI：自动化与集成

本仓库是从 `LocalTrans-old` 迁移后的重构版本。
English doc: [README.md](./README.md)

## 产品定位

LocalTrans 以本地优先的实时闭环为核心：
- 实时 ASR -> 机器翻译 -> TTS 播报
- GUI 与 CLI 共用同一命令层与运行时
- 支持跨进程状态同步（CLI 可驱动正在运行的 GUI 会话）
- 提供可直接分发的便携版 `localtrans.exe`
- 通过锁定版本的 `Loci-refactor` 实现工作流治理式运行时
- 内置“系统就绪中心”和“交付中心”，方便售前、实施、支持协同

## 重构后的结构

- `src/`: React + Vite 前端
- `src-tauri/`: Rust + Tauri 运行时
- `src-tauri/src/commands/`: GUI invoke / CLI / IPC 共享命令层
- `src-tauri/src/ipc.rs`: GUI 与 CLI 跨进程命令桥
- `tools/prepare_mt_runtime.ps1`: 内置 MT 运行时准备脚本
- `.github/workflows/build-windows.yml`: Windows 自动构建

## 构建环境（Windows）

- Node.js 20+
- Rust stable
- LLVM / `libclang.dll`（ASR 相关构建路径需要）

示例：

```powershell
$env:LIBCLANG_PATH="C:\Program Files\LLVM\bin"
```

仓库也自带一份便携 LLVM，可直接用于本机构建：

```powershell
$env:LIBCLANG_PATH="$PWD\tools\llvm-portable\clang+llvm-22.1.1-x86_64-pc-windows-msvc\bin"
```

## Docker 构建

当前仓库的生产构建分两层：

1. Docker 负责可复现的前端构建与 Rust 语法/feature 校验
2. Windows 主机负责最终 Tauri release 打包

原因：
- 当前桌面程序目标是 Windows
- 你的 Docker daemon 若是 `desktop-linux`，无法直接产出完整 Windows Tauri bundle
- 但仍然可以用 Docker 固定前端与校验环境，减少“我本机能跑”的漂移

执行 Docker 预构建：

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\docker-build.ps1
```

执行完整 release：

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\build-release.ps1
```

其中：
- `docker-build.ps1` 会在容器内运行 `npm ci && npm run build`
- `docker-build.ps1` 也会在容器内运行 `cargo check -q --no-default-features`
- `build-release.ps1` 会自动设置仓库内置的 `LIBCLANG_PATH`，先执行 `npm run build`，再在主机打一个启用 `loci-backend` 的 NSIS release
- 如需本机额外产出 MSI，可执行 `build-release.ps1 -Bundles "nsis,msi"`
- 如果工作站策略偶发拦截 `esbuild` 子进程，可先手工执行 `npm run build`，再用 `build-release.ps1 -SkipFrontendBuild` 继续打包

## 内置 MT 运行时（用户机器无需 Python）

Release 包中的 `translate-text` 与实时机翻可直接使用内置运行时：

- 内置 Python runtime
- 内置 `mt_translate.py`
- 内置 Argos 语言包（CT2 格式）

程序中的查找顺序：
1. 可执行文件旁 `resources/mt-runtime`（优先）
2. `LOCALTRANS_MT_PYTHON`
3. conda 回退（`LOCALTRANS_MT_CONDA_ENV`，默认 `localtrans`）

### 准备内置 MT 运行时

打包前执行：

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\prepare_mt_runtime.ps1
```

可指定 Python 源：

```powershell
powershell -ExecutionPolicy Bypass -File .\tools\prepare_mt_runtime.ps1 -SourcePython "D:\miniconda3\envs\localtrans\python.exe"
```

脚本会生成：
- `src-tauri/resources/mt-runtime/python`
- `src-tauri/resources/mt-runtime/argos-packages`
- `src-tauri/resources/mt-runtime/mt_translate.py`

## 运行与打包

安装依赖：

```powershell
npm ci
```

开发模式：

```powershell
npm run tauri dev
```

Release 打包：

```powershell
$env:LIBCLANG_PATH="C:\Program Files\LLVM\bin"
npm run tauri build
```

便携版产物：

```text
src-tauri/target/release/localtrans.exe
```

## CLI 命令矩阵

| 命令 | 作用 | 示例 |
|---|---|---|
| `hello` | 连通性冒烟测试 | `localtrans.exe hello --name Alice` |
| `version` | 输出版本信息 | `localtrans.exe version` |
| `process-file` | 处理输入文件 | `localtrans.exe process-file .\README.md` |
| `download-model` | 下载模型资源 | `localtrans.exe download-model --model-type asr --model-id asr:sherpa-multi-zipformer` |
| `list-models` | 按类型列出模型 | `localtrans.exe list-models --model-type asr` |
| `delete-model` | 按 id 删除模型 | `localtrans.exe delete-model --model-id asr:sherpa-multi-zipformer` |
| `session-start` | 启动实时会话 | `localtrans.exe session-start --source-lang zh --target-lang en --latency-profile low-latency` |
| `session-pause` | 暂停实时会话 | `localtrans.exe session-pause` |
| `session-resume` | 恢复实时会话 | `localtrans.exe session-resume` |
| `session-stop` | 停止实时会话 | `localtrans.exe session-stop` |
| `session-status` | 查询会话状态 | `localtrans.exe session-status` |
| `session-stats` | 查询会话指标 | `localtrans.exe session-stats` |
| `session-history` | 查询会话历史 | `localtrans.exe session-history --count 20` |
| `session-clear-history` | 清空历史和指标 | `localtrans.exe session-clear-history` |
| `session-export-history` | 导出历史 | `localtrans.exe session-export-history --output .\history.json` |
| `session-update-languages` | 运行中切换语言方向 | `localtrans.exe session-update-languages --source-lang zh --target-lang en` |
| `translate-text` | 单次文本翻译 | `localtrans.exe translate-text --text "你好" --source-lang zh --target-lang en` |
| `session-preflight` | 查询启动阻塞项与生效运行时 | `localtrans.exe session-preflight` |
| `log-status` | 查询日志状态 | `localtrans.exe log-status` |
| `mt-runtime-check` | 校验内置 MT 运行时完整性 | `localtrans.exe mt-runtime-check` |
| `support-snapshot` | 导出结构化支持诊断快照 | `localtrans.exe support-snapshot` |
| `workflow-profiles` | 列出内置工作流预设 | `localtrans.exe workflow-profiles` |
| `workflow-apply` | 应用工作流预设并同步配置 | `localtrans.exe workflow-apply --profile-id meeting-low-latency` |
| `loci-runtime-snapshot` | 查看插件治理下的运行时快照 | `localtrans.exe loci-runtime-snapshot` |
| `loci-governance-snapshot` | 查看插件治理配置快照 | `localtrans.exe loci-governance-snapshot` |
| `loci-workflow-policy` | 查看当前工作流策略解析结果 | `localtrans.exe loci-workflow-policy` |
| `tts-voices` | 列出 TTS 音色 | `localtrans.exe tts-voices --language zh` |
| `tts-config` | 查询 TTS 配置 | `localtrans.exe tts-config` |
| `tts-default-voice` | 查询默认音色 | `localtrans.exe tts-default-voice --language zh` |
| `tts-custom-voices` | 扫描本地/自定义音色模型 | `localtrans.exe tts-custom-voices --models-dir .\models` |
| `config-set` | 设置配置项 | `localtrans.exe config-set --key translationEngine --value nllb` |
| `config-get` | 读取配置项 | `localtrans.exe config-get --key translationEngine` |
| `call` | 通用命令桥 | `localtrans.exe call --name check_mt_runtime --args-json "{}"` |

`session-start --latency-profile` 支持：`low-latency`、`balanced`、`high-accuracy`。

GUI 当前已提供：
- `就绪` 标签页：用于售卖前和启动前检查
- `交付` 标签页：用于版本、产物、支持快照和工作流治理态势确认

## 便携版测试清单

1. 执行 release 打包（`npm run tauri build`）。
2. 启动 GUI（无参数运行 `localtrans.exe`）。
3. 在另一个终端执行 CLI 并观察 GUI 状态同步：
   - `session-start`、`session-pause`、`session-resume`、`session-stop`
4. 校验 MT 运行时：
   - `localtrans.exe mt-runtime-check`
   - `localtrans.exe translate-text --text "你好，我们下午三点开会。" --source-lang zh --target-lang en`
   - `localtrans.exe translate-text --text "Could you share the latest deployment checklist?" --source-lang en --target-lang zh`

## GitHub Actions

工作流：`.github/workflows/build-windows.yml`

当前 CI 会打包 Tauri release 产物。若希望 CI 产物也包含内置 MT 运行时，需要在 `npm run tauri build` 前确保 `src-tauri/resources/mt-runtime` 已存在（例如：仓库已提交，或在前置步骤中用可用 Python 环境生成）。

## 输出约定

- 成功：JSON 输出到 `stdout`
- 失败：JSON 输出到 `stderr`，退出码 `1`

