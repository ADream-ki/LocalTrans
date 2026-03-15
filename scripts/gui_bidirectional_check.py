"""GUI双向联调预检脚本。

用途:
- 检查双向 GUI 会话所需的模型/设备是否齐全
- 给出推荐的本地麦克风 / 对方回录设备
- 输出最近 GUI 日志路径，便于人工联调时排障
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Optional


ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from localtrans.audio import AudioCapturer, VirtualAudioDevice
from localtrans.config import settings
from localtrans.pipeline.realtime import RealtimePipeline
from localtrans.utils import ModelDownloader


MIC_PATTERNS = (
    "mic",
    "microphone",
    "麦克风",
    "mic in",
    "array",
    "阵列",
    "headset",
    "耳机",
)


@dataclass
class DeviceCandidate:
    device_id: int
    name: str
    channels: int
    sample_rate: float


def _device_score(name: str, patterns: Iterable[str]) -> int:
    lowered = (name or "").lower()
    return sum(1 for pattern in patterns if pattern in lowered)


def list_input_devices() -> list[DeviceCandidate]:
    capturer = AudioCapturer()
    devices = []
    for dev in capturer.list_devices():
        devices.append(
            DeviceCandidate(
                device_id=int(dev["id"]),
                name=str(dev["name"]),
                channels=int(dev["channels"]),
                sample_rate=float(dev["sample_rate"]),
            )
        )
    return devices


def recommend_loopback_device(devices: list[DeviceCandidate]) -> Optional[DeviceCandidate]:
    patterns = tuple(getattr(RealtimePipeline, "_AUTO_INPUT_PATTERNS", ()))
    ranked = sorted(
        devices,
        key=lambda dev: (_device_score(dev.name, patterns), dev.channels),
        reverse=True,
    )
    best = ranked[0] if ranked else None
    if best and _device_score(best.name, patterns) > 0:
        return best
    return None


def recommend_microphone_device(devices: list[DeviceCandidate]) -> Optional[DeviceCandidate]:
    auto_patterns = tuple(getattr(RealtimePipeline, "_AUTO_INPUT_PATTERNS", ()))
    ranked = sorted(
        devices,
        key=lambda dev: (
            _device_score(dev.name, MIC_PATTERNS),
            -_device_score(dev.name, auto_patterns),
            dev.channels,
        ),
        reverse=True,
    )
    if not ranked:
        return None
    best = ranked[0]
    if _device_score(best.name, auto_patterns) > 0 and _device_score(best.name, MIC_PATTERNS) <= 0:
        for dev in ranked[1:]:
            if _device_score(dev.name, auto_patterns) == 0:
                return dev
    return best


def required_models(source_lang: str, target_lang: str, enable_tts: bool) -> list[str]:
    models = ["funasr-sensevoice-small", f"argos-{source_lang}-{target_lang}", f"argos-{target_lang}-{source_lang}"]
    if enable_tts:
        if target_lang == "zh":
            models.append("piper-zh_CN-huayan")
        elif target_lang == "en":
            models.append("piper-en_US-lessac")
        if source_lang == "zh":
            models.append("piper-zh_CN-huayan")
        elif source_lang == "en":
            models.append("piper-en_US-lessac")
    return list(dict.fromkeys(models))


def is_model_available(downloader: ModelDownloader, model_name: str) -> bool:
    if downloader.is_downloaded(model_name):
        return True
    model_path = downloader.get_model_path(model_name)
    if model_path is None or not model_path.exists():
        return False
    try:
        next(model_path.iterdir())
        return True
    except StopIteration:
        return False
    except Exception:
        return True


def latest_gui_log() -> Optional[Path]:
    candidates = sorted(
        settings.logs_dir.glob("localtrans_gui_*.log"),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    return candidates[0] if candidates else None


def print_section(title: str) -> None:
    print()
    print("=" * 72)
    print(title)
    print("=" * 72)


def print_device(name: str, device: Optional[DeviceCandidate]) -> None:
    if not device:
        print(f"{name}: 未找到")
        return
    print(
        f"{name}: [{device.device_id}] {device.name} | "
        f"{device.channels}ch | {int(device.sample_rate)} Hz"
    )


def launch_gui() -> int:
    gui_exe = ROOT / "dist" / "localtrans-gui.exe"
    if gui_exe.exists():
        subprocess.Popen([str(gui_exe)], cwd=str(ROOT))
        print(f"已启动 GUI: {gui_exe}")
        return 0
    subprocess.Popen([sys.executable, "-m", "localtrans.gui.main"], cwd=str(ROOT))
    print("已通过 Python 模块方式启动 GUI")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="LocalTrans GUI 双向联调预检")
    parser.add_argument("-s", "--source", default="zh", help="我的语言，默认 zh")
    parser.add_argument("-t", "--target", default="en", help="对方语言，默认 en")
    parser.add_argument("--no-tts", action="store_true", help="按无语音合成模式检查")
    parser.add_argument("--launch", action="store_true", help="预检完成后直接启动 GUI")
    args = parser.parse_args()

    source_lang = (args.source or "zh").strip().lower()
    target_lang = (args.target or "en").strip().lower()
    enable_tts = not args.no_tts

    downloader = ModelDownloader()
    devices = list_input_devices()
    mic = recommend_microphone_device(devices)
    loopback = recommend_loopback_device(devices)

    print_section("双向 GUI 预检")
    print(f"工作目录: {ROOT}")
    print(f"数据目录: {settings.data_dir}")
    print(f"日志目录: {settings.logs_dir}")
    print(f"联调方向: {source_lang} <-> {target_lang}")
    print(f"虚拟声卡: {'可用' if VirtualAudioDevice.check_vb_cable_installed() else '未检测到'}")

    print_section("推荐设备")
    print_device("本地麦克风", mic)
    print_device("对方回录", loopback)
    if mic is None:
        print("建议先运行: .\\dist\\localtrans.exe devices")
    if loopback is None:
        print("未识别到会议回录设备。可检查 Stereo Mix / VB-CABLE / 会议软件输出设置。")

    print_section("模型状态")
    missing_models: list[str] = []
    for model_name in required_models(source_lang, target_lang, enable_tts):
        downloaded = is_model_available(downloader, model_name)
        status = "OK " if downloaded else "MISS"
        print(f"[{status}] {model_name}")
        if not downloaded:
            missing_models.append(model_name)

    if missing_models:
        print()
        print("缺失模型下载建议:")
        for model_name in missing_models:
            print(f"  .\\dist\\localtrans.exe download {model_name}")

    print_section("GUI 配置建议")
    print("1. 勾选: 启用双向翻译")
    print(f"2. 源语言/目标语言: {source_lang} -> {target_lang}")
    if mic is not None:
        print(f"3. 输入设备: [{mic.device_id}] {mic.name}")
    else:
        print("3. 输入设备: 手动选择你的麦克风")
    if loopback is not None:
        print(f"4. 对方语音设备: [{loopback.device_id}] {loopback.name}")
    else:
        print("4. 对方语音设备: 手动选择 Stereo Mix / VB-CABLE / 会议回录")
    print("5. ASR: funasr / FunAudioLLM/SenseVoiceSmall")
    print("6. 流式方案: managed")
    print("7. 运行档位: quality")
    print("8. 中文高准确率场景建议 buffer 约 2.6s")
    print(f"9. TTS: {'启用' if enable_tts else '禁用'}")
    print("10. 启动后查看 GUI 顶部“当前会话”摘要，确认两条链路和设备编号都正确")

    print_section("最近 GUI 日志")
    last_log = latest_gui_log()
    if last_log:
        print(last_log)
    else:
        print("尚未发现 GUI 日志")

    if args.launch:
        print_section("启动 GUI")
        return launch_gui()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
