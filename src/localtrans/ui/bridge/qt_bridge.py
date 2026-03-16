"""
Qt Bridge - Python 与 QML 的桥接层

注册 ViewModel 到 QML 引擎，提供全局访问点。
"""

from typing import Optional, List

from PySide6.QtCore import QObject, QUrl
from PySide6.QtQml import QQmlApplicationEngine, qmlRegisterType, QQmlComponent, QQmlContext

from loguru import logger

from localtrans.ui.viewmodels.session_vm import SessionViewModel
from localtrans.ui.viewmodels.settings_vm import SettingsViewModel
from localtrans.ui.viewmodels.model_vm import ModelViewModel
from localtrans.ui.viewmodels.audio_device_vm import AudioDeviceViewModel
from localtrans.ui.viewmodels.platform_vm import PlatformViewModel


class QtBridge(QObject):
    """
    Qt Bridge
    
    负责初始化 QML 引擎并注册 ViewModel。
    """
    
    _instance: Optional["QtBridge"] = None
    
    def __new__(cls):
        if cls._instance is None:
            cls._instance = super().__new__(cls)
            cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        super().__init__()
        
        # ViewModels
        self._session_vm: Optional[SessionViewModel] = None
        self._settings_vm: Optional[SettingsViewModel] = None
        self._model_vm: Optional[ModelViewModel] = None
        self._audio_device_vm: Optional[AudioDeviceViewModel] = None
        self._platform_vm: Optional[PlatformViewModel] = None
        
        # QML 引擎
        self._engine: Optional[QQmlApplicationEngine] = None
        
        self._initialized = True
    
    @classmethod
    def get_instance(cls) -> "QtBridge":
        return cls()
    
    def initialize(self) -> bool:
        """初始化 Bridge"""
        try:
            # 创建 ViewModels
            self._session_vm = SessionViewModel()
            self._settings_vm = SettingsViewModel()
            self._model_vm = ModelViewModel()
            self._audio_device_vm = AudioDeviceViewModel()
            self._platform_vm = PlatformViewModel()
            
            # 注册类型到 QML
            qmlRegisterType(SessionViewModel, "LocalTrans", 1, 0, "SessionViewModel")
            qmlRegisterType(SettingsViewModel, "LocalTrans", 1, 0, "SettingsViewModel")
            qmlRegisterType(ModelViewModel, "LocalTrans", 1, 0, "ModelViewModel")
            qmlRegisterType(AudioDeviceViewModel, "LocalTrans", 1, 0, "AudioDeviceViewModel")
            qmlRegisterType(PlatformViewModel, "LocalTrans", 1, 0, "PlatformViewModel")
            
            logger.info("Qt Bridge 初始化完成")
            return True
            
        except Exception as e:
            logger.exception(f"Qt Bridge 初始化失败: {e}")
            return False
    
    def create_engine(self) -> QQmlApplicationEngine:
        """创建 QML 引擎"""
        self._engine = QQmlApplicationEngine()
        
        # 设置上下文属性（全局访问）
        self._engine.rootContext().setContextProperty("sessionVM", self._session_vm)
        self._engine.rootContext().setContextProperty("settingsVM", self._settings_vm)
        self._engine.rootContext().setContextProperty("modelVM", self._model_vm)
        self._engine.rootContext().setContextProperty("audioDeviceVM", self._audio_device_vm)
        self._engine.rootContext().setContextProperty("platformVM", self._platform_vm)
        
        return self._engine
    
    def load_qml(self, qml_path: str) -> bool:
        """加载 QML 文件"""
        if not self._engine:
            self.create_engine()
        
        url = QUrl.fromLocalFile(qml_path)
        self._engine.load(url)
        
        if not self._engine.rootObjects():
            logger.error(f"加载 QML 失败: {qml_path}")
            return False
        
        logger.info(f"QML 加载成功: {qml_path}")
        return True
    
    @property
    def session_vm(self) -> SessionViewModel:
        return self._session_vm
    
    @property
    def settings_vm(self) -> SettingsViewModel:
        return self._settings_vm
    
    @property
    def model_vm(self) -> ModelViewModel:
        return self._model_vm
    
    @property
    def audio_device_vm(self) -> AudioDeviceViewModel:
        return self._audio_device_vm
    
    @property
    def platform_vm(self) -> PlatformViewModel:
        return self._platform_vm
    
    @property
    def engine(self) -> Optional[QQmlApplicationEngine]:
        return self._engine
    
    def cleanup(self):
        """清理资源"""
        if self._session_vm:
            self._session_vm.stopSession()
        
        if self._engine:
            self._engine.deleteLater()
            self._engine = None
        
        logger.info("Qt Bridge 已清理")
