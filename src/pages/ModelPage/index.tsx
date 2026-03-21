import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import GlassCard from "../../components/GlassCard";
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
  const [activeTab, setActiveTab] = useState<ModelType>("asr");
  const [models, setModels] = useState<Model[]>([]);
  const [modelsDir, setModelsDir] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

    try {
      await invoke<string>("download_model", {
        model_id: model.id,
        model_type: model.type,
      });
      await loadModels(activeTab);
    } catch (e) {
      setModels((prev) =>
        prev.map((m) => (m.id === model.id ? { ...m, status: "error" } : m))
      );
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
            下载中
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
