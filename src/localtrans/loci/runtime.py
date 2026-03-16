"""
Loci Runtime - ctypes 绑定层

封装 Loci 动态库的 C API，提供 Python 友好的接口。
"""

import ctypes
import os
import platform
from pathlib import Path
from typing import Optional, List, Callable, Any, Tuple

from loguru import logger

from localtrans.loci.types import (
    LociDeviceType,
    LociDeviceInfo,
    GenerationParams,
    GenerationResult,
    DeviceRecommendation,
    LociError,
    LociLoadError,
    LociInferenceError,
    LociDeviceError,
)


class _LociDeviceInfoC(ctypes.Structure):
    """C API 设备信息结构体"""
    _fields_ = [
        ("device_id", ctypes.c_int32),
        ("name", ctypes.c_char * 256),
        ("memory_bytes", ctypes.c_uint64),
        ("device_type", ctypes.c_int32),
        ("compute_capability", ctypes.c_float),
        ("available", ctypes.c_bool),
    ]

    def to_python(self) -> LociDeviceInfo:
        """转换为 Python 数据类"""
        return LociDeviceInfo(
            device_id=self.device_id,
            name=self.name.decode("utf-8", errors="replace"),
            memory_bytes=self.memory_bytes,
            device_type=LociDeviceType.from_int(self.device_type),
            compute_capability=self.compute_capability,
            available=self.available,
        )


# 流式回调函数类型
_StreamCallbackC = ctypes.CFUNCTYPE(
    ctypes.c_bool,  # 返回值：True 继续，False 停止
    ctypes.c_char_p,  # token
    ctypes.c_void_p,  # user_data
)


class LociRuntime:
    """
    Loci 运行时封装

    提供 Loci 动态库的 Python 绑定，包括：
    - 引擎创建与销毁
    - 文本生成（同步/流式）
    - 设备检测与选择
    - 插件管理
    """

    _instance: Optional["LociRuntime"] = None
    _lib: Optional[ctypes.CDLL] = None
    _initialized: bool = False

    def __new__(cls, lib_path: Optional[str] = None) -> "LociRuntime":
        if cls._instance is None:
            cls._instance = super().__new__(cls)
        return cls._instance

    def __init__(self, lib_path: Optional[str] = None):
        if self._initialized:
            return

        self._lib = self._load_library(lib_path)
        self._setup_function_signatures()
        self._initialized = True
        logger.info(f"Loci Runtime 初始化完成: version={self.version}")

    @classmethod
    def get_instance(cls, lib_path: Optional[str] = None) -> "LociRuntime":
        """获取单例实例"""
        return cls(lib_path)

    def _find_library_path(self) -> Path:
        """查找动态库路径"""
        system = platform.system().lower()
        machine = platform.machine().lower()

        # 相对于本模块的位置查找
        module_dir = Path(__file__).parent.parent / "native" / "loci"

        if system == "windows":
            lib_name = "loci.dll"
        elif system == "darwin":
            lib_name = "libloci.dylib" if machine == "arm64" else "libloci.dylib"
        else:  # Linux
            lib_name = "libloci.so"

        # 优先查找 native/loci 目录
        lib_path = module_dir / lib_name
        if lib_path.exists():
            return lib_path

        # 查找系统路径
        return Path(lib_name)

    def _load_library(self, lib_path: Optional[str] = None) -> ctypes.CDLL:
        """加载动态库"""
        if lib_path:
            path = Path(lib_path)
        else:
            path = self._find_library_path()

        if not path.exists():
            raise LociLoadError(f"Loci 动态库不存在: {path}")

        try:
            # Windows 需要设置 DLL 搜索路径
            if platform.system() == "Windows":
                os.add_dll_directory(str(path.parent))

            lib = ctypes.CDLL(str(path))
            logger.debug(f"加载 Loci 动态库: {path}")
            return lib
        except OSError as e:
            raise LociLoadError(f"加载 Loci 动态库失败: {e}") from e

    def _setup_function_signatures(self):
        """设置 C API 函数签名"""
        lib = self._lib

        # === 版本与错误 ===
        lib.loci_version.argtypes = []
        lib.loci_version.restype = ctypes.c_char_p

        lib.loci_has_gpu_support.argtypes = []
        lib.loci_has_gpu_support.restype = ctypes.c_bool

        lib.loci_get_last_error.argtypes = []
        lib.loci_get_last_error.restype = ctypes.c_char_p

        # === 引擎生命周期 ===
        lib.loci_engine_new.argtypes = [
            ctypes.c_char_p,  # model_path
            ctypes.c_uint32,  # n_ctx
            ctypes.c_int32,   # n_gpu_layers
        ]
        lib.loci_engine_new.restype = ctypes.c_void_p

        lib.loci_engine_new_auto.argtypes = [
            ctypes.c_char_p,  # model_path
            ctypes.c_uint32,  # n_ctx
        ]
        lib.loci_engine_new_auto.restype = ctypes.c_void_p

        lib.loci_engine_new_with_device.argtypes = [
            ctypes.c_char_p,  # model_path
            ctypes.c_uint32,  # n_ctx
            ctypes.c_int32,   # device_id
            ctypes.c_int32,   # n_gpu_layers
        ]
        lib.loci_engine_new_with_device.restype = ctypes.c_void_p

        lib.loci_engine_free.argtypes = [ctypes.c_void_p]
        lib.loci_engine_free.restype = None

        lib.loci_engine_free_safe.argtypes = [ctypes.POINTER(ctypes.c_void_p)]
        lib.loci_engine_free_safe.restype = None

        # === 文本生成 ===
        lib.loci_generate.argtypes = [
            ctypes.c_void_p,   # engine
            ctypes.c_char_p,   # prompt
            ctypes.c_uint32,   # max_tokens
            ctypes.c_float,    # temperature
        ]
        lib.loci_generate.restype = ctypes.c_void_p  # char*

        lib.loci_generate_with_len.argtypes = [
            ctypes.c_void_p,   # engine
            ctypes.c_char_p,   # prompt
            ctypes.c_size_t,   # prompt_len
            ctypes.c_uint32,   # max_tokens
            ctypes.c_float,    # temperature
        ]
        lib.loci_generate_with_len.restype = ctypes.c_void_p

        lib.loci_generate_wait.argtypes = [
            ctypes.c_void_p,   # engine
            ctypes.c_char_p,   # prompt
            ctypes.c_uint32,   # max_tokens
            ctypes.c_float,    # temperature
            ctypes.c_uint32,   # wait_timeout_ms
        ]
        lib.loci_generate_wait.restype = ctypes.c_void_p

        lib.loci_generate_stream.argtypes = [
            ctypes.c_void_p,        # engine
            ctypes.c_char_p,        # prompt
            ctypes.c_uint32,        # max_tokens
            ctypes.c_float,         # temperature
            _StreamCallbackC,       # callback
            ctypes.c_void_p,        # user_data
        ]
        lib.loci_generate_stream.restype = ctypes.c_bool

        # === 内存管理 ===
        lib.loci_free_string.argtypes = [ctypes.c_void_p]
        lib.loci_free_string.restype = None

        # === 设备检测 ===
        lib.loci_device_selector_new.argtypes = []
        lib.loci_device_selector_new.restype = ctypes.c_void_p

        lib.loci_device_selector_free.argtypes = [ctypes.c_void_p]
        lib.loci_device_selector_free.restype = None

        lib.loci_get_device_count.argtypes = [ctypes.c_void_p]
        lib.loci_get_device_count.restype = ctypes.c_int32

        lib.loci_get_device_info.argtypes = [
            ctypes.c_void_p,              # selector
            ctypes.c_int32,               # index
            ctypes.POINTER(_LociDeviceInfoC),  # info
        ]
        lib.loci_get_device_info.restype = ctypes.c_bool

        lib.loci_auto_select_device.argtypes = [ctypes.c_void_p]
        lib.loci_auto_select_device.restype = ctypes.c_int32

        lib.loci_recommend_device_for_model.argtypes = [
            ctypes.c_void_p,   # selector
            ctypes.c_float,    # model_size_gb
        ]
        lib.loci_recommend_device_for_model.restype = ctypes.c_int32

        lib.loci_has_backend.argtypes = [
            ctypes.c_void_p,   # selector
            ctypes.c_int32,    # device_type (LociDeviceType)
        ]
        lib.loci_has_backend.restype = ctypes.c_bool

        # === 插件注册表 ===
        lib.loci_registry_new.argtypes = []
        lib.loci_registry_new.restype = ctypes.c_void_p

        lib.loci_registry_free.argtypes = [ctypes.c_void_p]
        lib.loci_registry_free.restype = None

        lib.loci_registry_load_plugin.argtypes = [
            ctypes.c_void_p,  # registry
            ctypes.c_char_p,  # plugin_path
        ]
        lib.loci_registry_load_plugin.restype = ctypes.c_bool

        lib.loci_registry_unload_plugin.argtypes = [
            ctypes.c_void_p,  # registry
            ctypes.c_char_p,  # plugin_name
        ]
        lib.loci_registry_unload_plugin.restype = ctypes.c_bool

        lib.loci_registry_enable_plugin.argtypes = [
            ctypes.c_void_p,  # registry
            ctypes.c_char_p,  # plugin_name
        ]
        lib.loci_registry_enable_plugin.restype = ctypes.c_bool

        lib.loci_registry_disable_plugin.argtypes = [
            ctypes.c_void_p,  # registry
            ctypes.c_char_p,  # plugin_name
        ]
        lib.loci_registry_disable_plugin.restype = ctypes.c_bool

        lib.loci_registry_list_json.argtypes = [ctypes.c_void_p]
        lib.loci_registry_list_json.restype = ctypes.c_void_p

    # === 属性 ===

    @property
    def is_available(self) -> bool:
        """检查 Loci 是否可用"""
        return self._lib is not None and self._initialized

    @property
    def version(self) -> str:
        """获取 Loci 版本"""
        result = self._lib.loci_version()
        return result.decode("utf-8") if result else "unknown"

    @property
    def has_gpu_support(self) -> bool:
        """检查是否支持 GPU"""
        return self._lib.loci_has_gpu_support()

    @property
    def lib_path(self) -> str:
        """获取动态库路径"""
        return str(self._find_library_path())

    @property
    def last_error(self) -> str:
        """获取最后的错误信息"""
        result = self._lib.loci_get_last_error()
        return result.decode("utf-8", errors="replace") if result else ""

    # === 设备检测 ===

    def create_device_selector(self) -> ctypes.c_void_p:
        """创建设备选择器"""
        return self._lib.loci_device_selector_new()

    def free_device_selector(self, selector: ctypes.c_void_p):
        """释放设备选择器"""
        if selector:
            self._lib.loci_device_selector_free(selector)

    def get_device_count(self, selector: ctypes.c_void_p) -> int:
        """获取设备数量"""
        return self._lib.loci_get_device_count(selector)

    def get_device_info(self, selector: ctypes.c_void_p, index: int) -> Optional[LociDeviceInfo]:
        """获取指定索引的设备信息"""
        info = _LociDeviceInfoC()
        if self._lib.loci_get_device_info(selector, index, ctypes.byref(info)):
            return info.to_python()
        return None

    def get_all_devices(self) -> List[LociDeviceInfo]:
        """获取所有设备信息"""
        selector = self.create_device_selector()
        try:
            count = self.get_device_count(selector)
            devices = []
            for i in range(count):
                info = self.get_device_info(selector, i)
                if info:
                    devices.append(info)
            return devices
        finally:
            self.free_device_selector(selector)

    def auto_select_device(self) -> int:
        """自动选择最佳设备，返回设备 ID"""
        selector = self.create_device_selector()
        try:
            return self._lib.loci_auto_select_device(selector)
        finally:
            self.free_device_selector(selector)

    def recommend_device_for_model(self, model_size_gb: float) -> DeviceRecommendation:
        """
        根据模型大小推荐设备配置

        Args:
            model_size_gb: 模型大小 (GB)

        Returns:
            DeviceRecommendation: 设备推荐结果
        """
        selector = self.create_device_selector()
        try:
            device_id = self._lib.loci_recommend_device_for_model(selector, ctypes.c_float(model_size_gb))
            info = self.get_device_info(selector, device_id) if device_id >= 0 else None

            if info:
                # 根据显存大小推荐 GPU 层数
                if info.device_type == LociDeviceType.CPU:
                    n_gpu_layers = 0
                else:
                    # 估算可卸载的层数 (粗略估算)
                    n_gpu_layers = -1  # 全部卸载到 GPU

                return DeviceRecommendation(
                    device_id=device_id,
                    n_gpu_layers=n_gpu_layers,
                    device_type=info.device_type,
                    reason=f"推荐使用 {info.name} ({info.memory_gb:.1f}GB)",
                )
            else:
                return DeviceRecommendation(
                    device_id=0,
                    n_gpu_layers=0,
                    device_type=LociDeviceType.CPU,
                    reason="使用 CPU",
                )
        finally:
            self.free_device_selector(selector)

    def has_backend(self, device_type: LociDeviceType) -> bool:
        """检查指定类型的后端是否可用"""
        selector = self.create_device_selector()
        try:
            return self._lib.loci_has_backend(selector, ctypes.c_int32(device_type.value))
        finally:
            self.free_device_selector(selector)

    # === 引擎管理 ===

    def engine_new(
        self,
        model_path: str,
        n_ctx: int = 2048,
        n_gpu_layers: int = -1,
    ) -> ctypes.c_void_p:
        """
        创建推理引擎

        Args:
            model_path: 模型文件路径 (.gguf)
            n_ctx: 上下文大小
            n_gpu_layers: GPU 层数 (-1 表示全部)

        Returns:
            引擎句柄
        """
        engine = self._lib.loci_engine_new(
            model_path.encode("utf-8"),
            ctypes.c_uint32(n_ctx),
            ctypes.c_int32(n_gpu_layers),
        )
        if not engine:
            raise LociLoadError(f"创建引擎失败: {self.last_error}")
        return engine

    def engine_new_auto(
        self,
        model_path: str,
        n_ctx: int = 2048,
    ) -> ctypes.c_void_p:
        """
        自动选择设备创建引擎

        Args:
            model_path: 模型文件路径
            n_ctx: 上下文大小

        Returns:
            引擎句柄
        """
        engine = self._lib.loci_engine_new_auto(
            model_path.encode("utf-8"),
            ctypes.c_uint32(n_ctx),
        )
        if not engine:
            raise LociLoadError(f"创建引擎失败: {self.last_error}")
        return engine

    def engine_new_with_device(
        self,
        model_path: str,
        n_ctx: int,
        device_id: int,
        n_gpu_layers: int,
    ) -> ctypes.c_void_p:
        """
        指定设备创建引擎

        Args:
            model_path: 模型文件路径
            n_ctx: 上下文大小
            device_id: 设备 ID
            n_gpu_layers: GPU 层数

        Returns:
            引擎句柄
        """
        engine = self._lib.loci_engine_new_with_device(
            model_path.encode("utf-8"),
            ctypes.c_uint32(n_ctx),
            ctypes.c_int32(device_id),
            ctypes.c_int32(n_gpu_layers),
        )
        if not engine:
            raise LociLoadError(f"创建引擎失败: {self.last_error}")
        return engine

    def engine_free(self, engine: ctypes.c_void_p):
        """释放引擎"""
        if engine:
            self._lib.loci_engine_free(engine)

    def engine_free_safe(self, engine_ptr: ctypes.POINTER(ctypes.c_void_p)):
        """安全释放引擎 (指针置空)"""
        if engine_ptr and engine_ptr.contents:
            self._lib.loci_engine_free_safe(engine_ptr)

    # === 文本生成 ===

    def generate(
        self,
        engine: ctypes.c_void_p,
        prompt: str,
        params: Optional[GenerationParams] = None,
    ) -> GenerationResult:
        """
        同步生成文本

        Args:
            engine: 引擎句柄
            prompt: 输入提示
            params: 生成参数

        Returns:
            GenerationResult: 生成结果
        """
        if params is None:
            params = GenerationParams()

        prompt_bytes = prompt.encode("utf-8")
        result_ptr = self._lib.loci_generate(
            engine,
            prompt_bytes,
            ctypes.c_uint32(params.max_tokens),
            ctypes.c_float(params.temperature),
        )

        if not result_ptr:
            raise LociInferenceError(f"生成失败: {self.last_error}")

        try:
            result_text = ctypes.cast(result_ptr, ctypes.c_char_p).value
            if result_text:
                text = result_text.decode("utf-8", errors="replace")
            else:
                text = ""
        finally:
            self._lib.loci_free_string(result_ptr)

        return GenerationResult(
            text=text,
            prompt=prompt,
            tokens_generated=len(text.split()),  # 粗略估算
            finish_reason="stop",
        )

    def generate_with_timeout(
        self,
        engine: ctypes.c_void_p,
        prompt: str,
        params: Optional[GenerationParams] = None,
        timeout_ms: int = 30000,
    ) -> GenerationResult:
        """
        带超时的同步生成

        Args:
            engine: 引擎句柄
            prompt: 输入提示
            params: 生成参数
            timeout_ms: 超时时间 (毫秒)

        Returns:
            GenerationResult: 生成结果
        """
        if params is None:
            params = GenerationParams()

        prompt_bytes = prompt.encode("utf-8")
        result_ptr = self._lib.loci_generate_wait(
            engine,
            prompt_bytes,
            ctypes.c_uint32(params.max_tokens),
            ctypes.c_float(params.temperature),
            ctypes.c_uint32(timeout_ms),
        )

        if not result_ptr:
            error_msg = self.last_error or "timeout"
            if "timeout" in error_msg.lower():
                return GenerationResult(
                    text="",
                    prompt=prompt,
                    tokens_generated=0,
                    finish_reason="timeout",
                )
            raise LociInferenceError(f"生成失败: {error_msg}")

        try:
            result_text = ctypes.cast(result_ptr, ctypes.c_char_p).value
            text = result_text.decode("utf-8", errors="replace") if result_text else ""
        finally:
            self._lib.loci_free_string(result_ptr)

        return GenerationResult(
            text=text,
            prompt=prompt,
            tokens_generated=len(text.split()),
            finish_reason="stop",
        )

    def generate_stream(
        self,
        engine: ctypes.c_void_p,
        prompt: str,
        callback: Callable[[str], bool],
        params: Optional[GenerationParams] = None,
    ) -> bool:
        """
        流式生成文本

        Args:
            engine: 引擎句柄
            prompt: 输入提示
            callback: 回调函数，接收每个 token，返回 True 继续，False 停止
            params: 生成参数

        Returns:
            bool: 是否成功完成
        """
        if params is None:
            params = GenerationParams()

        # 保存回调引用，防止被垃圾回收
        user_data = {"callback": callback, "stopped": False}

        @ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_char_p, ctypes.c_void_p)
        def wrapper(token_ptr: ctypes.c_char_p, user_data_ptr: ctypes.c_void_p) -> bool:
            if user_data["stopped"]:
                return False
            token = token_ptr.decode("utf-8", errors="replace") if token_ptr else ""
            should_continue = user_data["callback"](token)
            if not should_continue:
                user_data["stopped"] = True
            return should_continue

        prompt_bytes = prompt.encode("utf-8")
        success = self._lib.loci_generate_stream(
            engine,
            prompt_bytes,
            ctypes.c_uint32(params.max_tokens),
            ctypes.c_float(params.temperature),
            wrapper,
            None,  # user_data 通过闭包传递
        )

        return success

    # === 插件管理 ===

    def registry_new(self) -> ctypes.c_void_p:
        """创建插件注册表"""
        return self._lib.loci_registry_new()

    def registry_free(self, registry: ctypes.c_void_p):
        """释放插件注册表"""
        if registry:
            self._lib.loci_registry_free(registry)

    def registry_load_plugin(self, registry: ctypes.c_void_p, plugin_path: str) -> bool:
        """加载插件"""
        return self._lib.loci_registry_load_plugin(
            registry,
            plugin_path.encode("utf-8"),
        )

    def registry_list_json(self, registry: ctypes.c_void_p) -> str:
        """获取插件列表 (JSON 格式)"""
        result_ptr = self._lib.loci_registry_list_json(registry)
        if not result_ptr:
            return "[]"
        try:
            result = ctypes.cast(result_ptr, ctypes.c_char_p).value
            return result.decode("utf-8", errors="replace") if result else "[]"
        finally:
            self._lib.loci_free_string(result_ptr)
