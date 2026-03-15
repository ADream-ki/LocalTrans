"""双向 GUI 预检脚本测试"""

from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import shutil
import sys


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "scripts" / "gui_bidirectional_check.py"
SPEC = spec_from_file_location("gui_bidirectional_check", SCRIPT_PATH)
gui_check = module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = gui_check
SPEC.loader.exec_module(gui_check)


class TestGuiBidirectionalCheck:
    def test_required_models_for_zh_en_with_tts(self):
        models = gui_check.required_models("zh", "en", enable_tts=True)

        assert "funasr-sensevoice-small" in models
        assert "argos-zh-en" in models
        assert "argos-en-zh" in models
        assert "piper-zh_CN-huayan" in models
        assert "piper-en_US-lessac" in models

    def test_recommend_loopback_device_prefers_stereo_mix(self):
        devices = [
            gui_check.DeviceCandidate(1, "Microphone Array", 2, 48000.0),
            gui_check.DeviceCandidate(2, "Stereo Mix (Realtek)", 2, 48000.0),
            gui_check.DeviceCandidate(3, "CABLE Output (VB-Audio)", 2, 48000.0),
        ]

        chosen = gui_check.recommend_loopback_device(devices)

        assert chosen is not None
        assert chosen.device_id in {2, 3}

    def test_recommend_microphone_device_avoids_loopback_when_possible(self):
        devices = [
            gui_check.DeviceCandidate(2, "Stereo Mix (Realtek)", 2, 48000.0),
            gui_check.DeviceCandidate(7, "Microphone Array (Intel)", 2, 48000.0),
            gui_check.DeviceCandidate(9, "CABLE Output (VB-Audio)", 2, 48000.0),
        ]

        chosen = gui_check.recommend_microphone_device(devices)

        assert chosen is not None
        assert chosen.device_id == 7

    def test_is_model_available_accepts_existing_directory_without_cache(self):
        base_dir = Path(__file__).resolve().parents[1] / ".pytest_tmp" / "gui_check_models"
        shutil.rmtree(base_dir, ignore_errors=True)
        base_dir.mkdir(parents=True, exist_ok=True)

        class StubDownloader:
            def is_downloaded(self, model_name):
                return False

            def get_model_path(self, model_name):
                path = base_dir / model_name
                path.mkdir(parents=True, exist_ok=True)
                (path / "marker.bin").write_text("ok", encoding="utf-8")
                return path

        assert gui_check.is_model_available(StubDownloader(), "funasr-sensevoice-small") is True
