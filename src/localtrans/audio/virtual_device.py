"""
虚拟音频设备管理
处理虚拟声卡的配置和管理
"""

import subprocess
from pathlib import Path
from typing import Optional
from dataclasses import dataclass

from loguru import logger


@dataclass
class VirtualDeviceConfig:
    """虚拟设备配置"""
    name: str
    input_device_id: Optional[int] = None
    output_device_id: Optional[int] = None
    sample_rate: int = 48000
    channels: int = 2


class VirtualAudioDevice:
    """
    虚拟音频设备管理器
    支持VB-Cable等虚拟声卡
    """
    
    # Windows常用虚拟声卡
    VIRTUAL_DEVICES = {
        "vb-cable": {
            "input": "CABLE Output",
            "output": "CABLE Input",
        },
        "voicemeeter": {
            "input": "Voicemeeter Out B1",
            "output": "Voicemeeter AUX Input",
        },
        "virtual-audio-cable": {
            "input": "Virtual Cable Output",
            "output": "Virtual Cable Input",
        },
    }
    
    def __init__(self, device_type: str = "vb-cable"):
        self.device_type = device_type
        self._config = self._get_device_config(device_type)
        logger.info(f"初始化虚拟音频设备: {device_type}")
    
    def _get_device_config(self, device_type: str) -> VirtualDeviceConfig:
        """获取设备配置"""
        if device_type not in self.VIRTUAL_DEVICES:
            raise ValueError(f"不支持的虚拟设备类型: {device_type}")
        
        device_info = self.VIRTUAL_DEVICES[device_type]
        return VirtualDeviceConfig(
            name=device_type,
            input_device_id=self._find_device(device_info["input"]),
            output_device_id=self._find_device(device_info["output"]),
        )
    
    def _find_device(self, name_pattern: str) -> Optional[int]:
        """查找设备ID"""
        import sounddevice as sd
        
        devices = sd.query_devices()
        for i, dev in enumerate(devices):
            if name_pattern.lower() in dev['name'].lower():
                return i
        return None
    
    @property
    def is_available(self) -> bool:
        """检查虚拟设备是否可用"""
        return self._config.input_device_id is not None
    
    @property
    def input_device_id(self) -> Optional[int]:
        """获取输入设备ID（用于捕获）"""
        return self._config.input_device_id
    
    @property
    def output_device_id(self) -> Optional[int]:
        """获取输出设备ID（用于播放）"""
        return self._config.output_device_id
    
    @staticmethod
    def check_vb_cable_installed() -> bool:
        """检查VB-Cable是否已安装"""
        try:
            import sounddevice as sd
            devices = sd.query_devices()
            for dev in devices:
                if "cable" in dev['name'].lower():
                    return True
            return False
        except Exception:
            return False
    
    @staticmethod
    def get_installation_guide() -> str:
        """获取安装指南"""
        return """
VB-Cable虚拟声卡安装指南:
1. 访问 https://vb-audio.com/Cable/
2. 下载VB-Cable驱动安装包
3. 以管理员权限运行安装程序
4. 重启系统后即可使用

配置会议软件:
1. 打开会议软件音频设置
2. 将扬声器/输出设备设置为 "CABLE Input (VB-Audio Virtual Cable)"
3. LocalTrans将从 "CABLE Output" 捕获音频
"""
    
    def configure_meeting_app(self, app_name: str) -> dict:
        """返回会议软件配置指南"""
        guides = {
            "zoom": {
                "output": "设置 -> 音频 -> 扬声器 -> 选择 'CABLE Input'",
                "notes": "确保未启用'原始声音'功能"
            },
            "tencent_meeting": {
                "output": "设置 -> 音频 -> 扬声器 -> 选择 'CABLE Input'",
                "notes": "如使用降噪功能可能影响识别效果"
            },
            "teams": {
                "output": "设置 -> 设备 -> 扬声器 -> 选择 'CABLE Input'",
                "notes": ""
            },
            "dingtalk": {
                "output": "设置 -> 音频 -> 扬声器 -> 选择 'CABLE Input'",
                "notes": ""
            },
        }
        return guides.get(app_name.lower(), {"output": "请手动设置音频输出到虚拟设备"})
