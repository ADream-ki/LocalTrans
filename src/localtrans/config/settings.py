"""
全局配置管理
使用pydantic-settings实现配置的层级管理和环境变量覆盖
"""

from pathlib import Path
from typing import Optional
from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict

from localtrans.config.models import AudioConfig, ASRConfig, MTConfig, TTSConfig


class Settings(BaseSettings):
    """全局配置类"""
    
    model_config = SettingsConfigDict(
        env_prefix="LOCALTRANS_",
        env_nested_delimiter="__",
        case_sensitive=False,
        extra="ignore",
    )
    
    # 应用基础配置
    app_name: str = "LocalTrans"
    version: str = "1.0.0"
    debug: bool = False
    
    # 数据目录
    data_dir: Path = Field(default=Path.home() / ".localtrans")
    models_dir: Path = Field(default=Path.home() / ".localtrans" / "models")
    logs_dir: Path = Field(default=Path.home() / ".localtrans" / "logs")
    cache_dir: Path = Field(default=Path.home() / ".localtrans" / "cache")
    
    # 模块配置
    audio: AudioConfig = Field(default_factory=AudioConfig)
    asr: ASRConfig = Field(default_factory=ASRConfig)
    mt: MTConfig = Field(default_factory=MTConfig)
    tts: TTSConfig = Field(default_factory=TTSConfig)
    
    # 性能配置
    max_workers: int = 4
    buffer_size: int = 4096
    
    # 延迟目标（毫秒）
    target_latency_ms: int = 500
    
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self._ensure_directories()
    
    def _ensure_directories(self) -> None:
        """确保必要的目录存在"""
        for dir_path in [self.data_dir, self.models_dir, self.logs_dir, self.cache_dir]:
            dir_path.mkdir(parents=True, exist_ok=True)
    
    @property
    def term_bank_path(self) -> Path:
        """术语库路径"""
        return self.data_dir / "term_bank.json"


# 全局配置实例
settings = Settings()
