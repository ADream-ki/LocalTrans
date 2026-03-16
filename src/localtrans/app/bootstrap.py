"""
Bootstrap - 应用启动引导

负责应用初始化和启动流程。
"""

import sys
from typing import Optional, List, Callable
from pathlib import Path

from loguru import logger

from localtrans.app.service_container import ServiceContainer, get_service_container
from localtrans.app.lifecycle import LifecycleManager, LifecycleState


class Application:
    """
    应用程序
    
    职责：
    - 初始化应用环境
    - 配置日志
    - 启动服务容器
    - 提供应用级配置
    """
    
    def __init__(self, name: str = "LocalTrans", debug: bool = False):
        self.name = name
        self.debug = debug
        self._container = get_service_container()
        self._lifecycle = LifecycleManager.get_instance()
        self._initialized = False
    
    def initialize(self) -> bool:
        """初始化应用"""
        if self._initialized:
            return True
        
        logger.info(f"初始化应用: {self.name}")
        
        # 配置日志
        self._configure_logging()
        
        # 确保目录存在
        self._ensure_directories()
        
        # 注册生命周期钩子
        self._register_lifecycle_hooks()
        
        self._initialized = True
        logger.info("应用初始化完成")
        return True
    
    def _configure_logging(self) -> None:
        """配置日志"""
        from localtrans.config import settings
        
        # 移除默认处理器
        logger.remove()
        
        # 控制台输出
        log_level = "DEBUG" if self.debug else "INFO"
        logger.add(
            sys.stderr,
            level=log_level,
            format="<green>{time:HH:mm:ss}</green> | <level>{level: <8}</level> | <cyan>{name}</cyan>:<cyan>{function}</cyan>:<cyan>{line}</cyan> - <level>{message}</level>",
        )
        
        # 文件输出
        log_file = settings.logs_dir / "localtrans_{time:YYYY-MM-DD}.log"
        logger.add(
            str(log_file),
            level="DEBUG",
            rotation="00:00",
            retention="7 days",
            compression="zip",
            encoding="utf-8",
        )
    
    def _ensure_directories(self) -> None:
        """确保目录存在"""
        from localtrans.config import settings
        
        settings.data_dir.mkdir(parents=True, exist_ok=True)
        settings.models_dir.mkdir(parents=True, exist_ok=True)
        settings.logs_dir.mkdir(parents=True, exist_ok=True)
        settings.cache_dir.mkdir(parents=True, exist_ok=True)
    
    def _register_lifecycle_hooks(self) -> None:
        """注册生命周期钩子"""
        # 启动钩子
        self._lifecycle.on_startup(
            "init_services",
            self._init_services,
            priority=10,
        )
        
        # 关闭钩子
        self._lifecycle.on_shutdown(
            "stop_session",
            self._stop_session,
            priority=100,
        )
    
    def _init_services(self) -> None:
        """初始化服务"""
        # 预热服务
        from localtrans.services import (
            SessionService,
            ModelService,
            AudioDeviceService,
            AudioIOService,
            PlatformCapabilityService,
        )
        
        # 获取服务实例（触发单例创建）
        self._container.get(SessionService)
        self._container.get(ModelService)
        self._container.get(AudioDeviceService)
        self._container.get(AudioIOService)
        self._container.get(PlatformCapabilityService)
    
    def _stop_session(self) -> None:
        """停止会话"""
        from localtrans.services import SessionService
        
        session = self._container.get(SessionService)
        if session and session.is_running:
            session.stop()
    
    # === 生命周期代理 ===
    
    def start(self) -> bool:
        """启动应用"""
        if not self._initialized:
            self.initialize()
        return self._lifecycle.start()
    
    def shutdown(self) -> None:
        """关闭应用"""
        self._lifecycle.shutdown()
    
    @property
    def is_running(self) -> bool:
        return self._lifecycle.is_running
    
    # === 服务访问 ===
    
    @property
    def container(self) -> ServiceContainer:
        return self._container
    
    @property
    def session(self):
        return self._container.session_service
    
    @property
    def models(self):
        return self._container.model_service
    
    @property
    def audio_devices(self):
        return self._container.audio_device_service
    
    @property
    def platform(self):
        return self._container.platform_service
    
    # === 上下文管理器 ===
    
    def __enter__(self):
        self.start()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.shutdown()
        return False


def create_application(
    name: str = "LocalTrans",
    debug: bool = False,
    auto_start: bool = True,
) -> Application:
    """
    创建应用实例
    
    Args:
        name: 应用名称
        debug: 是否启用调试模式
        auto_start: 是否自动启动
    
    Returns:
        Application 实例
    """
    app = Application(name=name, debug=debug)
    
    if auto_start:
        app.start()
    
    return app
