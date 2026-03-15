"""CLI集成测试"""

import io
import sys
import threading
import time
from types import SimpleNamespace
from unittest.mock import patch

import numpy as np

from localtrans.asr.engine import TranscriptionResult
from localtrans.config import settings
from localtrans.main import main as cli_main
from localtrans.pipeline.realtime import RealtimeConfig, RealtimePipeline


class FakeASREngine:
    """返回逐步增长文本的假ASR，驱动 managed 流式增量输出。"""

    def transcribe(self, audio, **kwargs):
        audio_len = len(audio)
        if audio_len >= 3200:
            text = "你好世界"
        elif audio_len >= 1600:
            text = "你好"
        else:
            text = "你"
        return TranscriptionResult(
            text=text,
            language="zh",
            confidence=0.95,
            start_time=0.0,
            end_time=audio_len / 16000.0,
        )


class FakeMTEngine:
    def translate(self, text, source_lang="zh", target_lang="en"):
        return SimpleNamespace(translated_text=f"EN:{text}")


class FakeAudioCapturer:
    pipeline = None

    def __init__(self, *args, **kwargs):
        self._thread = None

    def list_devices(self):
        return []

    def start(self, callback):
        def _feed():
            speech = np.full(800, 22000, dtype=np.int16)
            silence = np.zeros(800, dtype=np.int16)
            for chunk in [speech, speech, speech, speech, silence, silence, silence]:
                callback(SimpleNamespace(data=chunk))
                time.sleep(0.03)

            def _stop_later():
                time.sleep(0.7)
                if FakeAudioCapturer.pipeline is not None:
                    FakeAudioCapturer.pipeline.stop()

            threading.Thread(target=_stop_later, daemon=True).start()

        self._thread = threading.Thread(target=_feed, daemon=True)
        self._thread.start()

    def stop(self):
        if self._thread is not None and self._thread.is_alive():
            self._thread.join(timeout=2.0)


class FakeVirtualAudioDevice:
    def __init__(self, *args, **kwargs):
        self.is_available = False


class FakeCliRealtimePipeline:
    instances = []

    def __init__(self, config=None, result_callback=None, asr_config=None, mt_config=None, tts_config=None):
        self.config = config
        self._asr_config = asr_config
        self._mt_config = mt_config
        self._tts_config = tts_config
        self.is_running = False
        FakeCliRealtimePipeline.instances.append(self)

    def start(self):
        self.is_running = True
        return True

    def stop(self):
        self.is_running = False

    def get_history(self, limit=50):
        return []

    def get_runtime_summary(self):
        return {
            "direction": self.config.direction_label,
            "source_lang": self.config.source_lang,
            "target_lang": self.config.target_lang,
            "input_device_id": self.config.input_device_id,
            "output_mode": "virtual-device" if self.config.output_to_virtual_device else "speaker",
            "output_device_id": 66 if self.config.output_to_virtual_device else None,
            "asr_backend": getattr(self._asr_config, "model_type", "funasr"),
            "asr_model": getattr(self._asr_config, "model_name", "") or "FunAudioLLM/SenseVoiceSmall",
            "mt_backend": getattr(self._mt_config, "model_type", "argos-ct2"),
            "mt_model": getattr(self._mt_config, "model_name", ""),
            "tts_enabled": bool(self.config.enable_tts),
            "tts_engine": getattr(self._tts_config, "engine", "piper"),
            "tts_model": getattr(self._tts_config, "model_name", ""),
        }


class FakeBidirectionalSession:
    def __init__(self, directions):
        self.directions = dict(directions)
        self.pipelines = list(self.directions.values())
        self.is_running = False
        self._history_calls = 0

    def start(self):
        for pipeline in self.pipelines:
            pipeline.start()
        self.is_running = True
        return True

    def stop(self):
        self.is_running = False
        for pipeline in self.pipelines:
            pipeline.stop()

    def get_history(self, limit=50):
        self._history_calls += 1
        history = [
            {"source": "你好", "translation": "hello", "direction": "我→对方", "timestamp": 1.0},
            {"source": "hello", "translation": "你好", "direction": "对方→我", "timestamp": 2.0},
        ]
        self.stop()
        return history[-limit:]

    def get_runtime_summaries(self):
        return [pipeline.get_runtime_summary() for pipeline in self.pipelines]


class TestCLI:
    def test_run_command_managed_mode_emits_incremental_history(self):
        create_kwargs = {}

        def fake_create_pipeline(**kwargs):
            create_kwargs.update(kwargs)
            pipeline = RealtimePipeline(
                RealtimeConfig(
                    source_lang=kwargs["source_lang"],
                    target_lang=kwargs["target_lang"],
                    enable_tts=kwargs["enable_tts"],
                    direct_asr_translate=kwargs["direct_asr_translate"],
                    output_to_virtual_device=False,
                    stream_profile=kwargs["stream_profile"],
                    asr_streaming_mode=kwargs["asr_streaming_mode"],
                    asr_vad_mode=kwargs["asr_vad_mode"],
                    asr_vad_enabled=False,
                    asr_buffer_duration=0.12,
                    asr_min_buffer_duration=0.08,
                    asr_max_buffer_duration=0.3,
                    asr_partial_decode_interval=kwargs["asr_partial_decode_interval"] or 0.08,
                    stream_flush_interval=0.05,
                    stream_min_chars=1,
                    stream_max_chars=2,
                    translation_batch_chars=1,
                    input_device_id=0,
                )
            )
            FakeAudioCapturer.pipeline = pipeline
            return pipeline

        stdout = io.StringIO()
        argv = [
            "localtrans",
            "run",
            "--no-tts",
            "-s",
            "zh",
            "-t",
            "en",
            "--asr-streaming-mode",
            "managed",
            "--asr-vad-mode",
            "energy",
            "--asr-partial-step",
            "0.08",
        ]

        with patch("localtrans.main.create_pipeline", side_effect=fake_create_pipeline):
            with patch("localtrans.main.setup_logging"):
                with patch("localtrans.main.show_welcome"):
                    with patch("localtrans.pipeline.realtime.AudioCapturer", FakeAudioCapturer):
                            with patch("localtrans.pipeline.realtime.VirtualAudioDevice", FakeVirtualAudioDevice):
                                with patch("localtrans.pipeline.realtime.MTEngine", return_value=FakeMTEngine()):
                                    with patch("localtrans.pipeline.realtime.ASREngine", return_value=FakeASREngine()):
                                        with patch("localtrans.asr.stream.ASREngine", return_value=FakeASREngine()):
                                            with patch.object(sys, "argv", argv):
                                                with patch("sys.stdout", stdout):
                                                    exit_code = cli_main()

        output = stdout.getvalue()
        assert exit_code == 0
        assert create_kwargs["asr_streaming_mode"] == "managed"
        assert create_kwargs["asr_vad_mode"] == "energy"
        assert create_kwargs["asr_partial_decode_interval"] == 0.08
        assert "[源] 你好" in output
        assert "[源] 世界" in output
        assert "[译] EN:你好" in output
        assert "[译] EN:世界" in output

    def test_run_command_bidirectional_builds_two_pipelines(self):
        FakeCliRealtimePipeline.instances.clear()
        stdout = io.StringIO()
        argv = [
            "localtrans",
            "run",
            "--bidirectional",
            "--input-device",
            "3",
            "--reverse-input-device",
            "7",
            "-s",
            "zh",
            "-t",
            "en",
        ]

        with patch.object(settings.asr, "model_type", "funasr"):
            with patch.object(settings.mt, "model_type", "argos-ct2"):
                with patch.object(settings.tts, "engine", "piper"):
                    with patch.object(settings.tts, "model_name", "piper-en_US-lessac"):
                        with patch("localtrans.main.RealtimePipeline", FakeCliRealtimePipeline):
                            with patch("localtrans.main.SessionOrchestrator", FakeBidirectionalSession):
                                with patch("localtrans.main.setup_logging"):
                                    with patch("localtrans.main.show_welcome"):
                                        with patch("time.sleep", return_value=None):
                                            with patch.object(sys, "argv", argv):
                                                with patch("sys.stdout", stdout):
                                                    exit_code = cli_main()

        output = stdout.getvalue()
        assert exit_code == 0
        assert len(FakeCliRealtimePipeline.instances) == 2

        forward, reverse = FakeCliRealtimePipeline.instances
        assert forward.config.source_lang == "zh"
        assert forward.config.target_lang == "en"
        assert forward.config.input_device_id == 3
        assert forward.config.output_to_virtual_device is True
        assert forward.config.direction_label == "我→对方"
        assert forward.config.asr_streaming_mode == "managed"

        assert reverse.config.source_lang == "en"
        assert reverse.config.target_lang == "zh"
        assert reverse.config.input_device_id == 7
        assert reverse.config.output_to_virtual_device is False
        assert reverse.config.direction_label == "对方→我"
        assert reverse.config.asr_streaming_mode == "legacy"
        assert reverse._mt_config.model_name == "argos-en-zh"
        assert reverse._tts_config.model_name == "piper-zh_CN-huayan"

        assert "[源] [我→对方] 你好" in output
        assert "[译] [我→对方] hello" in output
        assert "[源] [对方→我] hello" in output
        assert "[译] [对方→我] 你好" in output
        assert "会话摘要:" in output
        assert "我→对方: zh->en" in output
        assert "对方→我: en->zh" in output

    def test_run_command_bidirectional_requires_input_device(self):
        stdout = io.StringIO()
        argv = [
            "localtrans",
            "run",
            "--bidirectional",
            "--no-tts",
            "-s",
            "zh",
            "-t",
            "en",
        ]

        with patch("localtrans.main.setup_logging"):
            with patch("localtrans.main.show_welcome"):
                with patch.object(sys, "argv", argv):
                    with patch("sys.stdout", stdout):
                        exit_code = cli_main()

        output = stdout.getvalue()
        assert exit_code == 1
        assert "--input-device" in output
