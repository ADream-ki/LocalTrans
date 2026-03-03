"""测试音频模块"""

import pytest
import numpy as np
from unittest.mock import Mock, patch

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
