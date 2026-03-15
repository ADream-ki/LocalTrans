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

    def test_close_backend(self):
        """测试资源释放"""
        with patch.object(TTSEngine, "_load_backend"):
            engine = TTSEngine()
            engine._backend = Mock()
            engine.close()
            engine._backend.close.assert_called_once()

    def test_synthesize_retry_on_empty_audio(self):
        """TTS空音频时应自动重试并恢复。"""
        with patch.object(TTSEngine, "_load_backend"):
            engine = TTSEngine(TTSConfig(engine="piper", language="zh"))
            backend = Mock()
            backend.synthesize.side_effect = [
                SynthesisResult(audio=np.array([], dtype=np.int16), sample_rate=22050, text="你好", duration=0.0),
                SynthesisResult(audio=np.array([1, 2], dtype=np.int16), sample_rate=22050, text="你好。", duration=0.0),
            ]
            engine._backend = backend

            result = engine.synthesize("你好")

            assert len(result.audio) == 2
            assert backend.synthesize.call_count >= 2


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
