"""
全局配置管理
使用pydantic-settings实现配置的层级管理和环境变量覆盖
"""

from pathlib import Path
from typing import Any
import tempfile
from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict
from loguru import logger

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
        self._load_persisted_config()
    
    def _ensure_directories(self) -> None:
        """确保必要的目录存在"""
        if self._try_create_directories():
            return

        for fallback_root in self._fallback_data_roots():
            self._set_data_root(fallback_root)
            if self._try_create_directories():
                return

        # 最后兜底：创建一个随机可写临时目录，避免启动失败。
        emergency_root = Path(tempfile.mkdtemp(prefix="localtrans-"))
        self._set_data_root(emergency_root)
        if self._try_create_directories():
            logger.warning(f"目录权限异常，已回退到临时目录: {emergency_root}")
            return

        # 极端情况下保持原目录并继续，避免在导入 settings 时直接崩溃。
        logger.error("目录初始化失败，将继续使用当前配置路径，后续按需创建。")

    def _try_create_directories(self) -> bool:
        """尝试创建目录，失败则返回 False。"""
        try:
            for dir_path in [self.data_dir, self.models_dir, self.logs_dir, self.cache_dir]:
                dir_path.mkdir(parents=True, exist_ok=True)
            return True
        except Exception:
            return False

    def _fallback_data_roots(self) -> list[Path]:
        """目录不可写时的回退路径列表。"""
        candidates = [
            Path.cwd() / ".localtrans",
            Path(tempfile.gettempdir()) / "localtrans",
        ]
        return [p for p in candidates if p != self.data_dir]

    def _set_data_root(self, root: Path) -> None:
        """更新所有目录到新的根路径。"""
        self.data_dir = root
        self.models_dir = root / "models"
        self.logs_dir = root / "logs"
        self.cache_dir = root / "cache"
    
    @property
    def term_bank_path(self) -> Path:
        """术语库路径"""
        return self.data_dir / "term_bank.json"
    
    def _load_persisted_config(self) -> None:
        """从 data_dir/config.json 加载持久化配置（如果存在）"""
        import json
        
        config_file = self.data_dir / "config.json"
        if not config_file.exists():
            return
        
        try:
            with open(config_file, "r", encoding="utf-8") as f:
                persisted: Any = json.load(f)
            
            if not isinstance(persisted, dict):
                return
            
            merged = {**self.model_dump(mode="python"), **persisted}
            validated = self.__class__.model_validate(merged)
            for field_name in self.__class__.model_fields:
                setattr(self, field_name, getattr(validated, field_name))
            
            self._ensure_directories()
        except Exception:
            # 配置文件损坏时保持当前配置可用，避免启动失败
            return
    
    def save(self) -> None:
        """保存配置到文件"""
        import json
        config_file = self.data_dir / "config.json"
        with open(config_file, "w", encoding="utf-8") as f:
            json.dump(self.model_dump(mode="json"), f, ensure_ascii=False, indent=2, default=str)
    
    def reset(self) -> None:
        """重置为默认配置"""
        self.audio = AudioConfig()
        self.asr = ASRConfig()
        self.mt = MTConfig()
        self.tts = TTSConfig()


# 全局配置实例
settings = Settings()
