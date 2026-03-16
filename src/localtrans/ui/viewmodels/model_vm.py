"""
ModelViewModel - 模型管理视图模型

管理 AI 模型的下载、状态和配置。
"""

from typing import Optional, List, Dict
from pathlib import Path
from enum import Enum

from PySide6.QtCore import QObject, Signal, Property, Slot, QThread

from loguru import logger
from localtrans.config import settings


class ModelStatus(Enum):
    """模型状态"""
    NOT_DOWNLOADED = "not_downloaded"
    DOWNLOADING = "downloading"
    READY = "ready"
    ERROR = "error"


class ModelViewModel(QObject):
    """
    模型管理视图模型
    
    提供：
    - 模型列表和状态
    - 下载进度
    - 模型路径配置
    """
    
    # === Signals ===
    modelsChanged = Signal()
    downloadProgressChanged = Signal(str, float)  # model_name, progress
    downloadCompleted = Signal(str)
    downloadFailed = Signal(str, str)  # model_name, error
    modelStatusChanged = Signal(str, str)  # model_name, status
    
    def __init__(self, parent: Optional[QObject] = None):
        super().__init__(parent)
        
        self._models: Dict[str, Dict] = {}
        self._download_progress: Dict[str, float] = {}
        self._model_dir = Path(settings.models_dir)
        
        self._refresh_models()
    
    def _refresh_models(self):
        """刷新模型列表"""
        # ASR 模型
        self._models["asr"] = self._get_asr_models()
        
        # MT 模型
        self._models["mt"] = self._get_mt_models()
        
        # TTS 模型
        self._models["tts"] = self._get_tts_models()
        
        # Loci 模型
        self._models["loci"] = self._get_loci_models()
        
        self.modelsChanged.emit()
    
    def _get_asr_models(self) -> List[Dict]:
        """获取 ASR 模型列表"""
        return [
            {
                "name": "faster-whisper-small",
                "type": "asr",
                "size": "500MB",
                "status": self._check_model_status("faster-whisper-small"),
            },
            {
                "name": "faster-whisper-medium",
                "type": "asr",
                "size": "1.5GB",
                "status": self._check_model_status("faster-whisper-medium"),
            },
            {
                "name": "vosk-cn",
                "type": "asr",
                "size": "1.3GB",
                "status": self._check_model_status("vosk-cn"),
            },
        ]
    
    def _get_mt_models(self) -> List[Dict]:
        """获取 MT 模型列表"""
        return [
            {
                "name": "argos-zh-en",
                "type": "mt",
                "size": "300MB",
                "status": self._check_model_status("argos-zh-en"),
            },
            {
                "name": "argos-en-zh",
                "type": "mt",
                "size": "300MB",
                "status": self._check_model_status("argos-en-zh"),
            },
        ]
    
    def _get_tts_models(self) -> List[Dict]:
        """获取 TTS 模型列表"""
        return [
            {
                "name": "piper-zh_CN-huayan-medium",
                "type": "tts",
                "size": "60MB",
                "status": self._check_model_status("piper-zh_CN-huayan-medium"),
            },
        ]
    
    def _get_loci_models(self) -> List[Dict]:
        """获取 Loci 模型列表"""
        models_dir = self._model_dir / "loci"
        models = []
        
        if models_dir.exists():
            for gguf in models_dir.glob("*.gguf"):
                models.append({
                    "name": gguf.stem,
                    "path": str(gguf),
                    "type": "loci",
                    "size": f"{gguf.stat().st_size / (1024**3):.1f}GB",
                    "status": "ready",
                })
        
        return models
    
    def _check_model_status(self, model_name: str) -> str:
        """检查模型状态"""
        model_path = self._model_dir / model_name
        if model_path.exists():
            return ModelStatus.READY.value
        return ModelStatus.NOT_DOWNLOADED.value
    
    # === Properties ===
    
    @Property(list, notify=modelsChanged)
    def asrModels(self) -> List[Dict]:
        return self._models.get("asr", [])
    
    @Property(list, notify=modelsChanged)
    def mtModels(self) -> List[Dict]:
        return self._models.get("mt", [])
    
    @Property(list, notify=modelsChanged)
    def ttsModels(self) -> List[Dict]:
        return self._models.get("tts", [])
    
    @Property(list, notify=modelsChanged)
    def lociModels(self) -> List[Dict]:
        return self._models.get("loci", [])
    
    @Property(str)
    def modelDir(self) -> str:
        return str(self._model_dir)
    
    # === Slots ===
    
    @Slot(str)
    def downloadModel(self, model_name: str):
        """下载模型"""
        logger.info(f"开始下载模型: {model_name}")
        
        # TODO: 实现实际下载逻辑
        self._download_progress[model_name] = 0.0
        self.downloadProgressChanged.emit(model_name, 0.0)
    
    @Slot(str)
    def cancelDownload(self, model_name: str):
        """取消下载"""
        logger.info(f"取消下载: {model_name}")
        # TODO: 实现取消逻辑
    
    @Slot(str)
    def deleteModel(self, model_name: str):
        """删除模型"""
        model_path = self._model_dir / model_name
        gguf_path = self._model_dir / "loci" / f"{model_name}.gguf"
        if not model_path.exists() and gguf_path.exists():
            model_path = gguf_path
        if model_path.exists():
            import shutil
            if model_path.is_dir():
                shutil.rmtree(model_path)
            else:
                model_path.unlink()
            logger.info(f"已删除模型: {model_name}")
            self._refresh_models()
    
    @Slot(result=str)
    def selectLociModel(self) -> str:
        """选择 Loci 模型文件"""
        from PySide6.QtWidgets import QFileDialog
        
        file_path, _ = QFileDialog.getOpenFileName(
            None,
            "选择 Loci 模型文件",
            str(self._model_dir),
            "GGUF 模型 (*.gguf);;所有文件 (*.*)",
        )
        
        return file_path
    
    @Slot()
    def refreshModels(self):
        """刷新模型列表"""
        self._refresh_models()
    
    @Slot(str, result=float)
    def getDownloadProgress(self, model_name: str) -> float:
        """获取下载进度"""
        return self._download_progress.get(model_name, 0.0)
