import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  BadgeInfo,
  CheckCircle2,
  Clipboard,
  FileArchive,
  Gauge,
  LifeBuoy,
  Loader2,
  PackageCheck,
  ServerCog,
  ShieldCheck,
  Speech,
} from "lucide-react";
import GlassCard from "../../components/GlassCard";
import { useUiStore } from "../../stores/uiStore";

interface VersionOutput {
  app: string;
  version: string;
}

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

interface WorkflowProfileDescriptor {
  id: string;
  name: string;
  description: string;
  targetUser: string;
  translationEngine: string;
  asrEngine: string;
  ttsEngine: string;
  ttsEnabled: boolean;
  ttsAutoPlay: boolean;
  bidirectional: boolean;
  latencyProfile: string;
  lociWorkflowPlugin: string;
  workflows: string[];
}

interface LogStatus {
  logDir: string;
  latestLog?: string | null;
  exists: boolean;
}

interface SupportSnapshot {
  capturedAtUtc: string;
  version: VersionOutput;
  runtime: RuntimeStatus;
  preflight: SessionPreflightStatus;
  logStatus: LogStatus;
}

function ReleasePage() {
  const { setActiveTab, openModelOnboarding } = useUiStore();
  const [version, setVersion] = useState<VersionOutput | null>(null);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [preflight, setPreflight] = useState<SessionPreflightStatus | null>(null);
  const [profiles, setProfiles] = useState<WorkflowProfileDescriptor[]>([]);
  const [logStatus, setLogStatus] = useState<LogStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [copying, setCopying] = useState(false);
  const [copyMessage, setCopyMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setError(null);
    try {
      const [snapshot, nextProfiles] =
        await Promise.all([
          invoke<SupportSnapshot>("get_support_snapshot"),
          invoke<WorkflowProfileDescriptor[]>("list_workflow_profiles").catch(() => []),
        ]);
      setVersion(snapshot.version);
      setRuntime(snapshot.runtime);
      setPreflight(snapshot.preflight);
      setProfiles(nextProfiles);
      setLogStatus(snapshot.logStatus);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const supportSnapshot = useMemo(() => {
    if (!version || !runtime || !preflight) return "";
    return [
      `app=${version.app}`,
      `version=${version.version}`,
      `preflight=${preflight.canStart ? "ready" : "blocked"}`,
      `summary=${preflight.summary}`,
      `asr=${preflight.effectiveRuntime.asrEngine}`,
      `translation=${preflight.effectiveRuntime.translationEngine}`,
      `tts=${preflight.effectiveRuntime.ttsEngine}`,
      `tts_enabled=${preflight.effectiveRuntime.ttsEnabled}`,
      `workflow_plugin=${preflight.workflowPolicy.activeWorkflowRewriter || "none"}`,
      `workflow_status=${preflight.workflowPolicy.statusMessage}`,
      `models_dir=${runtime.modelsDir}`,
      `latest_log=${logStatus?.latestLog || "none"}`,
    ].join("\n");
  }, [logStatus?.latestLog, preflight, runtime, version]);

  const workflowSummary = useMemo(() => {
    if (!runtime || !preflight) return [];
    return [
      {
        label: "工作流插件",
        value: preflight.workflowPolicy.activeWorkflowRewriter || "未指定",
      },
      {
        label: "有效 ASR",
        value: preflight.effectiveRuntime.asrEngine,
      },
      {
        label: "有效 TTS",
        value: preflight.effectiveRuntime.ttsEnabled
          ? preflight.effectiveRuntime.ttsEngine
          : "已由治理策略关闭",
      },
      {
        label: "延迟档位",
        value: preflight.effectiveRuntime.latencyProfile || "balanced",
      },
      {
        label: "流式输出",
        value: runtime.lociWorkflowPolicy.supportsStreaming ? "支持" : "未声明",
      },
      {
        label: "音色克隆",
        value: runtime.lociWorkflowPolicy.supportsVoiceCloning ? "支持" : "未声明",
      },
    ];
  }, [preflight, runtime]);

  const deliveryChecks = useMemo(() => {
    if (!runtime || !preflight) return [];
    return [
      {
        label: "运行时完整性",
        value: preflight.canStart ? "可启动" : "存在阻塞",
        healthy: preflight.canStart,
      },
      {
        label: "工作流治理",
        value: preflight.workflowPolicy.policyActive ? "已生效" : "未生效",
        healthy: preflight.workflowPolicy.policyActive,
      },
      {
        label: "离线翻译链路",
        value: runtime.translation.ready ? "已就绪" : runtime.translation.message,
        healthy: runtime.translation.ready,
      },
      {
        label: "日志与支持",
        value: logStatus?.exists ? "日志目录已就绪" : "待首次运行生成",
        healthy: !!logStatus?.exists,
      },
    ];
  }, [logStatus?.exists, preflight, runtime]);

  const handleCopySnapshot = useCallback(async () => {
    if (!supportSnapshot) return;
    setCopying(true);
    try {
      await navigator.clipboard.writeText(supportSnapshot);
      setCopyMessage("支持快照已复制");
      window.setTimeout(() => setCopyMessage(null), 2000);
    } catch (err) {
      setCopyMessage(err instanceof Error ? err.message : String(err));
    } finally {
      setCopying(false);
    }
  }, [supportSnapshot]);

  return (
    <div className="h-full overflow-y-auto p-l">
      <div className="max-w-5xl mx-auto space-y-l">
        <div className="flex items-center justify-between gap-m">
          <div>
            <h1 className="text-xl font-semibold text-text-primary">交付与版本</h1>
            <p className="text-sm text-text-secondary mt-xs">
              给客户交付时需要的版本、产物、校验和支持信息都在这里。
            </p>
          </div>
          <button
            type="button"
            onClick={() => void refresh()}
            className="btn-secondary"
            disabled={loading}
          >
            {loading ? "刷新中" : "刷新状态"}
          </button>
        </div>

        {error && (
          <GlassCard className="p-m border border-error/20 bg-error/5">
            <div className="text-sm text-error break-words">{error}</div>
          </GlassCard>
        )}

        <div className="grid gap-m lg:grid-cols-2">
          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <BadgeInfo size={16} className="text-primary" />
              当前版本
            </h2>
            <div className="space-y-s text-sm">
              <div className="flex items-center justify-between gap-s">
                <span className="text-text-secondary">应用</span>
                <span className="font-mono text-text-primary">{version?.app || "-"}</span>
              </div>
              <div className="flex items-center justify-between gap-s">
                <span className="text-text-secondary">版本</span>
                <span className="font-mono text-text-primary">{version?.version || "-"}</span>
              </div>
              <div className="text-xs text-text-tertiary">
                远端发布 tag 与应用版本应保持一致，例如 `v{version?.version || "x.y.z"}`。
              </div>
            </div>
          </GlassCard>

          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <PackageCheck size={16} className="text-primary" />
              当前交付状态
            </h2>
            <div className="space-y-s text-sm">
              <div className="flex items-center justify-between gap-s">
                <span className="text-text-secondary">会话可启动</span>
                <span className={preflight?.canStart ? "text-success" : "text-error"}>
                  {preflight?.canStart ? "是" : "否"}
                </span>
              </div>
              <div className="flex items-center justify-between gap-s">
                <span className="text-text-secondary">阻塞项</span>
                <span className="font-mono text-text-primary">{preflight?.blockers.length ?? 0}</span>
              </div>
              <div className="flex items-center justify-between gap-s">
                <span className="text-text-secondary">警告项</span>
                <span className="font-mono text-text-primary">{preflight?.warnings.length ?? 0}</span>
              </div>
              <div className="text-xs text-text-tertiary break-words">
                {preflight?.summary || "正在读取预检状态"}
              </div>
            </div>
          </GlassCard>
        </div>

        <GlassCard className="p-l">
          <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
            <FileArchive size={16} className="text-primary" />
            交付产物
          </h2>
          <div className="grid gap-m md:grid-cols-3">
            <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
              <div className="text-sm font-medium text-text-primary">NSIS 安装包</div>
              <div className="text-xs text-text-secondary mt-xs">适合大多数 Windows 终端用户。</div>
            </div>
            <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
              <div className="text-sm font-medium text-text-primary">MSI 安装包</div>
              <div className="text-xs text-text-secondary mt-xs">适合企业软件分发与标准化部署。</div>
            </div>
            <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
              <div className="text-sm font-medium text-text-primary">Portable ZIP</div>
              <div className="text-xs text-text-secondary mt-xs">
                包含运行时 DLL 与 `resources`，终端用户无需安装 LLVM。
              </div>
            </div>
          </div>
          <div className="mt-m text-xs text-text-tertiary">
            CI 会自动把运行时 DLL、`resources/loci-plugins` 和内置 MT runtime 一起打进 Windows 产物，
            终端用户不需要单独安装 LLVM。
          </div>
        </GlassCard>

        <div className="grid gap-m lg:grid-cols-2">
          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <ShieldCheck size={16} className="text-primary" />
              工作流预设
            </h2>
            <div className="space-y-s">
              {profiles.length === 0 ? (
                <div className="text-sm text-text-secondary">未读取到预设列表。</div>
              ) : (
                profiles.map((profile) => (
                  <div key={profile.id} className="rounded-medium border border-bg-tertiary bg-bg-secondary/40 p-s">
                    <div className="flex items-center justify-between gap-s">
                      <div className="text-sm font-medium text-text-primary">{profile.name}</div>
                      <div className="text-[11px] font-mono text-text-tertiary">{profile.id}</div>
                    </div>
                    <div className="text-xs text-text-secondary mt-xs">{profile.description}</div>
                  </div>
                ))
              )}
            </div>
          </GlassCard>

          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <LifeBuoy size={16} className="text-primary" />
              支持快照
            </h2>
            <div className="text-xs text-text-secondary mb-s">
              复制后可直接发给支持或客户实施同事。
            </div>
            <pre className="p-s rounded-medium bg-bg-secondary/50 text-xs text-text-primary whitespace-pre-wrap break-words max-h-56 overflow-y-auto">
              {supportSnapshot || "正在生成支持快照"}
            </pre>
            <div className="mt-m flex items-center gap-s">
              <button
                type="button"
                onClick={() => void handleCopySnapshot()}
                className="btn-secondary inline-flex items-center gap-xs"
                disabled={!supportSnapshot || copying}
              >
                {copying ? <Loader2 size={14} className="animate-spin" /> : <Clipboard size={14} />}
                复制支持快照
              </button>
              {copyMessage && <div className="text-xs text-text-tertiary">{copyMessage}</div>}
            </div>
          </GlassCard>
        </div>

        <div className="grid gap-m lg:grid-cols-2">
          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <ServerCog size={16} className="text-primary" />
              插件化推理治理
            </h2>
            <div className="space-y-s">
              {workflowSummary.map((item) => (
                <div
                  key={item.label}
                  className="flex items-center justify-between gap-s rounded-medium border border-bg-tertiary bg-bg-secondary/40 p-s text-sm"
                >
                  <span className="text-text-secondary">{item.label}</span>
                  <span className="font-mono text-text-primary text-right">{item.value}</span>
                </div>
              ))}
            </div>
            <div className="mt-m text-xs text-text-tertiary">
              当前产品以 `Loci-refactor` 为核心推理引擎锚点，ASR / MT / TTS 均通过可替换适配层接入。
            </div>
          </GlassCard>

          <GlassCard className="p-l">
            <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
              <Gauge size={16} className="text-primary" />
              客户交付检查
            </h2>
            <div className="space-y-s">
              {deliveryChecks.map((item) => (
                <div
                  key={item.label}
                  className={`rounded-medium border p-s ${
                    item.healthy ? "border-success/20 bg-success/5" : "border-warning/20 bg-warning/5"
                  }`}
                >
                  <div className="flex items-center justify-between gap-s">
                    <div className="text-sm font-medium text-text-primary">{item.label}</div>
                    <div className={item.healthy ? "text-xs text-success" : "text-xs text-warning"}>
                      {item.healthy ? "PASS" : "ATTN"}
                    </div>
                  </div>
                  <div className="text-xs text-text-secondary mt-xs break-words">{item.value}</div>
                </div>
              ))}
            </div>
          </GlassCard>
        </div>

        <GlassCard className="p-l">
          <h2 className="text-m font-semibold text-text-primary mb-m flex items-center gap-s">
            <Speech size={16} className="text-primary" />
            引擎路线
          </h2>
          <div className="grid gap-m md:grid-cols-3">
            <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
              <div className="text-sm font-medium text-text-primary">Rust 主线</div>
              <div className="text-xs text-text-secondary mt-xs">
                `qwen3-asr-rs`、`qwen3-tts-rs`、Candle Marian-MT、Whisper/Candle 作为纯 Rust 主力参考。
              </div>
            </div>
            <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
              <div className="text-sm font-medium text-text-primary">Python 补充线</div>
              <div className="text-xs text-text-secondary mt-xs">
                Seamless Communication、speech-to-speech 用作 S2ST benchmark、服务化原型和能力对标。
              </div>
            </div>
            <div className="rounded-large border border-bg-tertiary bg-bg-secondary/40 p-m">
              <div className="text-sm font-medium text-text-primary">插件落地点</div>
              <div className="text-xs text-text-secondary mt-xs">
                真实模型引擎不直接耦合 GUI，统一落在 Loci 工作流声明和运行时适配层，保持 100% 插件化。
              </div>
            </div>
          </div>
        </GlassCard>

        <GlassCard className="p-l">
          <h2 className="text-m font-semibold text-text-primary mb-m">下一步动作</h2>
          <div className="grid gap-s text-sm sm:grid-cols-2 xl:grid-cols-4">
            <button
              type="button"
              onClick={() => setActiveTab("readiness")}
              className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
            >
              就绪检查
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("diagnostics")}
              className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
            >
              系统诊断
            </button>
            <button
              type="button"
              onClick={() => setActiveTab("settings")}
              className="px-m py-s rounded-medium bg-bg-secondary hover:bg-bg-tertiary text-text-primary transition-colors"
            >
              设备与治理
            </button>
            <button
              type="button"
              onClick={() => openModelOnboarding()}
              className="px-m py-s rounded-medium bg-primary text-white hover:bg-primary/90 transition-colors"
            >
              打开快速上手
            </button>
          </div>
          <div className="mt-m flex items-center gap-s text-xs text-text-tertiary">
            <CheckCircle2 size={14} className="text-success" />
            {logStatus?.exists
              ? `日志目录已就绪：${logStatus.logDir}`
              : "日志目录尚未生成，通常会在会话运行后创建。"}
          </div>
        </GlassCard>
      </div>
    </div>
  );
}

export default ReleasePage;
