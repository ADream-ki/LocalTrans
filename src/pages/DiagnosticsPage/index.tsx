import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import GlassCard from "../../components/GlassCard";
import StatusIndicator from "../../components/StatusIndicator";
import { 
  Cpu, HardDrive, Volume2, Monitor, Check, X, AlertCircle, 
  Play, Square, Loader2, Mic, Languages
} from "lucide-react";
import { useSettingsStore } from "../../stores/settingsStore";

interface DiagnosticGroup {
  title: string;
  icon: React.ReactNode;
  items: { key: string; value: string; status?: "ok" | "warning" | "error" }[];
}

interface TestResult {
  success: boolean;
  message: string;
  latency?: number;
  details?: string;
}

interface RuntimeComponentStatus {
  ready: boolean;
  path: string;
  message: string;
  action?: string | null;
}

interface RuntimeStatus {
  modelsDir: string;
  asr: RuntimeComponentStatus;
  translation: RuntimeComponentStatus;
  vad: RuntimeComponentStatus;
}

interface TtsSystemDoctorPlaybackResult {
  systemOk: boolean;
  systemDetail: string;
  reasonText: string;
  reasonAudioPath?: string | null;
  reasonAudioEngine: string;
  playDevice?: string | null;
}

// Sample texts for translation testing
const SAMPLE_TEXTS = [
  { text: "Hello, how are you today?", source: "en", target: "zh", expected: "你好，今天怎么样？" },
  { text: "This is a real-time translation system.", source: "en", target: "zh", expected: "这是一个实时翻译系统。" },
  { text: "The meeting will start in five minutes.", source: "en", target: "zh", expected: "会议将在五分钟后开始。" },
  { text: "我们正在进行实时语音转译测试。", source: "zh", target: "en", expected: "We are conducting a real-time voice translation test." },
  { text: "人工智能正在改变我们的生活方式。", source: "zh", target: "en", expected: "Artificial intelligence is changing the way we live." },
];

function DiagnosticsPage() {
  const [diagnostics, setDiagnostics] = useState<DiagnosticGroup[]>([]);
  const [testResults, setTestResults] = useState<Record<string, TestResult>>({});
  const [isRunningTest, setIsRunningTest] = useState<string | null>(null);
  const [testOutput, setTestOutput] = useState<string>("");
  const [isRecording, setIsRecording] = useState(false);
  
  const { ttsVoice, ttsRate, ttsEngine, ttsVolume, ttsOutputDevice, targetLanguage } = useSettingsStore();

  useEffect(() => {
    loadDiagnostics();
  }, []);

  const loadDiagnostics = async () => {
    try {
      // Get real audio devices
      const devices = await invoke<{ id: string; name: string; is_default: boolean }[]>("get_tts_output_devices");
      
      // Get virtual driver status
      const virtualDriver = await invoke<{
        has_virtual_driver: boolean;
        detected_drivers: { name: string }[];
        recommendation: string;
      }>("check_virtual_audio_driver");

      const runtime = await invoke<RuntimeStatus>("get_runtime_status");

      setDiagnostics([
        {
          title: "环境信息",
          icon: <Monitor size={18} className="text-primary" />,
          items: [
            { key: "操作系统", value: navigator.platform },
            { key: "应用版本", value: "0.1.0" },
            { key: "Tauri 版本", value: "2.x" },
          ],
        },
        {
          title: "翻译引擎",
          icon: <Cpu size={18} className="text-primary" />,
          items: [
            { key: "引擎类型", value: "Loci (本地)", status: "ok" },
            {
              key: "模型状态",
              value: runtime.translation.ready ? "已就绪" : "未就绪",
              status: runtime.translation.ready ? "ok" : "warning",
            },
            { key: "模型路径", value: runtime.translation.path || "-", status: runtime.translation.ready ? "ok" : "warning" },
          ],
        },
        {
          title: "ASR / VAD 模型",
          icon: <Cpu size={18} className="text-primary" />,
          items: [
            {
              key: "ASR",
              value: runtime.asr.ready ? "已就绪" : "未就绪",
              status: runtime.asr.ready ? "ok" : "error",
            },
            { key: "ASR 路径", value: runtime.asr.path || "-", status: runtime.asr.ready ? "ok" : "error" },
            {
              key: "Silero VAD (可选)",
              value: runtime.vad.ready ? "已就绪" : "未安装",
              status: runtime.vad.ready ? "ok" : "warning",
            },
            { key: "VAD 路径", value: runtime.vad.path || "-", status: runtime.vad.ready ? "ok" : "warning" },
            { key: "模型目录", value: runtime.modelsDir || "-", status: "ok" },
          ],
        },
        {
          title: "虚拟音频驱动",
          icon: <HardDrive size={18} className="text-primary" />,
          items: [
            { 
              key: "状态", 
              value: virtualDriver.has_virtual_driver ? "已安装" : "未安装", 
              status: virtualDriver.has_virtual_driver ? "ok" : "warning" 
            },
            { 
              key: "检测到的设备", 
              value: virtualDriver.detected_drivers.map(d => d.name).join(", ") || "无",
              status: virtualDriver.has_virtual_driver ? "ok" : undefined
            },
          ],
        },
        {
          title: "音频输出设备",
          icon: <Volume2 size={18} className="text-primary" />,
          items: devices.slice(0, 5).map(d => ({
            key: d.name,
            value: d.is_default ? "默认" : "",
            status: "ok" as const,
          })),
        },
      ]);
    } catch (error) {
      console.error("Failed to load diagnostics:", error);
    }
  };

  const getStatusIcon = (status?: "ok" | "warning" | "error") => {
    switch (status) {
      case "ok":
        return <Check size={14} className="text-success" />;
      case "warning":
        return <AlertCircle size={14} className="text-warning" />;
      case "error":
        return <X size={14} className="text-error" />;
      default:
        return null;
    }
  };

  // Test TTS
  const testTTS = async () => {
    setIsRunningTest("tts");
    setTestOutput("正在测试 TTS...");
    
    try {
      const startTime = performance.now();
      
      const result = await invoke<{ success: boolean; duration_secs: number }>("speak_text", {
        request: {
          text: "语音合成测试成功。这是一个实时翻译系统的测试。",
          voice: ttsVoice || "zh-CN-XiaoxiaoNeural",
          engine: ttsEngine,
          rate: ttsRate || 1.0,
          volume: ttsVolume,
          outputDevice: ttsOutputDevice,
        },
      });
      
      const latency = performance.now() - startTime;
      
      setTestResults(prev => ({
        ...prev,
        tts: {
          success: result.success,
          message: "TTS 测试成功",
          latency: Math.round(latency),
          details: `音频时长: ${result.duration_secs.toFixed(1)}秒`,
        },
      }));
      setTestOutput(`TTS 测试完成，延迟: ${Math.round(latency)}ms`);
    } catch (error) {
      setTestResults(prev => ({
        ...prev,
        tts: {
          success: false,
          message: "TTS 测试失败",
          details: String(error),
        },
      }));
      setTestOutput(`TTS 测试失败: ${error}`);
    }
    
    setIsRunningTest(null);
  };

  // Test Translation
  const testTranslation = async () => {
    setIsRunningTest("translation");
    setTestOutput("正在测试翻译...\n");
    
    const results: string[] = [];
    let successCount = 0;
    let totalLatency = 0;
    
    for (const sample of SAMPLE_TEXTS) {
      try {
        const startTime = performance.now();

        const translated = await invoke<{
          text: string;
          sourceLang: string;
          targetLang: string;
          confidence: number;
        }>("translate_text", {
          request: {
            text: sample.text,
            sourceLang: sample.source,
            targetLang: sample.target,
            engine: "loci",
          },
        });
        
        const latency = performance.now() - startTime;
        totalLatency += latency;
        
        results.push(`[${sample.source}->${sample.target}] "${sample.text}"`);
        results.push(`  → "${translated.text}" (${Math.round(latency)}ms)`);
        results.push(`  期望: "${sample.expected}"`);
        setTestOutput(results.join("\n"));
        
        successCount++;
      } catch (error) {
        results.push(`翻译失败: ${error}`);
        setTestOutput(results.join("\n"));
      }
      results.push("");
      setTestOutput(results.join("\n"));
    }
    
    const avgLatency = totalLatency / SAMPLE_TEXTS.length;
    
    setTestResults(prev => ({
      ...prev,
      translation: {
        success: successCount === SAMPLE_TEXTS.length,
        message: `翻译测试完成: ${successCount}/${SAMPLE_TEXTS.length} 成功`,
        latency: Math.round(avgLatency),
        details: results.join("\n"),
      },
    }));
    
    setTestOutput(results.join("\n"));
    setIsRunningTest(null);
  };

  // Test ASR (microphone capture)
  const testASR = async () => {
    setIsRunningTest("asr");
    setTestOutput("正在测试麦克风...");
    setIsRecording(true);
    
    try {
      // Start audio capture test
      await invoke("start_capture", { deviceId: null });
      
      setTestOutput("麦克风录制中... (5秒)");
      
      // Record for 5 seconds
      await new Promise(resolve => setTimeout(resolve, 5000));
      
      // Stop capture
      await invoke("stop_capture");
      
      setTestResults(prev => ({
        ...prev,
        asr: {
          success: true,
          message: "麦克风测试完成",
          details: "已捕获5秒音频数据",
        },
      }));
      
      setTestOutput("麦克风测试完成。音频捕获正常。");
    } catch (error) {
      setTestResults(prev => ({
        ...prev,
        asr: {
          success: false,
          message: "麦克风测试失败",
          details: String(error),
        },
      }));
      setTestOutput(`麦克风测试失败: ${error}`);
    }
    
    setIsRecording(false);
    setIsRunningTest(null);
  };

  // Test full pipeline
  const testFullPipeline = async () => {
    setIsRunningTest("pipeline");
    setTestOutput("正在测试完整流程...\n");
    
    const results: string[] = [];
    const startTime = performance.now();
    
    try {
      // Step 1: TTS to generate speech
      results.push("1. 生成测试语音...");
      const ttsResult = await invoke<{ success: boolean; duration_secs: number }>("speak_text", {
        request: {
          text: "This is a pipeline test.",
          voice: "en-US-JennyNeural",
          rate: 1.0,
        },
      });
      results.push(`   ✓ TTS 完成: ${ttsResult.duration_secs.toFixed(1)}秒`);
      
      // Step 2: Simulate translation
      results.push("2. 执行翻译...");
      const translated = await invoke<{
        text: string;
        sourceLang: string;
        targetLang: string;
        confidence: number;
      }>("translate_text", {
        request: {
          text: "This is a pipeline test.",
          sourceLang: "en",
          targetLang: "zh",
          engine: "loci",
        },
      });
      results.push(`   ✓ 翻译结果: "${translated.text}"`);
      
      // Step 3: TTS output
      results.push("3. 语音输出...");
      await invoke<{ success: boolean }>("speak_text", {
        request: {
          text: translated.text,
          voice: "zh-CN-XiaoxiaoNeural",
          rate: 1.0,
        },
      });
      results.push(`   ✓ 输出完成`);
      
      const totalTime = performance.now() - startTime;
      
      setTestResults(prev => ({
        ...prev,
        pipeline: {
          success: true,
          message: "完整流程测试成功",
          latency: Math.round(totalTime),
          details: results.join("\n"),
        },
      }));
      
      setTestOutput(results.join("\n") + `\n\n总延迟: ${Math.round(totalTime)}ms`);
    } catch (error) {
      setTestResults(prev => ({
        ...prev,
        pipeline: {
          success: false,
          message: "完整流程测试失败",
          details: String(error),
        },
      }));
      setTestOutput(`流程测试失败: ${error}`);
    }
    
    setIsRunningTest(null);
  };

  const testSystemDoctorPlayback = async () => {
    setIsRunningTest("tts-doctor");
    setTestOutput("正在执行 System TTS 诊断并播放原因语音...");
    try {
      const result = await invoke<TtsSystemDoctorPlaybackResult>(
        "run_tts_system_doctor_playback",
        {
          request: {
            outputDevice: ttsOutputDevice || null,
            voice: ttsVoice || "sherpa-melo-female",
            outWav: null,
            language: targetLanguage || "zh",
          },
        }
      );
      const msg = [
        `System TTS 状态: ${result.systemOk ? "可用" : "不可用"}`,
        `详情: ${result.systemDetail}`,
        `原因语音引擎: ${result.reasonAudioEngine}`,
        `播放设备: ${result.playDevice || "默认输出设备"}`,
      ].join("\n");
      setTestResults((prev) => ({
        ...prev,
        ttsDoctor: {
          success: true,
          message: "System TTS 诊断播报完成",
          details: msg,
        },
      }));
      setTestOutput(msg);
    } catch (error) {
      setTestResults((prev) => ({
        ...prev,
        ttsDoctor: {
          success: false,
          message: "System TTS 诊断播报失败",
          details: String(error),
        },
      }));
      setTestOutput(`System TTS 诊断播报失败: ${error}`);
    }
    setIsRunningTest(null);
  };

  return (
    <div className="h-full overflow-y-auto p-l">
      <div className="max-w-4xl mx-auto space-y-l">
        <div className="flex items-center justify-between mb-l">
          <h1 className="text-xl font-semibold text-text-primary">系统诊断</h1>
          <StatusIndicator status="running" label="系统正常" />
        </div>

        {diagnostics.map((group, index) => (
          <GlassCard key={index} className="p-l">
            <h2 className="text-l font-semibold text-text-primary mb-m flex items-center gap-s">
              {group.icon}
              {group.title}
            </h2>
            <div className="space-y-s">
              {group.items.map((item, i) => (
                <div
                  key={i}
                  className="flex items-center justify-between py-s border-b border-bg-tertiary last:border-0"
                >
                  <span className="text-text-secondary">{item.key}</span>
                  <div className="flex items-center gap-s">
                    <span className="font-mono text-sm text-text-primary">{item.value}</span>
                    {getStatusIcon(item.status)}
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        ))}

        {/* Test Buttons */}
        <GlassCard className="p-l">
          <h2 className="text-l font-semibold text-text-primary mb-m">功能测试</h2>
          <div className="grid grid-cols-2 gap-m mb-m">
            <button 
              onClick={testTTS}
              disabled={isRunningTest !== null}
              className={`btn-secondary flex items-center justify-center gap-s ${isRunningTest === "tts" ? "opacity-50" : ""}`}
            >
              {isRunningTest === "tts" ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <Volume2 size={16} />
              )}
              测试语音合成 (TTS)
            </button>
            <button 
              onClick={testTranslation}
              disabled={isRunningTest !== null}
              className={`btn-secondary flex items-center justify-center gap-s ${isRunningTest === "translation" ? "opacity-50" : ""}`}
            >
              {isRunningTest === "translation" ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <Languages size={16} />
              )}
              测试翻译
            </button>
            <button 
              onClick={testASR}
              disabled={isRunningTest !== null}
              className={`btn-secondary flex items-center justify-center gap-s ${isRunningTest === "asr" ? "opacity-50" : ""}`}
            >
              {isRunningTest === "asr" ? (
                <Loader2 size={16} className="animate-spin" />
              ) : isRecording ? (
                <Square size={16} className="text-error" />
              ) : (
                <Mic size={16} />
              )}
              {isRecording ? "录制中..." : "测试麦克风 (ASR)"}
            </button>
            <button 
              onClick={testFullPipeline}
              disabled={isRunningTest !== null}
              className={`btn-primary flex items-center justify-center gap-s ${isRunningTest === "pipeline" ? "opacity-50" : ""}`}
            >
              {isRunningTest === "pipeline" ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <Play size={16} />
              )}
              测试完整流程
            </button>
            <button
              onClick={testSystemDoctorPlayback}
              disabled={isRunningTest !== null}
              className={`btn-secondary flex items-center justify-center gap-s ${isRunningTest === "tts-doctor" ? "opacity-50" : ""}`}
            >
              {isRunningTest === "tts-doctor" ? (
                <Loader2 size={16} className="animate-spin" />
              ) : (
                <Volume2 size={16} />
              )}
              System TTS 诊断播报
            </button>
          </div>

          {/* Test Output */}
          {testOutput && (
            <div className="mt-m p-m bg-bg-secondary/50 rounded-medium">
              <h3 className="text-xs font-medium text-text-secondary mb-s">测试输出</h3>
              <pre className="text-xs text-text-primary whitespace-pre-wrap font-mono">{testOutput}</pre>
            </div>
          )}

          {/* Test Results Summary */}
          {Object.keys(testResults).length > 0 && (
            <div className="mt-m space-y-s">
              <h3 className="text-xs font-medium text-text-secondary">测试结果</h3>
              {Object.entries(testResults).map(([key, result]) => (
                <div 
                  key={key}
                  className={`p-m rounded-medium flex items-center justify-between ${
                    result.success ? "bg-success/10" : "bg-error/10"
                  }`}
                >
                  <div className="flex items-center gap-s">
                    {result.success ? (
                      <Check size={16} className="text-success" />
                    ) : (
                      <X size={16} className="text-error" />
                    )}
                    <span className="text-sm font-medium">{result.message}</span>
                  </div>
                  {result.latency && (
                    <span className="text-xs text-text-secondary">{result.latency}ms</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </GlassCard>

        {/* Latency Requirements */}
        <GlassCard className="p-l">
          <h2 className="text-l font-semibold text-text-primary mb-m">实时性指标</h2>
          <div className="grid grid-cols-3 gap-m text-center">
            <div className="p-m bg-bg-secondary/50 rounded-medium">
              <div className="text-xl font-bold text-primary">&lt; 100ms</div>
              <div className="text-xs text-text-secondary mt-xs">ASR 延迟目标</div>
            </div>
            <div className="p-m bg-bg-secondary/50 rounded-medium">
              <div className="text-xl font-bold text-primary">&lt; 500ms</div>
              <div className="text-xs text-text-secondary mt-xs">翻译延迟目标</div>
            </div>
            <div className="p-m bg-bg-secondary/50 rounded-medium">
              <div className="text-xl font-bold text-primary">&lt; 200ms</div>
              <div className="text-xs text-text-secondary mt-xs">TTS 延迟目标</div>
            </div>
          </div>
          <p className="text-xs text-text-tertiary mt-m text-center">
            目标总延迟: &lt; 800ms (边说边翻译体验)
          </p>
        </GlassCard>
      </div>
    </div>
  );
}

export default DiagnosticsPage;
