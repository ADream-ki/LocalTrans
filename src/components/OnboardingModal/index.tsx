import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  ChevronRight,
  Mic,
  Settings2,
  Shield,
  Sparkles,
  Volume2,
} from "lucide-react";
import { useUiStore } from "../../stores/uiStore";

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

interface VirtualDriverCheckResult {
  has_virtual_driver: boolean;
  recommendation: string;
}

function statusTone(ok: boolean) {
  return ok
    ? "bg-success/10 border-success/20 text-success"
    : "bg-warning/10 border-warning/20 text-warning";
}

export default function OnboardingModal() {
  const {
    modelOnboardingOpen,
    closeModelOnboarding,
    completeModelOnboarding,
    setActiveTab,
  } = useUiStore();

  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null);
  const [preflight, setPreflight] = useState<SessionPreflightStatus | null>(null);
  const [virtualDriver, setVirtualDriver] = useState<VirtualDriverCheckResult | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [runtime, nextPreflight, driver] = await Promise.all([
        invoke<RuntimeStatus>("get_runtime_status"),
        invoke<SessionPreflightStatus>("get_session_preflight"),
        invoke<VirtualDriverCheckResult>("check_virtual_audio_driver"),
      ]);
      setRuntimeStatus(runtime);
      setPreflight(nextPreflight);
      setVirtualDriver(driver);
      setLoadError(null);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    }
  }, []);

  useEffect(() => {
    if (!modelOnboardingOpen) return;
    void refresh();
  }, [modelOnboardingOpen, refresh]);

  const primaryBlocker = preflight?.blockers[0];
  const readinessCount = useMemo(() => {
    let ready = 0;
    if (runtimeStatus?.asr.ready) ready += 1;
    if (runtimeStatus?.translation.ready) ready += 1;
    if (runtimeStatus?.tts.ready || preflight?.effectiveRuntime.ttsEnabled === false) ready += 1;
    if (preflight?.workflowPolicy.policyActive || preflight?.effectiveRuntime.translationEngine !== "loci") {
      ready += 1;
    }
    return ready;
  }, [preflight, runtimeStatus]);

  const handleAction = useCallback(
    (action?: string | null) => {
      switch (action) {
        case "open_readiness_page":
          setActiveTab("readiness");
          closeModelOnboarding();
          break;
        case "open_model_page":
        case "download_loci_model":
        case "download_tts_model":
          setActiveTab("model");
          closeModelOnboarding();
          break;
        case "prepare_mt_runtime":
          setActiveTab("readiness");
          closeModelOnboarding();
          break;
        case "open_settings_page":
          setActiveTab("settings");
          closeModelOnboarding();
          break;
        case "open_diagnostics_page":
          setActiveTab("diagnostics");
          closeModelOnboarding();
          break;
        default:
          break;
      }
    },
    [closeModelOnboarding, setActiveTab]
  );

  if (!modelOnboardingOpen) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 bg-black/45 backdrop-blur-sm flex items-center justify-center p-l">
      <div className="w-full max-w-4xl rounded-large border border-bg-tertiary bg-white shadow-2xl overflow-hidden">
        <div className="px-l py-m border-b border-bg-tertiary bg-gradient-to-r from-bg-secondary via-white to-bg-secondary">
          <div className="flex items-start justify-between gap-m">
            <div>
              <div className="inline-flex items-center gap-xs px-s py-xs rounded-full bg-primary/10 text-primary text-xs font-medium mb-s">
                <Sparkles size={12} />
                首次启动向导
              </div>
              <h2 className="text-xl font-semibold text-text-primary">先把产品跑起来</h2>
              <p className="text-sm text-text-secondary mt-xs">
                目标不是让你研究架构，而是 2 分钟内确认这台机器能不能稳定跑实时转译。
              </p>
            </div>
            <div className="text-right">
              <div className="text-xs text-text-tertiary">准备度</div>
              <div className="text-2xl font-semibold text-text-primary">{readinessCount}/4</div>
            </div>
          </div>
        </div>

        <div className="p-l space-y-m max-h-[78vh] overflow-y-auto">
          {loadError && (
            <div className="rounded-large border border-error/20 bg-error/5 px-m py-s text-sm text-error">
              {loadError}
            </div>
          )}

          <div className="grid grid-cols-2 gap-m">
            <section className={`rounded-large border px-m py-m ${statusTone(!!runtimeStatus?.asr.ready)}`}>
              <div className="flex items-center gap-s mb-s">
                <Mic size={16} />
                <div className="font-medium">语音识别</div>
              </div>
              <div className="text-sm text-text-primary">
                {runtimeStatus?.asr.ready ? "ASR 模型已就绪" : "还没有可用的 ASR 模型"}
              </div>
              <div className="text-xs text-text-secondary mt-xs break-words">
                {runtimeStatus?.asr.message || "检测中"}
              </div>
            </section>

            <section
              className={`rounded-large border px-m py-m ${statusTone(
                !!runtimeStatus?.tts.ready || preflight?.effectiveRuntime.ttsEnabled === false
              )}`}
            >
              <div className="flex items-center gap-s mb-s">
                <Volume2 size={16} />
                <div className="font-medium">语音输出</div>
              </div>
              <div className="text-sm text-text-primary">
                {preflight?.effectiveRuntime.ttsEnabled === false
                  ? "自动播报已关闭"
                  : runtimeStatus?.tts.ready
                    ? "TTS 路由已就绪"
                    : "TTS 还不能稳定播报"}
              </div>
              <div className="text-xs text-text-secondary mt-xs break-words">
                {runtimeStatus?.tts.message || "检测中"}
              </div>
            </section>

            <section
              className={`rounded-large border px-m py-m ${statusTone(
                preflight?.effectiveRuntime.translationEngine === "loci"
                  ? !!preflight?.workflowPolicy.policyActive
                  : !!runtimeStatus?.translation.ready
              )}`}
            >
              <div className="flex items-center gap-s mb-s">
                <Settings2 size={16} />
                <div className="font-medium">工作流治理</div>
              </div>
              <div className="text-sm text-text-primary">
                {preflight?.effectiveRuntime.translationEngine === "loci"
                  ? preflight?.workflowPolicy.policyActive
                    ? "Loci workflow 已接管"
                    : "Loci workflow 尚未接管"
                  : "当前走确定性本地机翻"}
              </div>
              <div className="text-xs text-text-secondary mt-xs break-words">
                {preflight?.workflowPolicy.statusMessage || "检测中"}
              </div>
            </section>

            <section
              className={`rounded-large border px-m py-m ${statusTone(!!virtualDriver?.has_virtual_driver)}`}
            >
              <div className="flex items-center gap-s mb-s">
                <Shield size={16} />
                <div className="font-medium">设备与隐私</div>
              </div>
              <div className="text-sm text-text-primary">
                {virtualDriver?.has_virtual_driver ? "虚拟音频驱动已检测到" : "建议准备虚拟音频驱动"}
              </div>
              <div className="text-xs text-text-secondary mt-xs break-words">
                {virtualDriver?.recommendation || "检测中"}
              </div>
            </section>
          </div>

          <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
            <div className="text-sm font-medium text-text-primary mb-s">启动前结论</div>
            <div className="text-sm text-text-secondary">{preflight?.summary || "正在分析当前配置"}</div>
            {primaryBlocker && (
              <div className="mt-s rounded-medium border border-error/20 bg-error/5 px-s py-s text-sm">
                <div className="text-error font-medium">{primaryBlocker.label}</div>
                <div className="text-text-secondary mt-xs">{primaryBlocker.message}</div>
                {primaryBlocker.action && (
                  <button
                    type="button"
                    onClick={() => handleAction(primaryBlocker.action)}
                    className="mt-s inline-flex items-center gap-xs text-primary hover:underline"
                  >
                    去处理
                    <ChevronRight size={14} />
                  </button>
                )}
              </div>
            )}
            {!primaryBlocker && preflight?.warnings[0] && (
              <div className="mt-s rounded-medium border border-warning/20 bg-warning/5 px-s py-s text-sm">
                <div className="text-warning font-medium">{preflight.warnings[0].label}</div>
                <div className="text-text-secondary mt-xs">{preflight.warnings[0].message}</div>
              </div>
            )}
          </div>

          <div className="rounded-large border border-bg-tertiary bg-white p-m">
            <div className="text-sm font-medium text-text-primary mb-s">你现在该做什么</div>
            <div className="grid grid-cols-4 gap-s text-sm">
              <button
                type="button"
                onClick={() => {
                  setActiveTab("readiness");
                  closeModelOnboarding();
                }}
                className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
              >
                去做就绪检查
              </button>
              <button
                type="button"
                onClick={() => {
                  setActiveTab("model");
                  closeModelOnboarding();
                }}
                className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
              >
                去准备模型
              </button>
              <button
                type="button"
                onClick={() => {
                  setActiveTab("settings");
                  closeModelOnboarding();
                }}
                className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
              >
                去调设备和治理
              </button>
              <button
                type="button"
                onClick={() => {
                  setActiveTab("session");
                  closeModelOnboarding();
                }}
                className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
              >
                直接看会话页
              </button>
            </div>
          </div>
        </div>

        <div className="px-l py-m border-t border-bg-tertiary flex items-center justify-between bg-white">
          <button
            type="button"
            onClick={refresh}
            className="px-m py-s rounded-medium bg-bg-secondary text-text-secondary hover:bg-bg-tertiary transition-colors"
          >
            重新检测
          </button>
          <div className="flex items-center gap-s">
            <button
              type="button"
              onClick={closeModelOnboarding}
              className="px-m py-s rounded-medium bg-bg-secondary text-text-secondary hover:bg-bg-tertiary transition-colors"
            >
              稍后处理
            </button>
            <button
              type="button"
              onClick={completeModelOnboarding}
              className="px-m py-s rounded-medium bg-primary text-white hover:bg-primary/90 transition-colors inline-flex items-center gap-xs"
            >
              <CheckCircle2 size={14} />
              我已了解
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
