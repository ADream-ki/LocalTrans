"""
主程序入口
CLI命令行接口
"""

import copy
import sys
import signal
from typing import Optional

from loguru import logger

from localtrans import __version__
from localtrans.config import settings
from localtrans.pipeline import RealtimePipeline, create_pipeline
from localtrans.pipeline.realtime import RealtimeConfig, SessionOrchestrator
from localtrans.audio import VirtualAudioDevice


DEFAULT_PIPER_MODELS = {
    "zh": "piper-zh_CN-huayan",
    "en": "piper-en_US-lessac",
}


def _percentile(values: list[float], q: float) -> float:
    """计算百分位，空数组返回0"""
    if not values:
        return 0.0
    try:
        import numpy as np

        return float(np.percentile(values, q))
    except Exception:
        # 轻量回退：无numpy时使用排序近似
        ordered = sorted(values)
        idx = int(round((q / 100.0) * (len(ordered) - 1)))
        idx = max(0, min(idx, len(ordered) - 1))
        return float(ordered[idx])


def setup_logging(debug: bool = False) -> None:
    """配置日志"""
    log_level = "DEBUG" if debug else "INFO"
    
    logger.remove()
    logger.add(
        sys.stderr,
        level=log_level,
        format="<green>{time:HH:mm:ss}</green> | <level>{level: <8}</level> | <cyan>{name}</cyan>:<cyan>{function}</cyan>:<cyan>{line}</cyan> - <level>{message}</level>",
        colorize=True,
    )
    logger.add(
        settings.logs_dir / "localtrans_{time}.log",
        level="DEBUG",
        rotation="10 MB",
        retention="7 days",
        encoding="utf-8",
    )


def check_virtual_device() -> bool:
    """检查虚拟设备"""
    if VirtualAudioDevice.check_vb_cable_installed():
        print("[OK] 虚拟声卡已安装")
        return True
    else:
        print("[!] 虚拟声卡未安装")
        print(VirtualAudioDevice.get_installation_guide())
        return False


def show_welcome() -> None:
    """显示欢迎信息"""
    print()
    print("=" * 50)
    print("  LocalTrans - AI实时翻译软件")
    print(f"  版本: {__version__}")
    print("  端侧本地部署，保护隐私安全")
    print("=" * 50)
    print()


def show_devices() -> None:
    """显示音频设备列表"""
    from localtrans.audio import AudioCapturer
    
    capturer = AudioCapturer()
    devices = capturer.list_devices()
    
    print("\n音频输入设备:")
    print("-" * 60)
    print(f"{'ID':<6} {'名称':<30} {'声道':<8} {'采样率':<10}")
    print("-" * 60)
    
    for dev in devices:
        print(f"{dev['id']:<6} {dev['name'][:28]:<30} {dev['channels']:<8} {int(dev['sample_rate'])} Hz")
    
    print()


def show_models() -> None:
    """显示可用模型"""
    from localtrans.utils import ModelDownloader
    
    downloader = ModelDownloader()
    models = downloader.list_available()
    
    print("\n可用模型:")
    print("-" * 60)
    print(f"{'状态':<8} {'名称':<30} {'类型':<8} {'大小':<10}")
    print("-" * 60)
    
    for model in models:
        status = "[OK]" if downloader.is_downloaded(model.name) else "[  ]"
        size = f"{model.size_mb}MB" if model.size_mb else "-"
        print(f"{status:<8} {model.name:<30} {model.type.upper():<8} {size:<10}")
    
    print()
    print("使用 'localtrans download <model>' 下载模型")
    print()


def download_model(model_name: str, force: bool = False) -> None:
    """下载模型"""
    from localtrans.utils import ModelDownloader
    
    downloader = ModelDownloader()
    
    if downloader.is_downloaded(model_name) and not force:
        print(f"[OK] 模型 {model_name} 已存在")
        return
    
    print(f"正在下载模型: {model_name}...")
    
    try:
        downloader.download_model(model_name, force=force)
        print(f"[OK] 模型 {model_name} 下载完成")
    except Exception as e:
        print(f"[ERROR] 下载失败: {e}")


def _default_mt_model_name(source_lang: str, target_lang: str) -> Optional[str]:
    source = (source_lang or "").strip().lower()
    target = (target_lang or "").strip().lower()
    if not source or not target:
        return None
    return f"argos-{source}-{target}"


def _default_piper_model_name(language: str) -> Optional[str]:
    return DEFAULT_PIPER_MODELS.get((language or "").strip().lower())


def _warn_if_model_missing(model_name: Optional[str]) -> None:
    if not model_name:
        return
    try:
        from localtrans.utils import ModelDownloader

        downloader = ModelDownloader()
        if model_name in downloader.AVAILABLE_MODELS and not downloader.is_downloaded(model_name):
            print(f"[!] 未检测到本地模型: {model_name}")
            print(f"    可运行: localtrans download {model_name}")
    except Exception as exc:
        logger.debug(f"检查模型状态失败({model_name}): {exc}")


def _create_direction_pipeline(
    base_config: RealtimeConfig,
    *,
    source_lang: str,
    target_lang: str,
    input_device_id: Optional[int],
    output_to_virtual_device: bool,
    direction_label: str,
    direct_asr_translate: bool = False,
    mt_model_name: Optional[str] = None,
    tts_model_name: Optional[str] = None,
) -> RealtimePipeline:
    pipeline_config = copy.deepcopy(base_config)
    pipeline_config.source_lang = source_lang
    pipeline_config.target_lang = target_lang
    pipeline_config.input_device_id = input_device_id
    pipeline_config.output_to_virtual_device = output_to_virtual_device
    pipeline_config.direction_label = direction_label
    pipeline_config.direct_asr_translate = direct_asr_translate
    pipeline_config.asr_backend_type = settings.asr.model_type

    asr_config = settings.asr.model_copy(deep=True)
    asr_config.language = source_lang
    asr_config.task = "translate" if direct_asr_translate else "transcribe"

    mt_config = settings.mt.model_copy(deep=True)
    mt_config.source_lang = source_lang
    mt_config.target_lang = target_lang
    if mt_model_name:
        mt_config.model_name = mt_model_name

    tts_config = settings.tts.model_copy(deep=True)
    tts_config.language = target_lang
    if tts_model_name is not None:
        tts_config.model_name = tts_model_name or None

    return RealtimePipeline(
        config=pipeline_config,
        asr_config=asr_config,
        mt_config=mt_config,
        tts_config=tts_config,
    )


def _iter_runtime_summaries(runner: RealtimePipeline | SessionOrchestrator) -> list[dict]:
    if isinstance(runner, SessionOrchestrator):
        return runner.get_runtime_summaries()
    return [runner.get_runtime_summary()]


def _format_runtime_summary(summary: dict) -> str:
    direction = str(summary.get("direction", "")).strip()
    prefix = f"{direction}: " if direction else ""

    input_device = summary.get("input_device_id")
    input_label = f"[{input_device}]" if input_device is not None else "自动"

    output_mode = str(summary.get("output_mode", "") or "speaker")
    output_device_id = summary.get("output_device_id")
    if output_mode == "virtual-device":
        output_label = f"虚拟声卡[{output_device_id}]"
    elif output_mode == "speaker-fallback":
        output_label = "默认扬声器(虚拟声卡不可用)"
    elif output_mode == "disabled":
        output_label = "关闭"
    else:
        output_label = "默认扬声器"

    asr_model = str(summary.get("asr_model", "") or "-")
    mt_model = str(summary.get("mt_model", "") or "-")
    if summary.get("tts_enabled"):
        tts_label = f"{summary.get('tts_engine')}/{summary.get('tts_model') or '-'}"
    else:
        tts_label = "off"

    return (
        f"{prefix}{summary.get('source_lang')}->{summary.get('target_lang')} | "
        f"输入{input_label} | 输出{output_label} | "
        f"ASR {summary.get('asr_backend')}/{asr_model} | "
        f"MT {summary.get('mt_backend')}/{mt_model} | "
        f"TTS {tts_label}"
    )


def run_interactive(
    source_lang: str = "zh",
    target_lang: str = "en",
    enable_tts: bool = True,
    stream_profile: str = "realtime",
    direct_asr_translate: bool = False,
    asr_streaming_mode: str = "legacy",
    asr_vad_mode: str = "webrtc",
    asr_partial_step: Optional[float] = None,
    input_device_id: Optional[int] = None,
    reverse_input_device_id: Optional[int] = None,
    bidirectional: bool = False,
    use_virtual_device: bool = True,
) -> int:
    """运行交互式翻译"""
    
    print(f"源语言: {source_lang}")
    print(f"目标语言: {target_lang}")
    print(f"语音合成: {'启用' if enable_tts else '禁用'}")
    print(f"运行模式: {stream_profile}")
    print(f"ASR直译: {'启用' if direct_asr_translate else '禁用'}")
    print(f"ASR流式方案: {asr_streaming_mode}")
    print(f"流式VAD: {asr_vad_mode}")
    print(f"会话模式: {'双向' if bidirectional else '单向'}")
    print(f"本地输入设备: {input_device_id if input_device_id is not None else '自动'}")
    if bidirectional:
        print(f"对方语音设备: {reverse_input_device_id if reverse_input_device_id is not None else '自动'}")
    print(f"正向输出: {'虚拟声卡' if use_virtual_device else '默认扬声器'}")
    print()

    profile = (stream_profile or "realtime").lower()
    if profile == "quality":
        settings.asr.beam_size = max(3, int(settings.asr.beam_size))
        settings.asr.vad_filter = True
    elif profile == "balanced":
        settings.asr.beam_size = max(2, int(settings.asr.beam_size))
        settings.asr.vad_filter = True
    else:
        settings.asr.beam_size = 1
        settings.asr.vad_filter = False

    if direct_asr_translate:
        if settings.asr.model_type not in {"whisper", "faster-whisper"}:
            logger.warning("ASR直译仅支持 whisper/faster-whisper，已自动关闭")
            direct_asr_translate = False
        if (target_lang or "").lower() != "en":
            logger.warning("ASR直译当前仅支持目标语言英语，已自动关闭")
            direct_asr_translate = False

    settings.asr.task = "translate" if direct_asr_translate else "transcribe"

    runner: RealtimePipeline | SessionOrchestrator
    if bidirectional:
        if input_device_id is None:
            print("[ERROR] 双向模式需要通过 --input-device 显式指定你的麦克风")
            print("        可先运行: localtrans devices")
            return 1

        base_config = RealtimeConfig(
            source_lang=source_lang,
            target_lang=target_lang,
            enable_tts=enable_tts,
            direct_asr_translate=direct_asr_translate,
            output_to_virtual_device=use_virtual_device,
            stream_profile=profile,
            asr_streaming_mode=asr_streaming_mode,
            asr_vad_mode=asr_vad_mode,
            asr_backend_type=settings.asr.model_type,
            asr_partial_decode_interval=asr_partial_step,
            input_device_id=input_device_id,
        )

        reverse_mt_model = None
        reverse_tts_model = None
        if str(settings.mt.model_type or "").lower() in {"argos", "argos-ct2"}:
            reverse_mt_model = _default_mt_model_name(target_lang, source_lang)
            _warn_if_model_missing(reverse_mt_model)
        if enable_tts and str(settings.tts.engine or "").lower() == "piper":
            reverse_tts_model = _default_piper_model_name(source_lang)
            _warn_if_model_missing(reverse_tts_model)

        forward_pipeline = _create_direction_pipeline(
            base_config,
            source_lang=source_lang,
            target_lang=target_lang,
            input_device_id=input_device_id,
            output_to_virtual_device=use_virtual_device,
            direction_label="我→对方",
            direct_asr_translate=direct_asr_translate,
        )
        reverse_pipeline = _create_direction_pipeline(
            base_config,
            source_lang=target_lang,
            target_lang=source_lang,
            input_device_id=reverse_input_device_id,
            output_to_virtual_device=False,
            direction_label="对方→我",
            direct_asr_translate=False,
            mt_model_name=reverse_mt_model,
            tts_model_name=reverse_tts_model,
        )
        runner = SessionOrchestrator(
            {
                "forward": forward_pipeline,
                "reverse": reverse_pipeline,
            }
        )
    else:
        runner = create_pipeline(
            source_lang=source_lang,
            target_lang=target_lang,
            enable_tts=enable_tts,
            use_virtual_device=use_virtual_device,
            stream_profile=profile,
            direct_asr_translate=direct_asr_translate,
            asr_streaming_mode=asr_streaming_mode,
            asr_vad_mode=asr_vad_mode,
            asr_partial_decode_interval=asr_partial_step,
            input_device_id=input_device_id,
        )

    def signal_handler(sig, frame):
        print("\n正在停止...")
        runner.stop()
        sys.exit(0)
    
    signal.signal(signal.SIGINT, signal_handler)
    
    print("正在启动翻译流水线...")
    
    if not runner.start():
        print("[ERROR] 启动失败！")
        return 1
    
    print("[OK] 翻译已启动，按 Ctrl+C 停止")
    print("会话摘要:")
    for summary in _iter_runtime_summaries(runner):
        print(f"- {_format_runtime_summary(summary)}")
    print()
    
    try:
        import time
        session_summary_lines = [f"- {_format_runtime_summary(summary)}" for summary in _iter_runtime_summaries(runner)]
        while runner.is_running:
            time.sleep(0.5)
            history = runner.get_history(limit=8 if bidirectional else 5)
            if history:
                print("\033[H\033[J", end="")
                print("=" * 50)
                print("翻译结果 (按 Ctrl+C 停止)")
                print("=" * 50)
                print("会话摘要:")
                for line in session_summary_lines:
                    print(line)
                for item in history:
                    direction = str(item.get("direction", "")).strip()
                    prefix = f"[{direction}] " if direction else ""
                    print(f"\n[源] {prefix}{item['source']}")
                    print(f"[译] {prefix}{item['translation']}")
    except KeyboardInterrupt:
        pass
    finally:
        runner.stop()
        print("\n[OK] 翻译已停止")
    return 0


def benchmark_latency(
    source_lang: str = "zh",
    target_lang: str = "en",
    text: str = "这是一次实时翻译延迟测试。",
    iterations: int = 30,
    warmup: int = 5,
    include_tts: bool = True,
    audio_file: Optional[str] = None,
) -> int:
    """基准测试：输出 MT/TTS/总链路延迟 P50/P95"""
    import time
    import numpy as np

    from localtrans.asr import ASREngine
    from localtrans.mt import MTEngine
    from localtrans.tts import TTSEngine

    if iterations <= 0:
        print("[ERROR] --iterations 必须大于0")
        return 1
    if warmup < 0:
        print("[ERROR] --warmup 不能小于0")
        return 1

    asr_engine = None
    asr_audio = None
    if audio_file:
        try:
            import soundfile as sf

            raw_audio, sample_rate = sf.read(audio_file, dtype="float32")
            if raw_audio.ndim > 1:
                raw_audio = np.mean(raw_audio, axis=1)
            asr_audio = np.asarray(raw_audio, dtype=np.float32)
            print(f"[OK] 已加载音频: {audio_file} ({len(asr_audio) / float(sample_rate):.2f}s)")
            asr_engine = ASREngine()
        except Exception as exc:
            print(f"[ERROR] 读取音频失败: {exc}")
            return 1

    mt_engine = MTEngine()
    tts_engine = TTSEngine() if include_tts else None

    asr_backend = asr_engine._backend.__class__.__name__ if asr_engine else "N/A"
    mt_backend = mt_engine._backend.__class__.__name__
    tts_backend = tts_engine._backend.__class__.__name__ if tts_engine else "N/A"

    print("\n基准测试配置:")
    print("-" * 50)
    print(f"source -> target : {source_lang} -> {target_lang}")
    print(f"iterations/warmup: {iterations}/{warmup}")
    print(f"ASR backend      : {asr_backend}")
    print(f"MT backend       : {mt_backend}")
    print(f"TTS backend      : {tts_backend}")
    print(f"audio_file       : {audio_file or 'N/A (text-only)'}")
    print("-" * 50)

    asr_latencies: list[float] = []
    mt_latencies: list[float] = []
    tts_latencies: list[float] = []
    total_latencies: list[float] = []

    preview_source = ""
    preview_translation = ""

    total_rounds = warmup + iterations
    for idx in range(total_rounds):
        iter_start = time.perf_counter()
        source_text = text
        asr_ms = 0.0

        if asr_engine is not None and asr_audio is not None:
            asr_start = time.perf_counter()
            asr_result = asr_engine.transcribe(asr_audio)
            asr_ms = (time.perf_counter() - asr_start) * 1000.0
            if asr_result.text.strip():
                source_text = asr_result.text.strip()

        mt_start = time.perf_counter()
        mt_result = mt_engine.translate(
            source_text,
            source_lang=source_lang,
            target_lang=target_lang,
        )
        mt_ms = (time.perf_counter() - mt_start) * 1000.0

        tts_ms = 0.0
        if tts_engine is not None:
            tts_start = time.perf_counter()
            _ = tts_engine.synthesize(mt_result.translated_text)
            tts_ms = (time.perf_counter() - tts_start) * 1000.0

        total_ms = (time.perf_counter() - iter_start) * 1000.0

        if idx >= warmup:
            asr_latencies.append(asr_ms)
            mt_latencies.append(mt_ms)
            tts_latencies.append(tts_ms)
            total_latencies.append(total_ms)

            if not preview_source:
                preview_source = source_text
                preview_translation = mt_result.translated_text

    def _line(name: str, values: list[float]) -> str:
        if not values:
            return f"{name:<8}  p50=0.0ms  p95=0.0ms  avg=0.0ms"
        avg = sum(values) / len(values)
        return (
            f"{name:<8}  p50={_percentile(values, 50):>6.1f}ms  "
            f"p95={_percentile(values, 95):>6.1f}ms  avg={avg:>6.1f}ms"
        )

    print("\n延迟统计:")
    print("-" * 50)
    if asr_engine is not None:
        print(_line("ASR", asr_latencies))
    print(_line("MT", mt_latencies))
    if tts_engine is not None:
        print(_line("TTS", tts_latencies))
    print(_line("TOTAL", total_latencies))
    print("-" * 50)

    if preview_source or preview_translation:
        print("\n样例输出:")
        print(f"[源] {preview_source}")
        print(f"[译] {preview_translation}")

    if tts_engine is not None:
        try:
            tts_engine.close()
        except Exception:
            pass

    return 0


def main() -> int:
    """主函数"""
    import argparse
    
    parser = argparse.ArgumentParser(
        prog="localtrans",
        description="AI实时翻译软件 - 端侧本地部署",
    )
    
    parser.add_argument("-v", "--version", action="version", version=f"%(prog)s {__version__}")
    parser.add_argument("--debug", action="store_true", help="启用调试模式")
    
    subparsers = parser.add_subparsers(dest="command", help="命令")
    
    run_parser = subparsers.add_parser("run", help="启动实时翻译")
    run_parser.add_argument("-s", "--source", default="zh", help="源语言 (默认: zh)")
    run_parser.add_argument("-t", "--target", default="en", help="目标语言 (默认: en)")
    run_parser.add_argument("--no-tts", action="store_true", help="禁用语音合成")
    run_parser.add_argument("--input-device", type=int, default=None, help="输入设备ID；双向模式下该参数用于你的麦克风")
    run_parser.add_argument(
        "--reverse-input-device",
        type=int,
        default=None,
        help="双向模式下对方语音输入设备ID；不指定则自动探测会议回录/Stereo Mix/VB-CABLE",
    )
    run_parser.add_argument("--bidirectional", action="store_true", help="启用双向会话")
    run_parser.add_argument("--no-virtual-output", action="store_true", help="正向语音不输出到虚拟声卡，改为默认扬声器")
    run_parser.add_argument(
        "--profile",
        default="realtime",
        choices=["realtime", "balanced", "quality"],
        help="实时链路档位 (默认: realtime)",
    )
    run_parser.add_argument(
        "--asr-direct-translate",
        action="store_true",
        help="启用Whisper ASR直译（中->英，跳过MT）",
    )
    run_parser.add_argument(
        "--asr-streaming-mode",
        default="legacy",
        choices=["legacy", "managed"],
        help="ASR流式方案：legacy 为原始切窗，managed 为本地状态机/VAD",
    )
    run_parser.add_argument(
        "--asr-vad-mode",
        default="webrtc",
        choices=["webrtc", "energy", "silero"],
        help="managed 流式方案使用的VAD模式",
    )
    run_parser.add_argument(
        "--asr-partial-step",
        type=float,
        default=None,
        help="managed 流式方案的 partial 解码步长（秒）",
    )
    
    subparsers.add_parser("devices", help="列出音频设备")
    subparsers.add_parser("check", help="检查系统环境")
    subparsers.add_parser("models", help="列出可用模型")
    
    download_parser = subparsers.add_parser("download", help="下载模型")
    download_parser.add_argument("model", help="模型名称")
    download_parser.add_argument("-f", "--force", action="store_true", help="强制重新下载")

    bench_parser = subparsers.add_parser("benchmark", help="延迟基准测试")
    bench_parser.add_argument("-s", "--source", default="zh", help="源语言 (默认: zh)")
    bench_parser.add_argument("-t", "--target", default="en", help="目标语言 (默认: en)")
    bench_parser.add_argument("--text", default="这是一次实时翻译延迟测试。", help="文本模式测试输入")
    bench_parser.add_argument("--iterations", type=int, default=30, help="统计轮数")
    bench_parser.add_argument("--warmup", type=int, default=5, help="预热轮数")
    bench_parser.add_argument("--no-tts", action="store_true", help="基准中禁用TTS阶段")
    bench_parser.add_argument("--audio-file", help="可选WAV/音频文件，提供后将包含ASR阶段")
    
    args = parser.parse_args()
    
    setup_logging(args.debug)
    show_welcome()
    
    if args.command == "run":
        return run_interactive(
            source_lang=args.source,
            target_lang=args.target,
            enable_tts=not args.no_tts,
            stream_profile=args.profile,
            direct_asr_translate=args.asr_direct_translate,
            asr_streaming_mode=args.asr_streaming_mode,
            asr_vad_mode=args.asr_vad_mode,
            asr_partial_step=args.asr_partial_step,
            input_device_id=args.input_device,
            reverse_input_device_id=args.reverse_input_device,
            bidirectional=args.bidirectional,
            use_virtual_device=not args.no_virtual_output,
        )
    elif args.command == "devices":
        show_devices()
    elif args.command == "check":
        check_virtual_device()
    elif args.command == "models":
        show_models()
    elif args.command == "download":
        download_model(args.model, args.force)
    elif args.command == "benchmark":
        return benchmark_latency(
            source_lang=args.source,
            target_lang=args.target,
            text=args.text,
            iterations=args.iterations,
            warmup=args.warmup,
            include_tts=not args.no_tts,
            audio_file=args.audio_file,
        )
    else:
        parser.print_help()
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
