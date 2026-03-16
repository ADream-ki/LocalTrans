"""
ServiceContainer - 服务容器

提供服务注册、获取和生命周期管理的中心容器。
"""

import threading
from typing import Optional, Dict, Any, Type, TypeVar, Callable
from dataclasses import dataclass

from loguru import logger


T = TypeVar("T")


@dataclass
class ServiceDescriptor:
    """服务描述符"""
    service_type: Type
    instance: Optional[Any] = None
    factory: Optional[Callable[[], Any]] = None
    singleton: bool = True


class ServiceContainer:
    """
    服务容器
    
    职责：
    - 服务注册与解析
    - 单例管理
    - 依赖注入支持
    """
    
    _instance: Optional["ServiceContainer"] = None
    _lock = threading.Lock()
    
    def __new__(cls) -> "ServiceContainer":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._services: Dict[Type, ServiceDescriptor] = {}
        self._initialized = True
        
        # 注册默认服务
        self._register_default_services()
    
    @classmethod
    def get_instance(cls) -> "ServiceContainer":
        return cls()
    
    def _register_default_services(self) -> None:
        """注册默认服务"""
        # 延迟导入避免循环依赖
        from localtrans.services import (
            SessionService,
            ModelService,
            AudioDeviceService,
            AudioIOService,
            PlatformCapabilityService,
        )
        
        self.register_singleton(SessionService, SessionService.get_instance)
        self.register_singleton(ModelService, ModelService.get_instance)
        self.register_singleton(AudioDeviceService, AudioDeviceService.get_instance)
        self.register_singleton(AudioIOService, AudioIOService.get_instance)
        self.register_singleton(PlatformCapabilityService, PlatformCapabilityService.get_instance)
        
        logger.debug("默认服务已注册")
    
    # === 服务注册 ===
    
    def register_singleton(self, service_type: Type[T], factory: Callable[[], T]) -> None:
        """注册单例服务"""
        self._services[service_type] = ServiceDescriptor(
            service_type=service_type,
            factory=factory,
            singleton=True,
        )
    
    def register_transient(self, service_type: Type[T], factory: Callable[[], T]) -> None:
        """注册瞬态服务（每次获取创建新实例）"""
        self._services[service_type] = ServiceDescriptor(
            service_type=service_type,
            factory=factory,
            singleton=False,
        )
    
    def register_instance(self, service_type: Type[T], instance: T) -> None:
        """注册已有实例"""
        self._services[service_type] = ServiceDescriptor(
            service_type=service_type,
            instance=instance,
            singleton=True,
        )
    
    # === 服务获取 ===
    
    def get(self, service_type: Type[T]) -> Optional[T]:
        """获取服务实例"""
        descriptor = self._services.get(service_type)
        if descriptor is None:
            logger.warning(f"服务未注册: {service_type}")
            return None
        
        # 已有实例且是单例
        if descriptor.singleton and descriptor.instance is not None:
            return descriptor.instance
        
        # 创建实例
        if descriptor.factory:
            instance = descriptor.factory()
            if descriptor.singleton:
                descriptor.instance = instance
            return instance
        
        return descriptor.instance
    
    def get_required(self, service_type: Type[T]) -> T:
        """获取必需的服务实例，不存在则抛出异常"""
        service = self.get(service_type)
        if service is None:
            raise ValueError(f"必需的服务未注册: {service_type}")
        return service
    
    # === 便捷方法 ===
    
    @property
    def session_service(self):
        """获取 SessionService"""
        from localtrans.services import SessionService
        return self.get(SessionService)
    
    @property
    def model_service(self):
        """获取 ModelService"""
        from localtrans.services import ModelService
        return self.get(ModelService)
    
    @property
    def audio_device_service(self):
        """获取 AudioDeviceService"""
        from localtrans.services import AudioDeviceService
        return self.get(AudioDeviceService)
    
    @property
    def platform_service(self):
        """获取 PlatformCapabilityService"""
        from localtrans.services import PlatformCapabilityService
        return self.get(PlatformCapabilityService)

    @property
    def audio_io_service(self):
        """获取 AudioIOService"""
        from localtrans.services import AudioIOService
        return self.get(AudioIOService)
    
    # === 管理 ===
    
    def clear(self) -> None:
        """清除所有服务"""
        self._services.clear()
        self._register_default_services()
    
    def is_registered(self, service_type: Type) -> bool:
        """检查服务是否已注册"""
        return service_type in self._services


def get_service_container() -> ServiceContainer:
    """获取服务容器实例"""
    return ServiceContainer.get_instance()
