"""测试TTS模块"""

import pytest
import numpy as np
from unittest.mock import Mock, patch, MagicMock

from localtrans.tts.engine import TTSEngine, SynthesisResult
from localtrans.tts.stream import StreamingTTS, StreamState
from localtrans.config import TTSConfig


class TestSynthesisResult:
    """合成结果测试"""
    
    def test_result_creation(self):
        """测试结果创建"""
        audio = np.random.randint(-1000, 1000, size=16000, dtype=np.int16)
        result = SynthesisResult(
            audio=audio,
            sample_rate=16000,
            text="你好",
            duration=1.0,
        )
        
        assert result.sample_rate == 16000
        assert result.duration == 1.0
        assert result.duration_ms == 1000.0


class TestTTSEngine:
    """TTS引擎测试"""
    
    def test_backend_types(self):
        """测试后端类型"""
        assert "pyttsx3" in TTSEngine.BACKENDS
        assert "piper" in TTSEngine.BACKENDS
        assert "coqui" in TTSEngine.BACKENDS
        assert "edge-tts" in TTSEngine.BACKENDS
    
    def test_default_config(self):
        """测试默认配置"""
        config = TTSConfig()
        
        assert config.engine == "pyttsx3"
        assert config.stream_enabled is True


class TestStreamingTTS:
    """流式TTS测试"""
    
    def test_init(self):
        """测试初始化"""
        mock_engine = Mock()
        streaming = StreamingTTS(tts_engine=mock_engine)
        
        assert streaming.state == StreamState.IDLE
    
    def test_put_text_when_not_running(self):
        """测试非运行状态下添加文本"""
        mock_engine = Mock()
        streaming = StreamingTTS(tts_engine=mock_engine)
        
        streaming.put_text("Hello")
        
        # 队列应该为空
        assert streaming._text_queue.empty()
