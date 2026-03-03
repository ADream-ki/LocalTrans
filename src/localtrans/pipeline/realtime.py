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

from loguru import logger

from localtrans.audio import AudioCapturer, AudioOutputManager, VirtualAudioDevice
from localtrans.asr import StreamingASR, TranscriptionResult
from localtrans.mt import MTEngine
from localtrans.tts import TTSEngine


@dataclass
class RealtimeConfig:
    """实时翻译配置"""
    source_lang: str = "zh"
    target_lang: str = "en"
    enable_tts: bool = True
    output_to_virtual_device: bool = True
    
    # 延迟优化
    asr_buffer_duration: float = 0.6
    asr_overlap: float = 0.05
    max_translation_queue: int = 2
    stream_flush_interval: float = 0.35
    stream_min_chars: int = 4
    stream_max_chars: int = 18
    max_tts_queue: int = 2
    
    # 音频输出
    output_device: Optional[str] = None
    input_device_id: Optional[int] = None
    input_device_name: Optional[str] = None


class RealtimePipeline:
    """
    实时翻译流水线
    完整的实时音频翻译解决方案
    """
    _HARD_BREAK_CHARS = "。！？!?;\n"
    _SOFT_BREAK_CHARS = "，,、 "
    
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
        self._tts_thread: Optional[threading.Thread] = None
        self._translation_queue: "queue.Queue[Dict]" = queue.Queue(
            maxsize=max(1, int(self.config.max_translation_queue))
        )
        self._tts_queue: "queue.Queue[str]" = queue.Queue(maxsize=max(1, int(self.config.max_tts_queue)))
        
        # 增量流式状态
        self._text_state_lock = threading.Lock()
        self._last_asr_text = ""
        self._pending_source_text = ""
        self._last_source_emit_ts = 0.0
        
        # 历史记录
        self._history: deque = deque(maxlen=100)
        
        logger.info("RealtimePipeline初始化完成")

    @staticmethod
    def _normalize_text(text: str) -> str:
        return " ".join((text or "").replace("\r", " ").replace("\n", " ").split())

    @staticmethod
    def _suffix_prefix_overlap(prev_text: str, curr_text: str) -> int:
        max_overlap = min(len(prev_text), len(curr_text))
        for size in range(max_overlap, 0, -1):
            if prev_text.endswith(curr_text[:size]):
                return size
        return 0

    def _extract_incremental_text(self, prev_text: str, curr_text: str) -> str:
        if not curr_text:
            return ""
        if not prev_text:
            return curr_text
        if curr_text.startswith(prev_text):
            return curr_text[len(prev_text):]

        overlap = self._suffix_prefix_overlap(prev_text, curr_text)
        if overlap > 0:
            return curr_text[overlap:]

        # 识别器上下文重置时，直接把当前文本当作新增内容
        if len(curr_text) + 4 < len(prev_text):
            return curr_text
        return ""

    def _find_split_pos(self, text: str, max_chars: int) -> int:
        if len(text) <= max_chars:
            return len(text)
        start = min(len(text), max_chars)
        lower = max(1, start - 8)
        for idx in range(start, lower - 1, -1):
            ch = text[idx - 1]
            if ch.isspace() or ch in self._SOFT_BREAK_CHARS:
                return idx
        return start

    def _collect_ready_segments_locked(self, force: bool = False) -> List[str]:
        segments: List[str] = []
        now = time.time()

        while True:
            buffer = self._pending_source_text.lstrip()
            self._pending_source_text = buffer
            if not buffer:
                break

            split_pos = -1
            for ch in self._HARD_BREAK_CHARS:
                idx = buffer.find(ch)
                if idx >= 0 and (split_pos < 0 or idx < split_pos):
                    split_pos = idx + 1

            if split_pos < 0:
                due = force or (
                    len(buffer) >= max(1, int(self.config.stream_min_chars))
                    and (now - self._last_source_emit_ts) >= max(0.05, float(self.config.stream_flush_interval))
                )
                if len(buffer) >= max(2, int(self.config.stream_max_chars)):
                    due = True
                if not due:
                    break
                split_pos = self._find_split_pos(buffer, max(2, int(self.config.stream_max_chars)))

            segment = self._normalize_text(buffer[:split_pos])
            self._pending_source_text = buffer[split_pos:].lstrip()
            if segment:
                segments.append(segment)
                self._last_source_emit_ts = now

        return segments

    def _drain_pending_segments(self, force: bool = False) -> List[str]:
        with self._text_state_lock:
            return self._collect_ready_segments_locked(force=force)

    def _enqueue_translation_text(self, text: str, language: str) -> None:
        payload = {
            "text": text,
            "language": language,
            "created_at": time.time(),
        }
        try:
            self._translation_queue.put_nowait(payload)
        except queue.Full:
            try:
                self._translation_queue.get_nowait()
                self._translation_queue.put_nowait(payload)
                logger.warning("翻译队列已满，已丢弃最旧片段")
            except queue.Empty:
                pass

    def _enqueue_tts_text(self, text: str) -> None:
        if not text.strip():
            return
        try:
            self._tts_queue.put_nowait(text)
        except queue.Full:
            try:
                self._tts_queue.get_nowait()
                self._tts_queue.put_nowait(text)
                logger.warning("TTS队列已满，已丢弃最旧片段")
            except queue.Empty:
                pass

    def _reset_stream_state(self) -> None:
        with self._text_state_lock:
            self._last_asr_text = ""
            self._pending_source_text = ""
            self._last_source_emit_ts = time.time()
    
    def _on_transcription(self, result: TranscriptionResult) -> None:
        """转录回调"""
        if not self._running or not result.text.strip():
            return
        
        try:
            current_text = self._normalize_text(result.text)
            with self._text_state_lock:
                delta_text = self._extract_incremental_text(self._last_asr_text, current_text)
                self._last_asr_text = current_text
                if delta_text:
                    self._pending_source_text = f"{self._pending_source_text} {delta_text}".strip()
                segments = self._collect_ready_segments_locked(force=False)

            for segment in segments:
                self._enqueue_translation_text(segment, result.language)
        except Exception as e:
            logger.error(f"转录入队失败: {e}")

    def _translation_loop(self) -> None:
        """翻译与TTS处理线程"""
        while self._running or not self._translation_queue.empty():
            try:
                payload = self._translation_queue.get(timeout=0.1)
            except queue.Empty:
                for segment in self._drain_pending_segments(force=False):
                    self._enqueue_translation_text(segment, self.config.source_lang)
                if not self._running and self._translation_queue.empty():
                    break
                continue

            source_text = str(payload.get("text", "")).strip()
            if not source_text:
                continue
            source_language = str(payload.get("language", self.config.source_lang))
            created_at = float(payload.get("created_at", time.time()))

            logger.info(f"[识别] {source_text}")

            try:
                translation = self._mt_engine.translate(
                    source_text,
                    source_lang=self.config.source_lang,
                    target_lang=self.config.target_lang,
                )

                logger.info(f"[翻译] {translation.translated_text}")

                item = {
                    "timestamp": time.time(),
                    "source": source_text,
                    "translation": translation.translated_text,
                    "language": source_language,
                    "latency_ms": (time.time() - created_at) * 1000.0,
                }
                logger.debug(f"[延迟] {item['latency_ms']:.0f}ms")
                self._history.append(item)

                if self._result_callback:
                    try:
                        self._result_callback(item)
                    except Exception as callback_exc:
                        logger.error(f"结果回调错误: {callback_exc}")

                if self.config.enable_tts and self._tts_engine:
                    self._enqueue_tts_text(translation.translated_text)

            except Exception as e:
                logger.error(f"翻译错误: {e}")

    def _tts_loop(self) -> None:
        while self._running or not self._tts_queue.empty():
            try:
                text = self._tts_queue.get(timeout=0.1)
            except queue.Empty:
                if not self._running and self._tts_queue.empty():
                    break
                continue

            try:
                self._synthesize_and_play(text)
            except Exception as exc:
                logger.error(f"TTS线程错误: {exc}")
    
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
            else:
                self._tts_engine = None
                self._audio_output = None
            
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
                self._reset_stream_state()
                self._clear_queue(self._translation_queue)
                self._clear_queue(self._tts_queue)
                self._running = True
                
                self._translator_thread = threading.Thread(
                    target=self._translation_loop,
                    daemon=True,
                )
                self._translator_thread.start()

                if self.config.enable_tts and self._tts_engine:
                    self._tts_thread = threading.Thread(
                        target=self._tts_loop,
                        daemon=True,
                    )
                    self._tts_thread.start()
                
                # 启动ASR
                self._streaming_asr.start()
                
                # 启动音频捕获
                self._audio_capturer.start(
                    callback=lambda chunk: self._streaming_asr.put_audio(chunk.data)
                )
                
                logger.info("实时翻译已启动")
                return True
                
            except Exception as e:
                self._running = False
                logger.error(f"启动失败: {e}")
                return False
    
    def stop(self) -> None:
        """停止实时翻译"""
        with self._lock:
            if not self._running:
                return

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

            for segment in self._drain_pending_segments(force=True):
                self._enqueue_translation_text(segment, self.config.source_lang)

            self._running = False

            if self._translator_thread:
                self._translator_thread.join(timeout=5.0)
                self._translator_thread = None
            if self._tts_thread:
                self._tts_thread.join(timeout=5.0)
                self._tts_thread = None

            self._clear_queue(self._translation_queue)
            self._clear_queue(self._tts_queue)
            self._reset_stream_state()

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
    source_lang: str = "zh",
    target_lang: str = "en",
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
