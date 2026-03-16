# LocalTrans 基于 Loci + QML 的软件架构图

## 1. 定位前提

本架构以前提定位为基础：

**LocalTrans 是一个面向会议与实时沟通场景的跨平台本地 AI 翻译工作台。**

因此该架构默认遵守以下约束：

- 核心主链路是实时语音翻译
- `ASR / TTS / 音频路由` 是产品底座
- `Loci` 负责增强翻译与语言理解能力
- 桌面跨平台优先于移动端扩展
- 引入 `Loci` 后仍必须满足低延迟实时翻译

## 2. 架构目标

目标架构服务于以下需求：

- QML 跨平台桌面界面
- Loci 驱动的高质量 AI 翻译与语言理解能力
- 现有 ASR / TTS / 音频模块复用
- Windows / macOS / Linux 桌面跨平台运行

## 3. 总体架构图

```mermaid
flowchart TB
    subgraph UI["Presentation Layer"]
        Q1["QML Pages"]
        Q2["QML Components"]
        Q3["ViewModels"]
        Q4["Qt Bridge"]
    end

    subgraph APP["Application Layer"]
        A1["SessionService"]
        A2["TranslationService"]
        A3["ModelService"]
        A4["SettingsService"]
        A5["DiagnosticsService"]
    end

    subgraph DOMAIN["Domain Layer"]
        D1["Session Domain"]
        D2["Translation Domain"]
        D3["Audio Domain"]
        D4["Model Domain"]
        D5["Config Domain"]
    end

    subgraph AI["AI Adapter Layer"]
        L1["Loci Adapter"]
        L2["ASR Adapters"]
        L3["MT Adapters"]
        L4["TTS Adapters"]
    end

    subgraph PLATFORM["Platform Layer"]
        P1["Audio Device Service"]
        P2["Audio Routing Service"]
        P3["File System Service"]
        P4["Runtime Capability Service"]
        P5["Packaging/Launcher"]
    end

    subgraph NATIVE["Native Runtime"]
        N1["Loci Dynamic Library"]
        N2["ASR Runtime"]
        N3["TTS Runtime"]
        N4["OS Audio Stack"]
    end

    Q1 --> Q3
    Q2 --> Q3
    Q3 --> Q4
    Q4 --> A1
    Q4 --> A2
    Q4 --> A3
    Q4 --> A4
    Q4 --> A5

    A1 --> D1
    A2 --> D2
    A3 --> D4
    A4 --> D5
    A5 --> D3

    A1 --> L2
    A1 --> L4
    A2 --> L1
    A2 --> L3
    A3 --> L1
    A3 --> L2
    A3 --> L3
    A3 --> L4

    A1 --> P1
    A1 --> P2
    A3 --> P3
    A5 --> P4
    Q4 --> P5

    L1 --> N1
    L2 --> N2
    L3 --> N2
    L4 --> N3
    P1 --> N4
    P2 --> N4
```

## 4. 运行时实时翻译链路

```mermaid
sequenceDiagram
    participant User as User
    participant QML as QML UI
    participant VM as SessionViewModel
    participant SS as SessionService
    participant ASR as ASR Adapter
    participant MT as TranslationService
    participant Loci as Loci Adapter
    participant TTS as TTS Adapter
    participant Audio as Audio Output

    User->>QML: 点击开始翻译
    QML->>VM: startSession()
    VM->>SS: createSession(config)
    SS->>ASR: startStreaming()

    loop 实时音频输入
        ASR-->>SS: transcript(text)
        SS->>MT: translate(text, mode)
        alt Loci增强模式
            MT->>Loci: generateTranslation(prompt, context)
            Loci-->>MT: translatedText
        else 传统MT模式
            MT-->>SS: translatedText
        end
        SS->>TTS: synthesize(translatedText)
        TTS-->>Audio: play(audio)
        SS-->>VM: updateSessionState()
        VM-->>QML: 更新字幕/状态/延迟
    end
```

## 4.1 低延迟混合翻译链路（推荐默认）

```mermaid
sequenceDiagram
    participant ASR as ASR Adapter
    participant TS as TranslationService
    participant FMT as Fast MT
    participant Loci as Loci Adapter
    participant SS as SessionService
    participant TTS as TTS Adapter

    ASR-->>TS: partial/final transcript
    par 快速路径
        TS->>FMT: fastTranslate(text)
        FMT-->>TS: fastResult
    and Loci增强路径
        TS->>Loci: refine(text, fastResult, context, deadline)
        Loci-->>TS: refinedResult or timeout
    end

    alt Loci按时返回
        TS-->>SS: finalResult = refinedResult
    else Loci超时或失败
        TS-->>SS: finalResult = fastResult
    end

    SS->>TTS: synthesize(finalResult)
```

## 5. Loci 集成架构图

```mermaid
flowchart LR
    subgraph Python["Python Runtime"]
        PY1["LociRuntime"]
        PY2["LociEnginePool"]
        PY3["LociMTBackend"]
        PY4["Prompt Builder"]
        PY5["Result Normalizer"]
    end

    subgraph Native["Native Layer"]
        C1["ctypes/cffi Binding"]
        C2["libloci / loci.dll / libloci.dylib"]
    end

    subgraph LociCore["Loci Core"]
        LC1["Model Loader"]
        LC2["Inference Engine"]
        LC3["Device Selector"]
        LC4["Streaming / Generation API"]
    end

    PY3 --> PY4
    PY4 --> PY1
    PY1 --> PY2
    PY2 --> C1
    C1 --> C2
    C2 --> LC1
    C2 --> LC2
    C2 --> LC3
    C2 --> LC4
    LC4 --> PY5
```

## 6. 平台能力分层图

```mermaid
flowchart TB
    subgraph CrossPlatform["Cross-platform Generic Logic"]
        G1["Session Orchestration"]
        G2["Translation Strategy"]
        G3["Settings / Models / Logs"]
        G4["QML UI"]
    end

    subgraph PlatformSpecific["Platform-specific Adapters"]
        W1["Windows Audio Adapter"]
        M1["macOS Audio Adapter"]
        L1["Linux Audio Adapter"]
        W2["Windows Virtual Audio"]
    end

    subgraph OS["Operating System"]
        O1["Windows WASAPI"]
        O2["CoreAudio"]
        O3["PulseAudio / PipeWire / ALSA"]
    end

    G1 --> W1
    G1 --> M1
    G1 --> L1
    G1 --> W2

    W1 --> O1
    W2 --> O1
    M1 --> O2
    L1 --> O3
```

## 7. 模块职责表

| 层级 | 模块 | 主要职责 |
|---|---|---|
| 表现层 | QML Pages / Components | 页面与交互展示 |
| 表现层 | ViewModels | 提供可绑定状态和命令 |
| 应用层 | SessionService | 会话生命周期和实时链路编排 |
| 应用层 | TranslationService | 翻译模式选择、时延预算控制、结果合并与回退 |
| 应用层 | ModelService | 模型状态、下载、路径管理 |
| 应用层 | DiagnosticsService | 环境检测、日志、健康检查 |
| AI 适配层 | Loci Adapter | Loci 模型加载、受约束推理调用与翻译增强 |
| AI 适配层 | ASR / MT / TTS Adapters | 封装现有后端 |
| 平台层 | Audio Device Service | 设备枚举与选择 |
| 平台层 | Audio Routing Service | 输出设备、虚拟设备路由 |
| Native 层 | Loci Dynamic Library | 原生推理能力 |

## 8. 面向产品定位的能力分层

### 8.1 基础实时能力

这些能力支撑“会议与实时沟通场景”的基本可用性：

- 设备枚举
- 音频输入输出
- ASR
- 翻译主链路
- TTS
- 会话编排
- 诊断与日志

### 8.2 Loci 增强能力

这些能力由 `Loci` 提供增强，不替代主链路底座：

- 高质量翻译
- 上下文增强翻译
- 术语控制翻译
- 风格控制
- 会话摘要
- 表达润色

实时模式要求：

- `Loci` 仅以受预算约束的增强方式接入
- 不允许阻塞快速翻译路径
- 超时时必须回退到快速路径结果

### 8.3 暂不纳入第一阶段主路径的能力

- 通用聊天助手
- 自由创作
- 多 Agent 编排
- 移动端统一架构

## 9. 推荐模块边界

### 7.1 QML 不应直接做的事

- 不直接调用 Loci
- 不直接调用 ASR / TTS
- 不直接读写本地配置文件
- 不直接依赖音频设备 API

### 7.2 ViewModel 不应直接做的事

- 不持有具体模型对象
- 不管理底层线程
- 不直接依赖第三方 AI 库

### 7.3 Loci Adapter 不应直接做的事

- 不处理 UI 状态
- 不处理页面跳转
- 不负责系统设备管理
- 不绕过 `TranslationService` 直接决定最终会话输出

### 7.4 TranslationService 必须负责的事

- 统一管理 `deadline/timeout`
- 决定 `fastResult/refinedResult` 的合并策略
- 在 `Loci` 超时时执行无阻塞回退
- 决定交给 `TTS` 的最终文本

## 10. 数据流说明

### 8.1 配置流

QML -> ViewModel -> SettingsService -> Config Domain -> 本地配置文件

### 8.2 翻译流

低延迟实时模式（默认）：

音频输入 -> ASR -> TranslationService -> Fast MT(立即产出) + Loci(并行增强, deadline) -> 合并决策 -> TTS -> 音频输出

高质量模式（可选）：

音频输入 -> ASR -> TranslationService -> `Loci` 主导翻译 -> TTS -> 音频输出

### 8.3 状态流

Native / Service -> ViewModel -> QML 绑定更新

## 11. 推荐实现策略

### 9.1 第一阶段

- 先实现 `Loci Adapter`
- 先保留现有 Python 后端结构
- 不立即迁移所有页面
- 先落地 `fast path + Loci refinement + deadline fallback`

### 9.2 第二阶段

- 新建 `PySide6 + QML` UI
- 建立 `ViewModel + Service` 模型

### 9.3 第三阶段

- 完成三平台桌面适配
- 完成统一打包

## 12. 架构结论

推荐将 `LocalTrans` 重构为：

- `QML` 负责表现层
- `Python` 负责应用服务与流程编排
- `Loci` 负责高质量 AI 翻译与语言理解增强能力
- 现有 `ASR / TTS` 继续作为专用能力模块存在
- 平台相关能力通过适配层隔离
- 实时默认策略采用 `fast path + Loci refinement + deadline fallback`

这样可以在保留现有工程资产的前提下，以最低风险演进到“面向会议与实时沟通场景的跨平台本地 AI 翻译工作台”。
