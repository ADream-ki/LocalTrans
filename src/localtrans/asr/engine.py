"""
ASR引擎
支持faster-whisper、whisper等多种后端
"""

import time
import json
from abc import ABC, abstractmethod
from typing import Generator, List, Optional, Iterator
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from loguru import logger

from localtrans.config import settings, ASRConfig


@dataclass
class TranscriptionResult:
    """转录结果"""
    text: str
    language: str
    confidence: float
    start_time: float
    end_time: float
    words: Optional[List[dict]] = None
    
    @property
    def duration(self) -> float:
        return self.end_time - self.start_time


class ASRBackend(ABC):
    """ASR后端抽象基类"""
    
    @abstractmethod
    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        """转录音频"""
        pass
    
    @abstractmethod
    def transcribe_stream(self, audio_stream: Iterator[np.ndarray], **kwargs) -> Generator[TranscriptionResult, None, None]:
        """流式转录"""
        pass


class FallbackASRBackend(ASRBackend):
    """依赖缺失时的轻量回退后端"""

    def __init__(self, config: ASRConfig, reason: str = ""):
        self.config = config
        self.reason = reason
        logger.warning(f"使用Fallback ASR后端: {reason}")

    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        """基于能量阈值的占位识别结果"""
        audio_arr = np.asarray(audio)
        if audio_arr.ndim > 1:
            audio_arr = audio_arr.reshape(-1)

        if audio_arr.size == 0:
            text = ""
            duration = 0.0
        else:
            rms = float(np.sqrt(np.mean(np.square(audio_arr.astype(np.float32)))))
            duration = audio_arr.shape[0] / settings.audio.sample_rate
            text = "[fallback-asr] speech detected" if rms > 100.0 else ""

        return TranscriptionResult(
            text=text,
            language=self.config.language or "unknown",
            confidence=0.0,
            start_time=0.0,
            end_time=duration,
            words=None,
        )

    def transcribe_stream(
        self,
        audio_stream: Iterator[np.ndarray],
        **kwargs
    ) -> Generator[TranscriptionResult, None, None]:
        """对每个chunk执行占位识别"""
        for chunk in audio_stream:
            result = self.transcribe(chunk, **kwargs)
            if result.text:
                yield result


class FasterWhisperBackend(ASRBackend):
    """faster-whisper后端 - 使用CTranslate2加速"""
    
    def __init__(self, config: ASRConfig):
        self.config = config
        self._model = None
        self._load_model()
    
    def _load_model(self):
        """加载模型"""
        try:
            # ctranslate2.__init__ 会导入 converters，进而触发 transformers/torch。
            # 在 PyInstaller onefile 中 faster-whisper 仅需要推理路径，提前注入占位模块
            # 可避免 converters 链路带来的 _C 导入问题。
            import sys
            import types
            if getattr(sys, "frozen", False) and "ctranslate2.converters" not in sys.modules:
                sys.modules["ctranslate2.converters"] = types.ModuleType("ctranslate2.converters")

            from faster_whisper import WhisperModel
            
            device = self.config.device
            if device == "auto":
                try:
                    import ctranslate2
                    device = "cuda" if ctranslate2.get_cuda_device_count() > 0 else "cpu"
                except Exception:
                    import torch
                    device = "cuda" if torch.cuda.is_available() else "cpu"

            compute_type = self.config.compute_type
            if device == "cuda" and compute_type == "int8":
                # GPU上int8_float16通常延迟更低且兼容性更好
                compute_type = "int8_float16"
            if device == "cpu" and compute_type in {"float16", "int8_float16"}:
                compute_type = "int8"
            
            model_path = self.config.model_path or self.config.model_name or self.config.model_size
            if isinstance(model_path, Path):
                model_path = str(model_path)
            
            logger.info(f"加载faster-whisper模型: {model_path}, device={device}, compute_type={compute_type}")
            self._model = WhisperModel(
                model_path,
                device=device,
                compute_type=compute_type,
            )
            logger.info("faster-whisper模型加载完成")
            
        except ImportError:
            raise ImportError("请安装faster-whisper: pip install faster-whisper")

    @staticmethod
    def _prepare_audio(audio: np.ndarray) -> np.ndarray:
        """将输入音频标准化为 faster-whisper 期望的 1D float32[-1,1]"""
        arr = np.asarray(audio)
        if arr.ndim > 1:
            arr = arr.reshape(-1)

        if np.issubdtype(arr.dtype, np.integer):
            info = np.iinfo(arr.dtype)
            denom = max(abs(info.min), abs(info.max))
            arr = arr.astype(np.float32) / float(denom if denom > 0 else 32768)
        else:
            arr = arr.astype(np.float32, copy=False)

        np.clip(arr, -1.0, 1.0, out=arr)
        return np.ascontiguousarray(arr)
    
    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        """转录音频"""
        start_time = time.time()

        prepared_audio = self._prepare_audio(audio)

        transcribe_kwargs = {
            "language": self.config.language,
            "task": self.config.task,
            "beam_size": self.config.beam_size,
            "vad_filter": self.config.vad_filter,
            "word_timestamps": self.config.word_timestamps,
            "without_timestamps": not self.config.word_timestamps,
            "condition_on_previous_text": False,
        }
        transcribe_kwargs.update(kwargs)

        try:
            segments, info = self._model.transcribe(prepared_audio, **transcribe_kwargs)
        except Exception as exc:
            err_msg = str(exc)
            missing_vad_asset = (
                bool(transcribe_kwargs.get("vad_filter"))
                and (
                    "silero_vad.onnx" in err_msg
                    or ("NO_SUCHFILE" in err_msg and "vad" in err_msg.lower())
                )
            )
            if not missing_vad_asset:
                raise

            logger.warning("检测到silero_vad资源缺失，自动关闭vad_filter后重试一次")
            transcribe_kwargs["vad_filter"] = False
            segments, info = self._model.transcribe(prepared_audio, **transcribe_kwargs)
        
        # 合并所有片段
        text_parts = []
        words = []
        segment_confidences = []
        for segment in segments:
            text_parts.append(segment.text)
            avg_logprob = getattr(segment, "avg_logprob", None)
            if avg_logprob is not None:
                # avg_logprob 越接近0越可信，转为 0-1 近似置信度
                conf = float(np.exp(float(avg_logprob)))
                conf = max(0.0, min(conf, 1.0))
                segment_confidences.append(conf)
            if segment.words:
                word_items = [{
                    "word": w.word,
                    "start": w.start,
                    "end": w.end,
                    "probability": w.probability,
                } for w in segment.words]
                words.extend(word_items)
                probs = [float(item["probability"]) for item in word_items if item.get("probability") is not None]
                if probs:
                    segment_confidences.append(sum(probs) / len(probs))

        confidence = 0.0
        if segment_confidences:
            confidence = float(sum(segment_confidences) / len(segment_confidences))
        elif info.language_probability is not None:
            confidence = float(info.language_probability)
        
        result = TranscriptionResult(
            text=" ".join(text_parts).strip(),
            language=info.language,
            confidence=confidence,
            start_time=0.0,
            end_time=info.duration,
            words=words if words else None,
        )
        
        logger.debug(f"转录完成: {len(result.text)}字符, 耗时{time.time()-start_time:.2f}s")
        return result
    
    def transcribe_stream(
        self, 
        audio_stream: Iterator[np.ndarray],
        **kwargs
    ) -> Generator[TranscriptionResult, None, None]:
        """流式转录"""
        buffer = []
        buffer_duration = 0.0
        min_chunk_duration = 2.0  # 最小处理块时长（秒）
        
        for chunk in audio_stream:
            buffer.append(chunk)
            # 计算buffer时长
            chunk_duration = len(chunk) / settings.audio.sample_rate
            buffer_duration += chunk_duration
            
            if buffer_duration >= min_chunk_duration:
                # 合并并处理
                audio_data = np.concatenate(buffer)
                result = self.transcribe(audio_data, **kwargs)
                buffer = []
                buffer_duration = 0.0
                yield result
        
        # 处理剩余数据
        if buffer:
            audio_data = np.concatenate(buffer)
            yield self.transcribe(audio_data, **kwargs)


class WhisperBackend(ASRBackend):
    """OpenAI Whisper后端"""
    
    def __init__(self, config: ASRConfig):
        self.config = config
        self._model = None
        self._load_model()
    
    def _load_model(self):
        """加载模型"""
        try:
            import whisper
            
            device = self.config.device
            if device == "auto":
                import torch
                device = "cuda" if torch.cuda.is_available() else "cpu"
            
            logger.info(f"加载Whisper模型: {self.config.model_size}, device={device}")
            self._model = whisper.load_model(self.config.model_size, device=device)
            logger.info("Whisper模型加载完成")
            
        except ImportError:
            raise ImportError("请安装openai-whisper: pip install openai-whisper")
    
    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        """转录音频"""
        start_time = time.time()
        
        result = self._model.transcribe(
            audio,
            language=self.config.language,
            task=self.config.task,
            beam_size=self.config.beam_size,
            **kwargs
        )
        
        # 提取词级时间戳
        words = None
        if self.config.word_timestamps and "words" in result.get("segments", [{}])[0]:
            words = []
            for segment in result["segments"]:
                if "words" in segment:
                    words.extend(segment["words"])
        
        transcription = TranscriptionResult(
            text=result["text"].strip(),
            language=result.get("language", "unknown"),
            confidence=1.0,  # whisper不提供语言置信度
            start_time=0.0,
            end_time=audio.shape[0] / settings.audio.sample_rate,
            words=words,
        )
        
        logger.debug(f"转录完成: {len(transcription.text)}字符, 耗时{time.time()-start_time:.2f}s")
        return transcription
    
    def transcribe_stream(
        self, 
        audio_stream: Iterator[np.ndarray],
        **kwargs
    ) -> Generator[TranscriptionResult, None, None]:
        """流式转录"""
        # Whisper原生不支持流式，使用缓冲方式模拟
        buffer = []
        buffer_duration = 0.0
        min_chunk_duration = 3.0
        
        for chunk in audio_stream:
            buffer.append(chunk)
            chunk_duration = len(chunk) / settings.audio.sample_rate
            buffer_duration += chunk_duration
            
            if buffer_duration >= min_chunk_duration:
                audio_data = np.concatenate(buffer)
                yield self.transcribe(audio_data, **kwargs)
                buffer = []
                buffer_duration = 0.0
        
        if buffer:
            audio_data = np.concatenate(buffer)
            yield self.transcribe(audio_data, **kwargs)


class FunASRBackend(ASRBackend):
    """FunASR后端（优先用于中文实时场景）"""

    def __init__(self, config: ASRConfig):
        self.config = config
        self._model = None
        self._load_model()

    def _resolve_model_ref(self) -> str:
        if self.config.model_path:
            return str(self.config.model_path)
        if self.config.model_name:
            return str(self.config.model_name)

        model_size = (self.config.model_size or "").strip()
        if model_size.lower() in {"tiny", "base", "small", "medium", "large"}:
            return "iic/SenseVoiceSmall"
        return model_size or "iic/SenseVoiceSmall"

    def _load_model(self):
        try:
            from funasr import AutoModel
        except ImportError as exc:
            raise ImportError("请安装funasr: pip install funasr") from exc

        device = self.config.device
        if device == "auto":
            try:
                import torch

                device = "cuda:0" if torch.cuda.is_available() else "cpu"
            except Exception:
                device = "cpu"
        elif device == "cuda":
            device = "cuda:0"

        model_ref = self._resolve_model_ref()
        logger.info(f"加载FunASR模型: {model_ref}, device={device}")

        # 兼容不同版本 FunASR AutoModel 参数签名
        init_attempts = [
            {"trust_remote_code": True, "disable_update": True},
            {"trust_remote_code": True},
            {},
        ]
        last_exc = None
        for extra_kwargs in init_attempts:
            try:
                self._model = AutoModel(model=model_ref, device=device, **extra_kwargs)
                last_exc = None
                break
            except TypeError as exc:
                last_exc = exc
                continue

        if self._model is None:
            raise RuntimeError(f"FunASR模型初始化失败: {last_exc}") from last_exc

        logger.info("FunASR模型加载完成")

    @staticmethod
    def _extract_text(result_obj) -> str:
        if result_obj is None:
            return ""
        if isinstance(result_obj, str):
            return result_obj.strip()
        if isinstance(result_obj, dict):
            text_val = result_obj.get("text", "")
            if isinstance(text_val, str):
                return text_val.strip()
            sentence_info = result_obj.get("sentence_info")
            if isinstance(sentence_info, list):
                items = []
                for item in sentence_info:
                    if isinstance(item, dict):
                        text_item = item.get("text")
                        if isinstance(text_item, str) and text_item.strip():
                            items.append(text_item.strip())
                if items:
                    return " ".join(items).strip()
            return ""
        if isinstance(result_obj, list):
            parts = [FunASRBackend._extract_text(item) for item in result_obj]
            return " ".join([p for p in parts if p]).strip()
        return str(result_obj).strip()

    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        prepared_audio = FasterWhisperBackend._prepare_audio(audio)
        duration = len(prepared_audio) / settings.audio.sample_rate if len(prepared_audio) else 0.0

        generate_kwargs = {
            "input": prepared_audio,
            "language": self.config.language or "auto",
        }
        generate_kwargs.update(kwargs)

        try:
            result_obj = self._model.generate(**generate_kwargs)
        except TypeError:
            # 兼容旧版本参数
            generate_kwargs.pop("language", None)
            result_obj = self._model.generate(**generate_kwargs)

        text = self._extract_text(result_obj)
        return TranscriptionResult(
            text=text,
            language=self.config.language or "zh",
            confidence=0.0,
            start_time=0.0,
            end_time=duration,
            words=None,
        )

    def transcribe_stream(
        self,
        audio_stream: Iterator[np.ndarray],
        **kwargs
    ) -> Generator[TranscriptionResult, None, None]:
        buffer = []
        buffer_duration = 0.0
        min_chunk_duration = 1.2

        for chunk in audio_stream:
            buffer.append(chunk)
            chunk_duration = len(chunk) / settings.audio.sample_rate
            buffer_duration += chunk_duration

            if buffer_duration >= min_chunk_duration:
                audio_data = np.concatenate(buffer)
                result = self.transcribe(audio_data, **kwargs)
                if result.text.strip():
                    yield result
                buffer = []
                buffer_duration = 0.0

        if buffer:
            result = self.transcribe(np.concatenate(buffer), **kwargs)
            if result.text.strip():
                yield result


class SherpaOnnxBackend(ASRBackend):
    """sherpa-onnx 后端（优先使用本地模型目录）"""

    def __init__(self, config: ASRConfig):
        self.config = config
        self._recognizer = None
        self._sample_rate = settings.audio.sample_rate
        self._load_model()

    def _resolve_model_dir(self) -> Path:
        if self.config.model_path:
            path = Path(self.config.model_path)
            if not path.exists():
                raise FileNotFoundError(f"sherpa-onnx模型路径不存在: {path}")
            return path

        if self.config.model_name:
            named = settings.models_dir / "asr" / str(self.config.model_name)
            if named.exists():
                return named
            maybe_path = Path(self.config.model_name)
            if maybe_path.exists():
                return maybe_path

        fallback = settings.models_dir / "asr" / "sherpa-onnx-zh-en-zipformer"
        if fallback.exists():
            return fallback
        raise FileNotFoundError(
            "未找到sherpa-onnx模型目录，请先配置LOCALTRANS_ASR__MODEL_PATH或下载模型"
        )

    @staticmethod
    def _find_first_existing(base_dir: Path, candidates: list[str]) -> Optional[Path]:
        for name in candidates:
            p = base_dir / name
            if p.exists():
                return p
        return None

    @staticmethod
    def _extract_result_text(result_obj) -> str:
        if result_obj is None:
            return ""
        if isinstance(result_obj, str):
            return result_obj.strip()
        text = getattr(result_obj, "text", None)
        if isinstance(text, str):
            return text.strip()
        if isinstance(result_obj, dict):
            val = result_obj.get("text", "")
            return str(val).strip()
        return str(result_obj).strip()

    @staticmethod
    def _provider_from_device(device: str) -> str:
        dev = (device or "auto").lower()
        if dev == "cuda":
            return "cuda"
        return "cpu"

    def _build_transducer_recognizer(self, sherpa_onnx, model_dir: Path):
        tokens = self._find_first_existing(model_dir, ["tokens.txt", "tokens"])
        encoder = self._find_first_existing(model_dir, ["encoder.onnx", "encoder.int8.onnx"])
        decoder = self._find_first_existing(model_dir, ["decoder.onnx", "decoder.int8.onnx"])
        joiner = self._find_first_existing(model_dir, ["joiner.onnx", "joiner.int8.onnx"])
        if not all([tokens, encoder, decoder, joiner]):
            return None

        recognizer_cls = getattr(sherpa_onnx, "OfflineRecognizer", None)
        if recognizer_cls is None:
            return None

        ctor = getattr(recognizer_cls, "from_transducer", None)
        if ctor is None:
            return None

        provider = self._provider_from_device(self.config.device)
        common = {
            "tokens": str(tokens),
            "num_threads": max(1, int(getattr(settings, "max_workers", 1))),
            "sample_rate": self._sample_rate,
            "feature_dim": 80,
            "decoding_method": "greedy_search",
            "provider": provider,
        }

        attempts = [
            {
                **common,
                "encoder": str(encoder),
                "decoder": str(decoder),
                "joiner": str(joiner),
            },
            {
                **common,
                "encoder_model": str(encoder),
                "decoder_model": str(decoder),
                "joiner_model": str(joiner),
            },
        ]

        last_exc = None
        for kwargs in attempts:
            try:
                return ctor(**kwargs)
            except TypeError as exc:
                last_exc = exc
                continue

        if last_exc is not None:
            raise RuntimeError(f"sherpa-onnx transducer初始化失败: {last_exc}") from last_exc
        return None

    def _build_paraformer_recognizer(self, sherpa_onnx, model_dir: Path):
        tokens = self._find_first_existing(model_dir, ["tokens.txt", "tokens"])
        model = self._find_first_existing(
            model_dir,
            [
                "paraformer.onnx",
                "model.int8.onnx",
                "model.onnx",
            ],
        )
        if not all([tokens, model]):
            return None

        recognizer_cls = getattr(sherpa_onnx, "OfflineRecognizer", None)
        if recognizer_cls is None:
            return None

        ctor = getattr(recognizer_cls, "from_paraformer", None)
        if ctor is None:
            return None

        provider = self._provider_from_device(self.config.device)
        common = {
            "tokens": str(tokens),
            "num_threads": max(1, int(getattr(settings, "max_workers", 1))),
            "sample_rate": self._sample_rate,
            "feature_dim": 80,
            "decoding_method": "greedy_search",
            "provider": provider,
        }
        attempts = [
            {**common, "paraformer": str(model)},
            {**common, "model": str(model)},
            {**common, "model_path": str(model)},
        ]

        last_exc = None
        for kwargs in attempts:
            try:
                return ctor(**kwargs)
            except TypeError as exc:
                last_exc = exc
                continue

        if last_exc is not None:
            raise RuntimeError(f"sherpa-onnx paraformer初始化失败: {last_exc}") from last_exc
        return None

    def _load_model(self):
        try:
            import sherpa_onnx
        except ImportError as exc:
            raise ImportError("请安装sherpa-onnx: pip install sherpa-onnx") from exc

        model_dir = self._resolve_model_dir()
        logger.info(f"加载sherpa-onnx模型目录: {model_dir}")

        recognizer = self._build_transducer_recognizer(sherpa_onnx, model_dir)
        if recognizer is None:
            recognizer = self._build_paraformer_recognizer(sherpa_onnx, model_dir)

        if recognizer is None:
            raise RuntimeError(
                "未识别的sherpa-onnx模型布局，需包含 "
                "transducer(encoder/decoder/joiner/tokens) 或 paraformer(model/tokens)"
            )

        self._recognizer = recognizer
        logger.info("sherpa-onnx模型加载完成")

    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        prepared_audio = FasterWhisperBackend._prepare_audio(audio)
        duration = len(prepared_audio) / float(self._sample_rate) if len(prepared_audio) else 0.0
        if prepared_audio.size == 0:
            return TranscriptionResult(
                text="",
                language=self.config.language or "zh",
                confidence=0.0,
                start_time=0.0,
                end_time=duration,
                words=None,
            )

        stream = self._recognizer.create_stream()
        stream.accept_waveform(self._sample_rate, prepared_audio)

        decode_fn = getattr(self._recognizer, "decode_stream", None)
        if callable(decode_fn):
            decode_fn(stream)
        else:
            decode_streams = getattr(self._recognizer, "decode_streams", None)
            if callable(decode_streams):
                decode_streams([stream])
            else:
                raise RuntimeError("sherpa-onnx识别器不支持decode_stream/decode_streams")

        stream_result = getattr(stream, "result", None)
        text = self._extract_result_text(stream_result)

        return TranscriptionResult(
            text=text,
            language=self.config.language or "zh",
            confidence=0.0,
            start_time=0.0,
            end_time=duration,
            words=None,
        )

    def transcribe_stream(
        self,
        audio_stream: Iterator[np.ndarray],
        **kwargs
    ) -> Generator[TranscriptionResult, None, None]:
        buffer = []
        buffer_duration = 0.0
        min_chunk_duration = 1.0

        for chunk in audio_stream:
            buffer.append(chunk)
            chunk_duration = len(chunk) / settings.audio.sample_rate
            buffer_duration += chunk_duration

            if buffer_duration >= min_chunk_duration:
                audio_data = np.concatenate(buffer)
                result = self.transcribe(audio_data, **kwargs)
                if result.text.strip():
                    yield result
                buffer = []
                buffer_duration = 0.0

        if buffer:
            result = self.transcribe(np.concatenate(buffer), **kwargs)
            if result.text.strip():
                yield result


class VoskBackend(ASRBackend):
    """Vosk离线识别后端"""

    def __init__(self, config: ASRConfig):
        self.config = config
        self._model = None
        self._model_path: Optional[Path] = None
        self._sample_rate = settings.audio.sample_rate
        self._load_model()

    def _resolve_model_path(self) -> Path:
        def _is_valid_vosk_model_dir(path: Path) -> bool:
            if not path.exists() or not path.is_dir():
                return False
            # Vosk模型目录通常包含 am/conf 等关键子目录
            return (path / "am").exists() and (path / "conf").exists()

        if self.config.model_path:
            model_path = Path(self.config.model_path)
            if _is_valid_vosk_model_dir(model_path):
                return model_path
        models_dir = settings.models_dir / "asr"

        # 若 model_size 直接给出 vosk 模型目录名，优先使用
        model_size = (self.config.model_size or "").strip()
        if model_size.startswith("vosk-model-"):
            named_path = models_dir / model_size
            if _is_valid_vosk_model_dir(named_path):
                return named_path

        lang = (self.config.language or "").lower()
        candidates: list[Path]
        if lang.startswith("zh"):
            candidates = [
                models_dir / "vosk-model-small-cn-0.22",
                models_dir / "vosk-model-small-en-us-0.15",
            ]
        elif lang.startswith("en"):
            candidates = [
                models_dir / "vosk-model-small-en-us-0.15",
                models_dir / "vosk-model-small-cn-0.22",
            ]
        else:
            candidates = [
                models_dir / "vosk-model-small-en-us-0.15",
                models_dir / "vosk-model-small-cn-0.22",
            ]

        for path in candidates:
            if _is_valid_vosk_model_dir(path):
                return path

        # 默认模型路径（可由下载器提前准备）
        return models_dir / "vosk-model-small-en-us-0.15"

    def _load_model(self) -> None:
        try:
            from vosk import Model, SetLogLevel
        except ImportError as exc:
            raise ImportError("请安装vosk: pip install vosk") from exc

        model_path = self._resolve_model_path()
        if not model_path.exists():
            raise FileNotFoundError(f"Vosk模型不存在: {model_path}")

        SetLogLevel(-1)  # 降低Vosk输出噪声
        self._model = Model(str(model_path))
        self._model_path = model_path
        logger.info(f"Vosk模型加载完成: {model_path}")

    @staticmethod
    def _to_pcm16(audio: np.ndarray) -> np.ndarray:
        audio_arr = np.asarray(audio)
        if audio_arr.ndim > 1:
            audio_arr = audio_arr.reshape(-1)

        if audio_arr.dtype == np.int16:
            return audio_arr

        if np.issubdtype(audio_arr.dtype, np.floating):
            audio_arr = np.clip(audio_arr, -1.0, 1.0)
            return (audio_arr * 32767.0).astype(np.int16)

        return audio_arr.astype(np.int16)

    @staticmethod
    def _result_to_words(result_obj: dict) -> Optional[List[dict]]:
        items = result_obj.get("result")
        if not items:
            return None
        words: List[dict] = []
        for item in items:
            words.append(
                {
                    "word": item.get("word", ""),
                    "start": float(item.get("start", 0.0)),
                    "end": float(item.get("end", 0.0)),
                    "probability": float(item.get("conf", 0.0)),
                }
            )
        return words

    def transcribe(self, audio: np.ndarray, **kwargs) -> TranscriptionResult:
        from vosk import KaldiRecognizer

        pcm16 = self._to_pcm16(audio)
        recognizer = KaldiRecognizer(self._model, float(self._sample_rate))
        recognizer.SetWords(True)
        recognizer.AcceptWaveform(pcm16.tobytes())
        result_obj = json.loads(recognizer.FinalResult())

        text = result_obj.get("text", "").strip()
        words = self._result_to_words(result_obj)
        if words:
            confidence = float(sum(w["probability"] for w in words) / len(words))
        else:
            confidence = 0.0

        duration = len(pcm16) / self._sample_rate if len(pcm16) else 0.0
        return TranscriptionResult(
            text=text,
            language=self.config.language or "en",
            confidence=confidence,
            start_time=0.0,
            end_time=duration,
            words=words,
        )

    def transcribe_stream(
        self,
        audio_stream: Iterator[np.ndarray],
        **kwargs
    ) -> Generator[TranscriptionResult, None, None]:
        for chunk in audio_stream:
            result = self.transcribe(chunk, **kwargs)
            if result.text:
                yield result


class ASREngine:
    """
    ASR引擎封装
    统一的语音识别接口
    """
    
    BACKENDS = {
        "faster-whisper": FasterWhisperBackend,
        "whisper": WhisperBackend,
        "funasr": FunASRBackend,
        "sherpa-onnx": SherpaOnnxBackend,
        "vosk": VoskBackend,
    }
    
    def __init__(self, config: Optional[ASRConfig] = None):
        self.config = config or settings.asr
        self._backend: Optional[ASRBackend] = None
        self._load_backend()
    
    def _load_backend(self):
        """加载后端"""
        backend_class = self.BACKENDS.get(self.config.model_type)
        if not backend_class:
            raise ValueError(f"不支持的ASR后端: {self.config.model_type}")

        try:
            self._backend = backend_class(self.config)
            logger.info(f"ASR引擎初始化完成: {self.config.model_type}")
        except Exception as exc:
            logger.exception(f"ASR后端加载失败({self.config.model_type}): {exc}")

            # 运行时兜底：优先尝试 faster-whisper，避免直接降级到低精度 vosk
            fallback_candidates: list[tuple[str, str]] = []
            if self.config.model_type != "faster-whisper":
                fallback_candidates.extend(
                    [
                        ("faster-whisper", "small"),
                        ("faster-whisper", "base"),
                    ]
                )
            if self.config.model_type != "vosk":
                fallback_candidates.append(("vosk", "vosk-model-small-cn-0.22"))

            for model_type, model_size in fallback_candidates:
                try:
                    fallback_cfg = self.config.model_copy(deep=True)
                    fallback_cfg.model_type = model_type
                    fallback_cfg.model_size = model_size
                    fallback_cfg.model_name = None
                    fallback_cfg.model_path = None

                    backend_cls = self.BACKENDS.get(model_type)
                    if backend_cls is None:
                        continue
                    self._backend = backend_cls(fallback_cfg)
                    logger.warning(f"ASR已自动回退到 {model_type}/{model_size} 后端")
                    return
                except Exception as fallback_exc:
                    logger.warning(f"ASR回退到{model_type}/{model_size}失败: {fallback_exc}")

            # 缺少依赖/模型时，最终降级到可运行占位后端
            self._backend = FallbackASRBackend(self.config, reason=str(exc))
    
    def transcribe(self, audio: np.ndarray) -> TranscriptionResult:
        """转录音频"""
        return self._backend.transcribe(audio)
    
    def transcribe_stream(
        self, 
        audio_stream: Iterator[np.ndarray]
    ) -> Generator[TranscriptionResult, None, None]:
        """流式转录"""
        yield from self._backend.transcribe_stream(audio_stream)
    
    def detect_language(self, audio: np.ndarray) -> tuple:
        """检测音频语言"""
        result = self.transcribe(audio)
        return result.language, result.confidence
