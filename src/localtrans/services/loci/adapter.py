"""
Loci 服务适配层

提供高级别的 LLM 推理服务接口，包括：
- 引擎池管理
- 翻译增强
- 上下文管理
- 超时控制与降级
"""

import threading
import time
from concurrent.futures import ThreadPoolExecutor, Future
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional, List, Dict, Callable, Any
from queue import Queue, Empty

from loguru import logger

from localtrans.loci.runtime import LociRuntime
from localtrans.loci.types import (
    LociDeviceType,
    LociDeviceInfo,
    GenerationParams,
    GenerationResult,
    DeviceRecommendation,
    LociError,
    LociLoadError,
)


@dataclass
class LociConfig:
    """Loci 配置"""
    model_path: Optional[str] = None
    n_ctx: int = 4096
    n_gpu_layers: int = -1  # -1 表示全部
    device_id: int = -1  # -1 表示自动选择
    max_tokens: int = 512
    temperature: float = 0.7
    timeout_ms: int = 5000  # 默认 5 秒超时
    prewarm: bool = True
    enable_cache: bool = True


@dataclass
class TranslationContext:
    """翻译上下文"""
    source_lang: str
    target_lang: str
    previous_sentences: List[str] = field(default_factory=list)
    terminology: Dict[str, str] = field(default_factory=dict)
    style: str = "neutral"  # neutral, formal, casual
    domain: str = "general"  # general, technical, medical, legal


class LociAdapter:
    """
    Loci 适配器

    封装 LociRuntime，提供：
    - 单例引擎管理
    - 翻译增强能力
    - 超时控制
    - 上下文支持
    """

    _instance: Optional["LociAdapter"] = None
    _lock = threading.Lock()

    def __new__(cls, config: Optional[LociConfig] = None) -> "LociAdapter":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance

    def __init__(self, config: Optional[LociConfig] = None):
        if self._initialized:
            return

        self._config = config or LociConfig()
        self._runtime: Optional[LociRuntime] = None
        self._engine: Optional[Any] = None  # ctypes.c_void_p
        self._engine_lock = threading.Lock()
        self._executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="loci")
        self._is_loaded = False

        if self._config.prewarm and self._config.model_path:
            self._initialize()

    @classmethod
    def get_instance(cls, config: Optional[LociConfig] = None) -> "LociAdapter":
        """获取单例实例"""
        return cls(config)

    def _initialize(self):
        """初始化运行时和引擎"""
        try:
            self._runtime = LociRuntime.get_instance()
            logger.info(f"Loci Adapter 初始化: version={self._runtime.version}")
        except LociError as e:
            logger.warning(f"Loci Runtime 初始化失败: {e}")
            self._runtime = None
            return

        if self._config.model_path:
            self.load_model(self._config.model_path)

    @property
    def is_loaded(self) -> bool:
        """模型是否已加载"""
        return self._is_loaded and self._engine is not None

    @property
    def runtime(self) -> Optional[LociRuntime]:
        """获取运行时"""
        return self._runtime

    @property
    def version(self) -> str:
        """获取版本"""
        if self._runtime:
            return self._runtime.version
        return "unavailable"

    def load_model(self, model_path: str) -> bool:
        """
        加载模型

        Args:
            model_path: 模型文件路径 (.gguf)

        Returns:
            是否成功
        """
        if not self._runtime:
            logger.error("Loci Runtime 未初始化")
            return False

        path = Path(model_path)
        if not path.exists():
            logger.error(f"模型文件不存在: {model_path}")
            return False

        with self._engine_lock:
            try:
                # 先释放旧引擎
                if self._engine:
                    self._runtime.engine_free(self._engine)
                    self._engine = None
                    self._is_loaded = False

                # 创建新引擎
                if self._config.device_id >= 0:
                    self._engine = self._runtime.engine_new_with_device(
                        str(path),
                        self._config.n_ctx,
                        self._config.device_id,
                        self._config.n_gpu_layers,
                    )
                else:
                    self._engine = self._runtime.engine_new_auto(
                        str(path),
                        self._config.n_ctx,
                    )

                self._is_loaded = True
                logger.info(f"Loci 模型加载成功: {path}")
                return True

            except LociLoadError as e:
                logger.error(f"模型加载失败: {e}")
                self._engine = None
                self._is_loaded = False
                return False

    def unload_model(self):
        """卸载模型"""
        with self._engine_lock:
            if self._engine and self._runtime:
                self._runtime.engine_free(self._engine)
                self._engine = None
                self._is_loaded = False
                logger.info("Loci 模型已卸载")

    # === 设备检测 ===

    def get_devices(self) -> List[LociDeviceInfo]:
        """获取所有可用设备"""
        if not self._runtime:
            return []
        return self._runtime.get_all_devices()

    def get_best_device(self) -> Optional[LociDeviceInfo]:
        """获取最佳设备"""
        devices = self.get_devices()
        # 优先返回 GPU 设备
        for device in devices:
            if device.device_type != LociDeviceType.CPU and device.available:
                return device
        # 否则返回 CPU
        for device in devices:
            if device.device_type == LociDeviceType.CPU:
                return device
        return None

    def recommend_device(self, model_size_gb: float) -> DeviceRecommendation:
        """推荐设备配置"""
        if not self._runtime:
            return DeviceRecommendation(
                device_id=0,
                n_gpu_layers=0,
                device_type=LociDeviceType.CPU,
                reason="Runtime 不可用",
            )
        return self._runtime.recommend_device_for_model(model_size_gb)

    # === 文本生成 ===

    def generate(
        self,
        prompt: str,
        params: Optional[GenerationParams] = None,
        timeout_ms: Optional[int] = None,
    ) -> GenerationResult:
        """
        同步生成文本

        Args:
            prompt: 输入提示
            params: 生成参数
            timeout_ms: 超时时间 (毫秒)

        Returns:
            GenerationResult: 生成结果
        """
        if not self.is_loaded:
            return GenerationResult(
                text="",
                prompt=prompt,
                finish_reason="error",
            )

        if params is None:
            params = GenerationParams(
                max_tokens=self._config.max_tokens,
                temperature=self._config.temperature,
            )

        timeout = timeout_ms or self._config.timeout_ms

        with self._engine_lock:
            try:
                return self._runtime.generate_with_timeout(
                    self._engine,
                    prompt,
                    params,
                    timeout,
                )
            except LociError as e:
                logger.error(f"生成失败: {e}")
                return GenerationResult(
                    text="",
                    prompt=prompt,
                    finish_reason="error",
                )

    def generate_async(
        self,
        prompt: str,
        params: Optional[GenerationParams] = None,
        timeout_ms: Optional[int] = None,
    ) -> Future:
        """
        异步生成文本

        Args:
            prompt: 输入提示
            params: 生成参数
            timeout_ms: 超时时间

        Returns:
            Future: 异步结果
        """
        return self._executor.submit(
            self.generate,
            prompt,
            params,
            timeout_ms,
        )

    # === 翻译增强 ===

    def enhance_translation(
        self,
        source_text: str,
        initial_translation: str,
        context: TranslationContext,
        deadline_ms: Optional[int] = None,
    ) -> str:
        """
        使用 LLM 增强翻译结果

        Args:
            source_text: 原文
            initial_translation: 初始翻译（来自快速 MT）
            context: 翻译上下文
            deadline_ms: 截止时间 (毫秒)

        Returns:
            增强后的翻译结果
        """
        if not self.is_loaded:
            return initial_translation

        # 构建 prompt
        prompt = self._build_translation_prompt(
            source_text,
            initial_translation,
            context,
        )

        # 设置严格的时间预算
        timeout = deadline_ms or self._config.timeout_ms

        result = self.generate(prompt, timeout_ms=timeout)

        if result.finish_reason == "timeout":
            logger.debug("Loci 增强翻译超时，使用原始结果")
            return initial_translation

        if result.finish_reason == "error" or not result.text:
            return initial_translation

        # 解析结果
        enhanced = self._parse_translation_result(result.text)
        return enhanced if enhanced else initial_translation

    def translate_with_context(
        self,
        source_text: str,
        context: TranslationContext,
        deadline_ms: Optional[int] = None,
    ) -> str:
        """
        带上下文的翻译

        Args:
            source_text: 原文
            context: 翻译上下文
            deadline_ms: 截止时间

        Returns:
            翻译结果
        """
        if not self.is_loaded:
            return source_text

        prompt = self._build_contextual_translation_prompt(source_text, context)
        timeout = deadline_ms or self._config.timeout_ms

        result = self.generate(prompt, timeout_ms=timeout)

        if result.finish_reason in ("timeout", "error") or not result.text:
            return source_text

        return self._parse_translation_result(result.text) or source_text

    # === Prompt 构建 ===

    def _build_translation_prompt(
        self,
        source_text: str,
        initial_translation: str,
        context: TranslationContext,
    ) -> str:
        """构建翻译增强 prompt"""
        lang_names = {
            "en": "English",
            "zh": "Chinese",
            "ja": "Japanese",
            "ko": "Korean",
            "fr": "French",
            "de": "German",
            "es": "Spanish",
            "ru": "Russian",
        }

        src_lang = lang_names.get(context.source_lang, context.source_lang)
        tgt_lang = lang_names.get(context.target_lang, context.target_lang)

        prompt_parts = [
            f"You are a professional translator. Translate the following {src_lang} text to {tgt_lang}.",
            f"Source: {source_text}",
        ]

        if initial_translation and initial_translation != source_text:
            prompt_parts.append(f"Initial translation (for reference): {initial_translation}")

        if context.terminology:
            terms_str = ", ".join(f"{k}→{v}" for k, v in context.terminology.items())
            prompt_parts.append(f"Terminology: {terms_str}")

        if context.style != "neutral":
            prompt_parts.append(f"Style: {context.style}")

        prompt_parts.append("Provide only the translation, no explanations.")

        return "\n".join(prompt_parts)

    def _build_contextual_translation_prompt(
        self,
        source_text: str,
        context: TranslationContext,
    ) -> str:
        """构建上下文翻译 prompt"""
        prompt_parts = []

        if context.previous_sentences:
            prompt_parts.append("Previous context:")
            for i, sent in enumerate(context.previous_sentences[-3:], 1):
                prompt_parts.append(f"  {i}. {sent}")
            prompt_parts.append("")

        prompt_parts.append(self._build_translation_prompt(source_text, "", context))

        return "\n".join(prompt_parts)

    def _parse_translation_result(self, text: str) -> Optional[str]:
        """解析翻译结果"""
        if not text:
            return None

        # 清理可能的前缀
        text = text.strip()

        # 移除可能的 "Translation:" 前缀
        prefixes = ["Translation:", "翻译:", "Result:", "结果:"]
        for prefix in prefixes:
            if text.startswith(prefix):
                text = text[len(prefix):].strip()

        # 取第一行作为结果（避免模型生成多余内容）
        lines = text.split("\n")
        result = lines[0].strip()

        return result if result else None

    # === 生命周期 ===

    def shutdown(self):
        """关闭适配器"""
        self.unload_model()
        self._executor.shutdown(wait=False)
        logger.info("Loci Adapter 已关闭")

    def __del__(self):
        self.shutdown()
