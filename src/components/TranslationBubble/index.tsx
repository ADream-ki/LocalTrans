import { Volume2, Copy, Check } from "lucide-react";
import { useState } from "react";

interface TranslationBubbleProps {
  sourceText: string;
  translatedText: string;
  sourceLang: string;
  targetLang: string;
  timestamp?: string;
  lociEnhanced?: boolean;
  onSpeak?: () => void;
}

function TranslationBubble({
  sourceText,
  translatedText,
  sourceLang,
  targetLang,
  timestamp,
  lociEnhanced = false,
  onSpeak,
}: TranslationBubbleProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(translatedText);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="glass-card p-m space-y-s">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-s">
          <span className="px-s py-xs bg-primary/10 text-primary text-xs font-medium rounded-small">
            {sourceLang}
          </span>
          {lociEnhanced && (
            <span className="px-s py-xs bg-accent/10 text-accent text-xs font-medium rounded-small flex items-center gap-xs">
              <span className="w-1.5 h-1.5 bg-accent rounded-full"></span>
              Loci
            </span>
          )}
        </div>
        {timestamp && (
          <span className="text-xs text-text-tertiary">{timestamp}</span>
        )}
      </div>

      {/* Source Text */}
      <div className="text-text-primary text-m leading-relaxed">
        {sourceText}
      </div>

      {/* Divider */}
      <div className="border-t border-bg-tertiary pt-s">
        <div className="flex items-center justify-between mb-xs">
            <span className="px-s py-xs bg-success/10 text-success text-xs font-medium rounded-small">
              {targetLang}
            </span>
            <div className="flex items-center gap-s">
            <button
              onClick={onSpeak}
              disabled={!onSpeak}
              title={onSpeak ? "朗读翻译" : "未启用朗读"}
              className="p-xs text-text-tertiary hover:text-primary transition-colors duration-fast disabled:opacity-40 disabled:hover:text-text-tertiary"
              type="button"
            >
              <Volume2 size={14} />
            </button>
              <button
                onClick={handleCopy}
                className="p-xs text-text-tertiary hover:text-primary transition-colors duration-fast"
                type="button"
              >
                {copied ? <Check size={14} className="text-success" /> : <Copy size={14} />}
              </button>
            </div>
          </div>
        <div className="text-text-primary text-m leading-relaxed">
          {translatedText}
        </div>
      </div>
    </div>
  );
}

export default TranslationBubble;
