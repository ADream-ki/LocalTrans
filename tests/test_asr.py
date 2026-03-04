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
        assert "funasr" in ASREngine.BACKENDS
        assert "sherpa-onnx" in ASREngine.BACKENDS
    
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

    def test_vad_skips_silence_before_speech(self):
        """VAD开启时，纯静音不应触发ASR调用"""
        class CountingEngine:
            def __init__(self):
                self.calls = 0

            def transcribe(self, audio):
                self.calls += 1
                return TranscriptionResult(
                    text="测试",
                    language="zh",
                    confidence=1.0,
                    start_time=0.0,
                    end_time=len(audio) / 16000.0,
                )

        engine = CountingEngine()
        streaming = StreamingASR(
            asr_engine=engine,
            buffer_duration=0.2,
            overlap_duration=0.0,
            min_buffer_duration=0.1,
            max_buffer_duration=0.2,
            vad_enabled=True,
            vad_energy_threshold=0.05,
            vad_silence_duration=0.05,
        )
        streaming.start()
        try:
            silence = np.zeros(800, dtype=np.int16)
            for _ in range(6):
                streaming.put_audio(silence)
                time.sleep(0.01)
            time.sleep(0.1)
            assert engine.calls == 0

            speech = np.full(2000, 22000, dtype=np.int16)
            streaming.put_audio(speech)
            streaming.put_audio(speech)
            time.sleep(0.2)
            assert engine.calls >= 1
        finally:
            streaming.stop()
