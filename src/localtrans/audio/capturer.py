"""
系统音频捕获模块
使用WASAPI捕获系统音频输出（如会议软件）
"""

import threading
import queue
from typing import Callable, Optional
from dataclasses import dataclass
from enum import Enum

import numpy as np
import sounddevice as sd
from loguru import logger

from localtrans.config import settings


class CaptureState(Enum):
    """捕获状态"""
    IDLE = "idle"
    CAPTURING = "capturing"
    PAUSED = "paused"


@dataclass
class AudioChunk:
    """音频数据块"""
    data: np.ndarray
    sample_rate: int
    channels: int
    timestamp: float
    duration: float


class AudioCapturer:
    """
    系统音频捕获器
    支持捕获指定应用程序或系统全局音频
    """
    
    def __init__(
        self,
        sample_rate: int = None,
        channels: int = None,
        chunk_size: int = None,
        device_id: Optional[int] = None,
        device_name: Optional[str] = None,
    ):
        self.sample_rate = sample_rate or settings.audio.sample_rate
        self.channels = channels or settings.audio.channels
        self.chunk_size = chunk_size or settings.audio.chunk_size
        self.device_id = device_id
        self.device_name = device_name or settings.audio.virtual_input_device
        self._requested_sample_rate = int(self.sample_rate)
        self._requested_channels = int(self.channels)
        self._stream_sample_rate = int(self.sample_rate)
        self._stream_channels = int(self.channels)
        self._device_resolved_id = device_id
        
        self._state = CaptureState.IDLE
        self._stream: Optional[sd.InputStream] = None
        self._audio_queue: queue.Queue[AudioChunk] = queue.Queue(maxsize=100)
        self._callback: Optional[Callable[[AudioChunk], None]] = None
        self._lock = threading.Lock()
        
        logger.info(f"AudioCapturer初始化: sample_rate={self.sample_rate}, channels={self.channels}")
    
    def list_devices(self) -> list:
        """列出所有音频输入设备"""
        devices = sd.query_devices()
        input_devices = []
        for i, dev in enumerate(devices):
            if dev['max_input_channels'] > 0:
                input_devices.append({
                    'id': i,
                    'name': dev['name'],
                    'channels': dev['max_input_channels'],
                    'sample_rate': dev['default_samplerate'],
                })
        logger.debug(f"发现 {len(input_devices)} 个输入设备")
        return input_devices

    @staticmethod
    def _normalize_audio(indata: np.ndarray) -> np.ndarray:
        arr = np.asarray(indata)
        if arr.size == 0:
            return np.array([], dtype=np.float32)

        if arr.ndim > 1:
            arr = np.mean(arr, axis=1)

        if np.issubdtype(arr.dtype, np.integer):
            info = np.iinfo(arr.dtype)
            denom = float(max(abs(info.min), abs(info.max), 1))
            arr = arr.astype(np.float32) / denom
        else:
            arr = arr.astype(np.float32, copy=False)
        return np.ascontiguousarray(arr)

    @staticmethod
    def _resample_linear(audio: np.ndarray, source_rate: int, target_rate: int) -> np.ndarray:
        if source_rate == target_rate:
            return audio
        if audio.size == 0:
            return np.array([], dtype=np.float32)

        duration = audio.size / float(source_rate)
        target_len = max(1, int(round(duration * target_rate)))
        src_x = np.linspace(0.0, 1.0, num=audio.size, endpoint=True)
        dst_x = np.linspace(0.0, 1.0, num=target_len, endpoint=True)
        return np.interp(dst_x, src_x, audio).astype(np.float32)

    def _resolve_stream_params(self, device_id: Optional[int]) -> tuple[int, int]:
        desired_sr = int(self._requested_sample_rate)
        desired_ch = int(self._requested_channels)

        devices = sd.query_devices()
        max_input_channels = desired_ch
        default_sr = desired_sr
        if device_id is not None:
            try:
                dev_info = devices[int(device_id)]
                max_input_channels = int(max(1, dev_info.get("max_input_channels", desired_ch)))
                default_sr = int(dev_info.get("default_samplerate", desired_sr) or desired_sr)
            except Exception:
                pass

        channel_candidates = []
        for ch in (desired_ch, min(desired_ch, max_input_channels), 1):
            ch = int(max(1, ch))
            if ch <= max_input_channels and ch not in channel_candidates:
                channel_candidates.append(ch)
        if not channel_candidates:
            channel_candidates = [1]

        sample_rate_candidates = []
        for sr in (desired_sr, default_sr, 48000, 44100, 32000, 24000, 22050, 16000, 8000):
            sr = int(max(8000, sr))
            if sr not in sample_rate_candidates:
                sample_rate_candidates.append(sr)

        last_exc = None
        for ch in channel_candidates:
            for sr in sample_rate_candidates:
                try:
                    sd.check_input_settings(
                        device=device_id,
                        samplerate=sr,
                        channels=ch,
                        dtype=np.int16,
                    )
                    if sr != desired_sr or ch != desired_ch:
                        logger.warning(
                            f"输入设备参数自动降级: {desired_sr}Hz/{desired_ch}ch -> {sr}Hz/{ch}ch"
                        )
                    return sr, ch
                except Exception as exc:
                    last_exc = exc
                    continue

        if last_exc is not None:
            raise last_exc
        return desired_sr, desired_ch
    
    def find_virtual_device(self, name_pattern: str = "VB-Audio") -> Optional[int]:
        """查找虚拟音频设备"""
        devices = self.list_devices()
        for dev in devices:
            if name_pattern.lower() in dev['name'].lower():
                logger.info(f"找到虚拟设备: {dev['name']} (ID: {dev['id']})")
                return dev['id']
        logger.warning(f"未找到匹配 '{name_pattern}' 的虚拟设备")
        return None
    
    def _audio_callback(self, indata: np.ndarray, frames: int, time_info, status) -> None:
        """音频流回调函数"""
        if status:
            logger.warning(f"音频流状态: {status}")

        processed = self._normalize_audio(indata)
        if self._stream_sample_rate != self.sample_rate:
            processed = self._resample_linear(
                processed,
                source_rate=self._stream_sample_rate,
                target_rate=self.sample_rate,
            )
        
        # 创建音频块
        chunk = AudioChunk(
            data=processed,
            sample_rate=self.sample_rate,
            channels=self.channels,
            timestamp=time_info.currentTime,
            duration=(len(processed) / float(self.sample_rate)) if self.sample_rate > 0 else 0.0,
        )
        
        # 放入队列或直接回调
        try:
            if self._callback:
                self._callback(chunk)
            else:
                self._audio_queue.put_nowait(chunk)
        except queue.Full:
            logger.warning("音频队列已满，丢弃数据块")
    
    def start(self, callback: Optional[Callable[[AudioChunk], None]] = None) -> None:
        """开始捕获音频"""
        with self._lock:
            if self._state == CaptureState.CAPTURING:
                logger.warning("音频捕获已在运行")
                return
            
            self._callback = callback
            
            # 确定设备
            device_id = self.device_id
            if device_id is None and self.device_name:
                device_id = self.find_virtual_device(self.device_name)
            self._device_resolved_id = device_id
            stream_sr, stream_ch = self._resolve_stream_params(device_id)
            self._stream_sample_rate = int(stream_sr)
            self._stream_channels = int(stream_ch)
            
            # 创建音频流
            self._stream = sd.InputStream(
                device=device_id,
                channels=self._stream_channels,
                samplerate=self._stream_sample_rate,
                blocksize=self.chunk_size,
                dtype=np.int16,
                callback=self._audio_callback,
            )
            
            self._stream.start()
            self._state = CaptureState.CAPTURING
            logger.info(
                "音频捕获已启动 "
                f"(设备: {device_id or '默认'}, "
                f"stream={self._stream_sample_rate}Hz/{self._stream_channels}ch, "
                f"target={self.sample_rate}Hz/{self.channels}ch)"
            )
    
    def stop(self) -> None:
        """停止捕获音频"""
        with self._lock:
            if self._state != CaptureState.CAPTURING:
                return
            
            if self._stream:
                self._stream.stop()
                self._stream.close()
                self._stream = None
            
            self._state = CaptureState.IDLE
            logger.info("音频捕获已停止")
    
    def pause(self) -> None:
        """暂停捕获"""
        with self._lock:
            if self._stream and self._state == CaptureState.CAPTURING:
                self._stream.stop()
                self._state = CaptureState.PAUSED
                logger.info("音频捕获已暂停")
    
    def resume(self) -> None:
        """恢复捕获"""
        with self._lock:
            if self._stream and self._state == CaptureState.PAUSED:
                self._stream.start()
                self._state = CaptureState.CAPTURING
                logger.info("音频捕获已恢复")
    
    def get_chunk(self, timeout: float = 1.0) -> Optional[AudioChunk]:
        """从队列获取音频块"""
        try:
            return self._audio_queue.get(timeout=timeout)
        except queue.Empty:
            return None
    
    @property
    def state(self) -> CaptureState:
        """当前状态"""
        return self._state
    
    def __enter__(self):
        self.start()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.stop()
        return False
