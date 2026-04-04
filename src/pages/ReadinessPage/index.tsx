import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AlertCircle,
  CheckCircle2,
  Gauge,
  Loader2,
  RefreshCw,
  Shield,
  ShieldAlert,
  Sparkles,
  Wrench,
} from "lucide-react";
import GlassCard from "../../components/GlassCard";
import { useUiStore } from "../../stores/uiStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useSessionStore } from "../../stores/sessionStore";

interface RuntimeComponentStatus {
  ready: boolean;
  path: string;
  message: string;
  action?: string | null;
  engine?: string | null;
}

interface LociWorkflowPolicySnapshot {
  translationEngine: string;
  governanceEnabled: boolean;
  runtimeReady: boolean;
  policyActive: boolean;
  activeWorkflowRewriter?: string | null;
  workflows: string[];
  effectiveAsrEngine?: string | null;
  effectiveTtsEngine?: string | null;
  effectiveTtsEnabled?: boolean | null;
  preferredLatencyProfile?: string | null;
  forceBidirectional?: boolean | null;
  forceTtsAutoPlay?: boolean | null;
  supportsStreaming: boolean;
  supportsVoiceCloning: boolean;
  unresolvedWorkflows: string[];
  statusMessage: string;
}

interface RuntimeStatus {
  modelsDir: string;
  asr: RuntimeComponentStatus;
  translation: RuntimeComponentStatus;
  tts: RuntimeComponentStatus;
  vad: RuntimeComponentStatus;
  ttsEngine: string;
  lociWorkflowPolicy: LociWorkflowPolicySnapshot;
}

interface SessionPreflightItem {
  code: string;
  stage: string;
  severity: string;
  label: string;
  message: string;
  action?: string | null;
}

interface EffectiveRuntimeSummary {
  asrEngine: string;
  translationEngine: string;
  ttsEngine: string;
  ttsEnabled: boolean;
  ttsAutoPlay: boolean;
  bidirectional: boolean;
  latencyProfile?: string | null;
}

interface SessionPreflightStatus {
  canStart: boolean;
  summary: string;
  blockers: SessionPreflightItem[];
  warnings: SessionPreflightItem[];
  effectiveRuntime: EffectiveRuntimeSummary;
  workflowPolicy: LociWorkflowPolicySnapshot;
}

interface MtRuntimeCheck {
  bundledPython?: string | null;
  bundledScript?: string | null;
  bundledArgosPackages?: string | null;
  packageCount: number;
  languagePairs: string[];
  ready: boolean;
  message: string;
}

interface VirtualAudioDriver {
  name: string;
  device_id: string;
  driver_type: string;
}

interface VirtualDriverCheckResult {
  has_virtual_driver: boolean;
  detected_drivers: VirtualAudioDriver[];
  recommendation: string;
  download_url: string | null;
}

interface WorkflowPreset {
  id: string;
  name: string;
  description: string;
  targetUser: string;
  translationEngine: "loci" | "nllb";
  asrEngine: "whisper" | "sensevoice";
  ttsEngine: "sherpa-melo" | "piper";
  ttsEnabled: boolean;
  ttsAutoPlay: boolean;
  bidirectional: boolean;
  lociWorkflowPlugin: string;
}

interface ApplyWorkflowProfileResult {
  profile: WorkflowPreset;
  updatedKeys: string[];
  preflight: SessionPreflightStatus;
  message: string;
}

interface ReadinessCheckItem {
  id: string;
  label: string;
  detail: string;
  ready: boolean;
  critical: boolean;
}

const defaultWorkflowPresets: WorkflowPreset[] = [
  {
    id: "meeting-low-latency",
    name: "低延迟会议模式",
    description: "优先实时性和自动播报，适合口译场景。",
    targetUser: "销售演示 / 会议同传",
    translationEngine: "loci",
    asrEngine: "whisper",
    ttsEngine: "sherpa-melo",
    ttsEnabled: true,
    ttsAutoPlay: true,
    bidirectional: true,
    lociWorkflowPlugin: "localtrans-meeting-low-latency",
  },
  {
    id: "privacy-local-only",
    name: "隐私本地模式",
    description: "强制本地链路，不走在线 TTS。",
    targetUser: "政企内网 / 敏感语音",
    translationEngine: "loci",
    asrEngine: "whisper",
    ttsEngine: "piper",
    ttsEnabled: true,
    ttsAutoPlay: false,
    bidirectional: false,
    lociWorkflowPlugin: "localtrans-privacy-local-only",
  },
  {
    id: "caption-high-accuracy",
    name: "高精度字幕模式",
    description: "优先稳定字幕输出，关闭自动播报。",
    targetUser: "录制转写 / 会后字幕",
    translationEngine: "loci",
    asrEngine: "sensevoice",
    ttsEngine: "piper",
    ttsEnabled: false,
    ttsAutoPlay: false,
    bidirectional: false,
    lociWorkflowPlugin: "localtrans-caption-high-accuracy",
  },
];

function ReadinessPage() {
  const { setActiveTab } = useUiStore();
  const {
    setAsrEngine,
    setTranslationEngine,
    setTtsEngine,
    setTtsEnabled,
    setTtsAutoPlay,
    setLociWorkflowPlugin,
  } = useSettingsStore();
  const { setBidirectional, setAsrEngine: setSessionAsrEngine, setTranslationEngine: setSessionTranslationEngine } =
    useSessionStore();

  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [preflight, setPreflight] = useState<SessionPreflightStatus | null>(null);
  const [mtRuntime, setMtRuntime] = useState<MtRuntimeCheck | null>(null);
  const [virtualDriver, setVirtualDriver] = useState<VirtualDriverCheckResult | null>(null);
  const [workflowPresets, setWorkflowPresets] = useState<WorkflowPreset[]>(defaultWorkflowPresets);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [applyingPresetId, setApplyingPresetId] = useState<string | null>(null);

  const runPrechecks = useCallback(async () => {
    setError(null);
    try {
      const [nextRuntime, nextPreflight, nextMtRuntime, nextDriver, nextProfiles] = await Promise.all([
        invoke<RuntimeStatus>("get_runtime_status"),
        invoke<SessionPreflightStatus>("get_session_preflight"),
        invoke<MtRuntimeCheck>("check_mt_runtime"),
        invoke<VirtualDriverCheckResult>("check_virtual_audio_driver"),
        invoke<WorkflowPreset[]>("list_workflow_profiles").catch(() => defaultWorkflowPresets),
      ]);
      setRuntime(nextRuntime);
      setPreflight(nextPreflight);
      setMtRuntime(nextMtRuntime);
      setVirtualDriver(nextDriver);
      setWorkflowPresets(
        nextProfiles.length > 0 ? nextProfiles : defaultWorkflowPresets
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void runPrechecks();
    const id = window.setInterval(() => {
      void runPrechecks();
    }, 4000);
    return () => window.clearInterval(id);
  }, [runPrechecks]);

  const handlePreflightAction = useCallback(
    (action?: string | null) => {
      switch (action) {
        case "open_model_page":
        case "download_loci_model":
        case "download_tts_model":
          setActiveTab("model");
          break;
        case "prepare_mt_runtime":
          setActiveTab("readiness");
          break;
        case "open_settings_page":
          setActiveTab("settings");
          break;
        case "open_diagnostics_page":
          setActiveTab("diagnostics");
          break;
        default:
          break;
      }
    },
    [setActiveTab]
  );

  const readinessChecks = useMemo<ReadinessCheckItem[]>(() => {
    if (!runtime || !preflight) return [];

    const translationIsLoci = preflight.effectiveRuntime.translationEngine === "loci";
    const ttsRouteReady = !preflight.effectiveRuntime.ttsEnabled || runtime.tts.ready;

    return [
      {
        id: "asr",
        label: "ASR 模型就绪",
        detail: runtime.asr.message,
        ready: runtime.asr.ready,
        critical: true,
      },
      {
        id: "translation",
        label: "翻译链路可用",
        detail: runtime.translation.message,
        ready: runtime.translation.ready,
        critical: true,
      },
      {
        id: "tts",
        label: "TTS 路由稳定",
        detail: preflight.effectiveRuntime.ttsEnabled ? runtime.tts.message : "已由治理策略关闭自动播报",
        ready: ttsRouteReady,
        critical: true,
      },
      {
        id: "workflow",
        label: "Loci Workflow 治理",
        detail: preflight.workflowPolicy.statusMessage,
        ready: translationIsLoci ? preflight.workflowPolicy.policyActive : true,
        critical: translationIsLoci,
      },
      {
        id: "mt-runtime",
        label: "确定性 MT 运行时",
        detail: mtRuntime?.message || "尚未检测",
        ready: translationIsLoci ? true : !!mtRuntime?.ready,
        critical: !translationIsLoci,
      },
      {
        id: "privacy",
        label: "隐私风险可控",
        detail:
          preflight.effectiveRuntime.ttsEngine === "edge-tts"
            ? "当前 TTS 会联网（Edge TTS）"
            : "当前语音链路可保持本地处理",
        ready: preflight.effectiveRuntime.ttsEngine !== "edge-tts",
        critical: false,
      },
      {
        id: "virtual-driver",
        label: "虚拟音频驱动",
        detail: virtualDriver?.recommendation || "尚未检测",
        ready: !!virtualDriver?.has_virtual_driver,
        critical: false,
      },
    ];
  }, [mtRuntime, preflight, runtime, virtualDriver]);

  const readinessScore = useMemo(() => {
    if (readinessChecks.length === 0) return 0;
    const totalWeight = readinessChecks.reduce((acc, item) => acc + (item.critical ? 2 : 1), 0);
    const gotWeight = readinessChecks.reduce(
      (acc, item) => acc + (item.ready ? (item.critical ? 2 : 1) : 0),
      0
    );
    return Math.round((gotWeight / Math.max(totalWeight, 1)) * 100);
  }, [readinessChecks]);

  const readinessLabel = useMemo(() => {
    if (!preflight) return "检测中";
    if (!preflight.canStart) return "阻塞";
    if (readinessScore >= 85) return "可销售演示";
    if (readinessScore >= 70) return "可内部试运行";
    return "需继续完善";
  }, [preflight, readinessScore]);

  const handleApplyPreset = useCallback(
    async (preset: WorkflowPreset) => {
      setApplyingPresetId(preset.id);
      setError(null);
      try {
        const result = await invoke<ApplyWorkflowProfileResult>("apply_workflow_profile", {
          request: { profileId: preset.id },
        });
        const applied = result.profile;

        setTranslationEngine(applied.translationEngine);
        setAsrEngine(applied.asrEngine);
        setTtsEngine(applied.ttsEngine);
        setTtsEnabled(applied.ttsEnabled);
        setTtsAutoPlay(applied.ttsAutoPlay);
        setLociWorkflowPlugin(applied.lociWorkflowPlugin);
        setBidirectional(applied.bidirectional);
        setSessionAsrEngine(applied.asrEngine);
        setSessionTranslationEngine(applied.translationEngine);
        setPreflight(result.preflight);

        setRefreshing(true);
        await runPrechecks();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setApplyingPresetId(null);
      }
    },
    [
      runPrechecks,
      setAsrEngine,
      setBidirectional,
      setLociWorkflowPlugin,
      setSessionAsrEngine,
      setSessionTranslationEngine,
      setTranslationEngine,
      setTtsAutoPlay,
      setTtsEnabled,
      setTtsEngine,
    ]
  );

  return (
    <div className="h-full overflow-y-auto p-l">
      <div className="max-w-5xl mx-auto space-y-l">
        <div className="flex items-center justify-between gap-m">
          <div>
            <h1 className="text-xl font-semibold text-text-primary">系统就绪中心</h1>
            <p className="text-sm text-text-secondary mt-xs">
              面向交付和售卖的启动前检查面板，覆盖模型、治理、隐私和运行时完整性。
            </p>
          </div>
          <button
            type="button"
            onClick={() => {
              setRefreshing(true);
              void runPrechecks();
            }}
            className="btn-secondary inline-flex items-center gap-s"
            disabled={refreshing}
          >
            {refreshing ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
            重新检测
          </button>
        </div>

        <GlassCard className="p-l">
          <div className="flex items-center justify-between gap-m">
            <div className="flex items-center gap-s">
              {preflight?.canStart ? (
                <CheckCircle2 size={20} className="text-success" />
              ) : (
                <ShieldAlert size={20} className="text-error" />
              )}
              <div>
                <div className="text-m font-medium text-text-primary">{readinessLabel}</div>
                <div className="text-xs text-text-secondary">
                  {preflight?.summary || "正在检测当前运行时状态"}
                </div>
              </div>
            </div>
            <div className="text-right">
              <div className="text-xs text-text-tertiary">Readiness Score</div>
              <div className="text-2xl font-semibold text-text-primary">{readinessScore}</div>
            </div>
          </div>
        </GlassCard>

        {error && (
          <GlassCard className="p-m border border-error/20 bg-error/5">
            <div className="text-sm text-error break-words">{error}</div>
          </GlassCard>
        )}

        <div className="grid grid-cols-2 gap-m">
          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <Shield size={16} className="text-primary" />
              启动阻塞项
            </h2>
            {loading ? (
              <div className="text-sm text-text-secondary">加载中...</div>
            ) : preflight && preflight.blockers.length === 0 ? (
              <div className="text-sm text-success">无阻塞，可直接启动。</div>
            ) : (
              <div className="space-y-s">
                {preflight?.blockers.map((item) => (
                  <div key={item.code} className="rounded-medium border border-error/20 bg-error/5 p-s">
                    <div className="text-sm font-medium text-error">{item.label}</div>
                    <div className="text-xs text-text-secondary mt-xs break-words">{item.message}</div>
                    {item.action && (
                      <button
                        type="button"
                        onClick={() => handlePreflightAction(item.action)}
                        className="mt-s text-xs text-primary hover:underline"
                      >
                        立即处理
                      </button>
                    )}
                  </div>
                ))}
              </div>
            )}
          </GlassCard>

          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <AlertCircle size={16} className="text-warning" />
              风险提醒
            </h2>
            {loading ? (
              <div className="text-sm text-text-secondary">加载中...</div>
            ) : preflight && preflight.warnings.length === 0 ? (
              <div className="text-sm text-success">无高风险警告。</div>
            ) : (
              <div className="space-y-s">
                {preflight?.warnings.map((item) => (
                  <div key={item.code} className="rounded-medium border border-warning/20 bg-warning/5 p-s">
                    <div className="text-sm font-medium text-warning">{item.label}</div>
                    <div className="text-xs text-text-secondary mt-xs break-words">{item.message}</div>
                  </div>
                ))}
              </div>
            )}
          </GlassCard>
        </div>

        <GlassCard className="p-l">
          <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
            <Gauge size={16} className="text-primary" />
            就绪检查清单
          </h2>
          <div className="space-y-s">
            {readinessChecks.map((item) => (
              <div
                key={item.id}
                className={`rounded-medium border p-s ${
                  item.ready
                    ? "border-success/20 bg-success/5"
                    : item.critical
                      ? "border-error/20 bg-error/5"
                      : "border-warning/20 bg-warning/5"
                }`}
              >
                <div className="flex items-center justify-between gap-s">
                  <div className="text-sm font-medium text-text-primary">{item.label}</div>
                  <div className={`text-xs ${item.ready ? "text-success" : item.critical ? "text-error" : "text-warning"}`}>
                    {item.ready ? "PASS" : item.critical ? "BLOCKED" : "RISK"}
                  </div>
                </div>
                <div className="text-xs text-text-secondary mt-xs break-words">{item.detail}</div>
              </div>
            ))}
          </div>
        </GlassCard>

        <GlassCard className="p-l">
          <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
            <Sparkles size={16} className="text-primary" />
            一键场景预设
          </h2>
          <div className="grid grid-cols-3 gap-m">
            {workflowPresets.map((preset) => (
              <div key={preset.id} className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
                <div className="text-sm font-medium text-text-primary">{preset.name}</div>
                <div className="text-xs text-text-secondary mt-xs">{preset.description}</div>
                <div className="text-[11px] text-text-tertiary mt-s">{preset.targetUser}</div>
                <div className="text-[11px] font-mono text-text-tertiary mt-s break-words">
                  plugin={preset.lociWorkflowPlugin}
                </div>
                <button
                  type="button"
                  onClick={() => void handleApplyPreset(preset)}
                  className="mt-m w-full px-m py-s rounded-medium bg-primary text-white text-sm hover:bg-primary/90 transition-colors disabled:opacity-50 inline-flex items-center justify-center gap-xs"
                  disabled={applyingPresetId !== null}
                >
                  {applyingPresetId === preset.id ? (
                    <>
                      <Loader2 size={14} className="animate-spin" />
                      应用中
                    </>
                  ) : (
                    <>
                      <Wrench size={14} />
                      应用预设
                    </>
                  )}
                </button>
              </div>
            ))}
          </div>
          <div className="mt-m text-xs text-text-tertiary">
            预设通过后端 `apply_workflow_profile` 落盘并回传 preflight，GUI 只负责展示和同步状态。
          </div>
        </GlassCard>

        <GlassCard className="p-l">
          <h2 className="text-m font-semibold text-text-primary mb-s">打包前建议</h2>
          <div className="text-sm text-text-secondary space-y-xs">
            <div>1. 用“系统诊断”完成一次完整链路压测，记录延迟和失败重试行为。</div>
            <div>2. 如果要交付离线版本，确保不选择 `edge-tts` 并验证隐私提示。</div>
            <div>
              3. 机翻兼容模式发版前，执行 `tools/prepare_mt_runtime.ps1` 并复测本页 MT Runtime 状态。
            </div>
          </div>
        </GlassCard>
      </div>
    </div>
  );
}

export default ReadinessPage;
