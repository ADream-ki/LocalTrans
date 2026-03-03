"""配置管理模块"""

from localtrans.config.settings import settings, Settings
from localtrans.config.models import (
    LanguagePair,
    AudioConfig,
    ASRConfig,
    MTConfig,
    TTSConfig,
    TermBank,
)

__all__ = [
    "settings",
    "Settings",
    "LanguagePair",
    "AudioConfig",
    "ASRConfig",
    "MTConfig",
    "TTSConfig",
    "TermBank",
]
