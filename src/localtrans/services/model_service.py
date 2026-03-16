"""
ModelService - 模型管理服务

负责管理 AI 模型的状态、下载、路径和版本。
"""

import threading
from typing import Optional, Dict, List, Any, Callable
from pathlib import Path
from dataclasses import dataclass, field
from datetime import datetime
from enum import Enum

from loguru import logger

from localtrans.config import settings


class ModelType(Enum):
    """模型类型"""
    ASR = "asr"
    MT = "mt"
    TTS = "tts"
    LOCI = "loci"


class ModelState(Enum):
    """模型状态"""
    NOT_DOWNLOADED = "not_downloaded"
    DOWNLOADING = "downloading"
    READY = "ready"
    LOADING = "loading"
    LOADED = "loaded"
    ERROR = "error"


@dataclass
class ModelInfo:
    """模型信息"""
    id: str
    name: str
    type: ModelType
    state: ModelState = ModelState.NOT_DOWNLOADED
    size_bytes: int = 0
    path: Optional[Path] = None
    version: str = "1.0.0"
    description: str = ""
    download_url: Optional[str] = None
    progress: float = 0.0  # 下载进度 0-100
    error_message: str = ""
    
    @property
    def size_mb(self) -> float:
        return self.size_bytes / (1024 * 1024)


class ModelService:
    """
    模型管理服务
    
    职责：
    - 管理模型状态
    - 模型下载与缓存
    - 模型路径解析
    - 模型版本管理
    """
    
    _instance: Optional["ModelService"] = None
    _lock = threading.Lock()
    
    # 可用模型清单
    AVAILABLE_MODELS: Dict[str, Dict[str, Any]] = {
        # ASR 模型
        "whisper-tiny": {
            "name": "Whisper Tiny",
            "type": ModelType.ASR,
            "size_mb": 75,
            "url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        },
        "whisper-base": {
            "name": "Whisper Base",
            "type": ModelType.ASR,
            "size_mb": 142,
            "url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin",
        },
        "whisper-small": {
            "name": "Whisper Small",
            "type": ModelType.ASR,
            "size_mb": 466,
            "url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin",
        },
        "faster-whisper-small": {
            "name": "Faster Whisper Small",
            "type": ModelType.ASR,
            "size_mb": 500,
        },
        "faster-whisper-medium": {
            "name": "Faster Whisper Medium",
            "type": ModelType.ASR,
            "size_mb": 1500,
        },
        "vosk-small": {
            "name": "Vosk Small",
            "type": ModelType.ASR,
            "size_mb": 50,
        },
        # MT 模型
        "argos-zh-en": {
            "name": "Argos 中文→英文",
            "type": ModelType.MT,
            "size_mb": 300,
        },
        "argos-en-zh": {
            "name": "Argos 英文→中文",
            "type": ModelType.MT,
            "size_mb": 300,
        },
        "nllb-200": {
            "name": "NLLB-200",
            "type": ModelType.MT,
            "size_mb": 2500,
        },
        # TTS 模型
        "piper-en": {
            "name": "Piper English",
            "type": ModelType.TTS,
            "size_mb": 60,
        },
        "piper-zh": {
            "name": "Piper Chinese",
            "type": ModelType.TTS,
            "size_mb": 80,
        },
        # Loci 模型
        "loci-phi3-mini": {
            "name": "Phi-3 Mini (Loci)",
            "type": ModelType.LOCI,
            "size_mb": 2300,
        },
        "loci-llama3-8b": {
            "name": "Llama-3 8B (Loci)",
            "type": ModelType.LOCI,
            "size_mb": 4700,
        },
    }
    
    def __new__(cls) -> "ModelService":
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = super().__new__(cls)
                    cls._instance._initialized = False
        return cls._instance
    
    def __init__(self):
        if self._initialized:
            return
        
        self._models: Dict[str, ModelInfo] = {}
        self._download_callbacks: Dict[str, Callable[[float], None]] = {}
        
        # 初始化模型清单
        self._init_model_registry()
        
        self._initialized = True
    
    @classmethod
    def get_instance(cls) -> "ModelService":
        return cls()
    
    # === 模型查询 ===
    
    def get_model(self, model_id: str) -> Optional[ModelInfo]:
        """获取模型信息"""
        return self._models.get(model_id)
    
    def get_models_by_type(self, model_type: ModelType) -> List[ModelInfo]:
        """按类型获取模型"""
        return [m for m in self._models.values() if m.type == model_type]
    
    def get_all_models(self) -> List[ModelInfo]:
        """获取所有模型"""
        return list(self._models.values())
    
    def get_ready_models(self) -> List[ModelInfo]:
        """获取已就绪的模型"""
        return [m for m in self._models.values() if m.state == ModelState.READY]
    
    # === 模型状态 ===
    
    def is_model_ready(self, model_id: str) -> bool:
        """检查模型是否就绪"""
        model = self._models.get(model_id)
        return model is not None and model.state in (ModelState.READY, ModelState.LOADED)
    
    def get_model_path(self, model_id: str) -> Optional[Path]:
        """获取模型路径"""
        model = self._models.get(model_id)
        if model and model.path:
            return model.path
        
        # 尝试解析默认路径
        model_info = self.AVAILABLE_MODELS.get(model_id)
        if model_info:
            model_type = model_info["type"]
            if model_type == ModelType.ASR:
                return settings.models_dir / "asr" / model_id
            elif model_type == ModelType.MT:
                return settings.models_dir / "mt" / model_id
            elif model_type == ModelType.TTS:
                return settings.models_dir / "tts" / model_id
            elif model_type == ModelType.LOCI:
                return settings.models_dir / "loci" / f"{model_id}.gguf"
        
        return None
    
    # === 模型下载 ===
    
    def download_model(
        self,
        model_id: str,
        on_progress: Optional[Callable[[float], None]] = None,
    ) -> bool:
        """下载模型"""
        model = self._models.get(model_id)
        if not model:
            logger.error(f"未知模型: {model_id}")
            return False
        
        if model.state == ModelState.READY:
            logger.info(f"模型已就绪: {model_id}")
            return True
        
        if model.state == ModelState.DOWNLOADING:
            logger.warning(f"模型正在下载: {model_id}")
            return False
        
        try:
            model.state = ModelState.DOWNLOADING
            model.progress = 0.0
            
            if on_progress:
                self._download_callbacks[model_id] = on_progress
            
            # 使用模型下载器
            from localtrans.utils.model_downloader import ModelDownloader
            
            downloader = ModelDownloader()
            target_path = self.get_model_path(model_id)
            
            if not target_path:
                raise ValueError(f"无法确定模型路径: {model_id}")
            
            def progress_callback(progress: float):
                model.progress = progress * 100
                if model_id in self._download_callbacks:
                    self._download_callbacks[model_id](progress)
            
            # 执行下载
            success = downloader.download(
                model_id=model_id,
                target_path=target_path,
                on_progress=progress_callback,
            )
            
            if success:
                model.state = ModelState.READY
                model.path = target_path
                logger.info(f"模型下载完成: {model_id}")
            else:
                model.state = ModelState.ERROR
                model.error_message = "下载失败"
            
            return success
            
        except Exception as e:
            logger.exception(f"下载模型失败: {e}")
            model.state = ModelState.ERROR
            model.error_message = str(e)
            return False
        finally:
            if model_id in self._download_callbacks:
                del self._download_callbacks[model_id]
    
    def cancel_download(self, model_id: str) -> bool:
        """取消下载"""
        model = self._models.get(model_id)
        if model and model.state == ModelState.DOWNLOADING:
            model.state = ModelState.NOT_DOWNLOADED
            model.progress = 0.0
            if model_id in self._download_callbacks:
                del self._download_callbacks[model_id]
            logger.info(f"已取消下载: {model_id}")
            return True
        return False
    
    # === 模型删除 ===
    
    def delete_model(self, model_id: str) -> bool:
        """删除模型"""
        model = self._models.get(model_id)
        if not model:
            return False
        
        if model.state == ModelState.LOADED:
            logger.warning(f"模型正在使用，无法删除: {model_id}")
            return False
        
        try:
            if model.path and model.path.exists():
                import shutil
                if model.path.is_dir():
                    shutil.rmtree(model.path)
                else:
                    model.path.unlink()
            
            model.state = ModelState.NOT_DOWNLOADED
            model.path = None
            model.progress = 0.0
            
            logger.info(f"模型已删除: {model_id}")
            return True
            
        except Exception as e:
            logger.exception(f"删除模型失败: {e}")
            return False
    
    # === 统计 ===
    
    def get_storage_usage(self) -> Dict[str, Any]:
        """获取存储使用情况"""
        total_bytes = 0
        by_type: Dict[ModelType, int] = {}
        
        for model in self._models.values():
            if model.state == ModelState.READY and model.size_bytes > 0:
                total_bytes += model.size_bytes
                by_type[model.type] = by_type.get(model.type, 0) + model.size_bytes
        
        return {
            "total_bytes": total_bytes,
            "total_mb": total_bytes / (1024 * 1024),
            "by_type": {t.value: s for t, s in by_type.items()},
        }
    
    # === 内部方法 ===
    
    def _init_model_registry(self) -> None:
        """初始化模型注册表"""
        for model_id, info in self.AVAILABLE_MODELS.items():
            model = ModelInfo(
                id=model_id,
                name=info["name"],
                type=info["type"],
                size_bytes=int(info.get("size_mb", 0) * 1024 * 1024),
                version=info.get("version", "1.0.0"),
                description=info.get("description", ""),
                download_url=info.get("url"),
            )
            
            # 检查是否已下载
            path = self.get_model_path(model_id)
            if path and path.exists():
                model.state = ModelState.READY
                model.path = path
            
            self._models[model_id] = model
