"""
音频路由管理
管理音频流在不同设备间的路由
"""

import threading
from typing import Optional
from dataclasses import dataclass
from enum import Enum

import numpy as np
import sounddevice as sd
from loguru import logger

from localtrans.config import settings


class RoutingState(Enum):
    """路由状态"""
    INACTIVE = "inactive"
    ACTIVE = "active"
    ERROR = "error"


@dataclass
class AudioRoute:
    """音频路由配置"""
    source_device: Optional[int] = None
    target_device: Optional[int] = None
    sample_rate: int = 48000
    channels: int = 2


class AudioRouter:
    """
    音频路由管理器
    负责将音频流从一个设备路由到另一个设备
    """
    
    def __init__(self):
        self._route: Optional[AudioRoute] = None
        self._state = RoutingState.INACTIVE
        self._input_stream: Optional[sd.InputStream] = None
        self._output_stream: Optional[sd.OutputStream] = None
        self._lock = threading.Lock()
        self._running = False
        
        logger.info("AudioRouter初始化完成")
    
    def setup_route(
        self,
        source_device: Optional[int],
        target_device: Optional[int],
        sample_rate: int = 48000,
        channels: int = 2,
    ) -> None:
        """设置音频路由"""
        self._route = AudioRoute(
            source_device=source_device,
            target_device=target_device,
            sample_rate=sample_rate,
            channels=channels,
        )
        logger.info(f"音频路由已配置: {source_device} -> {target_device}")
    
    def _router_callback(self, indata: np.ndarray, frames: int, time_info, status) -> None:
        """路由回调 - 直接传递音频"""
        if self._output_stream and self._output_stream.active:
            self._output_stream.write(indata)
    
    def start(self) -> bool:
        """启动音频路由"""
        if not self._route:
            logger.error("请先设置音频路由")
            return False
        
        with self._lock:
            try:
                # 创建输入流
                self._input_stream = sd.InputStream(
                    device=self._route.source_device,
                    channels=self._route.channels,
                    samplerate=self._route.sample_rate,
                    dtype=np.float32,
                    callback=self._router_callback,
                )
                
                # 创建输出流
                self._output_stream = sd.OutputStream(
                    device=self._route.target_device,
                    channels=self._route.channels,
                    samplerate=self._route.sample_rate,
                    dtype=np.float32,
                )
                
                self._output_stream.start()
                self._input_stream.start()
                self._running = True
                self._state = RoutingState.ACTIVE
                
                logger.info("音频路由已启动")
                return True
                
            except Exception as e:
                logger.error(f"启动音频路由失败: {e}")
                self._state = RoutingState.ERROR
                return False
    
    def stop(self) -> None:
        """停止音频路由"""
        with self._lock:
            self._running = False
            
            if self._input_stream:
                self._input_stream.stop()
                self._input_stream.close()
                self._input_stream = None
            
            if self._output_stream:
                self._output_stream.stop()
                self._output_stream.close()
                self._output_stream = None
            
            self._state = RoutingState.INACTIVE
            logger.info("音频路由已停止")
    
    @property
    def state(self) -> RoutingState:
        return self._state
    
    @property
    def is_active(self) -> bool:
        return self._state == RoutingState.ACTIVE


class AudioOutputManager:
    """
    音频输出管理器
    管理TTS输出的音频播放
    """
    
    def __init__(self, sample_rate: int = 22050):
        self.sample_rate = sample_rate
        self._stream: Optional[sd.OutputStream] = None
        self._lock = threading.Lock()
        
        logger.info("AudioOutputManager初始化完成")
    
    def play(self, audio_data: np.ndarray, blocking: bool = False) -> None:
        """播放音频数据"""
        with self._lock:
            try:
                sd.play(audio_data, samplerate=self.sample_rate)
                if blocking:
                    sd.wait()
            except Exception as e:
                logger.error(f"音频播放失败: {e}")
    
    def play_to_device(
        self,
        audio_data: np.ndarray,
        device_id: Optional[int],
        blocking: bool = False,
    ) -> None:
        """播放音频到指定设备"""
        with self._lock:
            try:
                sd.play(audio_data, samplerate=self.sample_rate, device=device_id)
                if blocking:
                    sd.wait()
            except Exception as e:
                logger.error(f"音频播放失败: {e}")
    
    def stop(self) -> None:
        """停止播放"""
        sd.stop()
    
    def wait(self) -> None:
        """等待播放完成"""
        sd.wait()
