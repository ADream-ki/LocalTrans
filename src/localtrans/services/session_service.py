"""
SessionService - 会话管理服务

负责管理翻译会话的生命周期、状态和协调各组件。
"""

import threading
from typing import Optional, Callable, List, Dict, Any
from dataclasses import dataclass, field
from enum import Enum
from datetime import datetime

from loguru import logger


class SessionState(Enum):
    """会话状态"""
    IDLE = "idle"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    ERROR = "error"


@dataclass
class SessionConfig:
    """会话配置"""
    source_lang: str = "en"
    target_lang: str = "zh"
    enable_tts: bool = True
    enable_asr: bool = True
    bidirectional: bool = False
    
    # ASR 配置
    asr_backend: str = "faster-whisper"
    asr_model: str = "small"
    
    # MT 配置
    mt_backend: str = "argos-ct2"
    
    # TTS 配置
    tts_engine: str = "pyttsx3"
    
    # 延迟配置
    asr_buffer_duration: float = 0.6
    translation_timeout_ms: int = 800


@dataclass
class SessionEvent:
    """会话事件"""
    type: str  # transcription, translation, error, state_change
    timestamp: datetime = field(default_factory=datetime.now)
    data: Dict[str, Any] = field(default_factory=dict)


class SessionService:
    """
    会话管理服务
    
    职责：
    - 管理会话生命周期（创建、启动、停止、销毁）
    - 协调 ASR/MT/TTS 组件
    - 维护会话状态和历史
    - 提供回调接口
    """
    
    _instance: Optional["SessionService"] = None
    _lock = threading.Lock()
    
    def __new__(cls) -> "SessionService":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._state = SessionState.IDLE
        self._config = SessionConfig()
        self._pipeline = None
        self._history: List[SessionEvent] = []
        self._max_history = 500
        
        # 回调
        self._on_transcription: Optional[Callable[[str, bool], None]] = None
        self._on_translation: Optional[Callable[[str, str, bool], None]] = None
        self._on_error: Optional[Callable[[str], None]] = None
        self._on_state_change: Optional[Callable[[SessionState], None]] = None
        
        # 统计
        self._start_time: Optional[datetime] = None
        self._transcription_count = 0
        self._translation_count = 0
        
        self._initialized = True
    
    @classmethod
    def get_instance(cls) -> "SessionService":
        return cls()
    
    # === 属性 ===
    
    @property
    def state(self) -> SessionState:
        return self._state
    
    @property
    def config(self) -> SessionConfig:
        return self._config
    
    @property
    def is_running(self) -> bool:
        return self._state == SessionState.RUNNING
    
    @property
    def is_idle(self) -> bool:
        return self._state == SessionState.IDLE
    
    @property
    def duration_seconds(self) -> float:
        if self._start_time is None:
            return 0.0
        return (datetime.now() - self._start_time).total_seconds()
    
    @property
    def statistics(self) -> Dict[str, Any]:
        return {
            "state": self._state.value,
            "duration_seconds": self.duration_seconds,
            "transcription_count": self._transcription_count,
            "translation_count": self._translation_count,
            "history_size": len(self._history),
        }
    
    # === 回调设置 ===
    
    def on_transcription(self, callback: Callable[[str, bool], None]):
        """设置转录回调"""
        self._on_transcription = callback
    
    def on_translation(self, callback: Callable[[str, str, bool], None]):
        """设置翻译回调"""
        self._on_translation = callback
    
    def on_error(self, callback: Callable[[str], None]):
        """设置错误回调"""
        self._on_error = callback
    
    def on_state_change(self, callback: Callable[[SessionState], None]):
        """设置状态变更回调"""
        self._on_state_change = callback
    
    # === 会话控制 ===
    
    def configure(self, config: SessionConfig) -> None:
        """配置会话"""
        if self.is_running:
            raise RuntimeError("无法在运行时修改配置")
        self._config = config
        logger.info(f"会话配置已更新: {config}")
    
    def start(self) -> bool:
        """启动会话"""
        if not self.is_idle:
            logger.warning(f"无法启动：当前状态为 {self._state}")
            return False
        
        self._set_state(SessionState.STARTING)
        
        try:
            from localtrans.pipeline.realtime import RealtimePipeline, RealtimeConfig
            from localtrans.config import settings
            
            # 创建管道配置
            pipeline_config = RealtimeConfig(
                source_lang=self._config.source_lang,
                target_lang=self._config.target_lang,
                enable_tts=self._config.enable_tts,
            )
            
            # 创建管道
            self._pipeline = RealtimePipeline(
                asr_config=settings.asr,
                mt_config=settings.mt,
                tts_config=settings.tts,
                config=pipeline_config,
            )
            
            # 设置回调
            self._pipeline.on_transcription = self._handle_transcription
            self._pipeline.on_translation = self._handle_translation
            self._pipeline.on_error = self._handle_error
            
            # 启动
            self._pipeline.start()
            
            self._start_time = datetime.now()
            self._set_state(SessionState.RUNNING)
            
            self._add_event("state_change", {"state": "started"})
            logger.info("会话已启动")
            return True
            
        except Exception as e:
            logger.exception(f"启动会话失败: {e}")
            self._set_state(SessionState.ERROR)
            self._handle_error(str(e))
            return False
    
    def stop(self) -> None:
        """停止会话"""
        if not self.is_running:
            return
        
        self._set_state(SessionState.STOPPING)
        
        try:
            if self._pipeline:
                self._pipeline.stop()
                self._pipeline = None
            
            self._set_state(SessionState.IDLE)
            self._add_event("state_change", {"state": "stopped"})
            logger.info(f"会话已停止，持续 {self.duration_seconds:.1f}秒")
            
        except Exception as e:
            logger.exception(f"停止会话失败: {e}")
            self._set_state(SessionState.ERROR)
        finally:
            self._start_time = None
    
    def toggle(self) -> bool:
        """切换会话状态"""
        if self.is_running:
            self.stop()
            return False
        else:
            return self.start()
    
    # === 历史管理 ===
    
    def get_history(self, limit: int = 50) -> List[SessionEvent]:
        """获取历史事件"""
        return self._history[-limit:]
    
    def clear_history(self) -> None:
        """清空历史"""
        self._history.clear()
        self._transcription_count = 0
        self._translation_count = 0
    
    def export_history(self, format: str = "json") -> str:
        """导出历史"""
        import json
        
        events = [
            {
                "type": e.type,
                "timestamp": e.timestamp.isoformat(),
                "data": e.data,
            }
            for e in self._history
        ]
        
        return json.dumps(events, ensure_ascii=False, indent=2)
    
    # === 内部方法 ===
    
    def _set_state(self, state: SessionState) -> None:
        if self._state != state:
            self._state = state
            if self._on_state_change:
                self._on_state_change(state)
    
    def _add_event(self, event_type: str, data: Dict[str, Any]) -> None:
        event = SessionEvent(type=event_type, data=data)
        self._history.append(event)
        
        if len(self._history) > self._max_history:
            self._history = self._history[-self._max_history:]
    
    def _handle_transcription(self, text: str, is_final: bool) -> None:
        self._transcription_count += 1
        self._add_event("transcription", {"text": text, "is_final": is_final})
        if self._on_transcription:
            self._on_transcription(text, is_final)
    
    def _handle_translation(self, source: str, translated: str, is_enhanced: bool = False) -> None:
        self._translation_count += 1
        self._add_event("translation", {
            "source": source,
            "translated": translated,
            "is_enhanced": is_enhanced,
        })
        if self._on_translation:
            self._on_translation(source, translated, is_enhanced)
    
    def _handle_error(self, error: str) -> None:
        self._add_event("error", {"message": error})
        if self._on_error:
            self._on_error(error)
