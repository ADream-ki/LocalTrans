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
            
            model_path = self.config.model_path or self.config.model_size
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
        
        segments, info = self._model.transcribe(
            prepared_audio,
            language=self.config.language,
            beam_size=self.config.beam_size,
            vad_filter=self.config.vad_filter,
            word_timestamps=self.config.word_timestamps,
            without_timestamps=not self.config.word_timestamps,
            condition_on_previous_text=False,
            **kwargs
        )
        
        # 合并所有片段
        text_parts = []
        words = []
        for segment in segments:
            text_parts.append(segment.text)
            if segment.words:
                words.extend([{
                    "word": w.word,
                    "start": w.start,
                    "end": w.end,
                    "probability": w.probability,
                } for w in segment.words])
        
        result = TranscriptionResult(
            text=" ".join(text_parts).strip(),
            language=info.language,
            confidence=info.language_probability,
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


class VoskBackend(ASRBackend):
    """Vosk离线识别后端"""

    def __init__(self, config: ASRConfig):
        self.config = config
        self._model = None
        self._model_path: Optional[Path] = None
        self._sample_rate = settings.audio.sample_rate
        self._load_model()

    def _resolve_model_path(self) -> Path:
        if self.config.model_path:
            return Path(self.config.model_path)
        # 默认模型路径（可由下载器提前准备）
        return settings.models_dir / "asr" / "vosk-model-small-en-us-0.15"

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
            # 缺少重量级依赖/模型时，降级为可运行模式，保障端到端链路可验证
            logger.warning(f"ASR后端加载失败，自动降级到Fallback: {exc}")
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
