"""
流式TTS处理
实现实时文本转语音的流式输出
"""

import threading
import queue
from typing import Callable, Optional, Generator
from dataclasses import dataclass
from enum import Enum

import numpy as np
from loguru import logger

from localtrans.tts.engine import TTSEngine, SynthesisResult


class StreamState(Enum):
    """流状态"""
    IDLE = "idle"
    RUNNING = "running"
    STOPPING = "stopping"


@dataclass
class TextChunk:
    """文本块"""
    text: str
    is_final: bool = False


class StreamingTTS:
    """
    流式语音合成
    实现文本流的连续语音合成
    """
    
    def __init__(
        self,
        tts_engine: Optional[TTSEngine] = None,
        audio_callback: Optional[Callable[[np.ndarray], None]] = None,
    ):
        self.tts_engine = tts_engine or TTSEngine()
        self.audio_callback = audio_callback
        
        self._text_queue: queue.Queue[TextChunk] = queue.Queue(maxsize=50)
        self._audio_queue: queue.Queue[np.ndarray] = queue.Queue(maxsize=100)
        self._state = StreamState.IDLE
        self._worker_thread: Optional[threading.Thread] = None
        self._player_thread: Optional[threading.Thread] = None
        
        logger.info("StreamingTTS初始化完成")
    
    def put_text(self, text: str, is_final: bool = False) -> None:
        """添加文本到队列"""
        if self._state != StreamState.RUNNING:
            return
        
        try:
            self._text_queue.put_nowait(TextChunk(text=text, is_final=is_final))
        except queue.Full:
            logger.warning("文本队列已满，丢弃数据")
    
    def _synthesize_loop(self) -> None:
        """合成循环"""
        buffer = ""
        sentence_endings = set('。！？.!?')
        
        while self._state == StreamState.RUNNING:
            try:
                chunk = self._text_queue.get(timeout=0.1)
                buffer += chunk.text
                
                # 检查是否有完整句子
                while any(end in buffer for end in sentence_endings):
                    # 找到第一个句子结束符
                    idx = -1
                    end_char = None
                    for end in sentence_endings:
                        pos = buffer.find(end)
                        if pos != -1 and (idx == -1 or pos < idx):
                            idx = pos
                            end_char = end
                    
                    if idx != -1:
                        sentence = buffer[:idx + 1]
                        buffer = buffer[idx + 1:].strip()
                        
                        if sentence.strip():
                            try:
                                # 合成语音
                                for audio_chunk in self.tts_engine.synthesize_stream(sentence):
                                    if audio_chunk is not None and len(audio_chunk) > 0:
                                        self._audio_queue.put(audio_chunk)
                            except Exception as e:
                                logger.error(f"合成错误: {e}")
                
                # 最终块时处理剩余文本
                if chunk.is_final and buffer.strip():
                    try:
                        for audio_chunk in self.tts_engine.synthesize_stream(buffer):
                            if audio_chunk is not None and len(audio_chunk) > 0:
                                self._audio_queue.put(audio_chunk)
                    except Exception as e:
                        logger.error(f"最终合成错误: {e}")
                    buffer = ""
                    
            except queue.Empty:
                continue
            except Exception as e:
                logger.error(f"合成循环错误: {e}")
    
    def _player_loop(self) -> None:
        """播放循环"""
        import sounddevice as sd
        
        while self._state == StreamState.RUNNING:
            try:
                audio_chunk = self._audio_queue.get(timeout=0.1)
                
                if self.audio_callback:
                    self.audio_callback(audio_chunk)
                else:
                    # 直接播放
                    sd.play(audio_chunk, samplerate=self.tts_engine.config.sample_rate)
                    sd.wait()
                    
            except queue.Empty:
                continue
            except Exception as e:
                logger.error(f"播放循环错误: {e}")
    
    def start(self) -> None:
        """启动流式合成"""
        if self._state == StreamState.RUNNING:
            logger.warning("流式合成已在运行")
            return
        
        self._state = StreamState.RUNNING
        
        # 启动合成线程
        self._worker_thread = threading.Thread(target=self._synthesize_loop, daemon=True)
        self._worker_thread.start()
        
        # 启动播放线程
        self._player_thread = threading.Thread(target=self._player_loop, daemon=True)
        self._player_thread.start()
        
        logger.info("流式合成已启动")
    
    def stop(self) -> None:
        """停止流式合成"""
        if self._state != StreamState.RUNNING:
            return
        
        self._state = StreamState.STOPPING
        
        if self._worker_thread:
            self._worker_thread.join(timeout=2.0)
            self._worker_thread = None
        
        if self._player_thread:
            self._player_thread.join(timeout=2.0)
            self._player_thread = None
        
        self._state = StreamState.IDLE
        logger.info("流式合成已停止")
    
    def get_audio(self, timeout: float = 1.0) -> Optional[np.ndarray]:
        """获取音频数据"""
        try:
            return self._audio_queue.get(timeout=timeout)
        except queue.Empty:
            return None
    
    def audio_stream(self) -> Generator[np.ndarray, None, None]:
        """音频流生成器"""
        while self._state == StreamState.RUNNING:
            audio = self.get_audio(timeout=0.5)
            if audio is not None:
                yield audio
    
    @property
    def state(self) -> StreamState:
        return self._state
    
    @property
    def is_running(self) -> bool:
        return self._state == StreamState.RUNNING
    
    def __enter__(self):
        self.start()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.stop()
        return False
