"""
TTS语音合成引擎
支持Piper、Coqui TTS等后端
"""

import time
import re
import tempfile
from pathlib import Path
from abc import ABC, abstractmethod
from typing import Optional, Generator
from dataclasses import dataclass

import numpy as np
from loguru import logger

from localtrans.config import settings, TTSConfig


@dataclass
class SynthesisResult:
    """合成结果"""
    audio: np.ndarray
    sample_rate: int
    text: str
    duration: float
    
    @property
    def duration_ms(self) -> float:
        return self.duration * 1000


class TTSBackend(ABC):
    """TTS后端抽象基类"""
    
    @abstractmethod
    def synthesize(self, text: str) -> SynthesisResult:
        """合成语音"""
        pass
    
    @abstractmethod
    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        """流式合成"""
        pass

    def close(self) -> None:
        """释放后端资源"""
        return


class FallbackTTSBackend(TTSBackend):
    """依赖缺失时的静音回退后端"""

    def __init__(self, config: TTSConfig, reason: str = ""):
        self.config = config
        self.reason = reason
        logger.warning(f"使用Fallback TTS后端: {reason}")

    def synthesize(self, text: str) -> SynthesisResult:
        sample_rate = self.config.sample_rate
        # 生成150ms静音，保证音频链路可执行
        audio = np.zeros(int(sample_rate * 0.15), dtype=np.int16)
        return SynthesisResult(
            audio=audio,
            sample_rate=sample_rate,
            text=text,
            duration=len(audio) / sample_rate,
        )

    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        yield self.synthesize(text).audio


class PiperBackend(TTSBackend):
    """
    Piper TTS后端
    轻量级、低延迟的TTS引擎
    """
    
    def __init__(self, config: TTSConfig):
        self.config = config
        self._voice = None
        self._load_model()
    
    def _load_model(self):
        """加载模型"""
        try:
            from piper import PiperVoice
            
            model_path = self.config.model_path
            if not model_path:
                # 使用默认模型路径
                model_name = self.config.model_name or self._get_default_model()
                model_path = settings.models_dir / "tts" / model_name
            
            if not Path(model_path).exists():
                logger.warning(f"模型不存在: {model_path}，将自动下载")
            
            logger.info(f"加载Piper模型: {model_path}")
            self._voice = PiperVoice.load(str(model_path))
            
            logger.info("Piper模型加载完成")
            
        except ImportError:
            raise ImportError("请安装piper-tts: pip install piper-tts")

    @staticmethod
    def _to_int16_audio(audio_obj) -> np.ndarray:
        """兼容不同 Piper 版本输出，统一转为 int16"""
        if audio_obj is None:
            return np.array([], dtype=np.int16)

        if isinstance(audio_obj, np.ndarray):
            audio = audio_obj
        else:
            audio = np.array(audio_obj)

        if audio.dtype in (np.float32, np.float64):
            max_val = np.max(np.abs(audio)) if audio.size else 0.0
            if max_val > 0:
                audio = audio / max_val
            return (audio * 32767).astype(np.int16)

        return audio.astype(np.int16, copy=False)

    def _chunk_to_int16(self, chunk) -> tuple[np.ndarray, Optional[int]]:
        """从 Piper chunk 提取音频和采样率"""
        sample_rate = getattr(chunk, "sample_rate", None)

        # piper-tts 1.4.x: AudioChunk.audio_int16_array/audio_float_array
        if hasattr(chunk, "audio_int16_array"):
            audio = self._to_int16_audio(chunk.audio_int16_array)
            if audio.size:
                return audio, sample_rate

        if hasattr(chunk, "audio_float_array"):
            audio = self._to_int16_audio(chunk.audio_float_array)
            if audio.size:
                return audio, sample_rate

        # 兼容旧版直接返回 ndarray/tuple/iterable
        if isinstance(chunk, tuple) and len(chunk) > 0:
            audio = self._to_int16_audio(chunk[0])
            return audio, sample_rate

        audio = self._to_int16_audio(chunk)
        return audio, sample_rate
    
    def _get_default_model(self) -> str:
        """获取默认模型"""
        lang_models = {
            "zh": "zh_CN-huayan-medium.onnx",
            "en": "en_US-lessac-medium.onnx",
            "ja": "ja_JP-average.onnx",
            "ko": "ko_KR-kss-medium.onnx",
        }
        return lang_models.get(self.config.language, lang_models["en"])
    
    def synthesize(self, text: str) -> SynthesisResult:
        """合成语音"""
        start_time = time.time()

        synthesized = self._voice.synthesize(text)
        sample_rate = self.config.sample_rate
        chunks = []

        if isinstance(synthesized, (np.ndarray, tuple, list)):
            audio = self._to_int16_audio(synthesized[0] if isinstance(synthesized, tuple) else synthesized)
        else:
            for chunk in synthesized:
                chunk_audio, chunk_sr = self._chunk_to_int16(chunk)
                if chunk_audio.size:
                    chunks.append(chunk_audio)
                if chunk_sr:
                    sample_rate = int(chunk_sr)

            audio = np.concatenate(chunks) if chunks else np.array([], dtype=np.int16)

        duration = len(audio) / sample_rate if sample_rate > 0 else 0.0
        
        logger.debug(f"TTS合成完成: {len(text)}字符, {duration:.2f}s, 耗时{time.time()-start_time:.2f}s")
        
        return SynthesisResult(
            audio=audio,
            sample_rate=sample_rate,
            text=text,
            duration=duration,
        )
    
    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        """流式合成"""
        for chunk in self._voice.synthesize(text):
            audio, _ = self._chunk_to_int16(chunk)
            if audio.size:
                yield audio

    def close(self) -> None:
        self._voice = None


class CoquiTTSBackend(TTSBackend):
    """
    Coqui TTS后端
    高质量的多语言TTS引擎
    """
    
    def __init__(self, config: TTSConfig):
        self.config = config
        self._tts = None
        self._load_model()
    
    def _load_model(self):
        """加载模型"""
        try:
            from TTS.api import TTS
            
            model_name = self.config.model_name or self._get_default_model()
            
            logger.info(f"加载Coqui TTS模型: {model_name}")
            
            self._tts = TTS(
                model_name=model_name,
                progress_bar=False,
            )
            
            if self.config.device != "cpu":
                self._tts.to(self.config.device)
            
            logger.info("Coqui TTS模型加载完成")
            
        except ImportError:
            raise ImportError("请安装coqui-tts: pip install coqui-tts")
    
    def _get_default_model(self) -> str:
        """获取默认模型"""
        return "tts_models/multilingual/multi-dataset/xtts_v2"
    
    def synthesize(self, text: str) -> SynthesisResult:
        """合成语音"""
        start_time = time.time()
        
        audio = self._tts.tts(
            text=text,
            speaker=self.config.speaker,
            language=self.config.language,
            speed=self.config.speed,
        )
        
        audio = np.array(audio, dtype=np.float32)
        
        # 归一化并转换
        max_val = np.max(np.abs(audio))
        if max_val > 0:
            audio = audio / max_val
        audio = (audio * 32767).astype(np.int16)
        
        duration = len(audio) / self.config.sample_rate
        
        logger.debug(f"TTS合成完成: {len(text)}字符, {duration:.2f}s, 耗时{time.time()-start_time:.2f}s")
        
        return SynthesisResult(
            audio=audio,
            sample_rate=self.config.sample_rate,
            text=text,
            duration=duration,
        )
    
    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        """流式合成 - 按句子分割"""
        import re
        
        # 按句子分割
        sentences = re.split(r'(?<=[。！？.!?])\s*', text)
        
        for sentence in sentences:
            if sentence.strip():
                result = self.synthesize(sentence)
                yield result.audio

    def close(self) -> None:
        self._tts = None


class EdgeTTSBackend(TTSBackend):
    """
    Edge TTS后端
    微软Edge的在线TTS，需要网络但质量高
    """
    
    def __init__(self, config: TTSConfig):
        self.config = config
        self._voice = None
        self._load_model()
    
    def _load_model(self):
        """加载配置"""
        try:
            import edge_tts
            
            # 默认语音
            self._voice = self.config.model_name or self._get_default_voice()
            logger.info(f"Edge TTS配置完成: voice={self._voice}")
            
        except ImportError:
            raise ImportError("请安装edge-tts: pip install edge-tts")
    
    def _get_default_voice(self) -> str:
        """获取默认语音"""
        voices = {
            "zh": "zh-CN-XiaoxiaoNeural",
            "en": "en-US-AriaNeural",
            "ja": "ja-JP-NanamiNeural",
            "ko": "ko-KR-SunHiNeural",
        }
        return voices.get(self.config.language, voices["en"])
    
    async def _synthesize_async(self, text: str) -> SynthesisResult:
        """异步合成"""
        import edge_tts
        import io
        
        start_time = time.time()
        
        communicate = edge_tts.Communicate(text, self._voice)
        
        audio_data = io.BytesIO()
        async for chunk in communicate.stream():
            if chunk["type"] == "audio":
                audio_data.write(chunk["data"])
        
        audio_data.seek(0)
        
        # 使用soundfile读取
        import soundfile as sf
        audio, sr = sf.read(audio_data)
        
        # 转换为int16
        if audio.dtype == np.float32 or audio.dtype == np.float64:
            max_val = np.max(np.abs(audio))
            if max_val > 0:
                audio = audio / max_val
            audio = (audio * 32767).astype(np.int16)
        
        duration = len(audio) / sr
        
        logger.debug(f"Edge TTS合成完成: {len(text)}字符, {duration:.2f}s")
        
        return SynthesisResult(
            audio=audio,
            sample_rate=sr,
            text=text,
            duration=duration,
        )
    
    def synthesize(self, text: str) -> SynthesisResult:
        """合成语音"""
        import asyncio
        
        try:
            loop = asyncio.get_event_loop()
        except RuntimeError:
            loop = asyncio.new_event_loop()
            asyncio.set_event_loop(loop)
        
        return loop.run_until_complete(self._synthesize_async(text))
    
    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        """流式合成"""
        import re
        
        sentences = re.split(r'(?<=[。！？.!?])\s*', text)
        
        for sentence in sentences:
            if sentence.strip():
                result = self.synthesize(sentence)
                yield result.audio


class Pyttsx3Backend(TTSBackend):
    """pyttsx3本地系统语音后端（Windows/macOS/Linux）"""

    def __init__(self, config: TTSConfig):
        self.config = config
        self._engine = None
        self._load_model()

    def _load_model(self):
        try:
            import pyttsx3
        except ImportError as exc:
            raise ImportError("请安装pyttsx3: pip install pyttsx3") from exc

        self._engine = pyttsx3.init()
        self._apply_voice_preferences()
        logger.info("pyttsx3后端初始化完成")

    def _apply_voice_preferences(self) -> None:
        # 语速映射到系统语音速率
        base_rate = int(self._engine.getProperty("rate"))
        target_rate = max(80, min(300, int(base_rate * self.config.speed)))
        self._engine.setProperty("rate", target_rate)

        if self.config.speaker:
            for voice in self._engine.getProperty("voices"):
                if self.config.speaker.lower() in voice.id.lower():
                    self._engine.setProperty("voice", voice.id)
                    return

        lang = self.config.language.lower()
        for voice in self._engine.getProperty("voices"):
            blob = f"{voice.id} {getattr(voice, 'name', '')}".lower()
            if lang in blob:
                self._engine.setProperty("voice", voice.id)
                return
            if lang == "zh" and ("chinese" in blob or "zh" in blob):
                self._engine.setProperty("voice", voice.id)
                return

    def synthesize(self, text: str) -> SynthesisResult:
        import soundfile as sf

        # pyttsx3需要落地到文件再读取
        with tempfile.NamedTemporaryFile(suffix=".wav", delete=False) as tmp:
            tmp_path = Path(tmp.name)

        try:
            start_time = time.time()
            self._engine.save_to_file(text, str(tmp_path))
            self._engine.runAndWait()

            audio, sample_rate = sf.read(str(tmp_path), dtype="float32")
            if audio.ndim > 1:
                audio = np.mean(audio, axis=1)
            audio = np.clip(audio, -1.0, 1.0)
            audio_int16 = (audio * 32767).astype(np.int16)
            duration = len(audio_int16) / sample_rate if sample_rate > 0 else 0.0

            logger.debug(
                f"pyttsx3合成完成: {len(text)}字符, {duration:.2f}s, 耗时{time.time()-start_time:.2f}s"
            )
            return SynthesisResult(
                audio=audio_int16,
                sample_rate=sample_rate,
                text=text,
                duration=duration,
            )
        finally:
            if tmp_path.exists():
                tmp_path.unlink()

    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        yield self.synthesize(text).audio

    def close(self) -> None:
        if not self._engine:
            return
        try:
            self._engine.stop()
        except Exception:
            pass
        self._engine = None


class TTSEngine:
    """
    TTS引擎封装
    统一的语音合成接口
    """
    
    BACKENDS = {
        "piper": PiperBackend,
        "coqui": CoquiTTSBackend,
        "edge-tts": EdgeTTSBackend,
        "pyttsx3": Pyttsx3Backend,
    }
    
    def __init__(self, config: Optional[TTSConfig] = None):
        self.config = config or settings.tts
        self._backend: Optional[TTSBackend] = None
        self._load_backend()
    
    def _load_backend(self):
        """加载后端"""
        backend_class = self.BACKENDS.get(self.config.engine)
        if not backend_class:
            raise ValueError(f"不支持的TTS后端: {self.config.engine}")

        try:
            self._backend = backend_class(self.config)
            logger.info(f"TTS引擎初始化完成: {self.config.engine}")
        except Exception as exc:
            logger.warning(f"TTS后端加载失败，自动降级到Fallback: {exc}")
            self._backend = FallbackTTSBackend(self.config, reason=str(exc))

    def _retry_text_candidates(self, text: str) -> list[str]:
        raw = (text or "").strip()
        if not raw:
            return []

        candidates: list[str] = [raw]
        # 去掉表情等非常规字符，减少后端返回空音频概率。
        sanitized = re.sub(r"[^\w\s\u4e00-\u9fff.,!?;:，。！？；：'\"]+", "", raw).strip()
        if sanitized and sanitized not in candidates:
            candidates.append(sanitized)

        zh_like = (self.config.language or "").lower().startswith("zh")
        punct = "。" if zh_like else "."
        with_punct = sanitized or raw
        if with_punct and with_punct[-1] not in ".!?。！？":
            with_punct = f"{with_punct}{punct}"
            if with_punct not in candidates:
                candidates.append(with_punct)
        return candidates
    
    def synthesize(self, text: str) -> SynthesisResult:
        """合成语音"""
        candidates = self._retry_text_candidates(text)
        if not candidates:
            return self._backend.synthesize(text)

        last_result: Optional[SynthesisResult] = None
        for idx, candidate in enumerate(candidates):
            result = self._backend.synthesize(candidate)
            last_result = result
            if result.audio is not None and len(result.audio) > 0:
                if idx > 0:
                    logger.warning(f"TTS空音频已通过重试恢复: attempt={idx + 1}")
                return result

        # Piper 等后端在个别环境可能持续返回空音频；尝试 pyttsx3 应急可听输出。
        if str(self.config.engine or "").lower() != "pyttsx3":
            try:
                emergency_cfg = self.config.model_copy(deep=True)
                emergency_cfg.engine = "pyttsx3"
                emergency_backend = Pyttsx3Backend(emergency_cfg)
                emergency_result = emergency_backend.synthesize(str(text or ""))
                emergency_backend.close()
                if emergency_result.audio is not None and len(emergency_result.audio) > 0:
                    logger.warning("TTS空音频已回退到pyttsx3应急输出")
                    return emergency_result
            except Exception as exc:
                logger.warning(f"TTS应急回退pyttsx3失败: {exc}")

        logger.warning("TTS连续返回空音频，回退到静音后端保障链路稳定")
        fallback = FallbackTTSBackend(self.config, reason="empty-audio-retry-exhausted")
        return fallback.synthesize(text if isinstance(text, str) else "")
    
    def synthesize_stream(self, text: str) -> Generator[np.ndarray, None, None]:
        """流式合成"""
        yield from self._backend.synthesize_stream(text)
    
    def speak(self, text: str, blocking: bool = False) -> None:
        """直接播放语音"""
        import sounddevice as sd
        
        result = self.synthesize(text)
        
        sd.play(result.audio, samplerate=result.sample_rate)
        
        if blocking:
            sd.wait()

    def close(self) -> None:
        if not self._backend:
            return
        try:
            self._backend.close()
        except Exception as exc:
            logger.warning(f"TTS资源释放异常: {exc}")
