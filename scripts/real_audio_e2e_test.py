"""
真实音频端到端测试

流程:
1) 自动选择立体声混音/VB-CABLE输入设备
2) 启动 RealtimePipeline（真实音频采集 + ASR + MT）
3) 使用系统 TTS 播放中文测试句子
4) 输出识别/翻译结果条数与样例
"""

from __future__ import annotations

import subprocess
import sys
import time
import argparse
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

from localtrans.audio.capturer import AudioCapturer
from localtrans.config import settings
from localtrans.pipeline.realtime import RealtimeConfig, RealtimePipeline

import sounddevice as sd
import soundfile as sf


def _pick_input_device() -> Optional[int]:
    capturer = AudioCapturer()
    devices = capturer.list_devices()
    preferred_groups = [
        ("立体声混音", "stereo mix"),
        ("cable output", "vb-audio", "loopback"),
    ]
    for keywords in preferred_groups:
        for dev in devices:
            name = str(dev.get("name", "")).lower()
            if any(k in name for k in keywords):
                return int(dev["id"])
    return None


def _speak_test_text() -> None:
    text = "本地翻译真实音频测试，现在开始。你好，世界。"
    cmd = (
        "Add-Type -AssemblyName System.Speech; "
        "$s=New-Object System.Speech.Synthesis.SpeechSynthesizer; "
        f"$s.Speak('{text}')"
    )
    subprocess.run(
        ["powershell", "-NoProfile", "-Command", cmd],
        check=False,
        capture_output=True,
        text=True,
    )


def _pick_output_device_for_input(input_device_id: int) -> Optional[int]:
    devices = list(sd.query_devices())
    input_name = str(devices[input_device_id].get("name", "")).lower() if 0 <= input_device_id < len(devices) else ""
    output_candidates = [(idx, str(dev.get("name", "")).lower()) for idx, dev in enumerate(devices) if dev.get("max_output_channels", 0) > 0]

    if "stereo mix" in input_name or "立体声混音" in input_name:
        for idx, name in output_candidates:
            if "realtek" in name and ("speakers" in name or "扬声器" in name):
                return int(idx)

    if "cable output" in input_name or "vb-audio" in input_name:
        for idx, name in output_candidates:
            if "cable input" in name or "cable in" in name:
                return int(idx)

    return None


def _play_audio_file(path: Path, output_device_id: Optional[int]) -> float:
    data, sr = sf.read(str(path), dtype="float32", always_2d=False)
    try:
        sd.play(data, sr, blocking=True, device=output_device_id)
    except Exception:
        sd.play(data, sr, blocking=True)
    return float(len(data) / float(sr)) if sr > 0 else 0.0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--audio-file", default="", help="待播放的测试音频文件路径")
    parser.add_argument("--source", default="zh", help="源语言")
    parser.add_argument("--target", default="en", help="目标语言")
    parser.add_argument("--asr-model-path", default="", help="ASR模型目录路径")
    parser.add_argument("--input-device", type=int, default=-1, help="覆盖输入设备ID")
    parser.add_argument("--output-device", type=int, default=-1, help="覆盖播放输出设备ID")
    args = parser.parse_args()

    input_device_id = int(args.input_device) if args.input_device >= 0 else _pick_input_device()
    if input_device_id is None:
        print("real_audio_e2e: no suitable loopback input device found")
        return 2
    output_device_id = (
        int(args.output_device)
        if args.output_device >= 0
        else _pick_output_device_for_input(input_device_id)
    )

    results = []

    asr_cfg = settings.asr.model_copy(deep=True)
    asr_cfg.language = args.source
    if args.asr_model_path:
        asr_cfg.model_type = "vosk"
        asr_cfg.model_path = Path(args.asr_model_path)
        asr_cfg.model_name = None

    cfg = RealtimeConfig(
        source_lang=args.source,
        target_lang=args.target,
        enable_tts=False,
        output_mode="system",
        input_device_id=input_device_id,
        io_buffer_ms=60,
    )
    pipeline = RealtimePipeline(
        config=cfg,
        asr_config=asr_cfg,
        mt_config=settings.mt,
        tts_config=settings.tts,
        result_callback=lambda item: results.append(item),
    )

    ok = pipeline.start()
    if not ok:
        print("real_audio_e2e: pipeline_start_failed")
        return 3

    try:
        time.sleep(1.0)
        audio_file = Path(args.audio_file).expanduser().resolve() if args.audio_file else None
        if audio_file and audio_file.exists():
            duration = _play_audio_file(audio_file, output_device_id=output_device_id)
            time.sleep(min(3.0, max(0.5, duration * 0.2)))
        else:
            _speak_test_text()
            time.sleep(7.0)
    finally:
        pipeline.stop()

    print(
        "real_audio_e2e: "
        f"input_device={input_device_id}, "
        f"output_device={output_device_id}, "
        f"results={len(results)}"
    )
    for item in results[:3]:
        src = str(item.get("source", "")).strip()
        dst = str(item.get("translation", "")).strip()
        print(f"  source={src}")
        print(f"  trans={dst}")

    return 0 if results else 4


if __name__ == "__main__":
    raise SystemExit(main())
