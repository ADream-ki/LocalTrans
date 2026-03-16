"""
LocalTrans 应用层

提供应用启动、生命周期管理和服务容器。
"""

from localtrans.app.bootstrap import Application, create_application
from localtrans.app.service_container import ServiceContainer, get_service_container
from localtrans.app.lifecycle import LifecycleManager

__all__ = [
    "Application",
    "create_application",
    "ServiceContainer",
    "get_service_container",
    "LifecycleManager",
]
