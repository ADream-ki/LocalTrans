import { ReactNode } from "react";
import clsx from "clsx";

interface GlassCardProps {
  children: ReactNode;
  className?: string;
  hover?: boolean;
  onClick?: () => void;
}

function GlassCard({ children, className, hover = false, onClick }: GlassCardProps) {
  return (
    <div
      onClick={onClick}
      className={clsx(
        "rounded-large",
        "bg-gradient-to-br from-white/90 to-white/70",
        "backdrop-blur-xl",
        "border border-white/50",
        "shadow-lg",
        "transition-all duration-normal",
        hover && "cursor-pointer hover:shadow-xl hover:border-primary/30 hover:scale-[1.01]",
        className
      )}
    >
      {children}
    </div>
  );
}

export default GlassCard;
