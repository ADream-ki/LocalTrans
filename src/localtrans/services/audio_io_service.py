"""
AudioIOService - 音频 I/O 策略服务

提供生产级音频 I/O 策略归一化、参数校验和运行时配置构建。
"""

from __future__ import annotations

import threading
from dataclasses import dataclass, asdict
from typing import Optional, Dict, Any

from loguru import logger

from localtrans.config import settings
from localtrans.services.audio_device_service import AudioDeviceService


@dataclass
class RuntimeAudioOptions:
    """运行时音频参数（已完成策略归一化）。"""

    input_device_id: Optional[int]
    input_device_name: Optional[str]
    output_mode: str
    output_device_id: Optional[int]
    io_buffer_ms: int
    input_gain_db: float
    output_gain_db: float
    monitoring_enabled: bool
    io_profile: str

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


class AudioIOService:
    """音频 I/O 服务：配置持久化 + 运行时参数构建。"""

    _instance: Optional["AudioIOService"] = None
    _lock = threading.Lock()

    def __new__(cls) -> "AudioIOService":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance

    def __init__(self):
        if getattr(self, "_initialized", False):
            return
        self._devices = AudioDeviceService.get_instance()
        self._initialized = True

    @classmethod
    def get_instance(cls) -> "AudioIOService":
        return cls()

    def get_control_state(self) -> Dict[str, Any]:
        return {
            "output_mode": self._normalize_output_mode(getattr(settings.audio, "output_mode", "virtual")),
            "io_profile": self._normalize_profile(getattr(settings.audio, "io_profile", "balanced")),
            "io_buffer_ms": self._clamp_int(getattr(settings.audio, "io_buffer_ms", 60), 20, 300),
            "input_gain_db": self._clamp_float(getattr(settings.audio, "input_gain_db", 0.0), -24.0, 24.0),
            "output_gain_db": self._clamp_float(getattr(settings.audio, "output_gain_db", 0.0), -24.0, 24.0),
            "monitoring_enabled": bool(getattr(settings.audio, "monitoring_enabled", False)),
        }

    def update_control_state(self, **kwargs: Any) -> Dict[str, Any]:
        if "output_mode" in kwargs:
            settings.audio.output_mode = self._normalize_output_mode(kwargs.get("output_mode"))
        if "io_profile" in kwargs:
            settings.audio.io_profile = self._normalize_profile(kwargs.get("io_profile"))
        if "io_buffer_ms" in kwargs:
            settings.audio.io_buffer_ms = self._clamp_int(kwargs.get("io_buffer_ms"), 20, 300)
        if "input_gain_db" in kwargs:
            settings.audio.input_gain_db = self._clamp_float(kwargs.get("input_gain_db"), -24.0, 24.0)
        if "output_gain_db" in kwargs:
            settings.audio.output_gain_db = self._clamp_float(kwargs.get("output_gain_db"), -24.0, 24.0)
        if "monitoring_enabled" in kwargs:
            settings.audio.monitoring_enabled = bool(kwargs.get("monitoring_enabled"))

        settings.save()
        state = self.get_control_state()
        logger.info(f"音频 I/O 控制已更新: {state}")
        return state

    def build_runtime_options(self) -> RuntimeAudioOptions:
        self._devices.refresh()

        profile = self._normalize_profile(getattr(settings.audio, "io_profile", "balanced"))
        output_mode = self._normalize_output_mode(getattr(settings.audio, "output_mode", "virtual"))

        input_id = self._resolve_input_device_id()
        output_id = self._resolve_output_device_id()

        base_buffer = self._clamp_int(getattr(settings.audio, "io_buffer_ms", 60), 20, 300)
        input_gain = self._clamp_float(getattr(settings.audio, "input_gain_db", 0.0), -24.0, 24.0)
        output_gain = self._clamp_float(getattr(settings.audio, "output_gain_db", 0.0), -24.0, 24.0)
        monitoring_enabled = bool(getattr(settings.audio, "monitoring_enabled", False))

        # 档位策略：把用户值限制在安全范围内，但保留深度可控空间。
        if profile == "realtime":
            io_buffer_ms = self._clamp_int(base_buffer, 20, 80)
        elif profile == "studio":
            io_buffer_ms = self._clamp_int(base_buffer, 80, 300)
        else:
            io_buffer_ms = self._clamp_int(base_buffer, 40, 160)

        # 若指定设备模式但设备不可用，回退到系统默认，避免启动失败。
        if output_mode == "device" and output_id is None:
            output_mode = "system"

        options = RuntimeAudioOptions(
            input_device_id=input_id,
            input_device_name=getattr(settings.audio, "input_device_name", None),
            output_mode=output_mode,
            output_device_id=output_id,
            io_buffer_ms=io_buffer_ms,
            input_gain_db=input_gain,
            output_gain_db=output_gain,
            monitoring_enabled=monitoring_enabled,
            io_profile=profile,
        )
        return options

    def _resolve_input_device_id(self) -> Optional[int]:
        configured = int(getattr(settings.audio, "input_device_id", -1) or -1)
        if configured < 0:
            selected = self._devices.selected_input
            return int(selected.id) if selected else None
        if any(dev.id == configured for dev in self._devices.input_devices):
            return configured
        return None

    def _resolve_output_device_id(self) -> Optional[int]:
        configured = int(getattr(settings.audio, "output_device_id", -1) or -1)
        if configured < 0:
            selected = self._devices.selected_output
            return int(selected.id) if selected else None
        if any(dev.id == configured for dev in self._devices.output_devices):
            return configured
        return None

    @staticmethod
    def _normalize_profile(value: Any) -> str:
        v = str(value or "balanced").strip().lower()
        return v if v in {"realtime", "balanced", "studio"} else "balanced"

    @staticmethod
    def _normalize_output_mode(value: Any) -> str:
        v = str(value or "virtual").strip().lower()
        return v if v in {"virtual", "device", "system"} else "virtual"

    @staticmethod
    def _clamp_int(value: Any, low: int, high: int) -> int:
        try:
            iv = int(value)
        except Exception:
            iv = low
        return max(low, min(high, iv))

    @staticmethod
    def _clamp_float(value: Any, low: float, high: float) -> float:
        try:
            fv = float(value)
        except Exception:
            fv = low
        return max(low, min(high, fv))
