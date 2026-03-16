"""
PlatformCapabilityService - 平台能力检测服务

负责检测系统平台的能力和资源状态。
"""

import platform
import sys
import threading
from typing import Optional, Dict, Any, List
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum
from pathlib import Path

from loguru import logger

from localtrans.config import settings


class Platform(Enum):
    """平台类型"""
    WINDOWS = "windows"
    MACOS = "macos"
    LINUX = "linux"
    UNKNOWN = "unknown"


class Capability(Enum):
    """能力类型"""
    CUDA = "cuda"
    METAL = "metal"
    VULKAN = "vulkan"
    ROCM = "rocm"
    OPENCL = "opencl"
    VIRTUAL_AUDIO = "virtual_audio"
    WASAPI = "wasapi"
    COREAUDIO = "coreaudio"
    PULSEAUDIO = "pulseaudio"


@dataclass
class DiagnosticResult:
    """诊断结果"""
    name: str
    status: str  # ok, warning, error, unknown
    message: str
    details: Dict[str, Any] = field(default_factory=dict)


class PlatformCapabilityService:
    """
    平台能力检测服务
    
    职责：
    - 检测操作系统和硬件
    - 检测 GPU 支持
    - 检测音频能力
    - 系统资源监控
    - 环境诊断
    """
    
    _instance: Optional["PlatformCapabilityService"] = None
    _lock = threading.Lock()
    
    def __new__(cls) -> "PlatformCapabilityService":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._capabilities: Dict[Capability, bool] = {}
        self._diagnostics: List[DiagnosticResult] = []
        
        # 初始化检测
        self._detect_platform()
        self._detect_capabilities()
        
        self._initialized = True
    
    @classmethod
    def get_instance(cls) -> "PlatformCapabilityService":
        return cls()
    
    # === 平台信息 ===
    
    @property
    def platform(self) -> Platform:
        """获取当前平台"""
        if sys.platform == "win32":
            return Platform.WINDOWS
        elif sys.platform == "darwin":
            return Platform.MACOS
        elif sys.platform.startswith("linux"):
            return Platform.LINUX
        return Platform.UNKNOWN
    
    @property
    def is_windows(self) -> bool:
        return self.platform == Platform.WINDOWS
    
    @property
    def is_macos(self) -> bool:
        return self.platform == Platform.MACOS
    
    @property
    def is_linux(self) -> bool:
        return self.platform == Platform.LINUX
    
    @property
    def platform_info(self) -> Dict[str, str]:
        """获取平台详细信息"""
        return {
            "system": platform.system(),
            "node": platform.node(),
            "release": platform.release(),
            "version": platform.version(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "python": platform.python_version(),
        }
    
    # === 能力检测 ===
    
    def has_capability(self, capability: Capability) -> bool:
        """检查是否支持某能力"""
        return self._capabilities.get(capability, False)
    
    def get_capabilities(self) -> Dict[Capability, bool]:
        """获取所有能力"""
        return self._capabilities.copy()
    
    def _detect_platform(self) -> None:
        """检测平台信息"""
        logger.info(f"检测到平台: {self.platform.value}")
        logger.info(f"系统信息: {self.platform_info}")
    
    def _detect_capabilities(self) -> None:
        """检测系统能力"""
        # GPU 能力
        self._detect_cuda()
        self._detect_metal()
        self._detect_vulkan()
        self._detect_rocm()
        self._detect_opencl()
        
        # 音频能力
        self._detect_audio_capabilities()
        
        # 记录
        for cap, available in self._capabilities.items():
            status = "✓" if available else "✗"
            logger.debug(f"能力 {cap.value}: {status}")
    
    def _detect_cuda(self) -> None:
        """检测 CUDA 支持"""
        try:
            import torch
            available = torch.cuda.is_available()
            if available:
                device_count = torch.cuda.device_count()
                device_name = torch.cuda.get_device_name(0) if device_count > 0 else "Unknown"
                self._capabilities[Capability.CUDA] = True
                logger.info(f"CUDA 可用: {device_name} ({device_count} 设备)")
            else:
                self._capabilities[Capability.CUDA] = False
        except ImportError:
            self._capabilities[Capability.CUDA] = False
        except Exception as e:
            logger.debug(f"CUDA 检测失败: {e}")
            self._capabilities[Capability.CUDA] = False
    
    def _detect_metal(self) -> None:
        """检测 Metal 支持"""
        try:
            import torch
            if hasattr(torch.backends, "mps"):
                available = torch.backends.mps.is_available()
                self._capabilities[Capability.METAL] = available
                if available:
                    logger.info("Metal (MPS) 可用")
            else:
                self._capabilities[Capability.METAL] = False
        except ImportError:
            self._capabilities[Capability.METAL] = False
        except Exception:
            self._capabilities[Capability.METAL] = False
    
    def _detect_vulkan(self) -> None:
        """检测 Vulkan 支持"""
        # 简单检测
        self._capabilities[Capability.VULKAN] = False
        
        if self.is_windows:
            vulkan_dll = Path("C:/Windows/System32/vulkan-1.dll")
            self._capabilities[Capability.VULKAN] = vulkan_dll.exists()
        elif self.is_linux:
            vulkan_so = Path("/usr/lib/x86_64-linux-gnu/libvulkan.so.1")
            self._capabilities[Capability.VULKAN] = vulkan_so.exists()
    
    def _detect_rocm(self) -> None:
        """检测 ROCm 支持"""
        self._capabilities[Capability.ROCM] = False
        
        if self.is_linux:
            rocm_path = Path("/opt/rocm")
            self._capabilities[Capability.ROCM] = rocm_path.exists()
    
    def _detect_opencl(self) -> None:
        """检测 OpenCL 支持"""
        self._capabilities[Capability.OPENCL] = False
        
        if self.is_windows:
            opencl_dll = Path("C:/Windows/System32/OpenCL.dll")
            self._capabilities[Capability.OPENCL] = opencl_dll.exists()
        elif self.is_linux:
            opencl_so = Path("/usr/lib/x86_64-linux-gnu/libOpenCL.so.1")
            self._capabilities[Capability.OPENCL] = opencl_so.exists()
    
    def _detect_audio_capabilities(self) -> None:
        """检测音频能力"""
        # Windows WASAPI
        if self.is_windows:
            self._capabilities[Capability.WASAPI] = True
            self._capabilities[Capability.VIRTUAL_AUDIO] = True
        
        # macOS CoreAudio
        if self.is_macos:
            self._capabilities[Capability.COREAUDIO] = True
            self._capabilities[Capability.VIRTUAL_AUDIO] = False  # 需要额外配置
        
        # Linux PulseAudio
        if self.is_linux:
            self._capabilities[Capability.PULSEAUDIO] = True
            self._capabilities[Capability.VIRTUAL_AUDIO] = False
    
    # === 诊断 ===
    
    def run_diagnostics(self) -> List[DiagnosticResult]:
        """运行完整诊断"""
        self._diagnostics.clear()
        
        # 系统诊断
        self._diagnose_system()
        
        # Python 环境
        self._diagnose_python()
        
        # GPU 诊断
        self._diagnose_gpu()
        
        # 模型诊断
        self._diagnose_models()
        
        # 音频诊断
        self._diagnose_audio()
        
        # Loci 诊断
        self._diagnose_loci()
        
        return self._diagnostics
    
    def _diagnose_system(self) -> None:
        """系统诊断"""
        info = self.platform_info
        
        # 检查内存
        try:
            import psutil
            mem = psutil.virtual_memory()
            mem_status = "ok" if mem.available > 4 * 1024**3 else "warning"
            mem_msg = f"可用: {mem.available / 1024**3:.1f} GB / 总计: {mem.total / 1024**3:.1f} GB"
        except ImportError:
            mem_status = "unknown"
            mem_msg = "无法检测（缺少 psutil）"
        
        self._diagnostics.append(DiagnosticResult(
            name="操作系统",
            status="ok",
            message=f"{info['system']} {info['release']} ({info['machine']})",
        ))
        
        self._diagnostics.append(DiagnosticResult(
            name="内存",
            status=mem_status,
            message=mem_msg,
        ))
    
    def _diagnose_python(self) -> None:
        """Python 环境诊断"""
        py_version = platform.python_version()
        py_path = sys.executable
        
        status = "ok" if sys.version_info >= (3, 10) else "error"
        
        self._diagnostics.append(DiagnosticResult(
            name="Python 版本",
            status=status,
            message=f"{py_version} ({py_path})",
        ))
        
        # 检查关键依赖
        dependencies = [
            ("numpy", "NumPy"),
            ("sounddevice", "SoundDevice"),
            ("pydantic", "Pydantic"),
            ("loguru", "Loguru"),
        ]
        
        for module, name in dependencies:
            try:
                __import__(module)
                self._diagnostics.append(DiagnosticResult(
                    name=name,
                    status="ok",
                    message="已安装",
                ))
            except ImportError:
                self._diagnostics.append(DiagnosticResult(
                    name=name,
                    status="warning",
                    message="未安装",
                ))
    
    def _diagnose_gpu(self) -> None:
        """GPU 诊断"""
        if self.has_capability(Capability.CUDA):
            try:
                import torch
                device_name = torch.cuda.get_device_name(0)
                device_memory = torch.cuda.get_device_properties(0).total_memory / 1024**3
                
                self._diagnostics.append(DiagnosticResult(
                    name="GPU (CUDA)",
                    status="ok",
                    message=f"{device_name} ({device_memory:.1f} GB)",
                ))
            except Exception as e:
                self._diagnostics.append(DiagnosticResult(
                    name="GPU (CUDA)",
                    status="warning",
                    message=f"检测失败: {e}",
                ))
        else:
            self._diagnostics.append(DiagnosticResult(
                name="GPU",
                status="warning",
                message="未检测到 CUDA GPU，将使用 CPU",
            ))
    
    def _diagnose_models(self) -> None:
        """模型诊断"""
        models_dir = settings.models_dir
        
        if not models_dir.exists():
            self._diagnostics.append(DiagnosticResult(
                name="模型目录",
                status="warning",
                message=f"目录不存在: {models_dir}",
            ))
            return
        
        # 统计模型
        asr_models = list((models_dir / "asr").glob("*")) if (models_dir / "asr").exists() else []
        mt_models = list((models_dir / "mt").glob("*")) if (models_dir / "mt").exists() else []
        
        self._diagnostics.append(DiagnosticResult(
            name="ASR 模型",
            status="ok" if asr_models else "warning",
            message=f"已安装 {len(asr_models)} 个模型",
        ))
        
        self._diagnostics.append(DiagnosticResult(
            name="MT 模型",
            status="ok" if mt_models else "warning",
            message=f"已安装 {len(mt_models)} 个模型",
        ))
    
    def _diagnose_audio(self) -> None:
        """音频诊断"""
        try:
            import sounddevice as sd
            
            input_count = sum(1 for d in sd.query_devices() if d["max_input_channels"] > 0)
            output_count = sum(1 for d in sd.query_devices() if d["max_output_channels"] > 0)
            
            self._diagnostics.append(DiagnosticResult(
                name="音频输入设备",
                status="ok" if input_count > 0 else "error",
                message=f"检测到 {input_count} 个输入设备",
            ))
            
            self._diagnostics.append(DiagnosticResult(
                name="音频输出设备",
                status="ok" if output_count > 0 else "warning",
                message=f"检测到 {output_count} 个输出设备",
            ))
            
        except Exception as e:
            self._diagnostics.append(DiagnosticResult(
                name="音频系统",
                status="error",
                message=f"检测失败: {e}",
            ))
    
    def _diagnose_loci(self) -> None:
        """Loci 诊断"""
        from localtrans.loci import LociRuntime
        
        try:
            runtime = LociRuntime()
            
            if runtime.is_available:
                version = runtime.version
                gpu_support = runtime.has_gpu_support
                
                self._diagnostics.append(DiagnosticResult(
                    name="Loci 引擎",
                    status="ok",
                    message=f"版本 {version}, GPU支持: {'是' if gpu_support else '否'}",
                ))
            else:
                self._diagnostics.append(DiagnosticResult(
                    name="Loci 引擎",
                    status="warning",
                    message="动态库未找到或加载失败",
                ))
        except Exception as e:
            self._diagnostics.append(DiagnosticResult(
                name="Loci 引擎",
                status="warning",
                message=f"检测失败: {e}",
            ))
    
    # === 资源监控 ===
    
    def get_resource_usage(self) -> Dict[str, Any]:
        """获取资源使用情况"""
        result = {
            "cpu_percent": 0.0,
            "memory_percent": 0.0,
            "memory_available_gb": 0.0,
        }
        
        try:
            import psutil
            
            result["cpu_percent"] = psutil.cpu_percent(interval=0.1)
            mem = psutil.virtual_memory()
            result["memory_percent"] = mem.percent
            result["memory_available_gb"] = mem.available / 1024**3
            
        except ImportError:
            pass
        
        return result
    
    # === 推荐配置 ===
    
    def get_recommended_config(self) -> Dict[str, Any]:
        """获取推荐配置"""
        config = {
            "device": "cpu",
            "asr_backend": "vosk",
            "mt_backend": "argos-ct2",
            "tts_engine": "pyttsx3",
            "loci_enabled": False,
        }
        
        # GPU 推荐
        if self.has_capability(Capability.CUDA):
            config["device"] = "cuda"
            config["asr_backend"] = "faster-whisper"
        elif self.has_capability(Capability.METAL):
            config["device"] = "mps"
            config["asr_backend"] = "faster-whisper"
        
        # Loci 推荐
        try:
            from localtrans.loci import LociRuntime
            runtime = LociRuntime()
            if runtime.is_available:
                config["loci_enabled"] = True
        except Exception:
            pass
        
        return config
