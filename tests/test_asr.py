"""测试ASR模块"""

import pytest
import numpy as np
import time
from unittest.mock import Mock, patch, MagicMock

from localtrans.asr.engine import ASREngine, TranscriptionResult
from localtrans.asr.stream import StreamingASR, StreamState
from localtrans.config import ASRConfig


class TestTranscriptionResult:
    """转录结果测试"""
    
    def test_duration(self):
        """测试时长计算"""
        result = TranscriptionResult(
            text="Hello world",
            language="en",
            confidence=0.95,
            start_time=0.0,
            end_time=2.5,
        )
        
        assert result.duration == 2.5


class TestASREngine:
    """ASR引擎测试"""
    
    def test_backend_types(self):
        """测试后端类型"""
        assert "faster-whisper" in ASREngine.BACKENDS
        assert "whisper" in ASREngine.BACKENDS
    
    @patch('localtrans.asr.engine.FasterWhisperBackend')
    def test_engine_init(self, mock_backend):
        """测试引擎初始化"""
        config = ASRConfig(model_type="faster-whisper", model_size="tiny")
        
        # 不实际加载模型，只测试配置
        mock_backend.return_value = Mock()
        
        with patch.object(ASREngine, '_load_backend'):
            engine = ASREngine(config)
            engine._backend = mock_backend.return_value
            
            assert engine.config.model_type == "faster-whisper"


class TestStreamingASR:
    """流式ASR测试"""
    
    def test_init(self):
        """测试初始化"""
        mock_engine = Mock()
        streaming = StreamingASR(asr_engine=mock_engine)
        
        assert streaming.state == StreamState.IDLE
        assert streaming.buffer_duration == 2.0
        assert streaming.overlap_duration == 0.5
    
    def test_put_audio_when_not_running(self):
        """测试非运行状态下添加音频"""
        mock_engine = Mock()
        streaming = StreamingASR(asr_engine=mock_engine)
        
        audio = np.random.randint(-1000, 1000, size=1600, dtype=np.int16)
        streaming.put_audio(audio)
        
        # 队列应该为空，因为没有运行
        assert streaming._audio_queue.empty()

    def test_buffer_does_not_grow_unbounded_after_transcribe_error(self):
        """转录异常后应推进窗口，避免缓冲无限增长"""
        call_lengths = []

        class FailingEngine:
            def transcribe(self, audio):
                call_lengths.append(len(audio))
                raise RuntimeError("mock transcribe failure")

        streaming = StreamingASR(
            asr_engine=FailingEngine(),
            buffer_duration=0.1,
            overlap_duration=0.05,
        )
        streaming.start()

        try:
            chunk = np.random.randint(-1000, 1000, size=1600, dtype=np.int16)
            for _ in range(16):
                streaming.put_audio(chunk)
                time.sleep(0.01)
            time.sleep(0.2)
        finally:
            streaming.stop()

        assert len(call_lengths) >= 3
        # 正常推进时每次处理片段长度应保持在小窗口范围，不会线性失控增长
        assert max(call_lengths) < 16000
