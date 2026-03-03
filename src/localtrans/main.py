"""
主程序入口
CLI命令行接口
"""

import sys
import signal
from typing import Optional

from loguru import logger

from localtrans import __version__
from localtrans.config import settings
from localtrans.pipeline import RealtimePipeline, create_pipeline
from localtrans.audio import VirtualAudioDevice


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


def run_interactive(
    source_lang: str = "zh",
    target_lang: str = "en",
    enable_tts: bool = True,
) -> None:
    """运行交互式翻译"""
    
    print(f"源语言: {source_lang}")
    print(f"目标语言: {target_lang}")
    print(f"语音合成: {'启用' if enable_tts else '禁用'}")
    print()
    
    pipeline = create_pipeline(
        source_lang=source_lang,
        target_lang=target_lang,
        enable_tts=enable_tts,
    )
    
    def signal_handler(sig, frame):
        print("\n正在停止...")
        pipeline.stop()
        sys.exit(0)
    
    signal.signal(signal.SIGINT, signal_handler)
    
    print("正在启动翻译流水线...")
    
    if not pipeline.start():
        print("[ERROR] 启动失败！")
        return
    
    print("[OK] 翻译已启动，按 Ctrl+C 停止")
    print()
    
    try:
        import time
        while pipeline.is_running:
            time.sleep(0.5)
            history = pipeline.get_history(limit=5)
            if history:
                print("\033[H\033[J", end="")
                print("=" * 50)
                print("翻译结果 (按 Ctrl+C 停止)")
                print("=" * 50)
                for item in history:
                    print(f"\n[源] {item['source']}")
                    print(f"[译] {item['translation']}")
    except KeyboardInterrupt:
        pass
    finally:
        pipeline.stop()
        print("\n[OK] 翻译已停止")


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
    
    subparsers.add_parser("devices", help="列出音频设备")
    subparsers.add_parser("check", help="检查系统环境")
    subparsers.add_parser("models", help="列出可用模型")
    
    download_parser = subparsers.add_parser("download", help="下载模型")
    download_parser.add_argument("model", help="模型名称")
    download_parser.add_argument("-f", "--force", action="store_true", help="强制重新下载")
    
    args = parser.parse_args()
    
    setup_logging(args.debug)
    show_welcome()
    
    if args.command == "run":
        run_interactive(
            source_lang=args.source,
            target_lang=args.target,
            enable_tts=not args.no_tts,
        )
    elif args.command == "devices":
        show_devices()
    elif args.command == "check":
        check_virtual_device()
    elif args.command == "models":
        show_models()
    elif args.command == "download":
        download_model(args.model, args.force)
    else:
        parser.print_help()
    
    return 0


if __name__ == "__main__":
    sys.exit(main())
