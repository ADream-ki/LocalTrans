"""
流式ASR处理
实现实时语音识别的流式处理
"""

import threading
import queue
from typing import Callable, Optional, Generator
from dataclasses import dataclass
from enum import Enum

import numpy as np
from loguru import logger

from localtrans.asr.engine import ASREngine, TranscriptionResult
from localtrans.config import settings


class StreamState(Enum):
    """流状态"""
    IDLE = "idle"
    RUNNING = "running"
    STOPPING = "stopping"


@dataclass
class AudioBuffer:
    """音频缓冲区"""
    data: np.ndarray
    timestamp: float


class StreamingASR:
    """
    流式语音识别
    实现实时音频流的连续语音识别
    """
    
    def __init__(
        self,
        asr_engine: Optional[ASREngine] = None,
        callback: Optional[Callable[[TranscriptionResult], None]] = None,
        buffer_duration: float = 2.0,
        overlap_duration: float = 0.5,
    ):
        self.asr_engine = asr_engine or ASREngine()
        self.callback = callback
        self.buffer_duration = buffer_duration
        self.overlap_duration = overlap_duration
        
        self._audio_queue: queue.Queue[AudioBuffer] = queue.Queue(maxsize=200)
        self._result_queue: queue.Queue[TranscriptionResult] = queue.Queue(maxsize=100)
        self._state = StreamState.IDLE
        self._worker_thread: Optional[threading.Thread] = None
        
        # 音频缓冲
        self._sample_rate = settings.audio.sample_rate
        self._buffer_samples = int(buffer_duration * self._sample_rate)
        self._overlap_samples = int(overlap_duration * self._sample_rate)
        
        logger.info(f"StreamingASR初始化: buffer={buffer_duration}s, overlap={overlap_duration}s")
    
    def put_audio(self, audio_data: np.ndarray, timestamp: float = 0.0) -> None:
        """添加音频数据到队列"""
        if self._state != StreamState.RUNNING:
            return
        
        try:
            self._audio_queue.put_nowait(AudioBuffer(data=audio_data, timestamp=timestamp))
        except queue.Full:
            logger.warning("音频队列已满，丢弃数据")
    
    def _process_loop(self) -> None:
        """处理循环"""
        buffer = []
        total_samples = 0
        
        while self._state == StreamState.RUNNING:
            try:
                # 获取音频块
                audio_buffer = self._audio_queue.get(timeout=0.1)
                buffer.append(audio_buffer.data)
                total_samples += len(audio_buffer.data)
                
                # 达到缓冲大小时处理
                if total_samples >= self._buffer_samples:
                    # 合并音频
                    audio_data = np.concatenate(buffer)
                    
                    # 转录
                    try:
                        result = self.asr_engine.transcribe(audio_data)
                        
                        if result.text.strip():
                            # 回调处理
                            if self.callback:
                                self.callback(result)
                            
                            # 放入结果队列
                            self._result_queue.put(result)
                            
                            logger.debug(f"转录结果: {result.text[:50]}...")
                    
                    except Exception as e:
                        logger.error(f"转录错误: {e}")
                    
                    # 保留重叠部分
                    if self._overlap_samples > 0 and len(audio_data) > self._overlap_samples:
                        buffer = [audio_data[-self._overlap_samples:]]
                        total_samples = self._overlap_samples
                    else:
                        buffer = []
                        total_samples = 0
                        
            except queue.Empty:
                continue
            except Exception as e:
                logger.error(f"处理循环错误: {e}")
        
        # 处理剩余数据
        if buffer:
            audio_data = np.concatenate(buffer)
            try:
                result = self.asr_engine.transcribe(audio_data)
                if result.text.strip() and self.callback:
                    self.callback(result)
            except Exception as e:
                logger.error(f"最终转录错误: {e}")
    
    def start(self) -> None:
        """启动流式识别"""
        if self._state == StreamState.RUNNING:
            logger.warning("流式识别已在运行")
            return
        
        self._state = StreamState.RUNNING
        self._worker_thread = threading.Thread(target=self._process_loop, daemon=True)
        self._worker_thread.start()
        logger.info("流式识别已启动")
    
    def stop(self) -> None:
        """停止流式识别"""
        if self._state != StreamState.RUNNING:
            return
        
        self._state = StreamState.STOPPING
        
        if self._worker_thread:
            # 给正在执行中的识别/翻译回调留出足够退出时间，避免重启后状态残留
            self._worker_thread.join(timeout=5.0)
            if self._worker_thread.is_alive():
                logger.warning("流式识别线程未在超时内退出，后续将继续后台收敛")
            self._worker_thread = None
        
        self._state = StreamState.IDLE
        self._clear_queue(self._audio_queue)
        self._clear_queue(self._result_queue)
        logger.info("流式识别已停止")

    @staticmethod
    def _clear_queue(q: queue.Queue) -> None:
        """清空队列，避免下次启动读到旧数据"""
        try:
            while True:
                q.get_nowait()
        except queue.Empty:
            return
    
    def get_result(self, timeout: float = 1.0) -> Optional[TranscriptionResult]:
        """获取识别结果"""
        try:
            return self._result_queue.get(timeout=timeout)
        except queue.Empty:
            return None
    
    def results(self) -> Generator[TranscriptionResult, None, None]:
        """生成器方式获取结果"""
        while self._state == StreamState.RUNNING:
            result = self.get_result(timeout=0.5)
            if result:
                yield result
    
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
