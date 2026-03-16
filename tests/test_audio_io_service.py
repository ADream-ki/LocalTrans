from localtrans.config import settings
from localtrans.services.audio_io_service import AudioIOService


def test_audio_io_control_state_roundtrip():
    service = AudioIOService.get_instance()
    state = service.update_control_state(
        output_mode="device",
        io_profile="studio",
        io_buffer_ms=180,
        input_gain_db=3.5,
        output_gain_db=-2.0,
        monitoring_enabled=True,
    )
    assert state["output_mode"] == "device"
    assert state["io_profile"] == "studio"
    assert state["io_buffer_ms"] == 180
    assert abs(state["input_gain_db"] - 3.5) < 1e-6
    assert abs(state["output_gain_db"] + 2.0) < 1e-6
    assert state["monitoring_enabled"] is True


def test_audio_io_runtime_options_clamp():
    service = AudioIOService.get_instance()
    settings.audio.io_profile = "realtime"
    settings.audio.io_buffer_ms = 200  # realtime 档位应被限制到 80ms 内
    settings.audio.output_mode = "device"
    settings.audio.output_device_id = 999999  # 不存在，应该回退
    settings.save()

    runtime = service.build_runtime_options()
    assert runtime.io_buffer_ms <= 80
    assert runtime.output_mode in {"system", "device", "virtual"}
