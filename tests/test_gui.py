"""GUI装配测试"""

from types import SimpleNamespace
from unittest.mock import patch

from localtrans.config import settings
from localtrans.gui.app import MainWindow
from localtrans.pipeline.realtime import RealtimeConfig


class TestGuiAssembly:
    def test_create_direction_pipeline_clones_realtime_config(self, monkeypatch):
        monkeypatch.setattr(settings.asr, "model_type", "funasr")
        monkeypatch.setattr(settings.asr, "language", "zh")
        monkeypatch.setattr(settings.mt, "model_name", "argos-zh-en")
        monkeypatch.setattr(settings.tts, "model_name", "piper-en_US-lessac")

        stub_window = SimpleNamespace(
            translation_signal=SimpleNamespace(emit=lambda item: None)
        )
        base_config = RealtimeConfig(
            source_lang="zh",
            target_lang="en",
            enable_tts=True,
            input_device_id=11,
            output_to_virtual_device=True,
        )

        forward = MainWindow._create_direction_pipeline(
            stub_window,
            base_config,
            source_lang="zh",
            target_lang="en",
            input_device_id=11,
            output_to_virtual_device=True,
            direction_label="我→对方",
        )
        reverse = MainWindow._create_direction_pipeline(
            stub_window,
            base_config,
            source_lang="en",
            target_lang="zh",
            input_device_id=22,
            output_to_virtual_device=False,
            direction_label="对方→我",
            mt_model_name="argos-en-zh",
            tts_model_name="piper-zh_CN-huayan",
        )

        assert base_config.source_lang == "zh"
        assert base_config.target_lang == "en"
        assert base_config.input_device_id == 11
        assert base_config.output_to_virtual_device is True

        assert forward.config.source_lang == "zh"
        assert forward.config.target_lang == "en"
        assert forward.config.input_device_id == 11
        assert forward.config.output_to_virtual_device is True
        assert forward.config.direction_label == "我→对方"

        assert reverse.config.source_lang == "en"
        assert reverse.config.target_lang == "zh"
        assert reverse.config.input_device_id == 22
        assert reverse.config.output_to_virtual_device is False
        assert reverse.config.direction_label == "对方→我"
        assert reverse._mt_config.model_name == "argos-en-zh"
        assert reverse._tts_config.model_name == "piper-zh_CN-huayan"

    def test_start_translation_builds_bidirectional_session(self):
        direction_calls = []
        status_messages = []

        class StubControl:
            def __init__(self, checked=None, data=None):
                self._checked = checked
                self._data = data
                self.enabled = True
                self.text = ""
                self.style = ""

            def isChecked(self):
                return bool(self._checked)

            def currentData(self):
                return self._data

            def setEnabled(self, value):
                self.enabled = value

            def setText(self, value):
                self.text = value

            def setStyleSheet(self, value):
                self.style = value

        class StubStatusBar:
            def showMessage(self, message, timeout=0):
                status_messages.append((message, timeout))

        class StubSession:
            def __init__(self, directions):
                self.directions = dict(directions)
                self.pipelines = list(self.directions.values())
                self.is_running = False

            def start(self):
                self.is_running = True
                return True

            def stop(self):
                self.is_running = False

            def get_runtime_summaries(self):
                return [
                    {
                        "direction": pipeline.config.direction_label,
                        "source_lang": pipeline.config.source_lang,
                        "target_lang": pipeline.config.target_lang,
                        "input_device_id": pipeline.config.input_device_id,
                        "output_mode": "virtual-device" if pipeline.config.output_to_virtual_device else "speaker",
                        "output_device_id": 88 if pipeline.config.output_to_virtual_device else None,
                        "asr_backend": "funasr",
                        "asr_model": "FunAudioLLM/SenseVoiceSmall",
                        "mt_backend": "argos-ct2",
                        "mt_model": "argos-zh-en" if pipeline.config.target_lang == "en" else "argos-en-zh",
                        "tts_enabled": True,
                        "tts_engine": "piper",
                        "tts_model": "piper-en_US-lessac" if pipeline.config.target_lang == "en" else "piper-zh_CN-huayan",
                    }
                    for pipeline in self.pipelines
                ]

        def fake_create_direction_pipeline(base_config, **kwargs):
            direction_calls.append(kwargs)
            return SimpleNamespace(
                config=SimpleNamespace(
                    source_lang=kwargs["source_lang"],
                    target_lang=kwargs["target_lang"],
                    input_device_id=kwargs["input_device_id"],
                    output_to_virtual_device=kwargs["output_to_virtual_device"],
                    direction_label=kwargs["direction_label"],
                )
            )

        window = SimpleNamespace(
            statusBar=lambda: StubStatusBar(),
            _apply_runtime_settings=lambda: RealtimeConfig(
                source_lang="zh",
                target_lang="en",
                enable_tts=True,
                input_device_id=11,
                output_to_virtual_device=True,
            ),
            bidirectional_checkbox=StubControl(checked=True),
            reverse_input_device_combo=StubControl(data=22),
            _ensure_bidirectional_models_ready=lambda source_lang, target_lang: ("argos-en-zh", "piper-zh_CN-huayan"),
            _create_direction_pipeline=fake_create_direction_pipeline,
            start_btn=StubControl(),
            session_info_label=StubControl(),
            source_lang_combo=StubControl(),
            target_lang_combo=StubControl(),
            tts_checkbox=StubControl(),
            _config_tab=StubControl(),
            _pipeline=None,
        )

        with patch("localtrans.gui.app.SessionOrchestrator", StubSession):
            MainWindow._start_translation(window)

        assert len(direction_calls) == 2

        forward_call, reverse_call = direction_calls
        assert forward_call["source_lang"] == "zh"
        assert forward_call["target_lang"] == "en"
        assert forward_call["input_device_id"] == 11
        assert forward_call["output_to_virtual_device"] is True
        assert forward_call["direction_label"] == "我→对方"

        assert reverse_call["source_lang"] == "en"
        assert reverse_call["target_lang"] == "zh"
        assert reverse_call["input_device_id"] == 22
        assert reverse_call["output_to_virtual_device"] is False
        assert reverse_call["direction_label"] == "对方→我"
        assert reverse_call["mt_model_name"] == "argos-en-zh"
        assert reverse_call["tts_model_name"] == "piper-zh_CN-huayan"

        assert isinstance(window._pipeline, StubSession)
        assert window.start_btn.text == "■ 停止翻译"
        assert window.source_lang_combo.enabled is False
        assert window.target_lang_combo.enabled is False
        assert window.tts_checkbox.enabled is False
        assert window._config_tab.enabled is False
        assert "当前会话:" in window.session_info_label.text
        assert "我→对方:" in window.session_info_label.text
        assert "对方→我:" in window.session_info_label.text
        assert any(message == "双向翻译中..." for message, _ in status_messages)
