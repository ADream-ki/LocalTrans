import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import GlassCard from "../../components/GlassCard";
import { useUiStore } from "../../stores/uiStore";
import {
  Download,
  Trash2,
  Check,
  AlertCircle,
  Clock,
  HardDrive,
  RefreshCcw,
  Loader2,
  FolderOpen,
} from "lucide-react";

type ModelType = "asr" | "mt" | "tts" | "loci";

interface BackendModelInfo {
  id: string;
  name: string;
  model_type: string;
  size: string;
  status: string;
  path?: string | null;
  download_url?: string | null;
}

interface Model {
  id: string;
  name: string;
  type: ModelType;
  size: string;
  status: "ready" | "downloading" | "error" | "not_downloaded";
  path?: string;
  downloadUrl?: string;
}

interface ModelDownloadProgressEvent {
  modelId: string;
  modelType: string;
  progress: number;
  status: string;
}

type GuideStatus = "ready" | "partial" | "missing" | "checking";

interface GuideItemState {
  status: GuideStatus;
  detail: string;
}

interface GuideState {
  asr: GuideItemState;
  tts: GuideItemState;
  loci: GuideItemState;
}

const typeLabels: Record<ModelType, string> = {
  asr: "ASR 模型",
  mt: "翻译模型",
  tts: "TTS 模型",
  loci: "Loci 模型",
};

const typeColors: Record<ModelType, string> = {
  asr: "bg-primary/10 text-primary",
  mt: "bg-success/10 text-success",
  tts: "bg-warning/10 text-warning",
  loci: "bg-accent/10 text-accent",
};

const allTypes: ModelType[] = ["asr", "mt", "tts", "loci"];

const defaultGuideState: GuideState = {
  asr: { status: "checking", detail: "检测中..." },
  tts: { status: "checking", detail: "检测中..." },
  loci: { status: "checking", detail: "检测中..." },
};

const normalizeStatus = (status: string): Model["status"] => {
  switch (status) {
    case "ready":
    case "downloading":
    case "error":
    case "not_downloaded":
      return status;
    default:
      return "ready";
  }
};

function ModelPage() {
  const {
    modelOnboardingOpen,
    closeModelOnboarding,
    completeModelOnboarding,
  } = useUiStore();
  const [activeTab, setActiveTab] = useState<ModelType>("asr");
  const [models, setModels] = useState<Model[]>([]);
  const [modelsDir, setModelsDir] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [guideState, setGuideState] = useState<GuideState>(defaultGuideState);
  const [guideLoading, setGuideLoading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<Record<string, number>>({});

  const filteredModels = useMemo(
    () => models.filter((m) => m.type === activeTab),
    [models, activeTab]
  );

  const loadModels = useCallback(async (type: ModelType) => {
    setLoading(true);
    setError(null);

    try {
      const list = await invoke<BackendModelInfo[]>("list_models", {
        model_type: type,
      });

      const mapped: Model[] = list.flatMap((m) => {
        const t = m.model_type as ModelType;
        if (!allTypes.includes(t)) return [];

        const base: Model = {
          id: m.id,
          name: m.name,
          type: t,
          size: m.size,
          status: normalizeStatus(m.status),
          downloadUrl: m.download_url || undefined,
        };

        return [m.path ? { ...base, path: m.path } : base];
      });

      setModels(mapped);
    } catch (e) {
      setModels([]);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    invoke<string>("get_models_dir")
      .then((dir) => setModelsDir(dir))
      .catch(() => {});
  }, []);

  useEffect(() => {
    loadModels(activeTab);
  }, [activeTab, loadModels]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    void listen<ModelDownloadProgressEvent>("model-download-progress", (event) => {
      const payload = event.payload;
      setDownloadProgress((prev) => ({
        ...prev,
        [payload.modelId]: payload.progress,
      }));
      setModels((prev) =>
        prev.map((m) =>
          m.id === payload.modelId
            ? {
                ...m,
                status: payload.status === "completed" ? "ready" : "downloading",
              }
            : m
        )
      );
    }).then((off) => {
      unlisten = off;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleRefresh = () => {
    loadModels(activeTab);
  };

  const handleOpenModelsDir = async () => {
    if (!modelsDir) return;
    try {
      await invoke("open_url", { url: modelsDir });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDownload = async (model: Model) => {
    if (model.downloadUrl) {
      try {
        await invoke("open_url", { url: model.downloadUrl });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
      return;
    }

    setModels((prev) =>
      prev.map((m) => (m.id === model.id ? { ...m, status: "downloading" } : m))
    );
    setDownloadProgress((prev) => ({ ...prev, [model.id]: 1 }));

    try {
      await invoke<string>("download_model", {
        model_id: model.id,
        model_type: model.type,
      });
      await loadModels(activeTab);
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[model.id];
        return next;
      });
    } catch (e) {
      setModels((prev) =>
        prev.map((m) => (m.id === model.id ? { ...m, status: "error" } : m))
      );
      setDownloadProgress((prev) => {
        const next = { ...prev };
        delete next[model.id];
        return next;
      });
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDelete = async (model: Model) => {
    try {
      await invoke("delete_model", { model_id: model.id });
      await loadModels(activeTab);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleGuidedDownload = async (modelId: string, modelType: ModelType) => {
    try {
      await invoke<string>("download_model", {
        model_id: modelId,
        model_type: modelType,
      });
      await refreshGuideStatus();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const refreshGuideStatus = useCallback(async () => {
    setGuideLoading(true);
    try {
      const [asrList, ttsList, lociList] = await Promise.all([
        invoke<BackendModelInfo[]>("list_models", { model_type: "asr" }),
        invoke<BackendModelInfo[]>("list_models", { model_type: "tts" }),
        invoke<BackendModelInfo[]>("list_models", { model_type: "loci" }),
      ]);

      const asrReady = asrList.some((m) => normalizeStatus(m.status) === "ready");
      const sherpaTtsReady = ttsList.some(
        (m) => m.id === "tts:sherpa-melo-local" && normalizeStatus(m.status) === "ready"
      );
      const anyTtsReady = ttsList.some((m) => normalizeStatus(m.status) === "ready");
      const lociReady = lociList.some(
        (m) => m.id.startsWith("loci:") && normalizeStatus(m.status) === "ready"
      );

      setGuideState({
        asr: asrReady
          ? { status: "ready", detail: "已检测到 ASR 可用模型" }
          : { status: "missing", detail: "未检测到 ASR 模型，建议先下载" },
        tts: sherpaTtsReady
          ? { status: "ready", detail: "已检测到 Sherpa Melo 离线音色" }
          : anyTtsReady
            ? { status: "partial", detail: "检测到其他 TTS，建议补齐 Sherpa Melo" }
            : { status: "missing", detail: "未检测到离线 TTS 模型" },
        loci: lociReady
          ? { status: "ready", detail: "已检测到 Loci GGUF 模型" }
          : { status: "missing", detail: "可选增强，未安装也可运行" },
      });
    } catch (e) {
      setGuideState({
        asr: { status: "checking", detail: "检测失败，请稍后刷新" },
        tts: { status: "checking", detail: "检测失败，请稍后刷新" },
        loci: { status: "checking", detail: "检测失败，请稍后刷新" },
      });
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setGuideLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!modelOnboardingOpen) return;
    refreshGuideStatus();
  }, [modelOnboardingOpen, refreshGuideStatus]);

  const getGuideBadge = (state: GuideItemState) => {
    if (state.status === "ready") {
      return (
        <span className="px-s py-xs bg-success/10 text-success text-xs font-medium rounded-small">
          已就绪
        </span>
      );
    }
    if (state.status === "partial") {
      return (
        <span className="px-s py-xs bg-warning/10 text-warning text-xs font-medium rounded-small">
          部分就绪
        </span>
      );
    }
    if (state.status === "missing") {
      return (
        <span className="px-s py-xs bg-error/10 text-error text-xs font-medium rounded-small">
          缺失
        </span>
      );
    }
    return (
      <span className="px-s py-xs bg-bg-tertiary text-text-tertiary text-xs font-medium rounded-small">
        检测中
      </span>
    );
  };

  const getStatusBadge = (model: Model) => {
    switch (model.status) {
      case "ready":
        return (
          <span className="px-s py-xs bg-success/10 text-success text-xs font-medium rounded-small flex items-center gap-xs">
            <Check size={12} />
            已就绪
          </span>
        );
      case "downloading":
        return (
          <span className="px-s py-xs bg-warning/10 text-warning text-xs font-medium rounded-small flex items-center gap-xs">
            <Clock size={12} />
            下载中 {downloadProgress[model.id] ?? 0}%
          </span>
        );
      case "error":
        return (
          <span className="px-s py-xs bg-error/10 text-error text-xs font-medium rounded-small flex items-center gap-xs">
            <AlertCircle size={12} />
            错误
          </span>
        );
      default:
        return (
          <span className="px-s py-xs bg-bg-tertiary text-text-tertiary text-xs font-medium rounded-small">
            未下载
          </span>
        );
    }
  };

  return (
    <div className="h-full flex flex-col p-l">
      {modelOnboardingOpen && (
        <div className="fixed inset-0 z-40 bg-black/40 flex items-center justify-center p-l">
          <div className="w-full max-w-2xl rounded-large border border-bg-tertiary bg-white shadow-xl p-l">
            <div className="text-l font-semibold text-text-primary mb-s">首启引导：推荐模型安装</div>
            <p className="text-s text-text-secondary mb-m">
              产品定位是本地优先、低延时、高准确。建议先准备这 3 类模型。每个按钮都会打开官方下载页。
            </p>
            <div className="mb-m flex items-center justify-end">
              <button
                type="button"
                onClick={refreshGuideStatus}
                className="btn-secondary px-m py-s inline-flex items-center gap-s"
              >
                {guideLoading ? <Loader2 size={14} className="animate-spin" /> : <RefreshCcw size={14} />}
                刷新检测
              </button>
            </div>
            <div className="space-y-s mb-m">
              <div className="flex items-center justify-between p-s rounded-medium bg-bg-secondary/60">
                <div>
                  <div className="text-s font-medium text-text-primary flex items-center gap-s">
                    1) ASR：中文 Paraformer / 英文 Zipformer
                    {getGuideBadge(guideState.asr)}
                  </div>
                  <div className="text-xs text-text-secondary">放到 models/asr，可提升识别稳定性与准确率</div>
                  <div className="text-xs text-text-tertiary mt-xs">{guideState.asr.detail}</div>
                </div>
                <button
                  type="button"
                  onClick={() => handleGuidedDownload("asr:sherpa-multi-zipformer", "asr")}
                  className="btn-primary px-m py-s"
                >
                  下载 ASR
                </button>
              </div>
              <div className="flex items-center justify-between p-s rounded-medium bg-bg-secondary/60">
                <div>
                  <div className="text-s font-medium text-text-primary flex items-center gap-s">
                    2) TTS：Sherpa Melo（离线音色）
                    {getGuideBadge(guideState.tts)}
                  </div>
                  <div className="text-xs text-text-secondary">支持本地男/女声，适合会议同传</div>
                  <div className="text-xs text-text-tertiary mt-xs">{guideState.tts.detail}</div>
                </div>
                <button
                  type="button"
                  onClick={() => handleGuidedDownload("tts:sherpa-melo", "tts")}
                  className="btn-primary px-m py-s"
                >
                  下载 TTS
                </button>
              </div>
              <div className="flex items-center justify-between p-s rounded-medium bg-bg-secondary/60">
                <div>
                  <div className="text-s font-medium text-text-primary flex items-center gap-s">
                    3) 翻译增强：Loci GGUF（可选）
                    {getGuideBadge(guideState.loci)}
                  </div>
                  <div className="text-xs text-text-secondary">默认内置翻译可直接用，Loci 可提升复杂句质量</div>
                  <div className="text-xs text-text-tertiary mt-xs">{guideState.loci.detail}</div>
                </div>
                <button
                  type="button"
                  onClick={() => handleGuidedDownload("loci:qwen2.5-0.5b", "loci")}
                  className="btn-primary px-m py-s"
                >
                  下载 Loci
                </button>
              </div>
            </div>
            <div className="flex items-center justify-between">
              <button
                type="button"
                onClick={handleOpenModelsDir}
                className="btn-secondary px-m py-s"
              >
                打开模型目录
              </button>
              <div className="flex items-center gap-s">
                <button
                  type="button"
                  onClick={closeModelOnboarding}
                  className="btn-secondary px-m py-s"
                >
                  稍后再说
                </button>
                <button
                  type="button"
                  onClick={completeModelOnboarding}
                  className="btn-primary px-m py-s"
                >
                  我已了解
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Tabs */}
      <div className="flex items-center gap-s mb-l">
        {(Object.keys(typeLabels) as ModelType[]).map((type) => (
          <button
            key={type}
            onClick={() => setActiveTab(type)}
            className={`px-l py-s rounded-medium font-medium transition-all duration-fast ${
              activeTab === type
                ? "bg-primary text-white"
                : "text-text-secondary hover:bg-bg-tertiary"
            }`}
          >
            {typeLabels[type]}
          </button>
        ))}
      </div>

      {/* Model List */}
      <GlassCard className="flex-1 p-l overflow-hidden flex flex-col">
        <div className="flex items-center justify-between mb-m">
          <h2 className="text-l font-semibold text-text-primary">{typeLabels[activeTab]}</h2>
          <div className="flex items-center gap-s">
            <button
              onClick={handleRefresh}
              className="p-xs rounded-medium text-text-tertiary hover:bg-bg-tertiary hover:text-text-secondary transition-colors duration-fast"
              title="刷新"
              type="button"
            >
              {loading ? <Loader2 size={16} className="animate-spin" /> : <RefreshCcw size={16} />}
            </button>
            <button
              onClick={handleOpenModelsDir}
              disabled={!modelsDir}
              className="p-xs rounded-medium text-text-tertiary hover:bg-bg-tertiary hover:text-text-secondary transition-colors duration-fast disabled:opacity-40"
              title="打开模型目录"
              type="button"
            >
              <FolderOpen size={16} />
            </button>
            <div className="flex items-center gap-s text-text-secondary text-s">
              <HardDrive size={14} />
              <span className="font-mono">模型目录: {modelsDir || "(检测中...)"}</span>
            </div>
          </div>
        </div>

        {error && (
          <div className="p-m mb-m bg-error/5 rounded-large border border-error/20 text-xs text-text-secondary">
            <div className="flex items-start gap-s">
              <AlertCircle size={16} className="text-error mt-xs flex-shrink-0" />
              <div className="break-words">{error}</div>
            </div>
          </div>
        )}

        <div className="flex-1 overflow-y-auto space-y-m">
          {loading && filteredModels.length === 0 ? (
            <div className="flex-1 flex items-center justify-center text-text-tertiary text-s">
              正在读取模型列表...
            </div>
          ) : filteredModels.length === 0 ? (
            <div className="flex-1 flex items-center justify-center text-text-tertiary text-s">
              暂无已安装模型
            </div>
          ) : (
            filteredModels.map((model) => (
            <div
              key={model.id}
              className="flex items-center justify-between p-m bg-bg-secondary/50 rounded-large"
            >
              <div className="flex items-center gap-m">
                <div
                  className={`w-10 h-10 rounded-medium ${
                    typeColors[model.type]
                  } flex items-center justify-center`}
                >
                  <HardDrive size={20} />
                </div>
                <div>
                  <div className="font-medium text-text-primary">{model.name}</div>
                  <div className="text-xs text-text-secondary">{model.size}</div>
                  {model.status === "downloading" && (
                    <div className="mt-xs w-52 h-1.5 bg-bg-tertiary rounded-full overflow-hidden">
                      <div
                        className="h-full bg-warning transition-all duration-fast"
                        style={{ width: `${downloadProgress[model.id] ?? 0}%` }}
                      />
                    </div>
                  )}
                </div>
              </div>

                <div className="flex items-center gap-m">
                  {getStatusBadge(model)}
                  {model.status === "not_downloaded" && (
                    <button
                    onClick={() => handleDownload(model)}
                      className="p-s rounded-medium bg-primary text-white hover:bg-primary/90 transition-colors duration-fast"
                      type="button"
                    >
                      <Download size={16} />
                    </button>
                  )}
                  {model.status === "ready" && (
                    <button
                    onClick={() => handleDelete(model)}
                      className="p-s rounded-medium text-text-tertiary hover:bg-error/10 hover:text-error transition-colors duration-fast"
                      type="button"
                    >
                      <Trash2 size={16} />
                    </button>
                  )}
                {model.status === "downloading" && (
                  <div className="flex items-center gap-s text-xs text-text-tertiary">
                    <Loader2 size={14} className="animate-spin" />
                    下载中...
                  </div>
                )}
                </div>
              </div>
            ))
          )}
        </div>
      </GlassCard>
    </div>
  );
}

export default ModelPage;
