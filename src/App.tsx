import { useUiStore, type Tab } from "./stores/uiStore";
import { useEffect } from "react";
import SessionPage from "./pages/SessionPage";
import SettingsPage from "./pages/SettingsPage";
import ModelPage from "./pages/ModelPage";
import DiagnosticsPage from "./pages/DiagnosticsPage";
import OnboardingModal from "./components/OnboardingModal";
import TitleBar from "./components/TitleBar";
import { Mic, Settings, Package, Activity, Sparkles } from "lucide-react";
import { useRef } from "react";

const tabs = [
  { id: "session" as Tab, label: "会话", icon: Mic },
  { id: "settings" as Tab, label: "设置", icon: Settings },
  { id: "model" as Tab, label: "模型", icon: Package },
  { id: "diagnostics" as Tab, label: "诊断", icon: Activity },
];

function App() {
  const {
    activeTab,
    setActiveTab,
    modelOnboardingSeen,
    modelOnboardingOpen,
    openModelOnboarding,
  } = useUiStore();
  const bootstrappedOnboarding = useRef(false);

  useEffect(() => {
    if (!bootstrappedOnboarding.current && !modelOnboardingSeen && !modelOnboardingOpen) {
      bootstrappedOnboarding.current = true;
      openModelOnboarding();
    }
  }, [modelOnboardingSeen, modelOnboardingOpen, openModelOnboarding, setActiveTab]);

  return (
    <div className="flex flex-col h-full bg-gradient-to-br from-bg-secondary to-bg-tertiary">
      {/* Title Bar */}
      <TitleBar />

      {/* Main Content */}
      <div className="flex-1 flex flex-col overflow-hidden">
        {/* Tab Bar */}
        <div className="flex items-center gap-s px-l py-s bg-white/50 border-b border-bg-tertiary">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`
                  flex items-center gap-s px-l py-s rounded-medium font-medium
                  transition-all duration-fast
                  ${
                    isActive
                      ? "bg-primary text-white shadow-md"
                      : "text-text-secondary hover:bg-bg-tertiary hover:text-text-primary"
                  }
                `}
              >
                <Icon size={16} />
                <span>{tab.label}</span>
              </button>
            );
          })}
          {!modelOnboardingSeen && (
            <button
              onClick={() => openModelOnboarding()}
              className="ml-auto flex items-center gap-s px-l py-s rounded-medium font-medium text-primary hover:bg-primary/10 transition-colors duration-fast"
            >
              <Sparkles size={16} />
              <span>首次向导</span>
            </button>
          )}
        </div>

        {/* Page Content */}
        <div className="flex-1 overflow-hidden">
          {activeTab === "session" && <SessionPage />}
          {activeTab === "settings" && <SettingsPage />}
          {activeTab === "model" && <ModelPage />}
          {activeTab === "diagnostics" && <DiagnosticsPage />}
        </div>
      </div>
      <OnboardingModal />
    </div>
  );
}

export default App;
