import clsx from "clsx";

type StatusType = "running" | "stopped" | "error" | "loading";

interface StatusIndicatorProps {
  status: StatusType;
  label?: string;
  showDot?: boolean;
}

const statusConfig = {
  running: {
    color: "bg-success",
    text: "运行中",
    animate: true,
  },
  stopped: {
    color: "bg-text-tertiary",
    text: "已停止",
    animate: false,
  },
  error: {
    color: "bg-error",
    text: "错误",
    animate: false,
  },
  loading: {
    color: "bg-warning",
    text: "加载中",
    animate: true,
  },
};

function StatusIndicator({ status, label, showDot = true }: StatusIndicatorProps) {
  const config = statusConfig[status];

  return (
    <div className="flex items-center gap-s">
      {showDot && (
        <div
          className={clsx(
            "w-2 h-2 rounded-full",
            config.color,
            config.animate && "animate-pulse"
          )}
        />
      )}
      <span className="text-m text-text-secondary">{label || config.text}</span>
    </div>
  );
}

export default StatusIndicator;
