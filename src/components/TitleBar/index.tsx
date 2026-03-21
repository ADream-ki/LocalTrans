import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

interface TitleBarProps {
  title?: string;
}

function TitleBar({ title = "LocalTrans" }: TitleBarProps) {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    const checkMaximized = async () => {
      const win = getCurrentWindow();
      setIsMaximized(await win.isMaximized());
    };
    checkMaximized();

    // Listen for window state changes
    const unlisten = getCurrentWindow().onResized(async () => {
      const win = getCurrentWindow();
      setIsMaximized(await win.isMaximized());
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleMinimize = async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (err) {
      console.error("Minimize failed:", err);
    }
  };

  const handleMaximize = async () => {
    try {
      await getCurrentWindow().toggleMaximize();
    } catch (err) {
      console.error("Maximize failed:", err);
    }
  };

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (err) {
      console.error("Close failed:", err);
    }
  };

  return (
    <div className="flex items-center justify-between h-12 px-0 bg-white/80 border-b border-bg-tertiary backdrop-blur-xl select-none">
      {/* Left: Logo and Title - Draggable area */}
      <div 
        className="flex items-center gap-m flex-1 h-full cursor-move px-l"
        data-tauri-drag-region
      >
        <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-primary to-accent flex items-center justify-center pointer-events-none">
          <span className="text-white font-bold text-s">LT</span>
        </div>
        <span className="font-semibold text-text-primary text-l pointer-events-none">{title}</span>
        <span className="text-xs text-text-tertiary ml-s pointer-events-none">实时语音转译</span>
      </div>

      {/* Right: Window Controls - NOT draggable, no data-tauri-drag-region */}
      <div className="flex items-center h-full">
        <button
          onClick={handleMinimize}
          className="w-12 h-full flex items-center justify-center text-text-secondary hover:bg-bg-tertiary transition-colors duration-fast"
          title="最小化"
          type="button"
        >
          <Minus size={16} />
        </button>
        <button
          onClick={handleMaximize}
          className="w-12 h-full flex items-center justify-center text-text-secondary hover:bg-bg-tertiary transition-colors duration-fast"
          title={isMaximized ? "还原" : "最大化"}
          type="button"
        >
          {isMaximized ? (
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.5">
              <rect x="2" y="4" width="8" height="8" rx="1" />
              <path d="M4 4V2a1 1 0 0 1 1-1h7a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1h-2" />
            </svg>
          ) : (
            <Square size={14} />
          )}
        </button>
        <button
          onClick={handleClose}
          className="w-12 h-full flex items-center justify-center text-text-secondary hover:bg-error hover:text-white transition-colors duration-fast"
          title="关闭"
          type="button"
        >
          <X size={16} />
        </button>
      </div>
    </div>
  );
}

export default TitleBar;