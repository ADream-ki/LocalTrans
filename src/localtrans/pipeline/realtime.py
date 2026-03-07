"""
实时翻译流水线
支持实时音频输入和输出的完整翻译流程
"""

import time
import re
import threading
import queue
from typing import Optional, Callable, List, Dict
from dataclasses import dataclass
from collections import deque

from loguru import logger

from localtrans.audio import AudioCapturer, AudioOutputManager, VirtualAudioDevice
from localtrans.asr import StreamingASR, TranscriptionResult
from localtrans.config import settings
from localtrans.mt import MTEngine
from localtrans.tts import TTSEngine


@dataclass
class RealtimeConfig:
    """实时翻译配置"""
    source_lang: str = "zh"
    target_lang: str = "en"
    enable_tts: bool = True
    direct_asr_translate: bool = False
    output_to_virtual_device: bool = True
    
    # 延迟优化
    asr_buffer_duration: float = 0.6
    asr_overlap: float = 0.05
    max_translation_queue: int = 2
    stream_flush_interval: float = 0.2
    stream_min_chars: int = 2
    stream_max_chars: int = 14
    stream_profile: str = "realtime"  # realtime, balanced, quality
    stream_agreement: int = 1
    translation_batch_chars: int = 28
    max_tts_queue: int = 2
    tts_merge_chars: int = 80
    asr_vad_enabled: bool = True
    asr_vad_energy_threshold: float = 0.01
    asr_vad_silence_duration: float = 0.18
    asr_min_buffer_duration: Optional[float] = None
    asr_max_buffer_duration: Optional[float] = None
    min_asr_confidence: float = 0.08
    min_cjk_ratio: float = 0.08
    drop_hallucination: bool = True
    
    # 音频输出
    output_device: Optional[str] = None
    input_device_id: Optional[int] = None
    input_device_name: Optional[str] = None

    def __post_init__(self) -> None:
        profile = (self.stream_profile or "realtime").lower()
        self.stream_profile = profile
        self.stream_agreement = max(1, int(self.stream_agreement))
        self.min_asr_confidence = max(0.0, min(1.0, float(self.min_asr_confidence)))
        self.min_cjk_ratio = max(0.0, min(1.0, float(self.min_cjk_ratio)))

        if profile == "quality":
            if self.stream_flush_interval <= 0.2:
                self.stream_flush_interval = 0.32
            if self.stream_min_chars <= 2:
                self.stream_min_chars = 4
            if self.stream_max_chars <= 14:
                self.stream_max_chars = 22
            if self.stream_agreement <= 1:
                self.stream_agreement = 2
            if self.translation_batch_chars <= 42:
                self.translation_batch_chars = 64
            if self.tts_merge_chars <= 120:
                self.tts_merge_chars = 180
            if self.asr_min_buffer_duration is None:
                self.asr_min_buffer_duration = max(0.45, self.asr_buffer_duration * 0.7)
            if self.asr_max_buffer_duration is None:
                self.asr_max_buffer_duration = max(self.asr_buffer_duration, 1.0)
            if self.asr_vad_silence_duration <= 0.18:
                self.asr_vad_silence_duration = 0.24
            if self.asr_vad_energy_threshold >= 0.01:
                self.asr_vad_energy_threshold = 0.008
            if self.min_asr_confidence < 0.18:
                self.min_asr_confidence = 0.18
            if self.min_cjk_ratio < 0.15:
                self.min_cjk_ratio = 0.15
            return

        if profile == "balanced":
            if self.stream_flush_interval <= 0.2:
                self.stream_flush_interval = 0.25
            if self.stream_min_chars <= 2:
                self.stream_min_chars = 3
            if self.stream_max_chars <= 14:
                self.stream_max_chars = 16
            if self.stream_agreement <= 1:
                self.stream_agreement = 2
            if self.translation_batch_chars <= 28:
                self.translation_batch_chars = 42
            if self.tts_merge_chars <= 80:
                self.tts_merge_chars = 120
            if self.asr_min_buffer_duration is None:
                self.asr_min_buffer_duration = max(0.35, self.asr_buffer_duration * 0.6)
            if self.asr_max_buffer_duration is None:
                self.asr_max_buffer_duration = max(self.asr_buffer_duration, 0.8)
            if self.asr_vad_silence_duration <= 0.18:
                self.asr_vad_silence_duration = 0.2
            if self.min_asr_confidence < 0.12:
                self.min_asr_confidence = 0.12
            if self.min_cjk_ratio < 0.1:
                self.min_cjk_ratio = 0.1
            return

        if self.asr_min_buffer_duration is None:
            self.asr_min_buffer_duration = max(0.22, self.asr_buffer_duration * 0.5)
        if self.asr_max_buffer_duration is None:
            self.asr_max_buffer_duration = max(self.asr_buffer_duration, 0.5)
        if self.translation_batch_chars <= 0:
            self.translation_batch_chars = 28
        if self.tts_merge_chars <= 0:
            self.tts_merge_chars = 80


class RealtimePipeline:
    """
    实时翻译流水线
    完整的实时音频翻译解决方案
    """
    _HARD_BREAK_CHARS = "。！？!?;\n"
    _SOFT_BREAK_CHARS = "，,、 "
    _HALLUCINATION_PHRASES = (
        "字幕by",
        "字幕 by",
        "字幕制作",
        "字幕製作",
        "字幕组",
        "字幕組",
        "字幕提供",
        "字幕製作人",
        "字幕制作人",
        "感谢观看",
        "谢谢观看",
        "謝謝觀看",
        "谢谢大家收看",
        "謝謝大家收看",
        "thanks for watching",
        "thank you for watching",
        "请点赞",
        "记得点赞",
        "记得订阅",
        "点赞订阅",
        "欢迎订阅",
        "下次再见",
        "关注我们",
    )
    _AUTO_INPUT_PATTERNS = (
        "cable output",
        "vb-audio virtual",
        "vb-audio point",
        "stereo mix",
        "立体声混音",
    )
    
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
        self._last_emitted_segment = ""
        self._agreement_candidate = ""
        self._agreement_hits = 0
        
        # 历史记录
        self._history: deque = deque(maxlen=100)
        
        logger.info("RealtimePipeline初始化完成")

    @staticmethod
    def _normalize_text(text: str) -> str:
        return " ".join((text or "").replace("\r", " ").replace("\n", " ").split())

    @staticmethod
    def _cjk_ratio(text: str) -> float:
        if not text:
            return 0.0
        total = 0
        cjk = 0
        for ch in text:
            if ch.isspace():
                continue
            total += 1
            code = ord(ch)
            if (
                0x4E00 <= code <= 0x9FFF
                or 0x3400 <= code <= 0x4DBF
                or 0x20000 <= code <= 0x2A6DF
            ):
                cjk += 1
        if total == 0:
            return 0.0
        return cjk / float(total)

    @staticmethod
    def _squash_repeated_tokens(text: str) -> str:
        tokens = text.split()
        if not tokens:
            return text
        dedup_tokens: List[str] = []
        prev = None
        streak = 0
        for token in tokens:
            if token == prev:
                streak += 1
            else:
                prev = token
                streak = 1
            if streak <= 2:
                dedup_tokens.append(token)
        deduped = " ".join(dedup_tokens)
        # 抑制字符级重复，如“啊啊啊啊啊”
        return re.sub(r"(.)\1{3,}", r"\1\1\1", deduped)

    def _sanitize_source_segment(self, text: str, language: str, confidence: float) -> str:
        cleaned = self._normalize_text(text)
        if not cleaned:
            return ""

        if self.config.drop_hallucination:
            lowered = cleaned.lower()
            had_hallucination = any(phrase in lowered for phrase in self._HALLUCINATION_PHRASES)
            if had_hallucination:
                cleaned = re.sub(r"字幕\s*by\S*", "", cleaned, flags=re.IGNORECASE)
                for phrase in self._HALLUCINATION_PHRASES:
                    cleaned = cleaned.replace(phrase, "")
                    cleaned = cleaned.replace(phrase.upper(), "")
                    cleaned = cleaned.replace(phrase.capitalize(), "")
                cleaned = self._normalize_text(cleaned)
                if had_hallucination and len(cleaned) <= max(28, int(self.config.stream_max_chars) * 2):
                    return ""
                if not cleaned:
                    return ""

            # 更激进过滤字幕/片尾类幻听文本，避免污染后续翻译。
            subtitle_like = (
                "字幕" in cleaned
                or "收看" in cleaned
                or "下次再见" in cleaned
                or "下次再見" in cleaned
            )
            if subtitle_like and len(cleaned) <= max(32, int(self.config.stream_max_chars) * 3):
                return ""

        cleaned = self._squash_repeated_tokens(cleaned)
        if not cleaned:
            return ""

        min_conf = max(0.0, float(self.config.min_asr_confidence))
        if confidence > 0.0 and confidence < min_conf and len(cleaned) <= max(10, int(self.config.stream_max_chars) * 2):
            return ""

        lang = (language or self.config.source_lang or "").lower()
        if lang.startswith("zh") or self.config.source_lang.lower().startswith("zh"):
            ratio = self._cjk_ratio(cleaned)
            # 非中文噪声片段（如幻听英文口号）丢弃
            if ratio < float(self.config.min_cjk_ratio) and len(cleaned) >= 6:
                if ratio < 0.05 and not any(ch.isdigit() for ch in cleaned):
                    return ""

        return cleaned

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
        if curr_text == prev_text:
            return ""
        if curr_text.startswith(prev_text):
            return curr_text[len(prev_text):]

        overlap = self._suffix_prefix_overlap(prev_text, curr_text)
        if overlap > 0:
            delta = curr_text[overlap:]
            if delta:
                return delta

        # 识别器上下文重置时，直接把当前文本当作新增内容
        if len(curr_text) + 4 < len(prev_text):
            return curr_text
        # 对分块ASR（每次返回独立片段）兜底：把当前块作为新增文本
        return curr_text

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

    def _enqueue_translation_text(self, text: str, language: str, confidence: float = 0.0) -> None:
        payload = {
            "text": text,
            "language": language,
            "confidence": confidence,
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
            self._last_emitted_segment = ""
            self._agreement_candidate = ""
            self._agreement_hits = 0

    def _should_emit_segment_locked(self, segment: str) -> bool:
        if segment == self._last_emitted_segment:
            return False

        agreement = max(1, int(self.config.stream_agreement))
        # 带强断句符的片段可直接提交，避免尾句等待
        if agreement <= 1 or (segment and segment[-1] in self._HARD_BREAK_CHARS):
            self._last_emitted_segment = segment
            self._agreement_candidate = ""
            self._agreement_hits = 0
            return True

        if segment == self._agreement_candidate:
            self._agreement_hits += 1
        else:
            self._agreement_candidate = segment
            self._agreement_hits = 1

        if self._agreement_hits < agreement:
            return False

        self._last_emitted_segment = segment
        self._agreement_candidate = ""
        self._agreement_hits = 0
        return True
    
    def _on_transcription(self, result: TranscriptionResult) -> None:
        """转录回调"""
        if not self._running or not result.text.strip():
            return
        
        try:
            current_text = self._normalize_text(result.text)
            confidence = float(getattr(result, "confidence", 0.0) or 0.0)
            with self._text_state_lock:
                delta_text = self._extract_incremental_text(self._last_asr_text, current_text)
                self._last_asr_text = current_text
                if delta_text:
                    sanitized_delta = self._sanitize_source_segment(delta_text, result.language, confidence)
                    if sanitized_delta:
                        self._pending_source_text = f"{self._pending_source_text} {sanitized_delta}".strip()
                segments = self._collect_ready_segments_locked(force=False)

            for segment in segments:
                sanitized_segment = self._sanitize_source_segment(segment, result.language, confidence)
                if not sanitized_segment:
                    continue
                with self._text_state_lock:
                    if not self._should_emit_segment_locked(sanitized_segment):
                        continue
                self._enqueue_translation_text(sanitized_segment, result.language, confidence=confidence)
        except Exception as e:
            logger.error(f"转录入队失败: {e}")

    def _translation_loop(self) -> None:
        """翻译与TTS处理线程"""
        while self._running or not self._translation_queue.empty():
            try:
                payload = self._translation_queue.get(timeout=0.1)
            except queue.Empty:
                for segment in self._drain_pending_segments(force=False):
                    self._enqueue_translation_text(
                        segment,
                        self.config.source_lang,
                        confidence=float(self.config.min_asr_confidence),
                    )
                if not self._running and self._translation_queue.empty():
                    break
                continue

            source_text, source_language, created_at, source_confidence = self._coalesce_translation_payload(payload)
            if not source_text.strip():
                continue

            logger.info(f"[识别] {source_text}")

            try:
                translation = self._mt_engine.translate(
                    source_text,
                    source_lang=self.config.source_lang,
                    target_lang=self.config.target_lang,
                ) if not self.config.direct_asr_translate else None

                translated_text = source_text if self.config.direct_asr_translate else translation.translated_text
                logger.info(f"[翻译] {translated_text}")

                item = {
                    "timestamp": time.time(),
                    "source": source_text,
                    "translation": translated_text,
                    "language": source_language,
                    "asr_confidence": source_confidence,
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
                    self._enqueue_tts_text(translated_text)

            except Exception as e:
                logger.error(f"翻译错误: {e}")

    def _coalesce_translation_payload(self, payload: Dict) -> tuple[str, str, float, float]:
        source_language = str(payload.get("language", self.config.source_lang))
        created_at = float(payload.get("created_at", time.time()))
        confidences = [float(payload.get("confidence", 0.0) or 0.0)]
        merged_text = [str(payload.get("text", "")).strip()]
        max_chars = max(8, int(self.config.translation_batch_chars))

        total_chars = len(merged_text[0]) if merged_text[0] else 0
        while total_chars < max_chars:
            try:
                nxt = self._translation_queue.get_nowait()
            except queue.Empty:
                break

            nxt_text = str(nxt.get("text", "")).strip()
            if not nxt_text:
                continue

            merged_text.append(nxt_text)
            total_chars += len(nxt_text) + 1
            created_at = min(created_at, float(nxt.get("created_at", created_at)))
            confidences.append(float(nxt.get("confidence", 0.0) or 0.0))
            if nxt_text[-1] in self._HARD_BREAK_CHARS:
                break

        avg_conf = sum(confidences) / len(confidences) if confidences else 0.0
        return self._normalize_text(" ".join(merged_text)), source_language, created_at, avg_conf

    def _coalesce_tts_text(self, text: str) -> str:
        merged = text.strip()
        if not merged:
            return ""

        max_chars = max(24, int(self.config.tts_merge_chars))
        while len(merged) < max_chars:
            try:
                nxt = self._tts_queue.get_nowait()
            except queue.Empty:
                break
            nxt = nxt.strip()
            if not nxt:
                continue
            merged = f"{merged} {nxt}".strip()
            if nxt[-1] in self._HARD_BREAK_CHARS:
                break
        return merged

    def _tts_loop(self) -> None:
        while self._running or not self._tts_queue.empty():
            try:
                text = self._tts_queue.get(timeout=0.1)
            except queue.Empty:
                if not self._running and self._tts_queue.empty():
                    break
                continue

            try:
                merged_text = self._coalesce_tts_text(text)
                if merged_text:
                    self._synthesize_and_play(merged_text)
            except Exception as exc:
                logger.error(f"TTS线程错误: {exc}")
    
    def _synthesize_and_play(self, text: str) -> None:
        """合成并播放"""
        try:
            result = self._tts_engine.synthesize(text)
            if result.audio is None or len(result.audio) == 0:
                return
            
            if self.config.output_to_virtual_device and self._virtual_device:
                # 输出到虚拟设备
                self._audio_output.play_to_device(
                    result.audio,
                    self._virtual_device.output_device_id,
                    sample_rate=result.sample_rate,
                )
            else:
                # 直接播放
                self._audio_output.play(result.audio, sample_rate=result.sample_rate)
            
        except Exception as e:
            logger.error(f"合成/播放错误: {e}")

    def _resolve_auto_input_device_id(self) -> Optional[int]:
        """在未显式指定输入设备时自动选择更适合会议转写的设备。"""
        if self.config.input_device_id is not None:
            return self.config.input_device_id

        try:
            capturer = AudioCapturer()
            devices = capturer.list_devices()
        except Exception as exc:
            logger.warning(f"自动探测输入设备失败: {exc}")
            return None

        for pattern in self._AUTO_INPUT_PATTERNS:
            for dev in devices:
                name = str(dev.get("name", "")).lower()
                if pattern in name:
                    dev_id = int(dev.get("id"))
                    logger.info(f"自动选择输入设备: [{dev_id}] {dev.get('name')}")
                    return dev_id
        return None
    
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
            resolved_input_device = self._resolve_auto_input_device_id()
            if resolved_input_device is not None:
                self.config.input_device_id = resolved_input_device
            self._audio_capturer = AudioCapturer(
                device_id=self.config.input_device_id,
                device_name=self.config.input_device_name,
            )
            
            # 初始化ASR
            settings.asr.language = self.config.source_lang or settings.asr.language
            if self.config.direct_asr_translate:
                backend = (settings.asr.model_type or "").lower()
                if backend not in {"faster-whisper", "whisper"}:
                    logger.warning("ASR直译仅支持 whisper/faster-whisper，已降级为常规MT流程")
                    self.config.direct_asr_translate = False
                elif (self.config.target_lang or "").lower() != "en":
                    logger.warning("ASR直译目前仅支持目标语言为英语，已降级为常规MT流程")
                    self.config.direct_asr_translate = False

            settings.asr.task = "translate" if self.config.direct_asr_translate else "transcribe"
            asr_min_buffer = (
                float(self.config.asr_min_buffer_duration)
                if self.config.asr_min_buffer_duration is not None
                else max(0.25, float(self.config.asr_buffer_duration) * 0.55)
            )
            asr_max_buffer = (
                float(self.config.asr_max_buffer_duration)
                if self.config.asr_max_buffer_duration is not None
                else max(float(self.config.asr_buffer_duration), asr_min_buffer + 0.1)
            )
            self._streaming_asr = StreamingASR(
                callback=self._on_transcription,
                buffer_duration=self.config.asr_buffer_duration,
                overlap_duration=self.config.asr_overlap,
                min_buffer_duration=asr_min_buffer,
                max_buffer_duration=asr_max_buffer,
                vad_enabled=self.config.asr_vad_enabled,
                vad_energy_threshold=self.config.asr_vad_energy_threshold,
                vad_silence_duration=self.config.asr_vad_silence_duration,
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
                if self._audio_capturer:
                    try:
                        self._audio_capturer.stop()
                    except Exception:
                        pass
                if self._streaming_asr:
                    try:
                        self._streaming_asr.stop()
                    except Exception:
                        pass
                if self._audio_output:
                    try:
                        self._audio_output.stop()
                    except Exception:
                        pass
                if self._tts_engine:
                    try:
                        self._tts_engine.close()
                    except Exception:
                        pass
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

            if self._audio_output:
                try:
                    self._audio_output.stop()
                except Exception as exc:
                    logger.warning(f"停止音频输出异常: {exc}")
            if self._tts_engine:
                try:
                    self._tts_engine.close()
                except Exception as exc:
                    logger.warning(f"释放TTS资源异常: {exc}")

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
    stream_profile: str = "realtime",
    direct_asr_translate: bool = False,
    input_device_id: Optional[int] = None,
    input_device_name: Optional[str] = None,
    result_callback: Optional[Callable[[Dict], None]] = None,
) -> RealtimePipeline:
    """创建实时翻译流水线的便捷函数"""
    config = RealtimeConfig(
        source_lang=source_lang,
        target_lang=target_lang,
        enable_tts=enable_tts,
        direct_asr_translate=direct_asr_translate,
        output_to_virtual_device=use_virtual_device,
        stream_profile=stream_profile,
        input_device_id=input_device_id,
        input_device_name=input_device_name,
    )
    return RealtimePipeline(config, result_callback=result_callback)
