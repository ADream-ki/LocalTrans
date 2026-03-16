"""
LocalTrans 服务层

提供服务抽象，隔离业务逻辑与底层实现。
"""

from localtrans.services.loci import LociAdapter
from localtrans.services.session_service import SessionService, SessionState, SessionConfig
from localtrans.services.model_service import ModelService, ModelType, ModelState
from localtrans.services.audio_device_service import AudioDeviceService, AudioDevice, DeviceType
from localtrans.services.audio_io_service import AudioIOService, RuntimeAudioOptions
from localtrans.services.platform_service import (
    PlatformCapabilityService,
    Platform,
    Capability,
    DiagnosticResult,
)

__all__ = [
    # Loci
    "LociAdapter",
    # Session
    "SessionService",
    "SessionState",
    "SessionConfig",
    # Model
    "ModelService",
    "ModelType",
    "ModelState",
    # Audio Device
    "AudioDeviceService",
    "AudioDevice",
    "DeviceType",
    "AudioIOService",
    "RuntimeAudioOptions",
    # Platform
    "PlatformCapabilityService",
    "Platform",
    "Capability",
    "DiagnosticResult",
]
