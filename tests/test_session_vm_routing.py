"""SessionViewModel 双向路由测试。"""

from dataclasses import dataclass

from localtrans.ui.viewmodels.session_vm import SessionViewModel


@dataclass
class _FakeAudioOptions:
    input_device_id: int | None = 1
    output_device_id: int | None = 5
    output_mode: str = "system"
    io_buffer_ms: int = 60
    input_gain_db: float = 0.0
    output_gain_db: float = 0.0


class _FakeAudioIOService:
    def __init__(self, opts: _FakeAudioOptions):
        self._opts = opts

    def build_runtime_options(self):
        return self._opts


class _FakeRealtimeConfig:
    def __init__(self, **kwargs):
        self.__dict__.update(kwargs)


class _FakeRealtimePipeline:
    def __init__(self, *, config, asr_config, mt_config, tts_config, result_callback):
        self.config = config
        self.result_callback = result_callback
        self.is_running = False

    def start(self):
        self.is_running = True
        return True

    def stop(self):
        self.is_running = False

    def get_runtime_summary(self):
        return {
            "direction": getattr(self.config, "direction_label", ""),
            "input_device_id": getattr(self.config, "input_device_id", None),
            "output_device_id": getattr(self.config, "output_device_id", None),
            "output_mode": getattr(self.config, "output_mode", ""),
        }

    def get_history(self, limit=50):
        return []

    def set_source_acceptor(self, acceptor):
        self._acceptor = acceptor

    def set_mt_engine(self, mt_engine):
        self._mt = mt_engine

    def set_tts_engine(self, tts_engine):
        self._tts = tts_engine

    def set_asr_engine(self, asr_engine):
        self._asr = asr_engine


class _FakeSessionOrchestrator:
    def __init__(self, directions, **kwargs):
        self.directions = directions
        self.kwargs = kwargs
        self.started = False

    def start(self):
        self.started = True
        for pipeline in self.directions.values():
            pipeline.start()
        return True

    def stop(self):
        self.started = False
        for pipeline in self.directions.values():
            pipeline.stop()

    def get_runtime_summaries(self):
        return [p.get_runtime_summary() for p in self.directions.values()]


def test_session_vm_builds_bidirectional_routes(monkeypatch):
    from localtrans.pipeline import realtime
    from localtrans.services import audio_io_service

    monkeypatch.setattr(realtime, "RealtimeConfig", _FakeRealtimeConfig)
    monkeypatch.setattr(realtime, "RealtimePipeline", _FakeRealtimePipeline)
    monkeypatch.setattr(realtime, "SessionOrchestrator", _FakeSessionOrchestrator)
    monkeypatch.setattr(
        audio_io_service.AudioIOService,
        "get_instance",
        staticmethod(lambda: _FakeAudioIOService(_FakeAudioOptions(output_mode="system"))),
    )

    vm = SessionViewModel()
    vm.sourceLang = "zh"
    vm.targetLang = "en"
    vm.setPeerInputDeviceId("2")   # A
    vm.setPeerOutputDeviceId("3")  # B
    vm.setSelfInputDeviceId("4")   # C
    vm.setSelfOutputDeviceId("6")  # D

    vm.startSession()
    assert vm.isRunning is True
    assert vm._orchestrator is not None

    peer = vm._orchestrator.directions["peer_to_me"]
    mine = vm._orchestrator.directions["me_to_peer"]

    assert peer.config.input_device_id == 2
    assert peer.config.output_device_id == 3
    assert peer.config.source_lang == "en"
    assert peer.config.target_lang == "zh"
    assert peer.config.direction_label == "对方→我"
    assert peer.config.output_mode == "device"

    assert mine.config.input_device_id == 4
    assert mine.config.output_device_id == 6
    assert mine.config.source_lang == "zh"
    assert mine.config.target_lang == "en"
    assert mine.config.direction_label == "我→对方"
    assert mine.config.output_mode == "device"

    vm.stopSession()
    assert vm.isRunning is False


def test_session_vm_route_without_output_device_fallback_to_io_mode(monkeypatch):
    from localtrans.pipeline import realtime
    from localtrans.services import audio_io_service

    monkeypatch.setattr(realtime, "RealtimeConfig", _FakeRealtimeConfig)
    monkeypatch.setattr(realtime, "RealtimePipeline", _FakeRealtimePipeline)
    monkeypatch.setattr(realtime, "SessionOrchestrator", _FakeSessionOrchestrator)
    monkeypatch.setattr(
        audio_io_service.AudioIOService,
        "get_instance",
        staticmethod(lambda: _FakeAudioIOService(_FakeAudioOptions(output_mode="system"))),
    )

    vm = SessionViewModel()
    vm.setPeerInputDeviceId("7")
    vm.setPeerOutputDeviceId("")
    vm.setSelfInputDeviceId("8")
    vm.setSelfOutputDeviceId("")

    vm.startSession()
    assert vm.isRunning is True

    peer = vm._orchestrator.directions["peer_to_me"]
    mine = vm._orchestrator.directions["me_to_peer"]
    assert peer.config.output_device_id is None
    assert mine.config.output_device_id is None
    assert peer.config.output_mode == "system"
    assert mine.config.output_mode == "system"

    vm.stopSession()
