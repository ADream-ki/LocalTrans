import os
import re
import subprocess
import sys
from pathlib import Path

import pytest
import sounddevice as sd
import soundfile as sf

from localtrans.config import TTSConfig
from localtrans.tts.engine import TTSEngine


ROOT = Path(__file__).resolve().parents[1]
AUDIO_DIR = ROOT / "tests" / "fixtures" / "audio"
EN_AUDIO = AUDIO_DIR / "OSR_us_000_0010_8k.wav"
ZH_AUDIO = AUDIO_DIR / "zh_piper_sample.wav"
SCRIPT = ROOT / "scripts" / "real_audio_e2e_test.py"


def _ensure_english_audio() -> Path:
    AUDIO_DIR.mkdir(parents=True, exist_ok=True)
    if EN_AUDIO.exists() and EN_AUDIO.stat().st_size > 1000:
        return EN_AUDIO
    url = "http://www.voiptroubleshooter.com/open_speech/american/OSR_us_000_0010_8k.wav"
    import urllib.request

    urllib.request.urlretrieve(url, EN_AUDIO)
    return EN_AUDIO


def _ensure_chinese_audio() -> Path:
    AUDIO_DIR.mkdir(parents=True, exist_ok=True)
    if ZH_AUDIO.exists() and ZH_AUDIO.stat().st_size > 1000:
        return ZH_AUDIO

    model = Path(os.path.expanduser("~/.localtrans/models/tts/piper-zh_CN-huayan/zh/zh_CN/huayan/medium/zh_CN-huayan-medium.onnx"))
    if not model.exists():
        pytest.skip("missing piper zh model")

    cfg = TTSConfig(engine="piper", model_path=model, language="zh", sample_rate=22050)
    tts = TTSEngine(cfg)
    result = tts.synthesize("你好，这是本地翻译双向测试音频。请测试中文到英文方向。")
    sf.write(str(ZH_AUDIO), result.audio, result.sample_rate, subtype="PCM_16")
    return ZH_AUDIO


def _pick_cable_route() -> tuple[int, int]:
    devices = list(sd.query_devices())
    input_id = -1
    output_id = -1
    for idx, dev in enumerate(devices):
        name = str(dev.get("name", "")).lower()
        if dev.get("max_input_channels", 0) > 0 and ("cable output" in name or "vb-audio" in name):
            input_id = idx
            break
    for idx, dev in enumerate(devices):
        name = str(dev.get("name", "")).lower()
        if dev.get("max_output_channels", 0) > 0 and ("cable input" in name or "cable in" in name):
            output_id = idx
            break
    if input_id < 0 or output_id < 0:
        pytest.skip("missing vb-cable route devices")
    return input_id, output_id


def _run_case(audio_file: Path, source: str, target: str, asr_model_path: Path, input_id: int, output_id: int) -> int:
    cmd = [
        sys.executable,
        str(SCRIPT),
        "--audio-file",
        str(audio_file),
        "--source",
        source,
        "--target",
        target,
        "--asr-model-path",
        str(asr_model_path),
        "--input-device",
        str(input_id),
        "--output-device",
        str(output_id),
    ]
    proc = subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True, timeout=300)
    assert proc.returncode == 0, proc.stdout + "\n" + proc.stderr
    m = re.search(r"results=(\d+)", proc.stdout)
    assert m, proc.stdout
    return int(m.group(1))


@pytest.mark.real_audio
def test_real_audio_en_to_zh():
    en_audio = _ensure_english_audio()
    asr_model = Path(os.path.expanduser("~/.localtrans/models/asr/vosk-model-small-en-us-0.15"))
    if not asr_model.exists():
        pytest.skip("missing english vosk model")
    input_id, output_id = _pick_cable_route()
    results = _run_case(en_audio, "en", "zh", asr_model, input_id, output_id)
    assert results >= 1


@pytest.mark.real_audio
def test_real_audio_zh_to_en():
    zh_audio = _ensure_chinese_audio()
    asr_model = Path(os.path.expanduser("~/.localtrans/models/asr/vosk-model-small-cn-0.22"))
    if not asr_model.exists():
        pytest.skip("missing chinese vosk model")
    input_id, output_id = _pick_cable_route()
    results = _run_case(zh_audio, "zh", "en", asr_model, input_id, output_id)
    assert results >= 1

