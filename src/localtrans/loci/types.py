"""
Loci 类型定义

定义与 C API 对应的数据结构、枚举和异常类型。
"""

from dataclasses import dataclass, field
from enum import IntEnum
from typing import Optional, List, Callable, Any


class LociDeviceType(IntEnum):
    """设备类型枚举"""
    CPU = 0
    CUDA = 1
    METAL = 2
    VULKAN = 3
    ROCM = 4
    OPENCL = 5

    @classmethod
    def from_int(cls, value: int) -> "LociDeviceType":
        """从整数值创建枚举"""
        try:
            return cls(value)
        except ValueError:
            return cls.CPU

    def __str__(self) -> str:
        names = {
            LociDeviceType.CPU: "CPU",
            LociDeviceType.CUDA: "CUDA (NVIDIA GPU)",
            LociDeviceType.METAL: "Metal (Apple GPU)",
            LociDeviceType.VULKAN: "Vulkan",
            LociDeviceType.ROCM: "ROCm (AMD GPU)",
            LociDeviceType.OPENCL: "OpenCL",
        }
        return names.get(self, f"Unknown({self.value})")


@dataclass
class LociDeviceInfo:
    """设备信息"""
    device_id: int = 0
    name: str = ""
    memory_bytes: int = 0
    device_type: LociDeviceType = LociDeviceType.CPU
    compute_capability: float = 0.0
    available: bool = False

    @property
    def memory_gb(self) -> float:
        """显存大小 (GB)"""
        return self.memory_bytes / (1024 ** 3)

    def __str__(self) -> str:
        return (
            f"LociDeviceInfo(id={self.device_id}, name='{self.name}', "
            f"type={self.device_type}, memory={self.memory_gb:.1f}GB, "
            f"available={self.available})"
        )


@dataclass
class GenerationParams:
    """生成参数"""
    max_tokens: int = 512
    temperature: float = 0.8
    top_p: float = 0.95
    min_p: float = 0.0
    top_k: int = 40
    repeat_penalty: float = 1.1


@dataclass
class GenerationResult:
    """生成结果"""
    text: str
    prompt: str
    tokens_generated: int = 0
    finish_reason: str = "stop"  # stop, length, error


@dataclass
class DeviceRecommendation:
    """设备推荐结果"""
    device_id: int
    n_gpu_layers: int
    device_type: LociDeviceType
    reason: str = ""


class LociError(Exception):
    """Loci 错误基类"""

    def __init__(self, message: str, code: Optional[int] = None):
        super().__init__(message)
        self.message = message
        self.code = code

    def __str__(self) -> str:
        if self.code is not None:
            return f"LociError[{self.code}]: {self.message}"
        return f"LociError: {self.message}"


class LociLoadError(LociError):
    """模型加载错误"""
    pass


class LociInferenceError(LociError):
    """推理错误"""
    pass


class LociDeviceError(LociError):
    """设备错误"""
    pass


class LociPluginError(LociError):
    """插件错误"""
    pass


# 回调函数类型
StreamCallback = Callable[[str], bool]
"""流式输出回调，返回 True 继续，返回 False 停止"""
