"""
实时翻译流水线
支持实时音频输入和输出的完整翻译流程
"""

import time
import threading
import queue
from typing import Optional, Callable, List, Dict
from dataclasses import dataclass
from collections import deque

import numpy as np
from loguru import logger

from localtrans.config import settings
from localtrans.audio import AudioCapturer, AudioOutputManager, VirtualAudioDevice
from localtrans.asr import ASREngine, StreamingASR, TranscriptionResult
from localtrans.mt import MTEngine, TranslationResult
from localtrans.tts import TTSEngine


@dataclass
class RealtimeConfig:
    """实时翻译配置"""
    source_lang: str = "en"
    target_lang: str = "zh"
    enable_tts: bool = True
    output_to_virtual_device: bool = True
    
    # 延迟优化
    asr_buffer_duration: float = 0.8
    asr_overlap: float = 0.1
    max_translation_queue: int = 4
    
    # 音频输出
    output_device: Optional[str] = None
    input_device_id: Optional[int] = None
    input_device_name: Optional[str] = None


class RealtimePipeline:
    """
    实时翻译流水线
    完整的实时音频翻译解决方案
    """
    
    def __init__(
        self,
        config: Optional[RealtimeConfig] = None,
        result_callback: Optional[Callable[[Dict], None]] = None,
    ):
        self.config = config or RealtimeConfig()
        self._result_callback = result_callback
        
        # 组件
        self._audio_capturer: Optional[AudioCapturer] = None
        self._streaming_asr: Optional[StreamingASR] = None
        self._mt_engine: Optional[MTEngine] = None
        self._tts_engine: Optional[TTSEngine] = None
        self._audio_output: Optional[AudioOutputManager] = None
        self._virtual_device: Optional[VirtualAudioDevice] = None
        
        # 状态
        self._running = False
        self._lock = threading.Lock()
        self._translator_thread: Optional[threading.Thread] = None
        self._translation_queue: "queue.Queue[TranscriptionResult]" = queue.Queue(
            maxsize=max(1, int(self.config.max_translation_queue))
        )
        
        # 历史记录
        self._history: deque = deque(maxlen=100)
        
        logger.info("RealtimePipeline初始化完成")
    
    def _on_transcription(self, result: TranscriptionResult) -> None:
        """转录回调"""
        if not self._running or not result.text.strip():
            return
        
        try:
            # 识别线程只做入队，MT/TTS在独立线程里处理，避免阻塞后续识别
            self._translation_queue.put_nowait(result)
        except queue.Full:
            try:
                self._translation_queue.get_nowait()
                self._translation_queue.put_nowait(result)
                logger.warning("翻译队列已满，已丢弃最旧片段")
            except queue.Empty:
                pass
        except Exception as e:
            logger.error(f"转录入队失败: {e}")

    def _translation_loop(self) -> None:
        """翻译与TTS处理线程"""
        while self._running:
            try:
                result = self._translation_queue.get(timeout=0.1)
            except queue.Empty:
                continue

            if not result.text.strip():
                continue

            logger.info(f"[识别] {result.text}")

            try:
                translation = self._mt_engine.translate(
                    result.text,
                    source_lang=self.config.source_lang,
                    target_lang=self.config.target_lang,
                )

                logger.info(f"[翻译] {translation.translated_text}")

                item = {
                    "timestamp": time.time(),
                    "source": result.text,
                    "translation": translation.translated_text,
                    "language": result.language,
                }
                self._history.append(item)

                if self._result_callback:
                    try:
                        self._result_callback(item)
                    except Exception as callback_exc:
                        logger.error(f"结果回调错误: {callback_exc}")

                if self.config.enable_tts and self._tts_engine:
                    self._synthesize_and_play(translation.translated_text)

            except Exception as e:
                logger.error(f"翻译错误: {e}")
    
    def _synthesize_and_play(self, text: str) -> None:
        """合成并播放"""
        try:
            result = self._tts_engine.synthesize(text)
            
            if self.config.output_to_virtual_device and self._virtual_device:
                # 输出到虚拟设备
                self._audio_output.play_to_device(
                    result.audio,
                    self._virtual_device.output_device_id
                )
            else:
                # 直接播放
                self._audio_output.play(result.audio)
            
        except Exception as e:
            logger.error(f"合成/播放错误: {e}")
    
    def initialize(self) -> bool:
        """初始化所有组件"""
        try:
            logger.info("初始化组件...")
            
            # 检查虚拟设备
            if self.config.output_to_virtual_device:
                self._virtual_device = VirtualAudioDevice()
                if not self._virtual_device.is_available:
                    logger.warning("虚拟设备不可用，将使用默认音频输出")
                    self._virtual_device = None
            
            # 初始化音频捕获
            self._audio_capturer = AudioCapturer(
                device_id=self.config.input_device_id,
                device_name=self.config.input_device_name,
            )
            
            # 初始化ASR
            self._streaming_asr = StreamingASR(
                callback=self._on_transcription,
                buffer_duration=self.config.asr_buffer_duration,
                overlap_duration=self.config.asr_overlap,
            )
            
            # 初始化MT
            self._mt_engine = MTEngine()
            
            # 初始化TTS
            if self.config.enable_tts:
                self._tts_engine = TTSEngine()
                self._audio_output = AudioOutputManager()
            
            logger.info("组件初始化完成")
            return True
            
        except Exception as e:
            logger.error(f"初始化失败: {e}")
            return False
    
    def start(self) -> bool:
        """启动实时翻译"""
        if self._running:
            logger.warning("已在运行")
            return True
        
        with self._lock:
            try:
                # 初始化（如果尚未完成）
                if not self._audio_capturer:
                    if not self.initialize():
                        return False
                
                # 启动ASR
                self._streaming_asr.start()

                self._translator_thread = threading.Thread(
                    target=self._translation_loop,
                    daemon=True,
                )
                self._translator_thread.start()
                
                # 启动音频捕获
                self._audio_capturer.start(
                    callback=lambda chunk: self._streaming_asr.put_audio(chunk.data)
                )
                
                self._running = True
                logger.info("实时翻译已启动")
                return True
                
            except Exception as e:
                logger.error(f"启动失败: {e}")
                return False
    
    def stop(self) -> None:
        """停止实时翻译"""
        with self._lock:
            if not self._running:
                return
            
            self._running = False
            
            if self._audio_capturer:
                try:
                    self._audio_capturer.stop()
                except Exception as exc:
                    logger.warning(f"停止音频捕获异常: {exc}")
            
            if self._streaming_asr:
                try:
                    self._streaming_asr.stop()
                except Exception as exc:
                    logger.warning(f"停止流式识别异常: {exc}")

            self._clear_queue(self._translation_queue)
            if self._translator_thread:
                self._translator_thread.join(timeout=3.0)
                self._translator_thread = None

            # 显式释放组件，避免重复启动时复用到旧状态
            self._audio_capturer = None
            self._streaming_asr = None
            self._mt_engine = None
            self._tts_engine = None
            self._audio_output = None
            self._virtual_device = None
            
            logger.info("实时翻译已停止")

    @staticmethod
    def _clear_queue(q: queue.Queue) -> None:
        try:
            while True:
                q.get_nowait()
        except queue.Empty:
            return
    
    def get_history(self, limit: int = 50) -> List[Dict]:
        """获取翻译历史"""
        return list(self._history)[-limit:]
    
    def clear_history(self) -> None:
        """清空历史"""
        self._history.clear()
    
    @property
    def is_running(self) -> bool:
        return self._running
    
    def __enter__(self):
        self.start()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.stop()
        return False


def create_pipeline(
    source_lang: str = "en",
    target_lang: str = "zh",
    enable_tts: bool = True,
    use_virtual_device: bool = True,
    input_device_id: Optional[int] = None,
    input_device_name: Optional[str] = None,
    result_callback: Optional[Callable[[Dict], None]] = None,
) -> RealtimePipeline:
    """创建实时翻译流水线的便捷函数"""
    config = RealtimeConfig(
        source_lang=source_lang,
        target_lang=target_lang,
        enable_tts=enable_tts,
        output_to_virtual_device=use_virtual_device,
        input_device_id=input_device_id,
        input_device_name=input_device_name,
    )
    return RealtimePipeline(config, result_callback=result_callback)
