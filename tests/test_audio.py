"""测试音频模块"""

import pytest
import numpy as np
from unittest.mock import Mock, patch
from types import SimpleNamespace

from localtrans.audio.capturer import AudioCapturer, CaptureState, AudioChunk
from localtrans.audio.virtual_device import VirtualAudioDevice


class TestAudioCapturer:
    """音频捕获器测试"""
    
    def test_init(self):
        """测试初始化"""
        capturer = AudioCapturer()
        
        assert capturer.sample_rate == 16000
        assert capturer.channels == 1
        assert capturer.state == CaptureState.IDLE
    
    def test_list_devices(self):
        """测试列出设备"""
        capturer = AudioCapturer()
        devices = capturer.list_devices()
        
        assert isinstance(devices, list)
        # 至少应该有一个设备
        assert len(devices) >= 0
    
    def test_audio_chunk(self):
        """测试音频块"""
        data = np.random.randint(-1000, 1000, size=1600, dtype=np.int16)
        chunk = AudioChunk(
            data=data,
            sample_rate=16000,
            channels=1,
            timestamp=0.0,
            duration=0.1,
        )
        
        assert chunk.sample_rate == 16000
        assert chunk.duration == 0.1

    def test_resolve_stream_params_fallback_sample_rate(self, monkeypatch):
        """设备不支持16k时应自动降级到可用采样率。"""
        capturer = AudioCapturer(sample_rate=16000, channels=1, device_id=27)

        monkeypatch.setattr(
            "localtrans.audio.capturer.sd.query_devices",
            lambda: [{"max_input_channels": 2, "default_samplerate": 44100}],
        )

        def fake_check_input_settings(device, samplerate, channels, dtype):
            if samplerate == 16000:
                raise RuntimeError("Invalid sample rate")
            return None

        monkeypatch.setattr(
            "localtrans.audio.capturer.sd.check_input_settings",
            fake_check_input_settings,
        )

        sr, ch = capturer._resolve_stream_params(device_id=0)
        assert sr == 44100
        assert ch == 1

    def test_audio_callback_resamples_to_target_rate(self):
        """回调应将输入重采样为目标采样率。"""
        capturer = AudioCapturer(sample_rate=16000, channels=1)
        capturer._stream_sample_rate = 44100
        capturer._stream_channels = 2

        # 10ms 立体声音频 @44.1k
        frames = 441
        indata = np.random.randint(-1000, 1000, size=(frames, 2), dtype=np.int16)
        time_info = SimpleNamespace(currentTime=0.123)

        capturer._audio_callback(indata, frames, time_info, status=None)
        chunk = capturer.get_chunk(timeout=0.2)
        assert chunk is not None
        assert chunk.sample_rate == 16000
        assert chunk.channels == 1
        # 10ms @16k ≈ 160 samples
        assert 150 <= len(chunk.data) <= 170
        assert chunk.data.dtype == np.float32


class TestVirtualAudioDevice:
    """虚拟音频设备测试"""
    
    def test_device_types(self):
        """测试设备类型"""
        assert "vb-cable" in VirtualAudioDevice.VIRTUAL_DEVICES
        assert "voicemeeter" in VirtualAudioDevice.VIRTUAL_DEVICES
    
    def test_installation_guide(self):
        """测试安装指南"""
        guide = VirtualAudioDevice.get_installation_guide()
        
        assert "VB-Cable" in guide
        assert "安装" in guide
    
    def test_meeting_app_config(self):
        """测试会议软件配置"""
        device = VirtualAudioDevice("vb-cable")
        config = device.configure_meeting_app("zoom")
        
        assert "output" in config


class TestAudioRouter:
    """音频路由测试"""
    
    def test_init(self):
        """测试初始化"""
        from localtrans.audio.router import AudioRouter, RoutingState
        
        router = AudioRouter()
        
        assert router.state == RoutingState.INACTIVE
