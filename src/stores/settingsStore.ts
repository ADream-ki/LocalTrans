import { create } from "zustand";
import { persist } from "zustand/middleware";

export interface SettingsState {
  // General
  theme: "light" | "dark" | "system";
  language: string;
  autoStart: boolean;
  minimizeToTray: boolean;

  // Audio
  inputDevice: string | null;
  outputDevice: string | null;
  peerInputDevice: string | null;
  sampleRate: number;
  channels: number;

  // ASR
  asrEngine: "whisper" | "sensevoice" | "vosk";
  asrModelPath: string;
  asrLanguage: string;
  asrModelSize: "tiny" | "base" | "small" | "medium" | "large";

  // Translation
  translationEngine: "loci" | "nllb" | "m2m" | "argos";
  translationModelPath: string;
  sourceLanguage: string;
  targetLanguage: string;

  // Loci
  lociModelPath: string;
  lociEnabled: boolean;

  // TTS
  ttsEnabled: boolean;
  ttsEngine: "sherpa-melo" | "edge-tts" | "custom" | "piper" | "system";
  ttsVoice: string;
  ttsRate: number;
  ttsVolume: number;
  ttsAutoPlay: boolean;
  ttsOutputDevice: string | null;
  peerTtsOutputDevice: string | null;
  
  // Custom Voice
  customVoiceEnabled: boolean;
  customVoiceModelPath: string;
  customVoiceModelType: "gpt-sovits" | "rvc" | "piper" | "vits";
  customVoiceReferenceAudio: string | null;
  customVoiceReferenceText: string | null;

  // Performance
  vadEnabled: boolean;
  vadThreshold: number;
  chunkSize: number;
  gpuAcceleration: boolean;
  streamTranslationIntervalMs: number;
  streamTranslationMinChars: number;
  streamTtsIntervalMs: number;
  streamTtsMinChars: number;

  // Actions
  setTheme: (theme: SettingsState["theme"]) => void;
  setLanguage: (language: string) => void;
  setAutoStart: (value: boolean) => void;
  setMinimizeToTray: (value: boolean) => void;

  setInputDevice: (deviceId: string | null) => void;
  setOutputDevice: (deviceId: string | null) => void;
  setPeerInputDevice: (deviceId: string | null) => void;
  setSampleRate: (rate: number) => void;
  setChannels: (channels: number) => void;

  setAsrEngine: (engine: SettingsState["asrEngine"]) => void;
  setAsrModelPath: (path: string) => void;
  setAsrLanguage: (lang: string) => void;
  setAsrModelSize: (size: SettingsState["asrModelSize"]) => void;

  setTranslationEngine: (engine: SettingsState["translationEngine"]) => void;
  setTranslationModelPath: (path: string) => void;
  setSourceLanguage: (lang: string) => void;
  setTargetLanguage: (lang: string) => void;

  setLociModelPath: (path: string) => void;
  setLociEnabled: (enabled: boolean) => void;

  setTtsEnabled: (enabled: boolean) => void;
  setTtsEngine: (engine: SettingsState["ttsEngine"]) => void;
  setTtsVoice: (voice: string) => void;
  setTtsRate: (rate: number) => void;
  setTtsVolume: (volume: number) => void;
  setTtsAutoPlay: (autoPlay: boolean) => void;
  setTtsOutputDevice: (device: string | null) => void;
  setPeerTtsOutputDevice: (device: string | null) => void;
  
  setCustomVoiceEnabled: (enabled: boolean) => void;
  setCustomVoiceModelPath: (path: string) => void;
  setCustomVoiceModelType: (type: SettingsState["customVoiceModelType"]) => void;
  setCustomVoiceReferenceAudio: (path: string | null) => void;
  setCustomVoiceReferenceText: (text: string | null) => void;

  setVadEnabled: (enabled: boolean) => void;
  setVadThreshold: (threshold: number) => void;
  setChunkSize: (size: number) => void;
  setGpuAcceleration: (enabled: boolean) => void;
  setStreamTranslationIntervalMs: (ms: number) => void;
  setStreamTranslationMinChars: (chars: number) => void;
  setStreamTtsIntervalMs: (ms: number) => void;
  setStreamTtsMinChars: (chars: number) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      // General
      theme: "light",
      language: "zh-CN",
      autoStart: false,
      minimizeToTray: true,

      // Audio
      inputDevice: null,
      outputDevice: null,
      peerInputDevice: null,
      sampleRate: 16000,
      channels: 1,

      // ASR
      asrEngine: "whisper",
      asrModelPath: "",
      asrLanguage: "auto",
      asrModelSize: "base",

      // Translation
      translationEngine: "loci",
      translationModelPath: "",
      sourceLanguage: "en",
      targetLanguage: "zh",

      // Loci
      lociModelPath: "",
      lociEnabled: true,

      // TTS
      ttsEnabled: true,
      ttsEngine: "sherpa-melo",
      ttsVoice: "sherpa-melo-female",
      ttsRate: 1.0,
      ttsVolume: 1.0,
      ttsAutoPlay: true,
      ttsOutputDevice: null,
      peerTtsOutputDevice: null,
      
      // Custom Voice
      customVoiceEnabled: false,
      customVoiceModelPath: "",
      customVoiceModelType: "gpt-sovits",
      customVoiceReferenceAudio: null,
      customVoiceReferenceText: null,

      // Performance
      vadEnabled: true,
      vadThreshold: 0.5,
      // VAD frame / chunk size in milliseconds
      chunkSize: 30,
      gpuAcceleration: true,
      streamTranslationIntervalMs: 900,
      streamTranslationMinChars: 8,
      streamTtsIntervalMs: 1500,
      streamTtsMinChars: 12,

      // Actions
      setTheme: (theme) => set({ theme }),
      setLanguage: (language) => set({ language }),
      setAutoStart: (value) => set({ autoStart: value }),
      setMinimizeToTray: (value) => set({ minimizeToTray: value }),

      setInputDevice: (deviceId) => set({ inputDevice: deviceId }),
      setOutputDevice: (deviceId) => set({ outputDevice: deviceId }),
      setPeerInputDevice: (deviceId) => set({ peerInputDevice: deviceId }),
      setSampleRate: (rate) => set({ sampleRate: rate }),
      setChannels: (channels) => set({ channels: channels }),

      setAsrEngine: (engine) => set({ asrEngine: engine }),
      setAsrModelPath: (path) => set({ asrModelPath: path }),
      setAsrLanguage: (lang) => set({ asrLanguage: lang }),
      setAsrModelSize: (size) => set({ asrModelSize: size }),

      setTranslationEngine: (engine) => set({ translationEngine: engine }),
      setTranslationModelPath: (path) => set({ translationModelPath: path }),
      setSourceLanguage: (lang) => set({ sourceLanguage: lang }),
      setTargetLanguage: (lang) => set({ targetLanguage: lang }),

      setLociModelPath: (path) => set({ lociModelPath: path }),
      setLociEnabled: (enabled) => set({ lociEnabled: enabled }),

      setTtsEnabled: (enabled) => set({ ttsEnabled: enabled }),
      setTtsEngine: (engine) => set({ ttsEngine: engine }),
      setTtsVoice: (voice) => set({ ttsVoice: voice }),
      setTtsRate: (rate) => set({ ttsRate: rate }),
      setTtsVolume: (volume) => set({ ttsVolume: volume }),
      setTtsAutoPlay: (autoPlay) => set({ ttsAutoPlay: autoPlay }),
      setTtsOutputDevice: (device) => set({ ttsOutputDevice: device }),
      setPeerTtsOutputDevice: (device) => set({ peerTtsOutputDevice: device }),
      
      setCustomVoiceEnabled: (enabled) => set({ customVoiceEnabled: enabled }),
      setCustomVoiceModelPath: (path) => set({ customVoiceModelPath: path }),
      setCustomVoiceModelType: (type) => set({ customVoiceModelType: type }),
      setCustomVoiceReferenceAudio: (path) => set({ customVoiceReferenceAudio: path }),
      setCustomVoiceReferenceText: (text) => set({ customVoiceReferenceText: text }),

      setVadEnabled: (enabled) => set({ vadEnabled: enabled }),
      setVadThreshold: (threshold) => set({ vadThreshold: threshold }),
      setChunkSize: (size) => set({ chunkSize: size }),
      setGpuAcceleration: (enabled) => set({ gpuAcceleration: enabled }),
      setStreamTranslationIntervalMs: (ms) => set({ streamTranslationIntervalMs: Math.max(300, Math.min(5000, Math.round(ms))) }),
      setStreamTranslationMinChars: (chars) => set({ streamTranslationMinChars: Math.max(2, Math.min(64, Math.round(chars))) }),
      setStreamTtsIntervalMs: (ms) => set({ streamTtsIntervalMs: Math.max(500, Math.min(10000, Math.round(ms))) }),
      setStreamTtsMinChars: (chars) => set({ streamTtsMinChars: Math.max(2, Math.min(128, Math.round(chars))) }),
    }),
    {
      name: "localtrans-settings",
      version: 4,
      migrate: (persistedState, version) => {
        const state = (persistedState ?? {}) as Partial<SettingsState>;
        if (version < 3) {
          state.peerInputDevice = state.peerInputDevice ?? null;
          state.peerTtsOutputDevice = state.peerTtsOutputDevice ?? null;
        }
        if (version < 4) {
          if (!state.translationEngine || state.translationEngine === "nllb") {
            state.translationEngine = "loci";
          }
        }
        return state as SettingsState;
      },
    }
  )
);
