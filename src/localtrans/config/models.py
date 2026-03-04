"""
配置数据模型
定义各模块的配置结构
"""

from pathlib import Path
from typing import Dict, List, Optional
from pydantic import BaseModel, Field


class LanguagePair(BaseModel):
    """语言对配置"""
    source: str = Field(default="zh", description="源语言代码")
    target: str = Field(default="en", description="目标语言代码")
    
    class Config:
        frozen = True


class AudioConfig(BaseModel):
    """音频配置"""
    sample_rate: int = Field(default=16000, description="采样率")
    channels: int = Field(default=1, description="声道数")
    chunk_size: int = Field(default=1024, description="音频块大小")
    format: str = Field(default="int16", description="音频格式")
    
    # 虚拟设备配置
    virtual_input_device: Optional[str] = Field(default=None, description="虚拟输入设备名称")
    virtual_output_device: Optional[str] = Field(default=None, description="虚拟输出设备名称")
    
    # WASAPI配置
    wasapi_exclusive: bool = Field(default=False, description="WASAPI独占模式")


class ASRConfig(BaseModel):
    """ASR语音识别配置"""
    # 模型配置
    model_type: str = Field(
        default="vosk",
        description="模型类型: vosk, whisper, faster-whisper, funasr, sherpa-onnx",
    )
    model_size: str = Field(default="base", description="模型大小: tiny, base, small, medium, large")
    model_name: Optional[str] = Field(default=None, description="模型名称（可覆盖model_size）")
    model_path: Optional[Path] = Field(default=None, description="本地模型路径")
    
    # 识别配置
    language: Optional[str] = Field(default=None, description="强制指定语言，None为自动检测")
    task: str = Field(default="transcribe", description="任务类型: transcribe, translate")
    
    # 性能配置
    device: str = Field(default="auto", description="推理设备: auto, cpu, cuda")
    compute_type: str = Field(default="int8", description="计算精度: float16, int8, int8_float16")
    
    # 流式配置
    beam_size: int = Field(default=1, description="beam search大小")
    vad_filter: bool = Field(default=False, description="启用VAD静音过滤")
    word_timestamps: bool = Field(default=False, description="生成词级时间戳")


class MTConfig(BaseModel):
    """MT机器翻译配置"""
    # 模型配置
    model_type: str = Field(default="argos-ct2", description="模型类型: argos-ct2, argos, nllb, nllb-ct2, marian")
    model_name: str = Field(default="argos-zh-en", description="模型名称")
    model_path: Optional[Path] = Field(default=None, description="本地模型路径")
    
    # 翻译配置
    source_lang: str = Field(default="zh", description="源语言代码")
    target_lang: str = Field(default="en", description="目标语言代码")
    
    # 性能配置
    device: str = Field(default="auto", description="推理设备: auto, cpu, cuda")
    compute_type: str = Field(default="int8", description="计算精度")
    max_length: int = Field(default=512, description="最大序列长度")
    
    # 术语库
    term_bank_enabled: bool = Field(default=True, description="启用术语库")
    term_bank_path: Optional[Path] = Field(default=None, description="术语库路径")


class TTSConfig(BaseModel):
    """TTS语音合成配置"""
    # 引擎配置
    engine: str = Field(default="pyttsx3", description="TTS引擎: pyttsx3, piper, coqui, edge-tts")
    model_name: Optional[str] = Field(default=None, description="模型名称")
    model_path: Optional[Path] = Field(default=None, description="本地模型路径")
    
    # 音频配置
    sample_rate: int = Field(default=22050, description="输出采样率")
    
    # 语音配置
    language: str = Field(default="zh", description="语言")
    speaker: Optional[str] = Field(default=None, description="说话人")
    speed: float = Field(default=1.0, description="语速")
    
    # 性能配置
    device: str = Field(default="cpu", description="推理设备")
    
    # 流式配置
    stream_enabled: bool = Field(default=True, description="启用流式合成")


class TermBankEntry(BaseModel):
    """术语库条目"""
    source: str = Field(description="源语言术语")
    target: str = Field(description="目标语言翻译")
    context: Optional[str] = Field(default=None, description="上下文说明")
    category: Optional[str] = Field(default=None, description="分类标签")


class TermBank(BaseModel):
    """术语库"""
    name: str = Field(default="default", description="术语库名称")
    source_lang: str = Field(description="源语言")
    target_lang: str = Field(description="目标语言")
    entries: Dict[str, TermBankEntry] = Field(default_factory=dict, description="术语条目")
    
    def add_term(self, source: str, target: str, context: str = None, category: str = None) -> None:
        """添加术语"""
        self.entries[source] = TermBankEntry(
            source=source,
            target=target,
            context=context,
            category=category
        )
    
    def lookup(self, text: str) -> Optional[TermBankEntry]:
        """查找术语"""
        return self.entries.get(text)
    
    def apply_translation(self, text: str) -> str:
        """应用术语翻译"""
        for source, entry in self.entries.items():
            if source in text:
                text = text.replace(source, entry.target)
        return text
