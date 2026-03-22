import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "./settingsStore";

export interface TranscriptionItem {
  id: string;
  sourceText: string;
  translatedText: string;
  sourceLang: string;
  targetLang: string;
  timestamp: string;
  lociEnhanced: boolean;
}

export interface AudioDevice {
  id: string;
  name: string;
  isInput: boolean;
  isDefault?: boolean;
  isVirtual?: boolean;
}

export interface SessionState {
  // Status
  isRunning: boolean;
  status: "idle" | "initializing" | "running" | "paused" | "stopping" | "error";
  lastError: string | null;

  // Current transcription
  currentSourceText: string;
  currentTranslatedText: string;
  isProcessing: boolean;

  // History
  history: TranscriptionItem[];

  // Settings
  sourceLang: string;
  targetLang: string;
  bidirectional: boolean;
  selectedInputDevice: string | null;
  selectedPeerInputDevice: string | null;
  selectedOutputDevice: string | null;

  // Audio devices
  audioDevices: AudioDevice[];

  // ASR settings
  asrEngine: "whisper" | "sensevoice" | "vosk";
  asrModelSize: "tiny" | "base" | "small" | "medium" | "large";

  // Translation settings
  translationEngine: "loci" | "nllb" | "m2m";

  // Actions
  startSession: () => Promise<void>;
  stopSession: () => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;

  setPipelineStatus: (status: SessionState["status"], error?: string | null) => void;
  setLastError: (error: string | null) => void;

  setCurrentTexts: (source: string, translated: string) => void;
  setCurrentSourceText: (source: string) => void;
  setCurrentTranslatedText: (translated: string) => void;
  setIsProcessing: (processing: boolean) => void;
  addToHistory: (item: TranscriptionItem) => void;
  clearHistory: () => void;

  setSourceLang: (lang: string) => void;
  setTargetLang: (lang: string) => void;
  setBidirectional: (value: boolean) => void;
  setInputDevice: (deviceId: string | null) => void;
  setPeerInputDevice: (deviceId: string | null) => void;
  setOutputDevice: (deviceId: string | null) => void;

  setAsrEngine: (engine: SessionState["asrEngine"]) => void;
  setAsrModelSize: (size: SessionState["asrModelSize"]) => void;
  setTranslationEngine: (engine: SessionState["translationEngine"]) => void;

  setAudioDevices: (devices: AudioDevice[]) => void;
}

type BackendSessionConfig = {
  sourceLang: string;
  targetLang: string;
  inputDevice: string | null;
  peerInputDevice: string | null;
  bidirectional: boolean;
  lociEnhanced: boolean;
  vadFrameMs?: number | null;
  vadThreshold?: number | null;
  streamTranslationIntervalMs?: number | null;
  streamTranslationMinChars?: number | null;
};

export const useSessionStore = create<SessionState>((set, get) => ({
  // Status
  isRunning: false,
  status: "idle",
  lastError: null,

  // Current transcription
  currentSourceText: "",
  currentTranslatedText: "",
  isProcessing: false,

  // History
  history: [],

  // Settings
  sourceLang: "en",
  targetLang: "zh",
  bidirectional: false,
  selectedInputDevice: null,
  selectedPeerInputDevice: null,
  selectedOutputDevice: null,

  // Audio devices
  audioDevices: [],

  // ASR settings
  asrEngine: "whisper",
  asrModelSize: "base",

  // Translation settings
  translationEngine: "loci",

  // Actions
  setPipelineStatus: (status, error) => {
    const isRunning = status === "initializing" || status === "running" || status === "paused";
    set((prev) => ({
      status,
      isRunning,
      lastError: error === undefined ? prev.lastError : error,
    }));
  },

  setLastError: (error) => set({ lastError: error }),

  startSession: async () => {
    const state = get();
    const settings = useSettingsStore.getState();
    const config: BackendSessionConfig = {
      sourceLang: state.sourceLang,
      targetLang: state.targetLang,
      inputDevice: state.selectedInputDevice,
      peerInputDevice: state.selectedPeerInputDevice,
      bidirectional: state.bidirectional,
      lociEnhanced: state.translationEngine === "loci",
      vadFrameMs: settings.chunkSize,
      streamTranslationIntervalMs: settings.streamTranslationIntervalMs,
      streamTranslationMinChars: settings.streamTranslationMinChars,
    };

    set({ status: "initializing", isRunning: true, lastError: null });
    try {
      await invoke("start_session", { config });
      set({ status: "running", isRunning: true });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ status: "error", isRunning: false, lastError: msg });
      console.error("start_session failed:", e);
    }
  },

  stopSession: async () => {
    set({ status: "stopping" });
    try {
      await invoke("stop_session");
    } finally {
      set({
        isRunning: false,
        status: "idle",
        currentSourceText: "",
        currentTranslatedText: "",
      });
    }
  },

  pauseSession: async () => {
    try {
      await invoke("pause_session");
      set({ status: "paused" });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ status: "error", isRunning: false, lastError: msg });
      console.error("pause_session failed:", e);
    }
  },

  resumeSession: async () => {
    try {
      await invoke("resume_session");
      set({ status: "running" });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ status: "error", isRunning: false, lastError: msg });
      console.error("resume_session failed:", e);
    }
  },

  setCurrentTexts: (source, translated) =>
    set({ currentSourceText: source, currentTranslatedText: translated }),

  setCurrentSourceText: (source) => set({ currentSourceText: source }),
  setCurrentTranslatedText: (translated) => set({ currentTranslatedText: translated }),
  setIsProcessing: (processing) => set({ isProcessing: processing }),

  addToHistory: (item) =>
    set((state) => ({ history: [item, ...state.history] })),

  clearHistory: () => set({ history: [] }),

  setSourceLang: (lang) => set({ sourceLang: lang }),
  setTargetLang: (lang) => set({ targetLang: lang }),
  setBidirectional: (value) => set({ bidirectional: value }),
  setInputDevice: (deviceId) => set({ selectedInputDevice: deviceId }),
  setPeerInputDevice: (deviceId) => set({ selectedPeerInputDevice: deviceId }),
  setOutputDevice: (deviceId) => set({ selectedOutputDevice: deviceId }),

  setAsrEngine: (engine) => set({ asrEngine: engine }),
  setAsrModelSize: (size) => set({ asrModelSize: size }),
  setTranslationEngine: (engine) => set({ translationEngine: engine }),

  setAudioDevices: (devices) => set({ audioDevices: devices }),
}));
