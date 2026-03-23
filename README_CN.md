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

## CLI 命令矩阵

| 命令 | 作用 | 示例 |
|---|---|---|
| `hello` | 连通性冒烟测试 | `localtrans.exe hello --name Alice` |
| `version` | 输出版本信息 | `localtrans.exe version` |
| `process-file` | 处理输入文件 | `localtrans.exe process-file .\README.md` |
| `download-model` | 下载模型元数据/运行资源 | `localtrans.exe download-model --model-type asr --model-id asr:sherpa-multi-zipformer` |
| `list-models` | 按类型列出已安装模型 | `localtrans.exe list-models --model-type asr` |
| `delete-model` | 按 id 删除模型 | `localtrans.exe delete-model --model-id asr:sherpa-multi-zipformer` |
| `session-start` | 启动实时会话 | `localtrans.exe session-start --source-lang en --target-lang zh` |
| `session-pause` | 暂停实时会话 | `localtrans.exe session-pause` |
| `session-resume` | 恢复实时会话 | `localtrans.exe session-resume` |
| `session-stop` | 停止实时会话 | `localtrans.exe session-stop` |
| `session-status` | 查询会话状态 | `localtrans.exe session-status` |
| `session-stats` | 查询会话指标 | `localtrans.exe session-stats` |
| `session-history` | 查询会话历史 | `localtrans.exe session-history --count 20` |
| `session-clear-history` | 清空历史和指标 | `localtrans.exe session-clear-history` |
| `session-export-history` | 导出历史到文件 | `localtrans.exe session-export-history --output .\history.json` |
| `session-update-languages` | 运行中切换语言方向 | `localtrans.exe session-update-languages --source-lang en --target-lang zh` |
| `translate-text` | 单次文本翻译 | `localtrans.exe translate-text --text "Hello" --source-lang en --target-lang zh` |
| `log-status` | 查询日志状态 | `localtrans.exe log-status` |
| `tts-voices` | 列出 TTS 音色 | `localtrans.exe tts-voices --language zh` |
| `tts-config` | 查询 TTS 配置 | `localtrans.exe tts-config` |
| `tts-default-voice` | 查询语言默认音色 | `localtrans.exe tts-default-voice --language zh` |
| `tts-custom-voices` | 扫描本地/自定义音色模型 | `localtrans.exe tts-custom-voices --models-dir .\models` |
| `config-set` | 设置配置项 | `localtrans.exe config-set --key translationEngine --value nllb` |
| `config-get` | 获取配置项 | `localtrans.exe config-get --key translationEngine` |
| `call` | 通用命令桥（内部 invoke 风格） | `localtrans.exe call --name get_audio_devices --args-json "{}"` |

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
