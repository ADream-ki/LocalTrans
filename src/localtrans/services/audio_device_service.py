"""
AudioDeviceService - 音频设备管理服务

负责管理音频输入/输出设备的枚举、选择和路由。
"""

import threading
from typing import Optional, List, Dict, Any, Callable
from dataclasses import dataclass
from enum import Enum

from loguru import logger
from localtrans.config import settings


class DeviceType(Enum):
    """设备类型"""
    INPUT = "input"
    OUTPUT = "output"
    VIRTUAL = "virtual"


@dataclass
class AudioDevice:
    """音频设备信息"""
    id: int
    name: str
    type: DeviceType
    sample_rate: int = 16000
    channels: int = 1
    is_default: bool = False
    is_virtual: bool = False
    
    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "type": self.type.value,
            "sample_rate": self.sample_rate,
            "channels": self.channels,
            "is_default": self.is_default,
            "is_virtual": self.is_virtual,
        }


class AudioDeviceService:
    """
    音频设备管理服务
    
    职责：
    - 枚举音频设备
    - 设备选择与配置
    - 虚拟设备管理
    - 音频路由配置
    """
    
    _instance: Optional["AudioDeviceService"] = None
    _lock = threading.Lock()
    
    def __new__(cls) -> "AudioDeviceService":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._input_devices: List[AudioDevice] = []
        self._output_devices: List[AudioDevice] = []
        self._virtual_devices: List[AudioDevice] = []
        
        self._selected_input: Optional[int] = None
        self._selected_output: Optional[int] = None
        self._selected_virtual: Optional[int] = None
        
        self._on_device_change: Optional[Callable[[], None]] = None
        
        # 初始化设备列表
        self._refresh_devices()
        self._load_persisted_selection()
        
        self._initialized = True
    
    @classmethod
    def get_instance(cls) -> "AudioDeviceService":
        return cls()
    
    # === 设备枚举 ===
    
    @property
    def input_devices(self) -> List[AudioDevice]:
        return self._input_devices
    
    @property
    def output_devices(self) -> List[AudioDevice]:
        return self._output_devices
    
    @property
    def virtual_devices(self) -> List[AudioDevice]:
        return self._virtual_devices
    
    def get_all_devices(self) -> Dict[str, List[AudioDevice]]:
        """获取所有设备"""
        return {
            "input": self._input_devices,
            "output": self._output_devices,
            "virtual": self._virtual_devices,
        }
    
    def refresh(self) -> None:
        """刷新设备列表"""
        self._refresh_devices()
        self._reconcile_selected_devices()
        if self._on_device_change:
            self._on_device_change()
    
    def _refresh_devices(self) -> None:
        """刷新设备列表"""
        try:
            import sounddevice as sd
            
            devices = sd.query_devices()
            
            self._input_devices.clear()
            self._output_devices.clear()
            
            default_input = sd.default.device[0]
            default_output = sd.default.device[1]
            
            for idx, dev in enumerate(devices):
                if dev["max_input_channels"] > 0:
                    device = AudioDevice(
                        id=idx,
                        name=dev["name"],
                        type=DeviceType.INPUT,
                        sample_rate=int(dev["default_samplerate"]),
                        channels=dev["max_input_channels"],
                        is_default=(idx == default_input),
                    )
                    self._input_devices.append(device)
                
                if dev["max_output_channels"] > 0:
                    device = AudioDevice(
                        id=idx,
                        name=dev["name"],
                        type=DeviceType.OUTPUT,
                        sample_rate=int(dev["default_samplerate"]),
                        channels=dev["max_output_channels"],
                        is_default=(idx == default_output),
                    )
                    self._output_devices.append(device)
            
            # 获取虚拟设备
            self._refresh_virtual_devices()
            
            logger.debug(
                f"设备刷新完成: {len(self._input_devices)} 输入, "
                f"{len(self._output_devices)} 输出, "
                f"{len(self._virtual_devices)} 虚拟"
            )
            
        except Exception as e:
            logger.exception(f"刷新设备失败: {e}")
    
    def _refresh_virtual_devices(self) -> None:
        """刷新虚拟设备列表"""
        self._virtual_devices.clear()
        
        try:
            import sounddevice as sd
            keywords = ("vb-audio", "virtual", "cable", "voicemeeter", "stereo mix", "立体声混音")
            devices = sd.query_devices()
            for idx, dev in enumerate(devices):
                name = str(dev.get("name", ""))
                lower = name.lower()
                if any(k in lower for k in keywords):
                    self._virtual_devices.append(
                        AudioDevice(
                            id=idx,
                            name=name,
                            type=DeviceType.VIRTUAL,
                            sample_rate=int(dev.get("default_samplerate", 48000)),
                            channels=max(
                                int(dev.get("max_input_channels", 0)),
                                int(dev.get("max_output_channels", 0)),
                            ),
                            is_virtual=True,
                        )
                    )
            
        except Exception as e:
            logger.debug(f"获取虚拟设备失败: {e}")
    
    # === 设备选择 ===
    
    @property
    def selected_input(self) -> Optional[AudioDevice]:
        if self._selected_input is None:
            return self._get_default_input()
        return self._find_device(self._selected_input, self._input_devices)
    
    @property
    def selected_output(self) -> Optional[AudioDevice]:
        if self._selected_output is None:
            return self._get_default_output()
        return self._find_device(self._selected_output, self._output_devices)
    
    @property
    def selected_virtual(self) -> Optional[AudioDevice]:
        if self._selected_virtual is None:
            return self._virtual_devices[0] if self._virtual_devices else None
        return self._find_device(self._selected_virtual, self._virtual_devices)
    
    def select_input(self, device_id: int) -> bool:
        """选择输入设备"""
        device = self._find_device(device_id, self._input_devices)
        if device:
            self._selected_input = device_id
            settings.audio.input_device_id = int(device_id)
            settings.audio.input_device_name = device.name
            settings.save()
            logger.info(f"已选择输入设备: {device.name}")
            return True
        return False
    
    def select_output(self, device_id: int) -> bool:
        """选择输出设备"""
        device = self._find_device(device_id, self._output_devices)
        if device:
            self._selected_output = device_id
            settings.audio.output_device_id = int(device_id)
            settings.audio.output_device_name = device.name
            settings.save()
            logger.info(f"已选择输出设备: {device.name}")
            return True
        return False
    
    def select_virtual(self, device_id: int) -> bool:
        """选择虚拟设备"""
        device = self._find_device(device_id, self._virtual_devices)
        if device:
            self._selected_virtual = device_id
            logger.info(f"已选择虚拟设备: {device.name}")
            return True
        return False

    def _load_persisted_selection(self) -> None:
        """从配置加载已保存的输入/输出设备。"""
        in_id = int(getattr(settings.audio, "input_device_id", -1))
        out_id = int(getattr(settings.audio, "output_device_id", -1))
        if in_id >= 0:
            self._selected_input = in_id
        if out_id >= 0:
            self._selected_output = out_id
        self._reconcile_selected_devices()

    def _reconcile_selected_devices(self) -> None:
        """设备列表变化后修正失效选择。"""
        if self._selected_input is not None and not self._find_device(self._selected_input, self._input_devices):
            self._selected_input = None
            settings.audio.input_device_id = -1
            settings.audio.input_device_name = None
            settings.save()
        if self._selected_output is not None and not self._find_device(self._selected_output, self._output_devices):
            self._selected_output = None
            settings.audio.output_device_id = -1
            settings.audio.output_device_name = None
            settings.save()
    
    def _find_device(self, device_id: int, devices: List[AudioDevice]) -> Optional[AudioDevice]:
        for dev in devices:
            if dev.id == device_id:
                return dev
        return None
    
    def _get_default_input(self) -> Optional[AudioDevice]:
        for dev in self._input_devices:
            if dev.is_default:
                return dev
        return self._input_devices[0] if self._input_devices else None
    
    def _get_default_output(self) -> Optional[AudioDevice]:
        for dev in self._output_devices:
            if dev.is_default:
                return dev
        return self._output_devices[0] if self._output_devices else None
    
    # === 虚拟设备管理 ===
    
    def has_virtual_device(self) -> bool:
        """检查是否有虚拟设备"""
        return len(self._virtual_devices) > 0
    
    def create_virtual_device(self, name: str = "LocalTrans") -> bool:
        """创建虚拟设备（Windows）"""
        try:
            from localtrans.audio import VirtualAudioDevice
            
            virtual = VirtualAudioDevice()
            success = virtual.install_virtual_cable()
            
            if success:
                self.refresh()
                logger.info(f"虚拟设备创建成功: {name}")
            return success
            
        except Exception as e:
            logger.exception(f"创建虚拟设备失败: {e}")
            return False
    
    # === 回调 ===
    
    def on_device_change(self, callback: Callable[[], None]) -> None:
        """设置设备变更回调"""
        self._on_device_change = callback
    
    # === 配置导出 ===
    
    def get_config(self) -> Dict[str, Any]:
        """获取当前配置"""
        return {
            "input_device": self.selected_input.to_dict() if self.selected_input else None,
            "output_device": self.selected_output.to_dict() if self.selected_output else None,
            "virtual_device": self.selected_virtual.to_dict() if self.selected_virtual else None,
        }
