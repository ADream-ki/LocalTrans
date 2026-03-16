"""
Loci 本地 LLM 推理引擎集成模块

提供对 Loci 动态库的 Python 绑定，支持：
- 文本生成与翻译增强
- 设备检测与自动选择
- 流式输出
"""

from localtrans.loci.types import (
    LociDeviceType,
    LociDeviceInfo,
    GenerationParams,
    LociError,
)
from localtrans.loci.runtime import LociRuntime

__all__ = [
    "LociDeviceType",
    "LociDeviceInfo",
    "GenerationParams",
    "LociError",
    "LociRuntime",
]
