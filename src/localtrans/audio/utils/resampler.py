"""
音频重采样工具
用于不同采样率之间的转换
"""

import numpy as np
from typing import Optional, Union
from fractions import Fraction

from loguru import logger


class AudioResampler:
    """
    音频重采样器
    支持高质量重采样，保持音频质量
    """
    
    def __init__(
        self,
        source_rate: int,
        target_rate: int,
        quality: str = "medium",
    ):
        """
        初始化重采样器
        
        Args:
            source_rate: 源采样率
            target_rate: 目标采样率
            quality: 质量 ("fast", "medium", "high")
        """
        self.source_rate = source_rate
        self.target_rate = target_rate
        self.quality = quality
        
        # 计算重采样比率
        self._ratio = target_rate / source_rate
        self._fraction = Fraction(target_rate, source_rate)
        
        # 质量设置
        self._quality_settings = {
            "fast": {"kaiser_beta": 6.0, "num_taps": 64},
            "medium": {"kaiser_beta": 8.0, "num_taps": 128},
            "high": {"kaiser_beta": 10.0, "num_taps": 256},
        }
        
        self._resampler = None
        self._init_resampler()
        
        logger.info(f"AudioResampler初始化: {source_rate}Hz -> {target_rate}Hz")
    
    def _init_resampler(self):
        """初始化重采样器"""
        # 尝试使用samplerate库（高质量）
        try:
            import samplerate
            self._resampler_type = "samplerate"
            self._resampler = samplerate
            logger.debug("使用samplerate库")
            return
        except ImportError:
            pass
        
        # 尝试使用resampy
        try:
            import resampy
            self._resampler_type = "resampy"
            self._resampler = resampy
            logger.debug("使用resampy库")
            return
        except ImportError:
            pass
        
        # 尝试使用scipy
        try:
            from scipy import signal
            self._resampler_type = "scipy"
            self._resampler = signal
            logger.debug("使用scipy.signal")
            return
        except ImportError:
            pass
        
        # 使用numpy简单实现
        self._resampler_type = "numpy"
        logger.debug("使用numpy实现")
    
    def resample(self, audio: np.ndarray) -> np.ndarray:
        """
        重采样音频
        
        Args:
            audio: 输入音频
            
        Returns:
            重采样后的音频
        """
        if self.source_rate == self.target_rate:
            return audio
        
        if len(audio) == 0:
            return audio
        
        # 确保是float格式
        if audio.dtype == np.int16:
            audio = audio.astype(np.float32) / 32768.0
        elif audio.dtype == np.int32:
            audio = audio.astype(np.float32) / 2147483648.0
        
        if self._resampler_type == "samplerate":
            return self._resample_samplerate(audio)
        elif self._resampler_type == "resampy":
            return self._resample_resampy(audio)
        elif self._resampler_type == "scipy":
            return self._resample_scipy(audio)
        else:
            return self._resample_numpy(audio)
    
    def _resample_samplerate(self, audio: np.ndarray) -> np.ndarray:
        """使用samplerate库"""
        return self._resampler.resample(audio, self._ratio, 'sinc_best')
    
    def _resample_resampy(self, audio: np.ndarray) -> np.ndarray:
        """使用resampy库"""
        return self._resampler.resample(audio, self.source_rate, self.target_rate)
    
    def _resample_scipy(self, audio: np.ndarray) -> np.ndarray:
        """使用scipy.signal"""
        num_samples = int(len(audio) * self._ratio)
        return self._resampler.resample(audio, num_samples)
    
    def _resample_numpy(self, audio: np.ndarray) -> np.ndarray:
        """简单的numpy实现"""
        # 线性插值
        num_samples = int(len(audio) * self._ratio)
        old_indices = np.arange(len(audio))
        new_indices = np.linspace(0, len(audio) - 1, num_samples)
        
        return np.interp(new_indices, old_indices, audio)
    
    def to_int16(self, audio: np.ndarray) -> np.ndarray:
        """转换为int16格式"""
        if audio.dtype != np.float32 and audio.dtype != np.float64:
            return audio.astype(np.int16)
        
        # 归一化并转换
        max_val = np.max(np.abs(audio))
        if max_val > 0:
            audio = audio / max_val
        
        return (audio * 32767).astype(np.int16)


class ResamplingPipeline:
    """
    重采样流水线
    管理多个采样率之间的转换
    """
    
    def __init__(self):
        self._resamplers = {}
    
    def get_resampler(
        self, 
        source_rate: int, 
        target_rate: int,
        quality: str = "medium",
    ) -> AudioResampler:
        """获取或创建重采样器"""
        key = (source_rate, target_rate)
        
        if key not in self._resamplers:
            self._resamplers[key] = AudioResampler(
                source_rate, target_rate, quality
            )
        
        return self._resamplers[key]
    
    def resample(
        self,
        audio: np.ndarray,
        source_rate: int,
        target_rate: int,
    ) -> np.ndarray:
        """执行重采样"""
        resampler = self.get_resampler(source_rate, target_rate)
        return resampler.resample(audio)


def convert_sample_rate(
    audio: np.ndarray,
    source_rate: int,
    target_rate: int,
) -> np.ndarray:
    """
    便捷函数：单次重采样
    
    Args:
        audio: 输入音频
        source_rate: 源采样率
        target_rate: 目标采样率
        
    Returns:
        重采样后的音频
    """
    resampler = AudioResampler(source_rate, target_rate)
    return resampler.resample(audio)


def ensure_sample_rate(
    audio: np.ndarray,
    current_rate: int,
    target_rate: int,
) -> np.ndarray:
    """
    确保音频具有目标采样率
    
    如果当前采样率与目标相同，直接返回
    """
    if current_rate == target_rate:
        return audio
    
    return convert_sample_rate(audio, current_rate, target_rate)
