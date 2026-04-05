import { create } from "zustand";
import { persist } from "zustand/middleware";

export type Tab = "session" | "readiness" | "settings" | "model" | "diagnostics" | "release";

interface UiState {
  activeTab: Tab;
  modelOnboardingSeen: boolean;
  modelOnboardingOpen: boolean;
  setActiveTab: (tab: Tab) => void;
  openModelOnboarding: () => void;
  closeModelOnboarding: () => void;
  completeModelOnboarding: () => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      activeTab: "session",
      modelOnboardingSeen: false,
      modelOnboardingOpen: false,
      setActiveTab: (tab) => set({ activeTab: tab }),
      openModelOnboarding: () => set({ modelOnboardingOpen: true }),
      closeModelOnboarding: () => set({ modelOnboardingOpen: false }),
      completeModelOnboarding: () =>
        set({ modelOnboardingSeen: true, modelOnboardingOpen: false }),
    }),
    {
      name: "localtrans-ui",
      version: 3,
      migrate: (persistedState, _version) => {
        const state = (persistedState ?? {}) as Partial<UiState>;
        if (
          state.activeTab !== "session" &&
          state.activeTab !== "readiness" &&
          state.activeTab !== "settings" &&
          state.activeTab !== "model" &&
          state.activeTab !== "diagnostics" &&
          state.activeTab !== "release"
        ) {
          state.activeTab = "session";
        }
        return state as UiState;
      },
    }
  )
);
