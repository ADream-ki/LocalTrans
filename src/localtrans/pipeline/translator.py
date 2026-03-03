"""
翻译流水线
整合ASR、MT、TTS实现端到端翻译
"""

import time
import threading
from typing import Optional, Callable, Dict, Any
from dataclasses import dataclass, field
from enum import Enum
from queue import Queue

import numpy as np
from loguru import logger

from localtrans.config import settings
from localtrans.audio import AudioCapturer, AudioChunk
from localtrans.asr import ASREngine, StreamingASR, TranscriptionResult
from localtrans.mt import MTEngine, TermBankManager, TranslationResult
from localtrans.tts import TTSEngine, StreamingTTS


class PipelineState(Enum):
    """流水线状态"""
    IDLE = "idle"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    ERROR = "error"


@dataclass
class PipelineMetrics:
    """流水线性能指标"""
    asr_latency_ms: float = 0.0
    mt_latency_ms: float = 0.0
    tts_latency_ms: float = 0.0
    total_latency_ms: float = 0.0
    
    audio_chunks_processed: int = 0
    texts_transcribed: int = 0
    texts_translated: int = 0
    audio_synthesized: int = 0
    
    errors: int = 0


@dataclass
class PipelineEvent:
    """流水线事件"""
    type: str  # "asr", "mt", "tts", "error"
    data: Any
    timestamp: float = field(default_factory=time.time)


class TranslationPipeline:
    """
    翻译流水线
    实现音频捕获 -> ASR -> MT -> TTS 的端到端流程
    """
    
    def __init__(
        self,
        source_lang: str = "en",
        target_lang: str = "zh",
        enable_tts: bool = True,
        audio_callback: Optional[Callable[[np.ndarray], None]] = None,
        event_callback: Optional[Callable[[PipelineEvent], None]] = None,
    ):
        self.source_lang = source_lang
        self.target_lang = target_lang
        self.enable_tts = enable_tts
        self.audio_callback = audio_callback
        self.event_callback = event_callback
        
        # 状态
        self._state = PipelineState.IDLE
        self._metrics = PipelineMetrics()
        
        # 模块实例
        self._audio_capturer: Optional[AudioCapturer] = None
        self._streaming_asr: Optional[StreamingASR] = None
        self._mt_engine: Optional[MTEngine] = None
        self._streaming_tts: Optional[StreamingTTS] = None
        
        # 线程
        self._threads: Dict[str, threading.Thread] = {}
        
        logger.info(f"TranslationPipeline初始化: {source_lang} -> {target_lang}")
    
    def _emit_event(self, event_type: str, data: Any) -> None:
        """发送事件"""
        if self.event_callback:
            event = PipelineEvent(type=event_type, data=data)
            self.event_callback(event)
    
    def _on_asr_result(self, result: TranscriptionResult) -> None:
        """ASR结果回调"""
        try:
            asr_time = time.time()
            
            self._metrics.texts_transcribed += 1
            self._metrics.asr_latency_ms = (asr_time - result.start_time) * 1000
            
            logger.info(f"[ASR] {result.text}")
            self._emit_event("asr", result)
            
            # 调用翻译
            self._process_translation(result.text)
            
        except Exception as e:
            logger.error(f"ASR处理错误: {e}")
            self._metrics.errors += 1
            self._emit_event("error", str(e))
    
    def _process_translation(self, text: str) -> None:
        """处理翻译"""
        if not text.strip():
            return
        
        try:
            mt_start = time.time()
            
            # 翻译
            result = self._mt_engine.translate(
                text,
                source_lang=self.source_lang,
                target_lang=self.target_lang,
            )
            
            self._metrics.mt_latency_ms = (time.time() - mt_start) * 1000
            self._metrics.texts_translated += 1
            
            logger.info(f"[MT] {result.translated_text}")
            self._emit_event("mt", result)
            
            # 调用TTS
            if self.enable_tts:
                self._process_tts(result.translated_text)
            
        except Exception as e:
            logger.error(f"翻译错误: {e}")
            self._metrics.errors += 1
            self._emit_event("error", str(e))
    
    def _process_tts(self, text: str) -> None:
        """处理语音合成"""
        if not text.strip():
            return
        
        try:
            tts_start = time.time()
            
            # 合成
            result = self._tts_engine.synthesize(text)
            
            self._metrics.tts_latency_ms = (time.time() - tts_start) * 1000
            self._metrics.audio_synthesized += 1
            
            logger.info(f"[TTS] 合成完成: {result.duration:.2f}s")
            self._emit_event("tts", result)
            
            # 播放或回调
            if self.audio_callback:
                self.audio_callback(result.audio)
            else:
                self._play_audio(result.audio)
            
            # 更新总延迟
            self._metrics.total_latency_ms = (
                self._metrics.asr_latency_ms +
                self._metrics.mt_latency_ms +
                self._metrics.tts_latency_ms
            )
            
            logger.debug(f"总延迟: {self._metrics.total_latency_ms:.1f}ms")
            
        except Exception as e:
            logger.error(f"TTS错误: {e}")
            self._metrics.errors += 1
            self._emit_event("error", str(e))
    
    def _play_audio(self, audio: np.ndarray) -> None:
        """播放音频"""
        import sounddevice as sd
        sd.play(audio, samplerate=self._tts_engine.config.sample_rate)
        sd.wait()
    
    def start(self) -> bool:
        """启动流水线"""
        if self._state == PipelineState.RUNNING:
            logger.warning("流水线已在运行")
            return True
        
        try:
            self._state = PipelineState.STARTING
            logger.info("正在启动流水线...")
            
            # 初始化音频捕获
            self._audio_capturer = AudioCapturer()
            
            # 初始化ASR
            self._streaming_asr = StreamingASR(
                callback=self._on_asr_result,
                buffer_duration=2.0,
                overlap_duration=0.5,
            )
            
            # 初始化MT
            self._mt_engine = MTEngine()
            
            # 初始化TTS
            if self.enable_tts:
                self._tts_engine = TTSEngine()
            
            # 启动音频捕获并连接ASR
            self._audio_capturer.start(
                callback=lambda chunk: self._streaming_asr.put_audio(chunk.data)
            )
            
            # 启动ASR
            self._streaming_asr.start()
            
            self._state = PipelineState.RUNNING
            logger.info("流水线启动完成")
            
            return True
            
        except Exception as e:
            logger.error(f"流水线启动失败: {e}")
            self._state = PipelineState.ERROR
            return False
    
    def stop(self) -> None:
        """停止流水线"""
        if self._state != PipelineState.RUNNING:
            return
        
        self._state = PipelineState.STOPPING
        logger.info("正在停止流水线...")
        
        # 停止各模块
        if self._audio_capturer:
            self._audio_capturer.stop()
        
        if self._streaming_asr:
            self._streaming_asr.stop()
        
        self._state = PipelineState.IDLE
        logger.info("流水线已停止")
    
    def pause(self) -> None:
        """暂停流水线"""
        if self._audio_capturer:
            self._audio_capturer.pause()
    
    def resume(self) -> None:
        """恢复流水线"""
        if self._audio_capturer:
            self._audio_capturer.resume()
    
    @property
    def state(self) -> PipelineState:
        return self._state
    
    @property
    def metrics(self) -> PipelineMetrics:
        return self._metrics
    
    def __enter__(self):
        self.start()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.stop()
        return False
