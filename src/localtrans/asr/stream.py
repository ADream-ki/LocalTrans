"""
流式ASR处理
实现实时语音识别的流式处理
"""

import queue
import threading
import time
from dataclasses import dataclass
from enum import Enum
from typing import Callable, Generator, Optional

import numpy as np
from loguru import logger

from localtrans.asr.engine import ASREngine, TranscriptionResult
from localtrans.audio.utils.vad import VoiceActivityDetector
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
    使用LocalTrans自管的VAD分段和partial/final状态机，而不是简单切窗重跑。
    """

    def __init__(
        self,
        asr_engine: Optional[ASREngine] = None,
        callback: Optional[Callable[[TranscriptionResult], None]] = None,
        buffer_duration: float = 2.0,
        overlap_duration: float = 0.5,
        min_buffer_duration: Optional[float] = None,
        max_buffer_duration: Optional[float] = None,
        vad_enabled: bool = True,
        vad_energy_threshold: float = 0.01,
        vad_silence_duration: float = 0.18,
        streaming_mode: str = "legacy",
        vad_mode: str = "webrtc",
        partial_decode_interval: Optional[float] = None,
    ):
        self.asr_engine = asr_engine or ASREngine()
        self.callback = callback
        self.buffer_duration = float(buffer_duration)
        self.overlap_duration = float(overlap_duration)
        self.min_buffer_duration = (
            float(min_buffer_duration)
            if min_buffer_duration is not None
            else max(0.2, float(buffer_duration) * 0.55)
        )
        self.max_buffer_duration = (
            float(max_buffer_duration)
            if max_buffer_duration is not None
            else max(float(buffer_duration), self.min_buffer_duration + 0.1)
        )
        self.vad_enabled = bool(vad_enabled)
        self.vad_energy_threshold = max(1e-5, float(vad_energy_threshold))
        self.vad_silence_duration = max(0.05, float(vad_silence_duration))
        self.streaming_mode = (streaming_mode or "legacy").strip().lower()
        if self.streaming_mode not in {"legacy", "managed"}:
            self.streaming_mode = "legacy"
        self.vad_mode = (vad_mode or "webrtc").strip().lower()
        self.partial_decode_interval = (
            float(partial_decode_interval)
            if partial_decode_interval is not None
            else max(0.25, min(self.buffer_duration * 0.5, self.max_buffer_duration * 0.5))
        )

        self._audio_queue: queue.Queue[AudioBuffer] = queue.Queue(maxsize=200)
        self._result_queue: queue.Queue[TranscriptionResult] = queue.Queue(maxsize=100)
        self._state = StreamState.IDLE
        self._worker_thread: Optional[threading.Thread] = None
        self._last_queue_overflow_log_ts = 0.0
        self._overflow_drop_count = 0
        self._queue_high_watermark = int(self._audio_queue.maxsize * 0.7)
        self._queue_recover_target = max(8, int(self._audio_queue.maxsize * 0.25))
        self._vad_runtime_fallback_logged = False

        self._sample_rate = settings.audio.sample_rate
        self._buffer_samples = int(self.buffer_duration * self._sample_rate)
        self._overlap_samples = int(self.overlap_duration * self._sample_rate)
        self._speech_overlap_samples = max(self._overlap_samples, int(0.12 * self._sample_rate))
        self._min_buffer_samples = int(self.min_buffer_duration * self._sample_rate)
        self._max_buffer_samples = int(self.max_buffer_duration * self._sample_rate)
        self._vad_silence_samples = int(self.vad_silence_duration * self._sample_rate)
        self._partial_step_samples = max(1, int(self.partial_decode_interval * self._sample_rate))

        self._vad_detector: Optional[VoiceActivityDetector] = None
        self._vad_frame_samples = max(1, int(0.03 * self._sample_rate))
        if self.vad_enabled and self.streaming_mode == "managed":
            self._vad_detector = VoiceActivityDetector(
                sample_rate=self._sample_rate,
                mode=self.vad_mode,
            )
            self._vad_frame_samples = max(
                1,
                int(getattr(self._vad_detector, "_frame_size", self._vad_frame_samples)),
            )

        effective_vad = self._vad_detector.mode if self._vad_detector else "off"
        logger.info(
            "StreamingASR初始化: "
            f"mode={self.streaming_mode}, "
            f"buffer={self.buffer_duration}s, overlap={self.overlap_duration}s, "
            f"speech_overlap={self._speech_overlap_samples / self._sample_rate:.3f}s, "
            f"min={self.min_buffer_duration}s, max={self.max_buffer_duration}s, "
            f"vad={self.vad_enabled}/{effective_vad}, "
            f"partial_step={self._partial_step_samples / self._sample_rate:.3f}s"
        )

    def put_audio(self, audio_data: np.ndarray, timestamp: float = 0.0) -> None:
        """添加音频数据到队列"""
        if self._state != StreamState.RUNNING:
            return

        try:
            self._audio_queue.put_nowait(AudioBuffer(data=audio_data, timestamp=timestamp))
        except queue.Full:
            try:
                self._audio_queue.get_nowait()
                self._audio_queue.put_nowait(AudioBuffer(data=audio_data, timestamp=timestamp))
            except queue.Empty:
                pass

            self._overflow_drop_count += 1
            now = time.time()
            if now - self._last_queue_overflow_log_ts >= 2.0:
                logger.warning(f"音频队列拥塞，最近丢弃{self._overflow_drop_count}个音频块")
                self._overflow_drop_count = 0
                self._last_queue_overflow_log_ts = now

    @staticmethod
    def _flatten_audio(audio_data: np.ndarray) -> np.ndarray:
        arr = np.asarray(audio_data)
        if arr.ndim > 1:
            arr = arr.reshape(-1)
        return np.ascontiguousarray(arr)

    def _chunk_rms(self, audio_chunk: np.ndarray) -> float:
        arr = self._flatten_audio(audio_chunk)
        if arr.size == 0:
            return 0.0
        if np.issubdtype(arr.dtype, np.integer):
            info = np.iinfo(arr.dtype)
            denom = float(max(abs(info.min), abs(info.max), 1))
            arr = arr.astype(np.float32) / denom
        else:
            arr = arr.astype(np.float32, copy=False)
        return float(np.sqrt(np.mean(np.square(arr))))

    def _chunk_speech_score(self, audio_chunk: np.ndarray) -> float:
        arr = self._flatten_audio(audio_chunk)
        if arr.size == 0:
            return 0.0

        if not self.vad_enabled or self._vad_detector is None:
            return 1.0 if self._chunk_rms(arr) >= self.vad_energy_threshold else 0.0

        speech_frames = 0
        total_frames = 0
        for start in range(0, len(arr), self._vad_frame_samples):
            frame = arr[start:start + self._vad_frame_samples]
            if len(frame) < self._vad_frame_samples:
                frame = np.pad(frame, (0, self._vad_frame_samples - len(frame)))
            try:
                if self._vad_detector.is_speech(frame):
                    speech_frames += 1
            except Exception as exc:
                if not self._vad_runtime_fallback_logged:
                    logger.warning(f"VAD运行失败，回退到能量阈值检测: {exc}")
                    self._vad_runtime_fallback_logged = True
                return 1.0 if self._chunk_rms(arr) >= self.vad_energy_threshold else 0.0
            total_frames += 1

        if total_frames == 0:
            return 0.0
        return speech_frames / float(total_frames)

    def _is_chunk_speech_energy(self, audio_chunk: np.ndarray) -> bool:
        if not self.vad_enabled:
            return True
        return self._chunk_rms(audio_chunk) >= self.vad_energy_threshold

    def _is_chunk_speech(self, audio_chunk: np.ndarray) -> bool:
        if not self.vad_enabled:
            return True

        score = self._chunk_speech_score(audio_chunk)
        if self._vad_detector is None or self._vad_detector.mode == "energy":
            return score > 0.0
        return score >= 0.25

    def _push_result(self, result: TranscriptionResult) -> None:
        if self.callback:
            self.callback(result)
        try:
            self._result_queue.put_nowait(result)
        except queue.Full:
            try:
                self._result_queue.get_nowait()
                self._result_queue.put_nowait(result)
            except queue.Empty:
                pass

    def _trim_audio_queue_if_needed(self) -> None:
        """队列积压时快速丢弃旧块，仅保留最新音频，避免全链路失去实时性。"""
        qsize = self._audio_queue.qsize()
        if qsize < self._queue_high_watermark:
            return

        drop_target = max(0, qsize - self._queue_recover_target)
        dropped = 0
        for _ in range(drop_target):
            try:
                self._audio_queue.get_nowait()
                dropped += 1
            except queue.Empty:
                break

        if dropped <= 0:
            return

        now = time.time()
        if now - self._last_queue_overflow_log_ts >= 1.5:
            logger.warning(
                f"ASR处理滞后，快速丢弃{dropped}个旧音频块(qsize={qsize} -> {self._audio_queue.qsize()})"
            )
            self._last_queue_overflow_log_ts = now

    def _transcribe_audio(
        self,
        audio_data: np.ndarray,
        is_final: bool,
        reason: str,
    ) -> Optional[TranscriptionResult]:
        audio_arr = self._flatten_audio(audio_data)
        if audio_arr.size == 0:
            return None

        try:
            result = self.asr_engine.transcribe(audio_arr)
        except Exception as exc:
            logger.error(f"转录错误({reason}): {exc}")
            return None

        result.is_final = is_final
        logger.debug(
            f"转录结果({reason}, final={is_final}): {result.text[:60] if result.text else ''}"
        )
        return result

    def _process_loop_managed(self) -> None:
        """LocalTrans自管流式状态机。"""
        speech_buffer: list[np.ndarray] = []
        speech_samples = 0
        silence_buffer: list[np.ndarray] = []
        silence_samples = 0
        has_speech = False
        last_partial_text = ""
        last_partial_samples = 0

        def reset_state(keep_tail: bool = False) -> None:
            nonlocal speech_buffer, speech_samples, silence_buffer, silence_samples
            nonlocal has_speech, last_partial_text, last_partial_samples

            tail: Optional[np.ndarray] = None
            if keep_tail and speech_buffer:
                merged = np.concatenate(speech_buffer)
                if merged.size > self._speech_overlap_samples:
                    tail = np.ascontiguousarray(merged[-self._speech_overlap_samples:])

            speech_buffer = [tail] if tail is not None and tail.size > 0 else []
            speech_samples = len(tail) if tail is not None else 0
            silence_buffer = []
            silence_samples = 0
            has_speech = False
            last_partial_text = ""
            last_partial_samples = 0

        def decode_current(reason: str, is_final: bool) -> None:
            nonlocal speech_buffer, speech_samples, silence_buffer, silence_samples
            nonlocal has_speech, last_partial_text, last_partial_samples

            if not speech_buffer:
                reset_state(keep_tail=False)
                return

            audio_parts = list(speech_buffer)
            if is_final and silence_buffer:
                audio_parts.extend(silence_buffer)

            result = self._transcribe_audio(np.concatenate(audio_parts), is_final=is_final, reason=reason)
            current_text = result.text.strip() if result and result.text else ""

            should_emit = False
            if result and current_text:
                should_emit = is_final or current_text != last_partial_text

            if result and should_emit:
                self._push_result(result)

            if is_final:
                reset_state(keep_tail=(reason == "max-window"))
                return

            last_partial_text = current_text
            last_partial_samples = speech_samples

        while self._state == StreamState.RUNNING:
            try:
                self._trim_audio_queue_if_needed()
                audio_buffer = self._audio_queue.get(timeout=0.1)
            except queue.Empty:
                if has_speech and speech_samples >= self._min_buffer_samples:
                    if not self.vad_enabled:
                        decode_current("idle-timeout", is_final=True)
                    elif silence_samples >= self._vad_silence_samples:
                        decode_current("silence-timeout", is_final=True)
                continue
            except Exception as exc:
                logger.error(f"处理循环错误: {exc}")
                continue

            chunk = self._flatten_audio(audio_buffer.data)
            if chunk.size == 0:
                continue

            chunk_is_speech = self._is_chunk_speech(chunk)

            if chunk_is_speech:
                if silence_buffer:
                    speech_buffer.extend(silence_buffer)
                    speech_samples += silence_samples
                    silence_buffer = []
                    silence_samples = 0

                speech_buffer.append(chunk)
                speech_samples += len(chunk)
                has_speech = True

                if speech_samples >= self._max_buffer_samples:
                    decode_current("max-window", is_final=True)
                    continue

                if (
                    speech_samples >= self._min_buffer_samples
                    and (speech_samples - last_partial_samples) >= self._partial_step_samples
                ):
                    decode_current("partial", is_final=False)
                continue

            if not has_speech:
                continue

            silence_buffer.append(chunk)
            silence_samples += len(chunk)

            total_samples = speech_samples + silence_samples
            if total_samples >= self._max_buffer_samples:
                decode_current("max-window", is_final=True)
            elif speech_samples >= self._min_buffer_samples and silence_samples >= self._vad_silence_samples:
                decode_current("silence", is_final=True)

        if speech_buffer:
            decode_current("final", is_final=True)

    def _process_loop_legacy(self) -> None:
        """兼容旧版：固定窗口 + 能量阈值切分。"""
        buffer = []
        total_samples = 0
        speech_samples = 0
        silence_samples = 0
        has_speech = False

        def flush_window(reason: str) -> None:
            nonlocal buffer, total_samples, speech_samples, silence_samples, has_speech
            if not buffer:
                return

            audio_data = np.concatenate(buffer)
            should_transcribe = (not self.vad_enabled) or has_speech

            if should_transcribe and len(audio_data) > 0:
                result = self._transcribe_audio(audio_data, is_final=True, reason=reason)
                if result and result.text.strip():
                    self._push_result(result)

            overlap_samples = self._speech_overlap_samples if has_speech else self._overlap_samples
            if overlap_samples > 0 and len(audio_data) > overlap_samples:
                tail = np.ascontiguousarray(audio_data[-overlap_samples:])
                buffer = [tail]
                total_samples = len(tail)
                if self.vad_enabled:
                    has_speech = self._is_chunk_speech_energy(tail)
                    speech_samples = total_samples if has_speech else 0
                    silence_samples = 0 if has_speech else total_samples
                else:
                    has_speech = total_samples > 0
                    speech_samples = total_samples
                    silence_samples = 0
            else:
                buffer = []
                total_samples = 0
                speech_samples = 0
                silence_samples = 0
                has_speech = False

        while self._state == StreamState.RUNNING:
            try:
                self._trim_audio_queue_if_needed()
                audio_buffer = self._audio_queue.get(timeout=0.1)
                chunk = self._flatten_audio(audio_buffer.data)
                if chunk.size == 0:
                    continue

                buffer.append(chunk)
                total_samples += len(chunk)

                if self.vad_enabled:
                    if self._is_chunk_speech_energy(chunk):
                        has_speech = True
                        speech_samples += len(chunk)
                        silence_samples = 0
                    else:
                        silence_samples += len(chunk)
                else:
                    has_speech = True
                    speech_samples = total_samples
                    silence_samples = 0

                should_flush = False
                if total_samples >= self._max_buffer_samples:
                    should_flush = True
                elif total_samples >= self._buffer_samples and has_speech:
                    should_flush = True
                elif (
                    self.vad_enabled
                    and has_speech
                    and total_samples >= self._min_buffer_samples
                    and silence_samples >= self._vad_silence_samples
                ):
                    should_flush = True

                if should_flush:
                    flush_window("window")

            except queue.Empty:
                if (
                    self.vad_enabled
                    and has_speech
                    and total_samples >= self._min_buffer_samples
                    and silence_samples >= self._vad_silence_samples
                ):
                    flush_window("silence-timeout")
                continue
            except Exception as exc:
                logger.error(f"处理循环错误: {exc}")

        if buffer:
            flush_window("final")

    def _process_loop(self) -> None:
        """根据配置选择流式处理方案。"""
        if self.streaming_mode == "managed":
            self._process_loop_managed()
            return
        self._process_loop_legacy()

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
