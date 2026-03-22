import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSettingsStore } from "../../stores/settingsStore";
import GlassCard from "../../components/GlassCard";
import { Zap, Target, Languages, Volume2, Gauge, Speaker, Settings, FolderOpen, AlertCircle, CheckCircle, Download, Info } from "lucide-react";

const presets = [
  {
    id: "fast",
    name: "极速模式",
    description: "最小延时，适合实时对话",
    icon: Zap,
    settings: { asrModelSize: "tiny" as const, chunkSize: 25, vadEnabled: true },
  },
  {
    id: "accurate",
    name: "高准确率",
    description: "最佳识别质量",
    icon: Target,
    settings: { asrModelSize: "medium" as const, chunkSize: 60, vadEnabled: true },
  },
  {
    id: "chinese",
    name: "中文优化",
    description: "针对中文场景优化",
    icon: Languages,
    settings: { asrModelSize: "small" as const, chunkSize: 35, vadEnabled: true },
  },
];

const ttsVoices = [
  { id: "zh-CN-XiaoxiaoNeural", name: "晓晓 (女声)", lang: "zh-CN" },
  { id: "zh-CN-YunxiNeural", name: "云希 (男声)", lang: "zh-CN" },
  { id: "zh-CN-YunyangNeural", name: "云扬 (男声)", lang: "zh-CN" },
  { id: "zh-CN-XiaoyiNeural", name: "晓伊 (女声)", lang: "zh-CN" },
  { id: "en-US-JennyNeural", name: "Jenny (Female)", lang: "en-US" },
  { id: "en-US-GuyNeural", name: "Guy (Male)", lang: "en-US" },
  { id: "en-GB-SoniaNeural", name: "Sonia (Female)", lang: "en-GB" },
  { id: "ja-JP-NanamiNeural", name: "七海 (女声)", lang: "ja-JP" },
  { id: "ja-JP-KeitaNeural", name: "圭太 (男声)", lang: "ja-JP" },
  { id: "ko-KR-SunHiNeural", name: "선히 (여성)", lang: "ko-KR" },
];

const sherpaVoices = [
  { id: "sherpa-melo-female", name: "Sherpa 本地女声" },
  { id: "sherpa-melo-male", name: "Sherpa 本地男声" },
];

interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
}

interface VirtualAudioDriver {
  name: string;
  device_id: string;
  driver_type: string;
}

interface VirtualDriverCheckResult {
  has_virtual_driver: boolean;
  detected_drivers: VirtualAudioDriver[];
  recommendation: string;
  download_url: string | null;
}

function SettingsPage() {
  const {
    sampleRate,
    asrEngine,
    asrModelSize,
    asrLanguage,
    vadEnabled,
    chunkSize,
    gpuAcceleration,
    ttsEnabled,
    ttsEngine,
    ttsVoice,
    ttsRate,
    ttsVolume,
    ttsAutoPlay,
    ttsOutputDevice,
    customVoiceEnabled,
    customVoiceModelPath,
    customVoiceModelType,
    customVoiceReferenceAudio,
    customVoiceReferenceText,
    setSampleRate,
    setAsrEngine,
    setAsrModelSize,
    setAsrLanguage,
    setVadEnabled,
    setChunkSize,
    setGpuAcceleration,
    setTtsEnabled,
    setTtsEngine,
    setTtsVoice,
    setTtsRate,
    setTtsVolume,
    setTtsAutoPlay,
    setTtsOutputDevice,
    setCustomVoiceEnabled,
    setCustomVoiceModelPath,
    setCustomVoiceModelType,
    setCustomVoiceReferenceAudio,
    setCustomVoiceReferenceText,
  } = useSettingsStore();

  const [outputDevices, setOutputDevices] = useState<AudioDevice[]>([]);
  const [virtualDriverCheck, setVirtualDriverCheck] = useState<VirtualDriverCheckResult | null>(null);
  const [checkingDriver, setCheckingDriver] = useState(true);

  useEffect(() => {
    // Check virtual audio driver
    setCheckingDriver(true);
    invoke<VirtualDriverCheckResult>("check_virtual_audio_driver")
      .then((result) => {
        setVirtualDriverCheck(result);
        setCheckingDriver(false);
      })
      .catch((err) => {
        console.error("Failed to check virtual driver:", err);
        setCheckingDriver(false);
      });

    // Load output devices
    invoke<AudioDevice[]>("get_tts_output_devices")
      .then((devices) => setOutputDevices(devices))
      .catch((err) => console.error("Failed to load devices:", err));
  }, []);

  const handlePreset = (preset: typeof presets[0]) => {
    setAsrModelSize(preset.settings.asrModelSize);
    setChunkSize(preset.settings.chunkSize);
    setVadEnabled(preset.settings.vadEnabled);
  };

  const handleOpenDownloadUrl = async () => {
    if (virtualDriverCheck?.download_url) {
      try {
        await invoke("open_url", { url: virtualDriverCheck.download_url });
      } catch (err) {
        console.error("Failed to open URL:", err);
        // Fallback: open in new window
        window.open(virtualDriverCheck.download_url, "_blank");
      }
    }
  };

  const handleRecheckDriver = async () => {
    setCheckingDriver(true);
    try {
      const result = await invoke<VirtualDriverCheckResult>("check_virtual_audio_driver");
      setVirtualDriverCheck(result);
      // Also refresh device list
      const devices = await invoke<AudioDevice[]>("get_tts_output_devices");
      setOutputDevices(devices);
    } catch (err) {
      console.error("Failed to recheck virtual driver:", err);
    }
    setCheckingDriver(false);
  };

  return (
    <div className="h-full overflow-y-auto p-l">
      <div className="max-w-4xl mx-auto space-y-l">
        {/* Quick Presets */}
        <GlassCard className="p-l">
          <h2 className="text-l font-semibold text-text-primary mb-m flex items-center gap-s">
            <Zap size={18} className="text-primary" />
            快速预设
          </h2>
          <div className="grid grid-cols-3 gap-m">
            {presets.map((preset) => {
              const Icon = preset.icon;
              return (
                <button
                  key={preset.id}
                  onClick={() => handlePreset(preset)}
                  className="p-m bg-bg-secondary/50 rounded-large text-left hover:bg-bg-tertiary/50 transition-colors duration-fast"
                >
                  <div className="flex items-center gap-s mb-s">
                    <div className="w-8 h-8 rounded-medium bg-primary/10 flex items-center justify-center">
                      <Icon size={16} className="text-primary" />
                    </div>
                    <span className="font-medium text-text-primary">{preset.name}</span>
                  </div>
                  <p className="text-xs text-text-secondary">{preset.description}</p>
                </button>
              );
            })}
          </div>
        </GlassCard>

        {/* TTS Settings - Moved to top as it's important for voice output */}
        <GlassCard className="p-l">
          <h2 className="text-l font-semibold text-text-primary mb-m flex items-center gap-s">
            <Speaker size={18} className="text-primary" />
            TTS 语音合成设置
          </h2>

          {/* Virtual Audio Driver Status */}
          <div className={`p-m rounded-large mb-m ${
            checkingDriver 
              ? "bg-bg-secondary/50" 
              : virtualDriverCheck?.has_virtual_driver 
                ? "bg-success/10 border border-success/20" 
                : "bg-warning/10 border border-warning/20"
          }`}>
            <div className="flex items-start justify-between">
              <div className="flex items-start gap-s">
                {checkingDriver ? (
                  <div className="w-5 h-5 border-2 border-primary/30 border-t-primary rounded-full animate-spin mt-0.5" />
                ) : virtualDriverCheck?.has_virtual_driver ? (
                  <CheckCircle size={20} className="text-success flex-shrink-0 mt-0.5" />
                ) : (
                  <AlertCircle size={20} className="text-warning flex-shrink-0 mt-0.5" />
                )}
                <div>
                  <div className="text-m font-medium text-text-primary">
                    {checkingDriver 
                      ? "检测中..." 
                      : virtualDriverCheck?.has_virtual_driver 
                        ? "虚拟音频驱动已就绪" 
                        : "未检测到虚拟音频驱动"}
                  </div>
                  {virtualDriverCheck && !checkingDriver && (
                    <>
                      <p className="text-xs text-text-secondary mt-xs">{virtualDriverCheck.recommendation}</p>
                      {virtualDriverCheck.detected_drivers.length > 0 && (
                        <div className="flex flex-wrap gap-xs mt-xs">
                          {virtualDriverCheck.detected_drivers.map((driver) => (
                            <span 
                              key={driver.device_id} 
                              className="px-2 py-1 bg-success/20 rounded-small text-xs text-success"
                            >
                              {driver.name}
                            </span>
                          ))}
                        </div>
                      )}
                    </>
                  )}
                </div>
              </div>
              <div className="flex gap-xs">
                {!checkingDriver && !virtualDriverCheck?.has_virtual_driver && virtualDriverCheck?.download_url && (
                  <button
                    onClick={handleOpenDownloadUrl}
                    className="px-3 py-1.5 bg-primary text-white rounded-medium text-xs font-medium hover:bg-primary/80 transition-colors flex items-center gap-1"
                  >
                    <Download size={14} />
                    下载 VB-Audio
                  </button>
                )}
                <button
                  onClick={handleRecheckDriver}
                  disabled={checkingDriver}
                  className="px-3 py-1.5 bg-bg-tertiary text-text-secondary rounded-medium text-xs hover:bg-bg-hover transition-colors disabled:opacity-50"
                >
                  重新检测
                </button>
              </div>
            </div>
          </div>

          {/* Alternative Solutions */}
          {virtualDriverCheck && !virtualDriverCheck.has_virtual_driver && (
            <div className="p-m bg-info/10 rounded-large mb-m border border-info/20">
              <div className="flex items-start gap-s">
                <Info size={16} className="text-info flex-shrink-0 mt-0.5" />
                <div className="text-xs text-text-secondary">
                  <div className="font-medium text-text-primary mb-1">替代方案：</div>
                  <ul className="space-y-1 list-disc list-inside">
                    <li>安装 VB-Audio Virtual Cable（免费）- 最推荐</li>
                    <li>使用 Voicemeeter（功能更强大）</li>
                    <li>使用 Windows 立体声混音（需手动启用）</li>
                    <li>使用物理音频线连接扬声器输出到麦克风输入</li>
                  </ul>
                </div>
              </div>
            </div>
          )}

          <div className="space-y-m">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-m text-text-primary">启用语音输出</div>
                <div className="text-xs text-text-secondary">翻译结果自动朗读</div>
              </div>
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={ttsEnabled}
                  onChange={(e) => setTtsEnabled(e.target.checked)}
                  className="sr-only peer"
                />
                <div className="w-11 h-6 bg-bg-tertiary peer-focus:ring-2 peer-focus:ring-primary/20 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
              </label>
            </div>
            
            {ttsEnabled && (
              <>
                {/* Output Device Selection - Important for virtual audio routing */}
                <div>
                  <label className="text-xs text-text-secondary block mb-xs">
                    输出设备 (支持虚拟音频设备)
                  </label>
                  <select
                    value={ttsOutputDevice || ""}
                    onChange={(e) => setTtsOutputDevice(e.target.value || null)}
                    className="select-field"
                  >
                    <option value="">系统默认扬声器</option>
                    <optgroup label="虚拟音频设备">
                      {outputDevices
                        .filter((d) => d.name.toLowerCase().includes("cable") || 
                                       d.name.toLowerCase().includes("virtual") ||
                                       d.name.includes("VB-Audio"))
                        .map((device) => (
                          <option key={device.id} value={device.id}>
                            🎧 {device.name}
                          </option>
                        ))}
                    </optgroup>
                    <optgroup label="物理设备">
                      {outputDevices
                        .filter((d) => !d.name.toLowerCase().includes("cable") && 
                                       !d.name.toLowerCase().includes("virtual") &&
                                       !d.name.includes("VB-Audio"))
                        .map((device) => (
                          <option key={device.id} value={device.id}>
                            {device.is_default ? "🔊 " : "   "}{device.name}
                          </option>
                        ))}
                    </optgroup>
                  </select>
                  <p className="text-xs text-text-tertiary mt-xs">
                    选择虚拟音频设备(如VB-Audio Cable)可将语音输出到会议软件
                  </p>
                </div>

                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-m text-text-primary">自动播放</div>
                    <div className="text-xs text-text-secondary">翻译完成后自动朗读</div>
                  </div>
                  <label className="relative inline-flex items-center cursor-pointer">
                    <input
                      type="checkbox"
                      checked={ttsAutoPlay}
                      onChange={(e) => setTtsAutoPlay(e.target.checked)}
                      className="sr-only peer"
                    />
                    <div className="w-11 h-6 bg-bg-tertiary peer-focus:ring-2 peer-focus:ring-primary/20 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
                  </label>
                </div>
                
                <div className="grid grid-cols-2 gap-m">
                  <div>
                    <label className="text-xs text-text-secondary block mb-xs">TTS 引擎</label>
                    <select
                      value={ttsEngine}
                      onChange={(e) => setTtsEngine(e.target.value as typeof ttsEngine)}
                      className="select-field"
                    >
                      <option value="sherpa-melo">Sherpa Melo (离线推荐)</option>
                      <option value="edge-tts">Edge TTS (在线)</option>
                      <option value="custom">自定义音色</option>
                      <option value="piper">Piper (离线)</option>
                      <option value="system">系统语音</option>
                    </select>
                  </div>
                  
                  {(ttsEngine === "edge-tts" || ttsEngine === "sherpa-melo") && (
                    <div>
                      <label className="text-xs text-text-secondary block mb-xs">语音</label>
                      <select
                        value={ttsVoice}
                        onChange={(e) => setTtsVoice(e.target.value)}
                        className="select-field"
                      >
                        {ttsEngine === "edge-tts" &&
                          ttsVoices.map((voice) => (
                            <option key={voice.id} value={voice.id}>
                              {voice.name} ({voice.lang})
                            </option>
                          ))}
                        {ttsEngine === "sherpa-melo" &&
                          sherpaVoices.map((voice) => (
                            <option key={voice.id} value={voice.id}>
                              {voice.name}
                            </option>
                          ))}
                      </select>
                    </div>
                  )}
                </div>

                {/* Custom Voice Settings */}
                {ttsEngine === "custom" && (
                  <div className="p-m bg-accent/5 rounded-large border border-accent/20 space-y-m">
                    <h3 className="text-m font-medium text-text-primary flex items-center gap-s">
                      <Settings size={16} className="text-accent" />
                      自定义音色设置
                    </h3>
                    
                    <div className="flex items-center justify-between">
                      <div>
                        <div className="text-s text-text-primary">启用自定义音色</div>
                        <div className="text-xs text-text-tertiary">使用GPT-SoVITS/RVC等</div>
                      </div>
                      <label className="relative inline-flex items-center cursor-pointer">
                        <input
                          type="checkbox"
                          checked={customVoiceEnabled}
                          onChange={(e) => setCustomVoiceEnabled(e.target.checked)}
                          className="sr-only peer"
                        />
                        <div className="w-11 h-6 bg-bg-tertiary peer-focus:ring-2 peer-focus:ring-primary/20 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
                      </label>
                    </div>

                    {customVoiceEnabled && (
                      <>
                        <div className="grid grid-cols-2 gap-m">
                          <div>
                            <label className="text-xs text-text-secondary block mb-xs">模型类型</label>
                            <select
                              value={customVoiceModelType}
                              onChange={(e) => setCustomVoiceModelType(e.target.value as typeof customVoiceModelType)}
                              className="select-field"
                            >
                              <option value="gpt-sovits">GPT-SoVITS</option>
                              <option value="rvc">RVC (变声)</option>
                              <option value="piper">Piper</option>
                              <option value="vits">VITS</option>
                            </select>
                          </div>
                          <div>
                            <label className="text-xs text-text-secondary block mb-xs">模型路径</label>
                            <div className="flex gap-s">
                              <input
                                type="text"
                                value={customVoiceModelPath}
                                onChange={(e) => setCustomVoiceModelPath(e.target.value)}
                                className="input-field flex-1"
                                placeholder="/path/to/model.pth"
                              />
                              <button className="p-s rounded-medium bg-bg-tertiary hover:bg-bg-hover transition-colors">
                                <FolderOpen size={16} className="text-text-secondary" />
                              </button>
                            </div>
                          </div>
                        </div>

                        {(customVoiceModelType === "gpt-sovits" || customVoiceModelType === "rvc") && (
                          <div className="grid grid-cols-2 gap-m">
                            <div>
                              <label className="text-xs text-text-secondary block mb-xs">参考音频</label>
                              <div className="flex gap-s">
                                <input
                                  type="text"
                                  value={customVoiceReferenceAudio || ""}
                                  onChange={(e) => setCustomVoiceReferenceAudio(e.target.value || null)}
                                  className="input-field flex-1"
                                  placeholder="/path/to/reference.wav"
                                />
                                <button className="p-s rounded-medium bg-bg-tertiary hover:bg-bg-hover transition-colors">
                                  <FolderOpen size={16} className="text-text-secondary" />
                                </button>
                              </div>
                            </div>
                            <div>
                              <label className="text-xs text-text-secondary block mb-xs">参考文本</label>
                              <input
                                type="text"
                                value={customVoiceReferenceText || ""}
                                onChange={(e) => setCustomVoiceReferenceText(e.target.value || null)}
                                className="input-field"
                                placeholder="参考音频对应的文本"
                              />
                            </div>
                          </div>
                        )}

                        <p className="text-xs text-text-tertiary">
                          💡 提示：GPT-SoVITS需要启动本地API服务(默认端口9880)，RVC需要启动WebUI(默认端口7865)
                        </p>
                      </>
                    )}
                  </div>
                )}
                
                <div className="grid grid-cols-2 gap-m">
                  <div>
                    <label className="text-xs text-text-secondary block mb-xs">
                      语速: {ttsRate.toFixed(1)}x
                    </label>
                    <input
                      type="range"
                      min={0.5}
                      max={2.0}
                      step={0.1}
                      value={ttsRate}
                      onChange={(e) => setTtsRate(Number(e.target.value))}
                      className="w-full h-2 bg-bg-tertiary rounded-lg appearance-none cursor-pointer accent-primary"
                    />
                  </div>
                  <div>
                    <label className="text-xs text-text-secondary block mb-xs">
                      音量: {Math.round(ttsVolume * 100)}%
                    </label>
                    <input
                      type="range"
                      min={0}
                      max={1}
                      step={0.1}
                      value={ttsVolume}
                      onChange={(e) => setTtsVolume(Number(e.target.value))}
                      className="w-full h-2 bg-bg-tertiary rounded-lg appearance-none cursor-pointer accent-primary"
                    />
                  </div>
                </div>
              </>
            )}
          </div>
        </GlassCard>

        {/* ASR Settings */}
        <GlassCard className="p-l">
          <h2 className="text-l font-semibold text-text-primary mb-m flex items-center gap-s">
            <Volume2 size={18} className="text-primary" />
            ASR 语音识别设置
          </h2>
          <div className="space-y-m">
            <div className="grid grid-cols-2 gap-m">
              <div>
                <label className="text-xs text-text-secondary block mb-xs">引擎</label>
                <select
                  value={asrEngine}
                  onChange={(e) => setAsrEngine(e.target.value as typeof asrEngine)}
                  className="select-field"
                >
                  <option value="whisper">Whisper</option>
                  <option value="sensevoice">SenseVoice</option>
                  <option value="vosk">Vosk</option>
                </select>
              </div>
              <div>
                <label className="text-xs text-text-secondary block mb-xs">模型大小</label>
                <select
                  value={asrModelSize}
                  onChange={(e) => setAsrModelSize(e.target.value as typeof asrModelSize)}
                  className="select-field"
                >
                  <option value="tiny">Tiny (39M)</option>
                  <option value="base">Base (74M)</option>
                  <option value="small">Small (244M)</option>
                  <option value="medium">Medium (769M)</option>
                  <option value="large">Large (1550M)</option>
                </select>
              </div>
            </div>
            <div className="grid grid-cols-2 gap-m">
              <div>
                <label className="text-xs text-text-secondary block mb-xs">源语言</label>
                <select
                  value={asrLanguage}
                  onChange={(e) => setAsrLanguage(e.target.value)}
                  className="select-field"
                >
                  <option value="auto">自动检测</option>
                  <option value="zh">中文</option>
                  <option value="en">English</option>
                  <option value="ja">日本語</option>
                  <option value="ko">한국어</option>
                </select>
              </div>
              <div>
                <label className="text-xs text-text-secondary block mb-xs">采样率</label>
                <select
                  value={sampleRate}
                  onChange={(e) => setSampleRate(Number(e.target.value))}
                  className="select-field"
                >
                  <option value={16000}>16000 Hz</option>
                  <option value={22050}>22050 Hz</option>
                  <option value={44100}>44100 Hz</option>
                  <option value={48000}>48000 Hz</option>
                </select>
              </div>
            </div>
          </div>
        </GlassCard>

        {/* Performance Settings */}
        <GlassCard className="p-l">
          <h2 className="text-l font-semibold text-text-primary mb-m flex items-center gap-s">
            <Gauge size={18} className="text-primary" />
            性能设置
          </h2>
          <div className="space-y-m">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-m text-text-primary">GPU 加速</div>
                <div className="text-xs text-text-secondary">使用 GPU 进行推理加速</div>
              </div>
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={gpuAcceleration}
                  onChange={(e) => setGpuAcceleration(e.target.checked)}
                  className="sr-only peer"
                />
                <div className="w-11 h-6 bg-bg-tertiary peer-focus:ring-2 peer-focus:ring-primary/20 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
              </label>
            </div>
            <div className="flex items-center justify-between">
              <div>
                <div className="text-m text-text-primary">VAD 检测</div>
                <div className="text-xs text-text-secondary">语音活动检测，减少无效处理</div>
              </div>
              <label className="relative inline-flex items-center cursor-pointer">
                <input
                  type="checkbox"
                  checked={vadEnabled}
                  onChange={(e) => setVadEnabled(e.target.checked)}
                  className="sr-only peer"
                />
                <div className="w-11 h-6 bg-bg-tertiary peer-focus:ring-2 peer-focus:ring-primary/20 rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"></div>
              </label>
            </div>
            <div>
              <label className="text-xs text-text-secondary block mb-xs">
                音频分块大小: {chunkSize}ms
              </label>
              <input
                type="range"
                min={20}
                max={120}
                step={5}
                value={chunkSize}
                onChange={(e) => setChunkSize(Number(e.target.value))}
                className="w-full h-2 bg-bg-tertiary rounded-lg appearance-none cursor-pointer accent-primary"
              />
            </div>
          </div>
        </GlassCard>
      </div>
    </div>
  );
}

export default SettingsPage;
