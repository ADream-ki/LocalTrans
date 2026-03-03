"""
MT翻译引擎
支持NLLB、MarianMT等本地翻译模型
"""

import time
from abc import ABC, abstractmethod
from pathlib import Path
from typing import Optional, List, Dict, Tuple
from dataclasses import dataclass

from loguru import logger

from localtrans.config import settings, MTConfig
from localtrans.mt.term_bank import TermBankManager


@dataclass
class TranslationResult:
    """翻译结果"""
    source_text: str
    translated_text: str
    source_lang: str
    target_lang: str
    confidence: float = 1.0
    alternatives: Optional[List[str]] = None


class MTBackend(ABC):
    """翻译后端抽象基类"""
    
    @abstractmethod
    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        """翻译文本"""
        pass
    
    @abstractmethod
    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        """批量翻译"""
        pass


class FallbackMTBackend(MTBackend):
    """依赖缺失时的回退翻译后端"""

    def __init__(self, reason: str = ""):
        self.reason = reason
        logger.warning(f"使用Fallback MT后端: {reason}")

    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        # 回退策略：保持原文，确保流程不断
        return TranslationResult(
            source_text=text,
            translated_text=text,
            source_lang=source_lang,
            target_lang=target_lang,
            confidence=0.0,
        )

    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        return [self.translate(t, source_lang, target_lang) for t in texts]


class NLLBBackend(MTBackend):
    """
    NLLB (No Language Left Behind) 后端
    Meta的通用翻译模型，支持200+语言
    """
    
    # NLLB语言代码映射
    LANG_CODE_MAP = {
        "en": "eng_Latn",
        "zh": "zho_Hans",
        "zh-cn": "zho_Hans",
        "zh-tw": "zho_Hant",
        "ja": "jpn_Jpan",
        "ko": "kor_Hang",
        "fr": "fra_Latn",
        "de": "deu_Latn",
        "es": "spa_Latn",
        "ru": "rus_Cyrl",
        "ar": "arb_Arab",
        "pt": "por_Latn",
        "it": "ita_Latn",
    }
    
    def __init__(self, config: MTConfig):
        self.config = config
        self._tokenizer = None
        self._model = None
        self._load_model()
    
    def _load_model(self):
        """加载模型"""
        try:
            from transformers import AutoTokenizer, AutoModelForSeq2SeqLM
            import torch
            
            device = self.config.device
            if device == "auto":
                device = "cuda" if torch.cuda.is_available() else "cpu"
            
            model_name = self.config.model_path or self.config.model_name
            
            logger.info(f"加载NLLB模型: {model_name}, device={device}")
            
            self._tokenizer = AutoTokenizer.from_pretrained(model_name)
            self._model = AutoModelForSeq2SeqLM.from_pretrained(model_name)
            self._model.to(device)
            self._model.eval()
            self._torch = torch  # 保存torch引用
            
            logger.info("NLLB模型加载完成")
            
        except ImportError:
            raise ImportError("请安装transformers: pip install transformers torch")
    
    def _get_lang_code(self, lang: str) -> str:
        """获取NLLB语言代码"""
        return self.LANG_CODE_MAP.get(lang.lower(), lang)
    
    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        """翻译文本"""
        import torch
        
        start_time = time.time()
        
        src_code = self._get_lang_code(source_lang)
        tgt_code = self._get_lang_code(target_lang)
        
        # 设置语言标记
        self._tokenizer.src_lang = src_code
        forced_bos_token_id = self._tokenizer.convert_tokens_to_ids(tgt_code)
        
        # 编码
        inputs = self._tokenizer(text, return_tensors="pt", padding=True, truncation=True, max_length=self.config.max_length)
        inputs = {k: v.to(self._model.device) for k, v in inputs.items()}
        
        # 生成
        outputs = self._model.generate(
            **inputs,
            forced_bos_token_id=forced_bos_token_id,
            max_length=self.config.max_length,
            num_beams=1,  # 实时翻译使用贪心解码
            do_sample=False,
        )
        
        # 解码
        translated = self._tokenizer.batch_decode(outputs, skip_special_tokens=True)[0]
        
        logger.debug(f"翻译完成: {len(text)}->{len(translated)}字符, 耗时{time.time()-start_time:.2f}s")
        
        return TranslationResult(
            source_text=text,
            translated_text=translated,
            source_lang=source_lang,
            target_lang=target_lang,
        )
    
    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        """批量翻译"""
        import torch
        
        src_code = self._get_lang_code(source_lang)
        tgt_code = self._get_lang_code(target_lang)
        
        self._tokenizer.src_lang = src_code
        forced_bos_token_id = self._tokenizer.convert_tokens_to_ids(tgt_code)
        
        inputs = self._tokenizer(texts, return_tensors="pt", padding=True, truncation=True, max_length=self.config.max_length)
        inputs = {k: v.to(self._model.device) for k, v in inputs.items()}
        
        outputs = self._model.generate(
            **inputs,
            forced_bos_token_id=forced_bos_token_id,
            max_length=self.config.max_length,
            num_beams=1,
        )
        
        translations = self._tokenizer.batch_decode(outputs, skip_special_tokens=True)
        
        return [
            TranslationResult(
                source_text=src,
                translated_text=tgt,
                source_lang=source_lang,
                target_lang=target_lang,
            )
            for src, tgt in zip(texts, translations)
        ]


class CTranslate2NLLBBackend(MTBackend):
    """
    CTranslate2加速的NLLB后端
    更快的推理速度，适合实时场景
    """
    
    LANG_CODE_MAP = NLLBBackend.LANG_CODE_MAP
    
    def __init__(self, config: MTConfig):
        self.config = config
        self._translator = None
        self._load_model()
    
    def _load_model(self):
        """加载模型"""
        try:
            import ctranslate2
            
            model_path = self.config.model_path
            if not model_path:
                raise ValueError("CTranslate2需要指定本地模型路径")
            
            device = self.config.device
            if device == "auto":
                device = "cuda" if ctranslate2.get_cuda_device_count() > 0 else "cpu"
            
            logger.info(f"加载CTranslate2模型: {model_path}, device={device}")
            
            self._translator = ctranslate2.Translator(model_path, device=device)
            
            logger.info("CTranslate2模型加载完成")
            
        except ImportError:
            raise ImportError("请安装ctranslate2: pip install ctranslate2")
    
    def _get_lang_code(self, lang: str) -> str:
        return self.LANG_CODE_MAP.get(lang.lower(), lang)
    
    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        """翻译文本"""
        start_time = time.time()
        
        src_code = self._get_lang_code(source_lang)
        tgt_code = self._get_lang_code(target_lang)
        
        # 分词（简化版）
        tokens = text.split()
        
        # 翻译
        target_prefix = [tgt_code]
        results = self._translator.translate_batch(
            [tokens],
            target_prefix=[target_prefix],
            max_decoding_length=self.config.max_length,
            beam_size=1,
        )
        
        translated = " ".join(results[0].hypotheses[0][1:])  # 跳过语言标记
        
        logger.debug(f"翻译完成: 耗时{time.time()-start_time:.2f}s")
        
        return TranslationResult(
            source_text=text,
            translated_text=translated,
            source_lang=source_lang,
            target_lang=target_lang,
        )
    
    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        """批量翻译"""
        src_code = self._get_lang_code(source_lang)
        tgt_code = self._get_lang_code(target_lang)
        
        tokenized = [text.split() for text in texts]
        target_prefix = [[tgt_code]] * len(texts)
        
        results = self._translator.translate_batch(
            tokenized,
            target_prefix=target_prefix,
            max_decoding_length=self.config.max_length,
            beam_size=1,
        )
        
        translations = [" ".join(r.hypotheses[0][1:]) for r in results]
        
        return [
            TranslationResult(
                source_text=src,
                translated_text=tgt,
                source_lang=source_lang,
                target_lang=target_lang,
            )
            for src, tgt in zip(texts, translations)
        ]


class MarianBackend(MTBackend):
    """
    MarianMT后端
    适合快速接入中小型翻译模型（如 Helsinki-NLP/opus-mt-*）
    """

    def __init__(self, config: MTConfig):
        self.config = config
        self._tokenizer = None
        self._model = None
        self._device = "cpu"
        self._load_model()

    def _load_model(self):
        """加载模型"""
        try:
            from transformers import AutoModelForSeq2SeqLM, AutoTokenizer
            import torch

            self._device = self.config.device
            if self._device == "auto":
                self._device = "cuda" if torch.cuda.is_available() else "cpu"

            model_name = self.config.model_path or self.config.model_name

            logger.info(f"加载Marian模型: {model_name}, device={self._device}")
            self._tokenizer = AutoTokenizer.from_pretrained(model_name)
            self._model = AutoModelForSeq2SeqLM.from_pretrained(model_name)
            self._model.to(self._device)
            self._model.eval()
            logger.info("Marian模型加载完成")

        except ImportError:
            raise ImportError("请安装transformers: pip install transformers torch sentencepiece")

    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        """翻译文本"""
        start_time = time.time()

        inputs = self._tokenizer(
            text,
            return_tensors="pt",
            truncation=True,
            max_length=self.config.max_length,
        )
        inputs = {k: v.to(self._device) for k, v in inputs.items()}

        outputs = self._model.generate(
            **inputs,
            max_length=self.config.max_length,
            num_beams=1,
            do_sample=False,
        )
        translated = self._tokenizer.batch_decode(outputs, skip_special_tokens=True)[0]

        logger.debug(f"Marian翻译完成: 耗时{time.time()-start_time:.2f}s")
        return TranslationResult(
            source_text=text,
            translated_text=translated,
            source_lang=source_lang,
            target_lang=target_lang,
        )

    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        """批量翻译"""
        inputs = self._tokenizer(
            texts,
            return_tensors="pt",
            padding=True,
            truncation=True,
            max_length=self.config.max_length,
        )
        inputs = {k: v.to(self._device) for k, v in inputs.items()}

        outputs = self._model.generate(
            **inputs,
            max_length=self.config.max_length,
            num_beams=1,
            do_sample=False,
        )
        translations = self._tokenizer.batch_decode(outputs, skip_special_tokens=True)

        return [
            TranslationResult(
                source_text=src,
                translated_text=tgt,
                source_lang=source_lang,
                target_lang=target_lang,
            )
            for src, tgt in zip(texts, translations)
        ]


class ArgosTranslateBackend(MTBackend):
    """Argos Translate离线翻译后端"""

    def __init__(self, config: MTConfig):
        self.config = config
        self._translate = None
        self._load_model()

    def _load_model(self):
        """加载Argos翻译运行时"""
        try:
            import argostranslate.translate as translate
        except ImportError as exc:
            raise ImportError("请安装argostranslate: pip install argostranslate") from exc

        self._translate = translate
        logger.info("Argos Translate后端初始化完成")

    def _get_translation(self, source_lang: str, target_lang: str):
        installed_languages = self._translate.get_installed_languages()
        src = next((lang for lang in installed_languages if lang.code == source_lang), None)
        tgt = next((lang for lang in installed_languages if lang.code == target_lang), None)

        if src is None or tgt is None:
            raise ValueError(
                f"Argos语言包未安装: {source_lang}->{target_lang}. "
                "请先下载并安装对应语言包。"
            )

        translation = src.get_translation(tgt)
        if translation is None:
            raise ValueError(f"Argos不支持语言对: {source_lang}->{target_lang}")
        return translation

    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        translator = self._get_translation(source_lang, target_lang)
        translated_text = translator.translate(text)
        return TranslationResult(
            source_text=text,
            translated_text=translated_text,
            source_lang=source_lang,
            target_lang=target_lang,
        )

    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        translator = self._get_translation(source_lang, target_lang)
        return [
            TranslationResult(
                source_text=text,
                translated_text=translator.translate(text),
                source_lang=source_lang,
                target_lang=target_lang,
            )
            for text in texts
        ]


class ArgosCTranslate2Backend(MTBackend):
    """基于 Argos 已安装包 + CTranslate2 的轻量离线翻译后端"""

    def __init__(self, config: MTConfig):
        self.config = config
        self._device = "cpu"
        self._compute_type = "int8"
        self._ctranslate2 = None
        self._argos_package = None
        self._runtime_cache: Dict[str, Tuple[object, object]] = {}
        self._load_runtime()

    @staticmethod
    def _normalize_lang_code(lang: str) -> str:
        code = (lang or "").strip().lower()
        code = code.replace("_", "-")
        return code.split("-")[0] if "-" in code else code

    def _load_runtime(self) -> None:
        try:
            # ctranslate2.__init__ 会导入 converters，进而触发 transformers/torch。
            # 在 PyInstaller onefile 下仅需 Translator 推理能力，提前注入轻量占位模块可避免不必要的重依赖导入。
            import sys
            import types
            if getattr(sys, "frozen", False) and "ctranslate2.converters" not in sys.modules:
                sys.modules["ctranslate2.converters"] = types.ModuleType("ctranslate2.converters")

            import ctranslate2
            import argostranslate.package as argos_package
        except ImportError as exc:
            raise ImportError(
                "请安装 ctranslate2 和 argostranslate: pip install ctranslate2 argostranslate sentencepiece"
            ) from exc

        self._ctranslate2 = ctranslate2
        self._argos_package = argos_package

        self._device = self.config.device
        if self._device == "auto":
            self._device = "cuda" if ctranslate2.get_cuda_device_count() > 0 else "cpu"

        self._compute_type = self.config.compute_type or "int8"
        if self._device == "cpu" and self._compute_type in {"float16", "int8_float16"}:
            self._compute_type = "int8"

        logger.info(
            f"Argos-CT2后端初始化完成: device={self._device}, compute_type={self._compute_type}"
        )

    def _iter_candidate_packages(self) -> List[object]:
        if self.config.model_path:
            base_path = Path(self.config.model_path)
            if not base_path.exists():
                raise FileNotFoundError(f"Argos模型路径不存在: {base_path}")

            candidate_dirs: List[Path] = []
            if (base_path / "metadata.json").exists():
                candidate_dirs.append(base_path)
            else:
                candidate_dirs.extend(
                    p for p in base_path.iterdir() if p.is_dir() and (p / "metadata.json").exists()
                )

            packages = []
            for directory in candidate_dirs:
                try:
                    packages.append(self._argos_package.Package(directory))
                except Exception:
                    continue
            return packages

        return self._argos_package.get_installed_packages()

    def _find_package(self, source_lang: str, target_lang: str):
        src = self._normalize_lang_code(source_lang)
        tgt = self._normalize_lang_code(target_lang)

        installed = self._iter_candidate_packages()
        for pkg in installed:
            if getattr(pkg, "type", "translate") != "translate":
                continue
            if self._normalize_lang_code(pkg.from_code) == src and self._normalize_lang_code(pkg.to_code) == tgt:
                return pkg

        installed_pairs = sorted(
            {
                f"{self._normalize_lang_code(getattr(p, 'from_code', ''))}->{self._normalize_lang_code(getattr(p, 'to_code', ''))}"
                for p in installed
                if getattr(p, "type", "translate") == "translate"
            }
        )
        raise ValueError(
            f"未找到Argos已安装语言包: {src}->{tgt}. 已安装语言对: {', '.join(installed_pairs) or '无'}"
        )

    def _get_runtime(self, source_lang: str, target_lang: str):
        key = f"{self._normalize_lang_code(source_lang)}->{self._normalize_lang_code(target_lang)}"
        cached = self._runtime_cache.get(key)
        if cached:
            return cached

        pkg = self._find_package(source_lang, target_lang)
        model_dir = pkg.package_path / "model"
        if not model_dir.exists():
            raise FileNotFoundError(f"Argos模型目录不存在: {model_dir}")

        translator = self._ctranslate2.Translator(
            str(model_dir),
            device=self._device,
            compute_type=self._compute_type,
        )

        self._runtime_cache[key] = (pkg, translator)
        logger.info(f"Argos-CT2翻译模型加载完成: {pkg.from_code}->{pkg.to_code} ({model_dir})")
        return pkg, translator

    def _translate_with_runtime(self, text: str, pkg, translator) -> str:
        if not text:
            return ""

        paragraphs = text.split("\n")
        translated_parts: List[str] = []

        for paragraph in paragraphs:
            if not paragraph.strip():
                translated_parts.append(paragraph)
                continue

            source_tokens = pkg.tokenizer.encode(paragraph)
            if not source_tokens:
                translated_parts.append("")
                continue

            target_prefix_value = getattr(pkg, "target_prefix", "")
            target_prefix = [[target_prefix_value]] if target_prefix_value else None

            results = translator.translate_batch(
                [source_tokens],
                target_prefix=target_prefix,
                replace_unknowns=True,
                max_batch_size=1,
                beam_size=1,
                num_hypotheses=1,
                max_decoding_length=self.config.max_length,
            )

            output_tokens = results[0].hypotheses[0] if results and results[0].hypotheses else []
            translated = pkg.tokenizer.decode(output_tokens) if output_tokens else ""

            if target_prefix_value and translated.startswith(target_prefix_value):
                translated = translated[len(target_prefix_value):]

            translated_parts.append(translated.lstrip())

        return "\n".join(translated_parts)

    def translate(self, text: str, source_lang: str, target_lang: str) -> TranslationResult:
        pkg, translator = self._get_runtime(source_lang, target_lang)
        translated_text = self._translate_with_runtime(text, pkg, translator)
        return TranslationResult(
            source_text=text,
            translated_text=translated_text,
            source_lang=source_lang,
            target_lang=target_lang,
        )

    def translate_batch(self, texts: List[str], source_lang: str, target_lang: str) -> List[TranslationResult]:
        pkg, translator = self._get_runtime(source_lang, target_lang)
        return [
            TranslationResult(
                source_text=text,
                translated_text=self._translate_with_runtime(text, pkg, translator),
                source_lang=source_lang,
                target_lang=target_lang,
            )
            for text in texts
        ]


class MTEngine:
    """
    机器翻译引擎
    统一的翻译接口
    """
    
    BACKENDS = {
        "nllb": NLLBBackend,
        "nllb-ct2": CTranslate2NLLBBackend,
        "marian": MarianBackend,
        "argos-ct2": ArgosCTranslate2Backend,
        "argos": ArgosTranslateBackend,
    }
    
    def __init__(
        self,
        config: Optional[MTConfig] = None,
        term_bank_manager: Optional[TermBankManager] = None,
    ):
        self.config = config or settings.mt
        self._backend: Optional[MTBackend] = None
        self._term_bank = term_bank_manager
        
        self._load_backend()
        
        if self.config.term_bank_enabled and not self._term_bank:
            self._term_bank = TermBankManager(self.config.term_bank_path or settings.term_bank_path)
    
    def _load_backend(self):
        """加载后端"""
        backend_class = self.BACKENDS.get(self.config.model_type)
        if not backend_class:
            raise ValueError(f"不支持的MT后端: {self.config.model_type}")

        try:
            self._backend = backend_class(self.config)
            logger.info(f"MT引擎初始化完成: {self.config.model_type}")
        except Exception as exc:
            logger.exception(f"MT后端加载失败，自动降级到Fallback: {exc}")
            self._backend = FallbackMTBackend(reason=str(exc))
    
    def translate(
        self,
        text: str,
        source_lang: Optional[str] = None,
        target_lang: Optional[str] = None,
    ) -> TranslationResult:
        """翻译文本"""
        source_lang = source_lang or self.config.source_lang
        target_lang = target_lang or self.config.target_lang
        
        result = self._backend.translate(text, source_lang, target_lang)
        
        # 应用术语库
        if self._term_bank:
            result.translated_text = self._term_bank.apply(result.translated_text)
        
        return result
    
    def translate_batch(
        self,
        texts: List[str],
        source_lang: Optional[str] = None,
        target_lang: Optional[str] = None,
    ) -> List[TranslationResult]:
        """批量翻译"""
        source_lang = source_lang or self.config.source_lang
        target_lang = target_lang or self.config.target_lang
        
        results = self._backend.translate_batch(texts, source_lang, target_lang)
        
        if self._term_bank:
            for r in results:
                r.translated_text = self._term_bank.apply(r.translated_text)
        
        return results
