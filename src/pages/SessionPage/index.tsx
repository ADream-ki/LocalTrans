import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSessionStore } from "../../stores/sessionStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { useUiStore } from "../../stores/uiStore";
import GlassCard from "../../components/GlassCard";
import StatusIndicator from "../../components/StatusIndicator";
import TranslationBubble from "../../components/TranslationBubble";
import {
  Play,
  Square,
  Pause,
  RotateCcw,
  ArrowRight,
  ArrowLeftRight,
  Mic,
  Speaker,
  MonitorUp,
  Shield,
  Volume2,
  Loader2,
  AlertCircle,
} from "lucide-react";

const languages = [
  { code: "zh", name: "中文" },
  { code: "en", name: "English" },
  { code: "ja", name: "日本語" },
  { code: "ko", name: "한국어" },
  { code: "fr", name: "Français" },
  { code: "de", name: "Deutsch" },
  { code: "es", name: "Español" },
  { code: "ru", name: "Русский" },
];

// TTS voice mapping for languages
const defaultTtsVoices: Record<string, string> = {
  zh: "zh-CN-XiaoxiaoNeural",
  en: "en-US-JennyNeural",
  ja: "ja-JP-NanamiNeural",
  ko: "ko-KR-SunHiNeural",
  fr: "fr-FR-DeniseNeural",
  de: "de-DE-KatjaNeural",
  es: "es-ES-ElviraNeural",
  ru: "ru-RU-SvetlanaNeural",
};

type PipelineState =
  | "idle"
  | "initializing"
  | "running"
  | "paused"
  | "stopping"
  | "error";

interface RuntimeComponentStatus {
  ready: boolean;
  path: string;
  message: string;
  action?: string | null;
}

interface RuntimeStatus {
  modelsDir: string;
  asr: RuntimeComponentStatus;
  translation: RuntimeComponentStatus;
  vad: RuntimeComponentStatus;
  lociUnhealthy?: boolean;
  lociUnhealthyRemainingSec?: number;
}

interface PipelineStatsPayload {
  type?: "stats";
  total_audio_duration_ms: number;
  speech_duration_ms: number;
  transcription_count: number;
  translation_count: number;
  average_latency_ms: number;
  asr_average_latency_ms?: number;
  translation_average_latency_ms?: number;
  tts_average_latency_ms?: number;
  timestamp?: string;
}

interface SessionStatusPayload {
  state: "idle" | "initializing" | "running" | "paused" | "stopping";
  label: string;
  is_running: boolean;
}

interface CustomVoiceRequest {
  modelType: string;
  modelPath: string;
  referenceAudio?: string | null;
  referenceText?: string | null;
}

interface TtsRequest {
  text: string;
  voice: string;
  engine?: string;
  rate: number;
  pitch?: number;
  volume?: number;
  outputDevice?: string | null;
  customVoice?: CustomVoiceRequest;
}

interface TtsResult {
  duration_secs: number;
  voice: string;
  success: boolean;
}

interface DirectionalTranslationInfo {
  source_text: string;
  target_text: string;
  source_lang: string;
  target_lang: string;
  confidence: number;
}

function SessionPage() {
  const {
    isRunning,
    status,
    lastError,
    currentSourceText,
    currentTranslatedText,
    history,
    sourceLang,
    targetLang,
    bidirectional,
    translationEngine,
    audioDevices,
    selectedInputDevice,
    selectedPeerInputDevice,
    startSession,
    stopSession,
    pauseSession,
    resumeSession,
    setSourceLang,
    setTargetLang,
    setBidirectional,
    setInputDevice,
    setPeerInputDevice,
    setAudioDevices,
    setTranslationEngine,
  } = useSessionStore();

  const {
    ttsEnabled,
    ttsEngine,
    ttsAutoPlay,
    ttsOutputDevice,
    peerTtsOutputDevice,
    setTtsOutputDevice,
    setPeerTtsOutputDevice,
    streamTranslationIntervalMs,
    streamTranslationMinChars,
    streamTtsIntervalMs,
    streamTtsMinChars,
    setStreamTranslationIntervalMs,
    setStreamTranslationMinChars,
    setStreamTtsIntervalMs,
    setStreamTtsMinChars,
    setTranslationEngine: setSettingsTranslationEngine,
  } = useSettingsStore();

  const { setActiveTab } = useUiStore();

  const [isCompact, setIsCompact] = useState(false);
  const [isSpeaking, setIsSpeaking] = useState(false);
  const lastSpokenTextRef = useRef("");
  const lastStreamSpeakAtRef = useRef(0);
  const [ttsOutputDevices, setTtsOutputDevices] = useState<
    { id: string; name: string; is_default: boolean }[]
  >([]);

  const [runtimeStatus, setRuntimeStatus] = useState<RuntimeStatus | null>(null);
  const [runtimeStatusError, setRuntimeStatusError] = useState<string | null>(null);
  const [pipelineStats, setPipelineStats] = useState<PipelineStatsPayload | null>(null);

  useEffect(() => {
    const handleResize = () => {
      setIsCompact(window.innerWidth < 1080);
    };
    handleResize();
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  // Load runtime/model readiness info
  useEffect(() => {
    let cancelled = false;
    invoke<RuntimeStatus>("get_runtime_status")
      .then((s) => {
        if (!cancelled) {
          setRuntimeStatus(s);
          setRuntimeStatusError(null);
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setRuntimeStatus(null);
          setRuntimeStatusError(e instanceof Error ? e.message : String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // If loci is temporarily unhealthy, auto fallback to nllb to avoid repeated freezes/crashes.
  useEffect(() => {
    if (!runtimeStatus?.lociUnhealthy) return;
    if (translationEngine !== "loci") return;
    setTranslationEngine("nllb");
    setSettingsTranslationEngine("nllb");
  }, [runtimeStatus?.lociUnhealthy, translationEngine, setTranslationEngine, setSettingsTranslationEngine]);

  useEffect(() => {
    let cancelled = false;

    invoke<{ id: string; name: string; is_input: boolean; is_default: boolean }[]>(
      "get_audio_devices"
    )
      .then((devices) => {
        if (cancelled) return;

        const mapped = devices.map((d) => {
          const name = d.name || "";
          const isVirtual = /vb-audio|\bcable\b|voicemeeter|virtual audio|stereo mix/i.test(
            name
          );
          return {
            id: d.id,
            name: d.name,
            isInput: d.is_input,
            isDefault: d.is_default,
            isVirtual,
          };
        });

        setAudioDevices(mapped);

        // Auto-select default input device when not set
        const current = useSessionStore.getState();
        if (!current.selectedInputDevice) {
          const defaultInput =
            mapped.find((x) => x.isInput && x.isDefault) ||
            mapped.find((x) => x.isInput);
          if (defaultInput) {
            setInputDevice(defaultInput.id);
          }
        }
        if (!current.selectedPeerInputDevice) {
          const peerInput =
            mapped.find(
              (x) =>
                x.isInput &&
                x.id !==
                  ((current.selectedInputDevice ||
                    mapped.find((d) => d.isInput && d.isDefault)?.id ||
                    mapped.find((d) => d.isInput)?.id ||
                    "") as string) &&
                x.isVirtual
            ) ||
            mapped.find(
              (x) =>
                x.isInput &&
                x.id !==
                  ((current.selectedInputDevice ||
                    mapped.find((d) => d.isInput && d.isDefault)?.id ||
                    mapped.find((d) => d.isInput)?.id ||
                    "") as string)
            );
          if (peerInput) {
            setPeerInputDevice(peerInput.id);
          }
        }
      })
      .catch((err) => {
        console.error("Failed to load audio devices:", err);
        if (!cancelled) setAudioDevices([]);
      });

    return () => {
      cancelled = true;
    };
  }, [setAudioDevices, setInputDevice, setPeerInputDevice]);

  useEffect(() => {
    let cancelled = false;

    invoke<{ id: string; name: string; is_default: boolean }[]>(
      "get_tts_output_devices"
    )
      .then((devices) => {
        if (!cancelled) setTtsOutputDevices(devices);
      })
      .catch((err) => {
        console.error("Failed to load TTS output devices:", err);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  // Speak text using TTS
  const speakText = useCallback(async (text: string, outputDeviceOverride?: string | null) => {
    const settings = useSettingsStore.getState();
    if (!text.trim() || !settings.ttsEnabled) return;

    const session = useSessionStore.getState();
    const voice =
      settings.ttsVoice ||
      defaultTtsVoices[session.targetLang] ||
      "en-US-JennyNeural";

    const request: TtsRequest = {
      text,
      voice,
      engine: settings.ttsEngine,
      rate: settings.ttsRate,
      volume: settings.ttsVolume,
      outputDevice: outputDeviceOverride ?? settings.ttsOutputDevice,
    };

    if (
      settings.ttsEngine === "custom" &&
      settings.customVoiceEnabled &&
      settings.customVoiceModelPath
    ) {
      request.customVoice = {
        modelType: settings.customVoiceModelType,
        modelPath: settings.customVoiceModelPath,
        referenceAudio: settings.customVoiceReferenceAudio,
        referenceText: settings.customVoiceReferenceText,
      };
    }

    try {
      setIsSpeaking(true);
      lastSpokenTextRef.current = text;

      const result = await invoke<TtsResult>("speak_text", { request });
      if (result.success) {
        console.log(`TTS completed: ${result.duration_secs}s`);
      }
    } catch (error) {
      console.error("TTS error:", error);
    } finally {
      setIsSpeaking(false);
    }
  }, []);

  const mergeIncremental = useCallback((prev: string, next: string) => {
    const p = (prev || "").trim();
    const n = (next || "").trim();
    if (!p) return n;
    if (!n) return p;
    if (n.startsWith(p)) return n;
    if (p.startsWith(n)) return p;
    let overlap = 0;
    const max = Math.min(p.length, n.length);
    for (let i = max; i >= 1; i -= 1) {
      if (p.slice(-i) === n.slice(0, i)) {
        overlap = i;
        break;
      }
    }
    return overlap > 0 ? `${p}${n.slice(overlap)}` : `${p} ${n}`;
  }, []);

  // Listen for backend events (pipeline + TTS)
  useEffect(() => {
    const unlisten: Array<Promise<() => void>> = [];

    unlisten.push(
      listen("tts:started", () => setIsSpeaking(true)),
      listen("tts:finished", () => setIsSpeaking(false)),

      listen<{
        type: "stateChanged";
        old_state: PipelineState;
        new_state: PipelineState;
        reason?: string | null;
        timestamp: string;
      }>("pipeline:state-changed", (event) => {
        useSessionStore.getState().setPipelineStatus(event.payload.new_state);
      }),

      listen<{
        type: "partialTranscription";
        text: string;
        language: string;
        confidence: number;
        timestamp: string;
      }>("pipeline:partial-transcription", (event) => {
        const st = useSessionStore.getState();
        st.setCurrentSourceText(mergeIncremental(st.currentSourceText, event.payload.text));
        st.setIsProcessing(true);
      }),

      listen<{
        type: "finalTranscription";
        text: string;
        segments: Array<{ id: string; start_ms: number; end_ms: number; text: string; confidence: number; speaker_id?: string | null }>;
        language: string;
        confidence: number;
        timestamp: string;
      }>("pipeline:final-transcription", (event) => {
        const st = useSessionStore.getState();
        st.setCurrentSourceText(mergeIncremental(st.currentSourceText, event.payload.text));
        st.setIsProcessing(true);
      }),

      listen<{
        type: "translation";
        id: string;
        source_text: string;
        target_text: string;
        source_lang: string;
        target_lang: string;
        confidence: number;
        timestamp: string;
      }>("pipeline:translation", (event) => {
        const p = event.payload;
        const isStreaming = !!p.id && p.id.startsWith("stream-");
        const ts = new Date(p.timestamp).toLocaleTimeString("zh-CN", {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        });

        const st = useSessionStore.getState();
        const mergedSource = isStreaming
          ? mergeIncremental(st.currentSourceText, p.source_text)
          : p.source_text;
        const mergedTarget = isStreaming
          ? mergeIncremental(st.currentTranslatedText, p.target_text)
          : p.target_text;
        st.setCurrentTexts(mergedSource, mergedTarget);
        st.setIsProcessing(isStreaming);
        if (!isStreaming) {
          st.addToHistory({
            id: p.id,
            sourceText: mergedSource,
            translatedText: mergedTarget,
            sourceLang: p.source_lang,
            targetLang: p.target_lang,
            timestamp: ts,
            lociEnhanced: st.translationEngine === "loci",
          });
        }

        const s = useSettingsStore.getState();
        const now = Date.now();
        const streamMinChars = Math.max(2, s.streamTtsMinChars ?? 12);
        const streamInterval = Math.max(500, s.streamTtsIntervalMs ?? 1500);
        const streamSpeakReady =
          !isStreaming ||
          ((/[。！？.!?]$/.test(p.target_text) || p.target_text.length >= streamMinChars) &&
            now - lastStreamSpeakAtRef.current >= streamInterval);
        if (
          s.ttsEnabled &&
          s.ttsAutoPlay &&
          streamSpeakReady &&
          mergedTarget &&
          mergedTarget !== lastSpokenTextRef.current
        ) {
          const forwardOutput = s.peerTtsOutputDevice ?? s.ttsOutputDevice;
          if (isStreaming) lastStreamSpeakAtRef.current = now;
          speakText(mergedTarget, forwardOutput);
        }
      }),

      listen<{
        type: "bidirectionalTranslation";
        forward?: DirectionalTranslationInfo | null;
        backward?: DirectionalTranslationInfo | null;
        timestamp: string;
      }>("pipeline:bidirectional-translation", (event) => {
        const p = event.payload;
        const s = useSettingsStore.getState();
        if (!s.ttsEnabled || !s.ttsAutoPlay) return;
        if (p.backward?.target_text && p.backward.target_text !== lastSpokenTextRef.current) {
          speakText(p.backward.target_text, s.ttsOutputDevice);
        }
      }),

      listen<{
        type: "error";
        code: string;
        message: string;
        recoverable: boolean;
        timestamp: string;
      }>("pipeline:error", (event) => {
        const p = event.payload;
        useSessionStore.getState().setLastError(`${p.code}: ${p.message}`);

        // If translation fails, stop showing "processing" state.
        if (p.code === "TRANSLATION_MODEL_NOT_LOADED" || p.code === "TRANSLATION_FAILED") {
          useSessionStore.getState().setIsProcessing(false);
        }
      })
      ,
      listen<PipelineStatsPayload>("pipeline:stats", (event) => {
        setPipelineStats(event.payload);
      })
    );

    return () => {
      unlisten.forEach((p) => {
        p.then((fn) => fn()).catch(() => {});
      });
    };
  }, [mergeIncremental, speakText]);

  // Poll session stats so CLI worker sessions also show latency in GUI.
  useEffect(() => {
    let stopped = false;
    const tick = async () => {
      try {
        const stats = await invoke<PipelineStatsPayload>("get_session_stats");
        if (!stopped) setPipelineStats(stats);
      } catch {
        // ignore polling errors
      }
    };
    void tick();
    const id = window.setInterval(() => {
      void tick();
    }, 1200);
    return () => {
      stopped = true;
      window.clearInterval(id);
    };
  }, []);

  // Cross-process sync: keep GUI status in sync when CLI changes session state.
  useEffect(() => {
    let stopped = false;

    const syncStatus = async () => {
      try {
        const payload = await invoke<SessionStatusPayload>("get_session_status");
        if (stopped) return;
        useSessionStore.getState().setPipelineStatus(payload.state, null);
      } catch {
        // ignore polling errors
      }
    };

    void syncStatus();
    const id = window.setInterval(() => {
      void syncStatus();
    }, 900);

    return () => {
      stopped = true;
      window.clearInterval(id);
    };
  }, []);

  // Stop TTS
  const stopSpeaking = useCallback(async () => {
    try {
      await invoke("stop_tts");
      setIsSpeaking(false);
    } catch (error) {
      console.error("Stop TTS error:", error);
    }
  }, []);

  const handleSwapLanguages = () => {
    const temp = sourceLang;
    setSourceLang(targetLang);
    setTargetLang(temp);
  };

  const handlePresetZhToEn = () => {
    setSourceLang("zh");
    setTargetLang("en");
  };

  const handlePresetEnToZh = () => {
    setSourceLang("en");
    setTargetLang("zh");
  };

  const handlePlayTranslation = () => {
    if (currentTranslatedText) {
      speakText(currentTranslatedText);
    }
  };

  const statusIndicator = (() => {
    const indicatorStatus =
      status === "error"
        ? "error"
        : status === "initializing" || status === "stopping"
          ? "loading"
          : status === "running" || status === "paused"
            ? "running"
            : "stopped";

    const label =
      status === "running"
        ? "实时转译中"
        : status === "paused"
          ? "已暂停"
          : status === "initializing"
            ? "启动中"
            : status === "stopping"
              ? "停止中"
              : status === "error"
                ? "发生错误"
                : "已停止";

    return { indicatorStatus, label } as const;
  })();

  return (
    <div className="flex h-full">
      {/* Left Panel - Controls */}
      <div
        className={`${
          isCompact ? "hidden" : "w-[296px]"
        } flex flex-col p-l gap-l border-r border-bg-tertiary bg-white/30 overflow-y-auto`}
      >
        {/* Status */}
        <GlassCard className="p-m">
          <div className="space-y-xs">
            <StatusIndicator
              status={statusIndicator.indicatorStatus}
              label={statusIndicator.label}
            />
            {pipelineStats && (
              <div className="text-xs text-text-tertiary flex flex-wrap gap-s">
                <span className="font-mono">平均延时≈{Math.round(pipelineStats.average_latency_ms)}ms</span>
                {pipelineStats.asr_average_latency_ms !== undefined && (
                  <span className="font-mono">ASR≈{Math.round(pipelineStats.asr_average_latency_ms)}ms</span>
                )}
                {pipelineStats.translation_average_latency_ms !== undefined && pipelineStats.translation_average_latency_ms > 0 && (
                  <span className="font-mono">翻译≈{Math.round(pipelineStats.translation_average_latency_ms)}ms</span>
                )}
                {pipelineStats.tts_average_latency_ms !== undefined && pipelineStats.tts_average_latency_ms > 0 && (
                  <span className="font-mono">TTS≈{Math.round(pipelineStats.tts_average_latency_ms)}ms</span>
                )}
                <span className="font-mono">转写:{pipelineStats.transcription_count}</span>
                <span className="font-mono">翻译:{pipelineStats.translation_count}</span>
              </div>
            )}
          </div>
        </GlassCard>

        <GlassCard className="p-m">
          <div className="text-xs font-medium text-text-primary mb-s">实时参数</div>
          <div className="grid grid-cols-1 gap-s text-xs">
            <label className="flex items-center justify-between gap-s">
              <span className="text-text-secondary">流式翻译间隔(ms)</span>
              <input
                type="number"
                min={300}
                max={5000}
                value={streamTranslationIntervalMs}
                onChange={(e) => setStreamTranslationIntervalMs(Number(e.target.value || 900))}
                className="w-24 px-xs py-[4px] rounded border border-bg-tertiary bg-white text-text-primary"
              />
            </label>
            <label className="flex items-center justify-between gap-s">
              <span className="text-text-secondary">流式翻译最小字数</span>
              <input
                type="number"
                min={2}
                max={64}
                value={streamTranslationMinChars}
                onChange={(e) => setStreamTranslationMinChars(Number(e.target.value || 8))}
                className="w-24 px-xs py-[4px] rounded border border-bg-tertiary bg-white text-text-primary"
              />
            </label>
            <label className="flex items-center justify-between gap-s">
              <span className="text-text-secondary">流式播报间隔(ms)</span>
              <input
                type="number"
                min={500}
                max={10000}
                value={streamTtsIntervalMs}
                onChange={(e) => setStreamTtsIntervalMs(Number(e.target.value || 1500))}
                className="w-24 px-xs py-[4px] rounded border border-bg-tertiary bg-white text-text-primary"
              />
            </label>
            <label className="flex items-center justify-between gap-s">
              <span className="text-text-secondary">流式播报最小字数</span>
              <input
                type="number"
                min={2}
                max={128}
                value={streamTtsMinChars}
                onChange={(e) => setStreamTtsMinChars(Number(e.target.value || 12))}
                className="w-24 px-xs py-[4px] rounded border border-bg-tertiary bg-white text-text-primary"
              />
            </label>
          </div>
        </GlassCard>

        {lastError && (
          <div className="flex items-start gap-s p-m bg-error/5 rounded-large border border-error/20">
            <AlertCircle size={16} className="text-error mt-xs flex-shrink-0" />
            <p className="text-xs text-text-secondary leading-relaxed break-words">
              {lastError}
            </p>
          </div>
        )}

        {(runtimeStatusError || (runtimeStatus && (!runtimeStatus.asr.ready || !runtimeStatus.translation.ready))) && (
          <div className="flex items-start gap-s p-m bg-warning/5 rounded-large border border-warning/20">
            <AlertCircle size={16} className="text-warning mt-xs flex-shrink-0" />
            <div className="flex-1">
              <div className="text-xs font-medium text-text-primary">模型未就绪</div>
              {runtimeStatusError ? (
                <div className="text-xs text-text-secondary mt-xs break-words">{runtimeStatusError}</div>
              ) : (
                <div className="mt-xs space-y-xs">
                  {!runtimeStatus?.asr.ready && (
                    <div className="text-xs text-text-secondary break-words">
                      ASR: {runtimeStatus?.asr.message}（{runtimeStatus?.asr.path}）
                    </div>
                  )}
                  {!runtimeStatus?.translation.ready && (
                    <div className="text-xs text-text-secondary break-words">
                      翻译: {runtimeStatus?.translation.message}（{runtimeStatus?.translation.path}）
                    </div>
                  )}
                </div>
              )}

              {runtimeStatus?.modelsDir && (
                <div className="mt-s flex gap-s">
                  <button
                    type="button"
                    onClick={() => setActiveTab("model")}
                    className="px-s py-xs rounded-medium bg-primary text-white text-xs font-medium hover:bg-primary/90 transition-colors"
                  >
                    打开模型页
                  </button>
                  <button
                    type="button"
                    onClick={() => invoke("open_url", { url: runtimeStatus.modelsDir }).catch(() => {})}
                    className="px-s py-xs rounded-medium bg-bg-tertiary text-text-secondary text-xs hover:bg-bg-hover transition-colors"
                  >
                    打开模型目录
                  </button>
                </div>
              )}
            </div>
          </div>
        )}

        {runtimeStatus?.lociUnhealthy && (
          <div className="flex items-start gap-s p-m bg-warning/5 rounded-large border border-warning/20">
            <AlertCircle size={16} className="text-warning mt-xs flex-shrink-0" />
            <div className="text-xs text-text-secondary leading-relaxed">
              Loci 当前处于保护状态，已自动回退 NLLB。
              {runtimeStatus.lociUnhealthyRemainingSec !== undefined && runtimeStatus.lociUnhealthyRemainingSec > 0 && (
                <span className="ml-xs font-mono">
                  预计 {runtimeStatus.lociUnhealthyRemainingSec}s 后可重试。
                </span>
              )}
            </div>
          </div>
        )}

        {/* Audio Routing */}
        <GlassCard className="p-m">
          <h3 className="text-m font-medium text-text-primary mb-m flex items-center gap-s">
            <MonitorUp size={16} className="text-primary" />
            音频路由
          </h3>
          <div className="space-y-s">
            <div>
              <label className="text-xs text-text-secondary block mb-xs">本端输入设备</label>
              <select
                value={selectedInputDevice || ""}
                onChange={(e) => setInputDevice(e.target.value || null)}
                className="select-field text-s"
              >
                <option value="">选择输入设备</option>
                {audioDevices
                  .filter((d) => d.isInput)
                  .map((device) => (
                    <option key={device.id} value={device.id}>
                      {device.name}
                      {device.isDefault && " (默认)"}
                    </option>
                  ))}
              </select>
            </div>
            <div>
              <label className="text-xs text-text-secondary block mb-xs">对端输入设备（双向）</label>
              <select
                value={selectedPeerInputDevice || ""}
                onChange={(e) => setPeerInputDevice(e.target.value || null)}
                className="select-field text-s"
              >
                <option value="">选择对端输入设备（可留空）</option>
                {audioDevices
                  .filter((d) => d.isInput)
                  .map((device) => (
                    <option key={device.id} value={device.id}>
                      {device.name}
                      {device.isVirtual && " (虚拟)"}
                    </option>
                  ))}
              </select>
            </div>
            <div className="flex items-center justify-center text-text-tertiary">
              <ArrowRight size={20} />
            </div>
            <div>
              <label className="text-xs text-text-secondary block mb-xs">本端输出设备</label>
              <select
                className="select-field text-s"
                value={ttsOutputDevice || ""}
                onChange={(e) => setTtsOutputDevice(e.target.value || null)}
              >
                <option value="">系统默认扬声器</option>
                {ttsOutputDevices.map((device) => (
                  <option key={device.id} value={device.id}>
                    {device.name}
                    {device.is_default && " (默认)"}
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="text-xs text-text-secondary block mb-xs">对端输出设备（双向）</label>
              <select
                className="select-field text-s"
                value={peerTtsOutputDevice || ""}
                onChange={(e) => setPeerTtsOutputDevice(e.target.value || null)}
              >
                <option value="">沿用本端输出设备</option>
                {ttsOutputDevices.map((device) => (
                  <option key={device.id} value={device.id}>
                    {device.name}
                    {device.is_default && " (默认)"}
                  </option>
                ))}
              </select>
            </div>
          </div>
        </GlassCard>

        {/* ASR Engine */}
        <GlassCard className="p-m">
          <h3 className="text-m font-medium text-text-primary mb-m flex items-center gap-s">
            <Mic size={16} className="text-primary" />
            ASR 引擎
          </h3>
          <div className="text-s text-text-secondary">
            Sherpa ZipFormer (离线)
            {runtimeStatus?.asr?.ready ? (
              <span className="ml-s text-xs text-success">已就绪</span>
            ) : (
              <span className="ml-s text-xs text-warning">未就绪</span>
            )}
          </div>
          {runtimeStatus?.asr?.path && (
            <div className="mt-xs text-xs font-mono text-text-tertiary break-words">
              {runtimeStatus.asr.path}
            </div>
          )}
        </GlassCard>

        {/* Translation Direction */}
        <GlassCard className="p-m">
          <h3 className="text-m font-medium text-text-primary mb-m flex items-center gap-s">
            <ArrowLeftRight size={16} className="text-primary" />
            翻译方向
          </h3>
          <div className="flex items-center gap-s">
            <select
              value={sourceLang}
              onChange={(e) => setSourceLang(e.target.value)}
              disabled={isRunning}
              className="select-field text-s flex-1"
            >
              {languages.map((lang) => (
                <option key={lang.code} value={lang.code}>
                  {lang.name}
                </option>
              ))}
            </select>
            <button
              onClick={handleSwapLanguages}
              disabled={isRunning}
              className="p-s rounded-medium hover:bg-bg-tertiary transition-colors duration-fast"
            >
              <ArrowLeftRight size={16} className="text-primary" />
            </button>
            <select
              value={targetLang}
              onChange={(e) => setTargetLang(e.target.value)}
              disabled={isRunning}
              className="select-field text-s flex-1"
            >
              {languages.map((lang) => (
                <option key={lang.code} value={lang.code}>
                  {lang.name}
                </option>
              ))}
            </select>
          </div>
          <div className="mt-s flex items-center gap-s">
            <button
              type="button"
              onClick={handlePresetZhToEn}
              disabled={isRunning}
              className="px-s py-xs rounded-medium border border-bg-tertiary bg-bg-secondary text-xs text-text-secondary hover:bg-bg-tertiary disabled:opacity-60 disabled:cursor-not-allowed transition-colors duration-fast"
            >
              中译英
            </button>
            <button
              type="button"
              onClick={handlePresetEnToZh}
              disabled={isRunning}
              className="px-s py-xs rounded-medium border border-bg-tertiary bg-bg-secondary text-xs text-text-secondary hover:bg-bg-tertiary disabled:opacity-60 disabled:cursor-not-allowed transition-colors duration-fast"
            >
              英译中
            </button>
          </div>

          <div className="mt-m flex items-center gap-s">
            <input
              type="checkbox"
              id="bidirectional"
              checked={bidirectional}
              onChange={(e) => setBidirectional(e.target.checked)}
              disabled={isRunning}
              className="w-4 h-4 rounded-small border-bg-tertiary"
            />
            <label htmlFor="bidirectional" className="text-s text-text-secondary">
              双向翻译模式
            </label>
          </div>
        </GlassCard>

        {/* TTS Quick Toggle */}
        <GlassCard className="p-m">
          <h3 className="text-m font-medium text-text-primary mb-m flex items-center gap-s">
            <Volume2 size={16} className="text-primary" />
            语音输出
          </h3>
          <div className="flex items-center justify-between">
            <span className="text-s text-text-secondary">自动朗读翻译</span>
            <button
              onClick={() => {
                useSettingsStore.getState().setTtsAutoPlay(!ttsAutoPlay);
              }}
              className={`relative inline-flex items-center h-6 rounded-full w-11 transition-colors duration-fast ${
                ttsAutoPlay ? "bg-primary" : "bg-bg-tertiary"
              }`}
            >
              <span
                className={`inline-block w-5 h-5 transform bg-white rounded-full transition-transform duration-fast ${
                  ttsAutoPlay ? "translate-x-5" : "translate-x-0.5"
                }`}
              />
            </button>
          </div>
        </GlassCard>

        {/* Privacy Notice */}
        <div
          className={`flex items-start gap-s p-m rounded-large border ${
            ttsEnabled && ttsEngine === "edge-tts"
              ? "bg-warning/5 border-warning/20"
              : "bg-success/5 border-success/20"
          }`}
        >
          <Shield
            size={16}
            className={`${
              ttsEnabled && ttsEngine === "edge-tts" ? "text-warning" : "text-success"
            } mt-xs flex-shrink-0`}
          />
          <p className="text-xs text-text-secondary leading-relaxed">
            {ttsEnabled && ttsEngine === "edge-tts"
              ? "ASR/翻译在本地完成；Edge TTS 为在线服务，会将要合成的文本发送至微软服务获取音频。"
              : "默认不上传音频；ASR/翻译在本地完成。若你配置了自定义音色/外部服务，请以其隐私策略为准。"}
          </p>
        </div>

        {/* Control Buttons */}
        <div className="mt-auto flex gap-s">
          {!isRunning ? (
            <button
              onClick={startSession}
              disabled={runtimeStatus ? !runtimeStatus.asr.ready : false}
              className="btn-primary flex-1 flex items-center justify-center gap-s"
            >
              <Play size={16} />
              {runtimeStatus && !runtimeStatus.asr.ready ? "需要 ASR 模型" : "开始"}
            </button>
          ) : (
            <>
              {status === "paused" ? (
                <button
                  onClick={resumeSession}
                  className="btn-primary flex-1 flex items-center justify-center gap-s"
                >
                  <RotateCcw size={16} />
                  继续
                </button>
              ) : (
                <button
                  onClick={pauseSession}
                  className="btn-secondary flex-1 flex items-center justify-center gap-s"
                >
                  <Pause size={16} />
                  暂停
                </button>
              )}
              <button
                onClick={stopSession}
                className="btn-secondary flex-1 flex items-center justify-center gap-s text-error border-error/30 hover:bg-error/10"
              >
                <Square size={16} />
                停止
              </button>
            </>
          )}
        </div>
      </div>

      {/* Right Panel - Transcription */}
      <div className="flex-1 flex flex-col p-l gap-l overflow-hidden">
        {/* Current Transcription */}
        <GlassCard className="p-l">
          <div className="flex items-center justify-between mb-m">
            <h3 className="text-l font-semibold text-text-primary flex items-center gap-s">
              <Mic size={18} className="text-primary" />
              实时转译
            </h3>
            {isRunning && (
              <div className="flex items-center gap-s">
                <div className="w-1.5 h-1.5 bg-success rounded-full animate-pulse"></div>
                <span className="text-xs text-success">Live</span>
              </div>
            )}
          </div>

          <div className="space-y-s">
            <div className="flex items-center gap-s">
              <span className="px-s py-xs bg-primary/10 text-primary text-xs font-medium rounded-small">
                {languages.find((l) => l.code === sourceLang)?.name || sourceLang}
              </span>
            </div>
            <div className="text-text-primary text-m leading-relaxed min-h-[60px] p-m bg-bg-secondary/50 rounded-medium">
              {currentSourceText || (
                <span className="text-text-tertiary">
                  {isRunning ? "等待语音输入..." : "点击\"开始\"启动实时转译"}
                </span>
              )}
            </div>

            <div className="flex items-center gap-s mt-m">
              <span className="px-s py-xs bg-success/10 text-success text-xs font-medium rounded-small">
                {languages.find((l) => l.code === targetLang)?.name || targetLang}
              </span>
              {translationEngine === "loci" && runtimeStatus?.translation?.ready && (
                <span className="px-s py-xs bg-accent/10 text-accent text-xs font-medium rounded-small flex items-center gap-xs">
                  <span className="w-1.5 h-1.5 bg-accent rounded-full"></span>
                  Loci 增强
                </span>
              )}
              {/* TTS Play Button */}
              {ttsEnabled && currentTranslatedText && (
                <button
                  onClick={isSpeaking ? stopSpeaking : handlePlayTranslation}
                  disabled={!currentTranslatedText}
                  className={`ml-auto p-s rounded-medium transition-colors duration-fast ${
                    isSpeaking
                      ? "bg-primary text-white"
                      : "bg-primary/10 text-primary hover:bg-primary/20"
                  }`}
                  title={isSpeaking ? "停止朗读" : "朗读翻译结果"}
                >
                  {isSpeaking ? (
                    <Loader2 size={14} className="animate-spin" />
                  ) : (
                    <Volume2 size={14} />
                  )}
                </button>
              )}
            </div>
            <div className="text-text-primary text-m leading-relaxed min-h-[60px] p-m bg-bg-secondary/50 rounded-medium">
              {currentTranslatedText || (
                <span className="text-text-tertiary">翻译结果将显示在这里</span>
              )}
            </div>
          </div>
        </GlassCard>

        {/* History */}
        <div className="flex-1 overflow-hidden flex flex-col">
          <h3 className="text-m font-medium text-text-primary mb-m flex items-center gap-s">
            <Speaker size={16} className="text-primary" />
            历史记录
          </h3>
          <div className="flex-1 overflow-y-auto space-y-m pr-s">
            {history.length === 0 ? (
              <div className="flex-1 flex items-center justify-center text-text-tertiary text-s">
                暂无转译记录
              </div>
            ) : (
              history.map((item) => (
                <TranslationBubble
                  key={item.id}
                  sourceText={item.sourceText}
                  translatedText={item.translatedText}
                  sourceLang={item.sourceLang}
                  targetLang={item.targetLang}
                  timestamp={item.timestamp}
                  lociEnhanced={item.lociEnhanced}
                  onSpeak={ttsEnabled ? () => speakText(item.translatedText) : undefined}
                />
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

export default SessionPage;
