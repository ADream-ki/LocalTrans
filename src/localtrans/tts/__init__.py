"""TTS语音合成模块"""

from localtrans.tts.engine import TTSEngine, SynthesisResult
from localtrans.tts.stream import StreamingTTS

__all__ = ["TTSEngine", "StreamingTTS", "SynthesisResult"]
