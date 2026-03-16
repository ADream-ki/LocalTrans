"""
LifecycleManager - 生命周期管理器

负责应用启动、关闭和资源清理。
"""

import atexit
import threading
from typing import Optional, List, Callable
from dataclasses import dataclass
from enum import Enum

from loguru import logger


class LifecycleState(Enum):
    """生命周期状态"""
    CREATED = "created"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    STOPPED = "stopped"


@dataclass
class LifecycleHook:
    """生命周期钩子"""
    name: str
    callback: Callable[[], None]
    priority: int = 0  # 越小越先执行


class LifecycleManager:
    """
    生命周期管理器
    
    职责：
    - 管理应用生命周期状态
    - 注册启动/关闭钩子
    - 资源清理
    """
    
    _instance: Optional["LifecycleManager"] = None
    _lock = threading.Lock()
    
    def __new__(cls) -> "LifecycleManager":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._state = LifecycleState.CREATED
        self._startup_hooks: List[LifecycleHook] = []
        self._shutdown_hooks: List[LifecycleHook] = []
        self._atexit_registered = False
        
        self._initialized = True
    
    @classmethod
    def get_instance(cls) -> "LifecycleManager":
        return cls()
    
    @property
    def state(self) -> LifecycleState:
        return self._state
    
    @property
    def is_running(self) -> bool:
        return self._state == LifecycleState.RUNNING
    
    # === 钩子注册 ===
    
    def on_startup(self, name: str, callback: Callable[[], None], priority: int = 0) -> None:
        """注册启动钩子"""
        hook = LifecycleHook(name=name, callback=callback, priority=priority)
        self._startup_hooks.append(hook)
        self._startup_hooks.sort(key=lambda h: h.priority)
        logger.debug(f"注册启动钩子: {name} (priority={priority})")
    
    def on_shutdown(self, name: str, callback: Callable[[], None], priority: int = 0) -> None:
        """注册关闭钩子"""
        hook = LifecycleHook(name=name, callback=callback, priority=priority)
        self._shutdown_hooks.append(hook)
        self._shutdown_hooks.sort(key=lambda h: h.priority)
        logger.debug(f"注册关闭钩子: {name} (priority={priority})")
    
    # === 生命周期控制 ===
    
    def start(self) -> bool:
        """启动应用"""
        if self._state != LifecycleState.CREATED:
            logger.warning(f"无法启动：当前状态为 {self._state}")
            return False
        
        self._state = LifecycleState.STARTING
        logger.info("应用启动中...")
        
        # 执行启动钩子
        for hook in self._startup_hooks:
            try:
                logger.debug(f"执行启动钩子: {hook.name}")
                hook.callback()
            except Exception as e:
                logger.exception(f"启动钩子执行失败: {hook.name} - {e}")
                self._state = LifecycleState.STOPPED
                return False
        
        self._state = LifecycleState.RUNNING
        logger.info("应用已启动")
        
        # 注册退出钩子
        if not self._atexit_registered:
            atexit.register(self.shutdown)
            self._atexit_registered = True
        
        return True
    
    def shutdown(self) -> None:
        """关闭应用"""
        if self._state == LifecycleState.STOPPED:
            return
        
        self._state = LifecycleState.STOPPING
        logger.info("应用关闭中...")
        
        # 执行关闭钩子（按优先级逆序）
        for hook in reversed(self._shutdown_hooks):
            try:
                logger.debug(f"执行关闭钩子: {hook.name}")
                hook.callback()
            except Exception as e:
                logger.exception(f"关闭钩子执行失败: {hook.name} - {e}")
        
        self._state = LifecycleState.STOPPED
        logger.info("应用已关闭")
    
    # === 上下文管理器 ===
    
    def __enter__(self):
        self.start()
        return self
    
    def __exit__(self, exc_type, exc_val, exc_tb):
        self.shutdown()
        return False
