import { create } from "zustand";

export type Tab = "session" | "settings" | "model" | "diagnostics";

interface UiState {
  activeTab: Tab;
  setActiveTab: (tab: Tab) => void;
}

export const useUiStore = create<UiState>((set) => ({
  activeTab: "session",
  setActiveTab: (tab) => set({ activeTab: tab }),
}));
