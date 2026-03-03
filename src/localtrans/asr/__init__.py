"""ASR语音识别模块"""

from localtrans.asr.engine import ASREngine, TranscriptionResult
from localtrans.asr.stream import StreamingASR

__all__ = ["ASREngine", "StreamingASR", "TranscriptionResult"]
