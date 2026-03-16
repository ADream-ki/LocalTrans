"""
PlatformViewModel - 平台诊断 ViewModel

管理平台信息和诊断功能。
"""

from typing import Optional, Dict, Any

from PySide6.QtCore import QObject, Signal, Property, Slot

from loguru import logger


class PlatformViewModel(QObject):
    """
    平台诊断 ViewModel
    
    提供平台信息、设备检测和诊断功能。
    """
    
    # 信号
    diagnosticsChanged = Signal()
    errorOccurred = Signal(str)
    
    def __init__(self, parent: Optional[QObject] = None):
        super().__init__(parent)
        
        self._diagnostics: Dict[str, Any] = {}
        self._refresh_diagnostics()
    
    def _refresh_diagnostics(self):
        """刷新诊断信息"""
        import platform
        import sys
        
        # 基础环境信息
        self._diagnostics = {
            "os": f"{platform.system()} {platform.release()}",
            "pythonVersion": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
            "appVersion": "1.0.0",
            "lociStatus": "未加载",
            "lociVersion": "-",
            "lociGpuSupport": "-",
            "cpuDevice": "可用",
            "cudaDevice": "未检测",
            "metalDevice": "不可用",
            "inputDevice": "默认",
            "outputDevice": "默认",
            "virtualDevice": "未安装"
        }
        
        # 检测 Loci 状态
        try:
            from localtrans.loci.runtime import LociRuntime
            runtime = LociRuntime.get_instance()
            
            if runtime.is_available:
                self._diagnostics["lociStatus"] = "已加载"
                self._diagnostics["lociVersion"] = runtime.version or "未知"
                self._diagnostics["lociGpuSupport"] = "是" if runtime.has_gpu_support else "否"
            else:
                self._diagnostics["lociStatus"] = "未加载"
                self._diagnostics["lociVersion"] = "-"
                self._diagnostics["lociGpuSupport"] = "-"
        except Exception as e:
            logger.debug(f"Loci 运行时不可用: {e}")
            self._diagnostics["lociStatus"] = "未安装"
        
        # 检测 CUDA
        try:
            import torch
            if torch.cuda.is_available():
                self._diagnostics["cudaDevice"] = torch.cuda.get_device_name(0)
            else:
                self._diagnostics["cudaDevice"] = "不可用"
        except ImportError:
            self._diagnostics["cudaDevice"] = "未安装 PyTorch"
        except Exception as e:
            logger.debug(f"CUDA 检测失败: {e}")
        
        # 检测音频设备
        try:
            import sounddevice as sd
            
            input_devices = [d for d in sd.query_devices() if d.get("max_input_channels", 0) > 0]
            output_devices = [d for d in sd.query_devices() if d.get("max_output_channels", 0) > 0]
            
            default_input = sd.query_devices(kind="input")
            default_output = sd.query_devices(kind="output")
            
            if default_input:
                self._diagnostics["inputDevice"] = default_input.get("name", "默认")
            if default_output:
                self._diagnostics["outputDevice"] = default_output.get("name", "默认")
            
            # 检测虚拟设备
            virtual_devices = [d for d in sd.query_devices() if "virtual" in d.get("name", "").lower() or "vb-audio" in d.get("name", "").lower()]
            if virtual_devices:
                self._diagnostics["virtualDevice"] = f"已安装 ({len(virtual_devices)} 个)"
            
        except ImportError:
            logger.debug("sounddevice 未安装")
        except Exception as e:
            logger.debug(f"音频设备检测失败: {e}")
        
        self.diagnosticsChanged.emit()
    
    @Slot(result=dict)
    def getDiagnostics(self) -> Dict[str, Any]:
        """获取诊断信息"""
        return self._diagnostics
    
    @Slot()
    def refresh(self):
        """刷新诊断信息"""
        self._refresh_diagnostics()
        logger.info("诊断信息已刷新")
    
    @Property(dict, notify=diagnosticsChanged)
    def diagnostics(self) -> Dict[str, Any]:
        return self._diagnostics
