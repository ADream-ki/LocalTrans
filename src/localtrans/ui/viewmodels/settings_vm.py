"""
SettingsViewModel - 设置视图模型

管理应用程序配置。
"""

from typing import Optional, List, Dict
from pathlib import Path

from PySide6.QtCore import QObject, Signal, Property, Slot

from loguru import logger

from localtrans.config import settings, MTConfig, ASRConfig, TTSConfig


class SettingsViewModel(QObject):
    """
    设置视图模型
    
    提供配置项的读写接口。
    """
    
    # === Signals ===
    asrBackendChanged = Signal()
    mtBackendChanged = Signal()
    ttsBackendChanged = Signal()
    sourceLangChanged = Signal()
    targetLangChanged = Signal()
    settingsSaved = Signal()
    settingsReset = Signal()
    
    # 语言选项
    LANGUAGES = [
        ("English", "en"),
        ("中文", "zh"),
        ("日本語", "ja"),
        ("한국어", "ko"),
        ("Français", "fr"),
        ("Deutsch", "de"),
        ("Español", "es"),
        ("Русский", "ru"),
        ("Português", "pt"),
        ("Italiano", "it"),
        ("العربية", "ar"),
    ]
    
    # ASR 后端选项
    ASR_BACKENDS = [
        ("Faster Whisper (推荐)", "faster-whisper"),
        ("Whisper", "whisper"),
        ("Vosk (离线)", "vosk"),
        ("FunASR (中文优化)", "funasr"),
        ("Sherpa-ONNX", "sherpa-onnx"),
    ]
    
    # MT 后端选项
    MT_BACKENDS = [
        ("Argos-CT2 (快速)", "argos-ct2"),
        ("Argos Translate", "argos"),
        ("NLLB", "nllb"),
        ("NLLB-CT2", "nllb-ct2"),
        ("MarianMT", "marian"),
        ("Loci (LLM)", "loci"),
        ("Loci 增强翻译", "loci-enhanced"),
    ]
    
    # TTS 后端选项
    TTS_BACKENDS = [
        ("Pyttsx3 (系统语音)", "pyttsx3"),
        ("Piper (高质量)", "piper"),
        ("Coqui TTS", "coqui"),
        ("Edge TTS (在线)", "edge-tts"),
    ]
    
    def __init__(self, parent: Optional[QObject] = None):
        super().__init__(parent)
        self._load_settings()
    
    def _load_settings(self):
        """加载设置"""
        self._asr_backend = settings.asr.model_type
        self._mt_backend = settings.mt.model_type
        self._tts_engine = settings.tts.engine
        self._source_lang = settings.mt.source_lang
        self._target_lang = settings.mt.target_lang
        
        # Loci 设置
        self._loci_model_path = str(settings.mt.loci_model_path) if settings.mt.loci_model_path else ""
        self._loci_timeout = settings.mt.loci_timeout_ms
    
    # === Properties ===
    
    @Property(str, notify=asrBackendChanged)
    def asrBackend(self) -> str:
        return self._asr_backend
    
    @asrBackend.setter
    def asrBackend(self, value: str):
        if self._asr_backend != value:
            self._asr_backend = value
            self.asrBackendChanged.emit()
    
    @Property(str, notify=mtBackendChanged)
    def mtBackend(self) -> str:
        return self._mt_backend
    
    @mtBackend.setter
    def mtBackend(self, value: str):
        if self._mt_backend != value:
            self._mt_backend = value
            self.mtBackendChanged.emit()
    
    @Property(str, notify=ttsBackendChanged)
    def ttsEngine(self) -> str:
        return self._tts_engine
    
    @ttsEngine.setter
    def ttsEngine(self, value: str):
        if self._tts_engine != value:
            self._tts_engine = value
            self.ttsBackendChanged.emit()
    
    @Property(str, notify=sourceLangChanged)
    def sourceLang(self) -> str:
        return self._source_lang
    
    @sourceLang.setter
    def sourceLang(self, value: str):
        if self._source_lang != value:
            self._source_lang = value
            self.sourceLangChanged.emit()
    
    @Property(str, notify=targetLangChanged)
    def targetLang(self) -> str:
        return self._target_lang
    
    @targetLang.setter
    def targetLang(self, value: str):
        if self._target_lang != value:
            self._target_lang = value
            self.targetLangChanged.emit()
    
    @Property(str)
    def lociModelPath(self) -> str:
        return self._loci_model_path
    
    @lociModelPath.setter
    def lociModelPath(self, value: str):
        self._loci_model_path = value
    
    @Property(int)
    def lociTimeout(self) -> int:
        return self._loci_timeout
    
    @lociTimeout.setter
    def lociTimeout(self, value: int):
        self._loci_timeout = value
    
    # === 数据提供 ===
    
    @Slot(result=list)
    def getLanguages(self) -> List[Dict]:
        """获取语言列表"""
        return [{"name": name, "code": code} for name, code in self.LANGUAGES]
    
    @Slot(result=list)
    def getASRBackends(self) -> List[Dict]:
        """获取 ASR 后端列表"""
        return [{"name": name, "value": value} for name, value in self.ASR_BACKENDS]
    
    @Slot(result=list)
    def getMTBackends(self) -> List[Dict]:
        """获取 MT 后端列表"""
        return [{"name": name, "value": value} for name, value in self.MT_BACKENDS]
    
    @Slot(result=list)
    def getTTSBackends(self) -> List[Dict]:
        """获取 TTS 后端列表"""
        return [{"name": name, "value": value} for name, value in self.TTS_BACKENDS]
    
    # === 命令 ===
    
    @Slot()
    def saveSettings(self):
        """保存设置"""
        # 更新配置
        settings.asr.model_type = self._asr_backend
        settings.mt.model_type = self._mt_backend
        settings.mt.source_lang = self._source_lang
        settings.mt.target_lang = self._target_lang
        settings.tts.engine = self._tts_engine
        
        # Loci 配置
        if self._loci_model_path:
            settings.mt.loci_model_path = Path(self._loci_model_path)
        settings.mt.loci_timeout_ms = self._loci_timeout
        
        # 保存到文件
        settings.save()
        
        self.settingsSaved.emit()
        logger.info("设置已保存")
    
    @Slot()
    def resetSettings(self):
        """重置为默认设置"""
        settings.reset()
        self._load_settings()
        
        # 发射所有变更信号
        self.asrBackendChanged.emit()
        self.mtBackendChanged.emit()
        self.ttsBackendChanged.emit()
        self.sourceLangChanged.emit()
        self.targetLangChanged.emit()
        
        self.settingsReset.emit()
        logger.info("设置已重置")
    
    @Slot(str, result=bool)
    def setLociModelPath(self, path: str) -> bool:
        """设置 Loci 模型路径"""
        if path and Path(path).exists():
            self._loci_model_path = path
            return True
        return False
