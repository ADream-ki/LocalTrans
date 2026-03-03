"""
VAD静音检测模块
用于检测音频中的语音活动，过滤静音片段
"""

import numpy as np
from typing import List, Tuple, Optional
from dataclasses import dataclass
from enum import Enum

from loguru import logger


class VADState(Enum):
    """VAD状态"""
    SILENCE = "silence"
    SPEECH = "speech"


@dataclass
class VADSegment:
    """VAD片段"""
    start: float  # 开始时间（秒）
    end: float    # 结束时间（秒）
    is_speech: bool
    
    @property
    def duration(self) -> float:
        return self.end - self.start


class VoiceActivityDetector:
    """
    语音活动检测器
    支持多种VAD后端：能量检测、WebRTC VAD、Silero VAD
    """
    
    def __init__(
        self,
        sample_rate: int = 16000,
        mode: str = "energy",
        frame_duration_ms: int = 30,
        aggressiveness: int = 3,
    ):
        """
        初始化VAD
        
        Args:
            sample_rate: 采样率
            mode: 检测模式 ("energy", "webrtc", "silero")
            frame_duration_ms: 帧时长（毫秒）
            aggressiveness: WebRTC VAD激进程度 (0-3)
        """
        self.sample_rate = sample_rate
        self.mode = mode
        self.frame_duration_ms = frame_duration_ms
        self.aggressiveness = aggressiveness
        
        self._frame_size = int(sample_rate * frame_duration_ms / 1000)
        self._vad = None
        self._init_vad()
        
        # 状态跟踪
        self._state = VADState.SILENCE
        self._speech_start: Optional[float] = None
        self._silence_start: Optional[float] = None
        
        # 能量检测参数
        self._energy_threshold = 0.01
        self._speech_threshold_ratio = 2.0
        
        logger.info(f"VAD初始化: mode={mode}, sample_rate={sample_rate}")
    
    def _init_vad(self):
        """初始化VAD后端"""
        if self.mode == "webrtc":
            try:
                import webrtcvad
                self._vad = webrtcvad.Vad(self.aggressiveness)
                logger.info("WebRTC VAD已加载")
            except ImportError:
                logger.warning("webrtcvad未安装，回退到能量检测")
                self.mode = "energy"
        
        elif self.mode == "silero":
            try:
                import torch
                model, utils = torch.hub.load(
                    repo_or_dir='snakers4/silero-vad',
                    model='silero_vad',
                    force_reload=False,
                    onnx=False,
                )
                self._vad = model
                self._vad.eval()
                logger.info("Silero VAD已加载")
            except Exception as e:
                logger.warning(f"Silero VAD加载失败: {e}，回退到能量检测")
                self.mode = "energy"
    
    def _compute_energy(self, frame: np.ndarray) -> float:
        """计算帧能量"""
        if frame.dtype == np.int16:
            frame = frame.astype(np.float32) / 32768.0
        return float(np.sqrt(np.mean(frame ** 2)))
    
    def _is_speech_energy(self, frame: np.ndarray) -> bool:
        """能量检测"""
        energy = self._compute_energy(frame)
        
        # 动态阈值更新
        if energy > 0:
            self._energy_threshold = (
                0.9 * self._energy_threshold + 
                0.1 * energy / self._speech_threshold_ratio
            )
        
        return energy > self._energy_threshold * self._speech_threshold_ratio
    
    def _is_speech_webrtc(self, frame: np.ndarray) -> bool:
        """WebRTC VAD检测"""
        if frame.dtype != np.int16:
            frame = (frame * 32767).astype(np.int16)
        
        # 确保帧大小正确
        if len(frame) < self._frame_size:
            frame = np.pad(frame, (0, self._frame_size - len(frame)))
        
        return self._vad.is_speech(frame.tobytes(), self.sample_rate)
    
    def _is_speech_silero(self, frame: np.ndarray) -> bool:
        """Silero VAD检测"""
        import torch
        
        if frame.dtype == np.int16:
            frame = frame.astype(np.float32) / 32768.0
        
        # 转换为tensor
        audio_tensor = torch.from_numpy(frame).unsqueeze(0)
        
        with torch.no_grad():
            speech_prob = self._vad(audio_tensor, self.sample_rate).item()
        
        return speech_prob > 0.5
    
    def is_speech(self, frame: np.ndarray) -> bool:
        """
        检测帧是否包含语音
        
        Args:
            frame: 音频帧
            
        Returns:
            是否为语音
        """
        if self.mode == "webrtc":
            return self._is_speech_webrtc(frame)
        elif self.mode == "silero":
            return self._is_speech_silero(frame)
        else:
            return self._is_speech_energy(frame)
    
    def process_audio(
        self, 
        audio: np.ndarray,
        min_speech_duration: float = 0.3,
        min_silence_duration: float = 0.5,
    ) -> List[VADSegment]:
        """
        处理音频，返回语音片段
        
        Args:
            audio: 音频数据
            min_speech_duration: 最小语音时长（秒）
            min_silence_duration: 最小静音时长（秒）
            
        Returns:
            VAD片段列表
        """
        segments = []
        
        # 分帧处理
        num_frames = len(audio) // self._frame_size
        
        speech_frames = []
        current_start = 0.0
        
        for i in range(num_frames):
            start_idx = i * self._frame_size
            end_idx = start_idx + self._frame_size
            frame = audio[start_idx:end_idx]
            
            frame_time = start_idx / self.sample_rate
            is_speech = self.is_speech(frame)
            
            if is_speech:
                if self._state == VADState.SILENCE:
                    # 从静音转到语音
                    self._state = VADState.SPEECH
                    current_start = frame_time
                    speech_frames = [frame]
                else:
                    speech_frames.append(frame)
            else:
                if self._state == VADState.SPEECH:
                    # 从语音转到静音
                    silence_duration = (i - len(speech_frames)) * self.frame_duration_ms / 1000
                    
                    if silence_duration >= min_silence_duration:
                        # 结束语音片段
                        speech_duration = len(speech_frames) * self.frame_duration_ms / 1000
                        
                        if speech_duration >= min_speech_duration:
                            segments.append(VADSegment(
                                start=current_start,
                                end=frame_time,
                                is_speech=True,
                            ))
                        
                        self._state = VADState.SILENCE
                        speech_frames = []
        
        # 处理最后的语音片段
        if speech_frames:
            speech_duration = len(speech_frames) * self.frame_duration_ms / 1000
            if speech_duration >= min_speech_duration:
                segments.append(VADSegment(
                    start=current_start,
                    end=len(audio) / self.sample_rate,
                    is_speech=True,
                ))
        
        return segments
    
    def filter_silence(
        self, 
        audio: np.ndarray,
        min_speech_duration: float = 0.3,
    ) -> Tuple[np.ndarray, List[VADSegment]]:
        """
        过滤静音，只保留语音片段
        
        Args:
            audio: 音频数据
            min_speech_duration: 最小语音时长
            
        Returns:
            (过滤后的音频, 语音片段列表)
        """
        segments = self.process_audio(audio, min_speech_duration)
        
        speech_audio = []
        for seg in segments:
            if seg.is_speech:
                start_idx = int(seg.start * self.sample_rate)
                end_idx = int(seg.end * self.sample_rate)
                speech_audio.append(audio[start_idx:end_idx])
        
        if speech_audio:
            return np.concatenate(speech_audio), segments
        else:
            return np.array([], dtype=audio.dtype), segments
    
    def get_speech_ratio(self, audio: np.ndarray) -> float:
        """计算语音占比"""
        segments = self.process_audio(audio)
        
        if not segments:
            return 0.0
        
        speech_duration = sum(seg.duration for seg in segments if seg.is_speech)
        total_duration = len(audio) / self.sample_rate
        
        return speech_duration / total_duration if total_duration > 0 else 0.0


class StreamingVAD:
    """
    流式VAD处理
    用于实时音频流的静音检测
    """
    
    def __init__(
        self,
        sample_rate: int = 16000,
        speech_threshold: float = 0.5,
        silence_threshold: float = 0.3,
        buffer_duration: float = 0.5,
    ):
        self.sample_rate = sample_rate
        self.speech_threshold = speech_threshold
        self.silence_threshold = silence_threshold
        
        self._vad = VoiceActivityDetector(sample_rate=sample_rate)
        self._buffer: List[np.ndarray] = []
        self._buffer_duration = buffer_duration
        self._buffer_samples = int(buffer_duration * sample_rate)
        
        self._speech_detected = False
        self._silence_count = 0
        self._max_silence_frames = int(silence_threshold / 0.03)  # 30ms帧
    
    def process_frame(self, frame: np.ndarray) -> Tuple[bool, Optional[np.ndarray]]:
        """
        处理音频帧
        
        Returns:
            (是否触发语音, 完整语音数据或None)
        """
        is_speech = self._vad.is_speech(frame)
        
        if is_speech:
            self._buffer.append(frame)
            self._speech_detected = True
            self._silence_count = 0
            return True, None
        else:
            if self._speech_detected:
                self._silence_count += 1
                self._buffer.append(frame)
                
                if self._silence_count >= self._max_silence_frames:
                    # 语音结束，返回缓冲数据
                    if self._buffer:
                        audio = np.concatenate(self._buffer)
                        self._buffer = []
                        self._speech_detected = False
                        return False, audio
            
            return False, None
    
    def reset(self):
        """重置状态"""
        self._buffer = []
        self._speech_detected = False
        self._silence_count = 0
