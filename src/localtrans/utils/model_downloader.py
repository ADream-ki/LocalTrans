"""
模型下载与管理工具
自动下载和配置ASR、MT、TTS模型
"""

import os
import sys
import hashlib
import json
import zipfile
import time
import shutil
from pathlib import Path
from typing import Dict, Optional, List
from dataclasses import dataclass
from urllib.parse import urlparse

import requests
from loguru import logger


@dataclass
class ModelInfo:
    """模型信息"""
    name: str
    type: str  # asr, mt, tts
    source: str  # huggingface, local
    url: Optional[str] = None
    size_mb: Optional[float] = None
    checksum: Optional[str] = None
    files: Optional[List[str]] = None


class ModelDownloader:
    """
    模型下载器
    支持从Hugging Face等源下载模型
    """
    
    # 预配置的模型
    AVAILABLE_MODELS = {
        # ASR模型
        "whisper-tiny": ModelInfo(
            name="whisper-tiny",
            type="asr",
            source="openai",
            size_mb=75,
        ),
        "whisper-base": ModelInfo(
            name="whisper-base",
            type="asr",
            source="openai",
            size_mb=150,
        ),
        "whisper-small": ModelInfo(
            name="whisper-small",
            type="asr",
            source="openai",
            size_mb=500,
        ),
        "whisper-turbo": ModelInfo(
            name="whisper-turbo",
            type="asr",
            source="openai",
            size_mb=809,
        ),
        "faster-whisper-base": ModelInfo(
            name="faster-whisper-base",
            type="asr",
            source="huggingface",
            url="Systran/faster-whisper-base",
        ),
        "faster-whisper-small": ModelInfo(
            name="faster-whisper-small",
            type="asr",
            source="huggingface",
            url="Systran/faster-whisper-small",
        ),
        "faster-whisper-medium": ModelInfo(
            name="faster-whisper-medium",
            type="asr",
            source="huggingface",
            url="Systran/faster-whisper-medium",
        ),
        "faster-whisper-large-v3": ModelInfo(
            name="faster-whisper-large-v3",
            type="asr",
            source="huggingface",
            url="Systran/faster-whisper-large-v3",
        ),
        "faster-whisper-distil-large-v3": ModelInfo(
            name="faster-whisper-distil-large-v3",
            type="asr",
            source="huggingface",
            url="Systran/faster-distil-whisper-large-v3",
        ),
        "funasr-sensevoice-small": ModelInfo(
            name="funasr-sensevoice-small",
            type="asr",
            source="huggingface",
            url="iic/SenseVoiceSmall",
        ),
        "sherpa-onnx-zh-en-zipformer": ModelInfo(
            name="sherpa-onnx-zh-en-zipformer",
            type="asr",
            source="huggingface",
            url="k2-fsa/sherpa-onnx-streaming-zipformer-bilingual-zh-en-2023-02-20",
        ),
        "vosk-model-small-en-us-0.15": ModelInfo(
            name="vosk-model-small-en-us-0.15",
            type="asr",
            source="direct",
            url="https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip",
            size_mb=40,
        ),
        "vosk-model-small-cn-0.22": ModelInfo(
            name="vosk-model-small-cn-0.22",
            type="asr",
            source="direct",
            url="https://alphacephei.com/vosk/models/vosk-model-small-cn-0.22.zip",
            size_mb=42,
        ),
        
        # MT模型
        "nllb-200-distilled-600M": ModelInfo(
            name="nllb-200-distilled-600M",
            type="mt",
            source="huggingface",
            url="facebook/nllb-200-distilled-600M",
            size_mb=1200,
        ),
        "nllb-200-1.3B": ModelInfo(
            name="nllb-200-1.3B",
            type="mt",
            source="huggingface",
            url="facebook/nllb-200-1.3B",
            size_mb=2600,
        ),
        "argos-en-zh": ModelInfo(
            name="argos-en-zh",
            type="mt",
            source="argos",
            url="en-zh",
        ),
        "argos-zh-en": ModelInfo(
            name="argos-zh-en",
            type="mt",
            source="argos",
            url="zh-en",
        ),
        
        # TTS模型
        "piper-zh_CN-huayan": ModelInfo(
            name="piper-zh_CN-huayan",
            type="tts",
            source="huggingface",
            url="rhasspy/piper-voices",
            files=["zh/zh_CN/huayan/medium/zh_CN-huayan-medium.onnx", 
                   "zh/zh_CN/huayan/medium/zh_CN-huayan-medium.onnx.json"],
        ),
        "piper-en_US-lessac": ModelInfo(
            name="piper-en_US-lessac",
            type="tts",
            source="huggingface",
            url="rhasspy/piper-voices",
            files=["en/en_US/lessac/medium/en_US-lessac-medium.onnx",
                   "en/en_US/lessac/medium/en_US-lessac-medium.onnx.json"],
        ),
    }
    
    def __init__(self, models_dir: Optional[Path] = None):
        self.models_dir = models_dir or Path.home() / ".localtrans" / "models"
        self.models_dir.mkdir(parents=True, exist_ok=True)
        
        self._cache_file = self.models_dir / "models_cache.json"
        self._cache = self._load_cache()
        
        logger.info(f"ModelDownloader初始化: {self.models_dir}")
    
    def _load_cache(self) -> Dict:
        """加载缓存"""
        if self._cache_file.exists():
            try:
                with open(self._cache_file, 'r', encoding='utf-8') as f:
                    return json.load(f)
            except Exception:
                pass
        return {}
    
    def _save_cache(self) -> None:
        """保存缓存"""
        with open(self._cache_file, 'w', encoding='utf-8') as f:
            json.dump(self._cache, f, indent=2, ensure_ascii=False)
    
    def list_available(self, model_type: str = None) -> List[ModelInfo]:
        """列出可用模型"""
        models = list(self.AVAILABLE_MODELS.values())
        
        if model_type:
            models = [m for m in models if m.type == model_type]
        
        return models
    
    def is_downloaded(self, name: str) -> bool:
        """检查模型是否已下载"""
        if name not in self.AVAILABLE_MODELS:
            return False
        
        model = self.AVAILABLE_MODELS[name]
        model_dir = self.models_dir / model.type / name
        
        return model_dir.exists() and self._cache.get(name, {}).get("downloaded", False)
    
    def get_model_path(self, name: str) -> Optional[Path]:
        """获取模型路径"""
        if name not in self.AVAILABLE_MODELS:
            return None
        
        model = self.AVAILABLE_MODELS[name]
        return self.models_dir / model.type / name
    
    def download_model(
        self, 
        name: str,
        force: bool = False,
        progress_callback: Optional[callable] = None,
    ) -> Path:
        """
        下载模型
        
        Args:
            name: 模型名称
            force: 强制重新下载
            progress_callback: 进度回调函数
            
        Returns:
            模型路径
        """
        if name not in self.AVAILABLE_MODELS:
            raise ValueError(f"未知模型: {name}")
        
        model = self.AVAILABLE_MODELS[name]
        model_dir = self.models_dir / model.type / name
        
        # 检查是否已下载
        if not force and self.is_downloaded(name):
            logger.info(f"模型 {name} 已存在")
            return model_dir
        
        logger.info(f"开始下载模型: {name}")
        
        if model.source == "huggingface":
            return self._download_from_huggingface(model, model_dir, progress_callback)
        elif model.source == "openai":
            return self._download_whisper(model, model_dir, progress_callback)
        elif model.source == "direct":
            return self._download_direct(model, model_dir, progress_callback)
        elif model.source == "argos":
            return self._download_argos_package(model, model_dir, progress_callback)
        else:
            raise ValueError(f"不支持的模型源: {model.source}")
    
    def _download_from_huggingface(
        self,
        model: ModelInfo,
        target_dir: Path,
        progress_callback: Optional[callable],
    ) -> Path:
        """从Hugging Face下载"""
        def _mark_downloaded(path: Path) -> Path:
            self._cache[model.name] = {
                "downloaded": True,
                "path": str(path),
            }
            self._save_cache()
            logger.info(f"模型 {model.name} 下载完成")
            return path

        # faster-whisper 走专用下载链路，规避 snapshot 缓存不一致导致的失败
        if model.name.startswith("faster-whisper-"):
            try:
                from faster_whisper.utils import download_model as fw_download_model
            except ImportError as exc:
                raise ImportError("请安装faster-whisper: pip install faster-whisper") from exc

            target_dir.mkdir(parents=True, exist_ok=True)
            size_or_id = model.url or model.name.replace("faster-whisper-", "", 1)
            if isinstance(size_or_id, str) and size_or_id.startswith("Systran/faster-whisper-"):
                size_or_id = size_or_id.replace("Systran/faster-whisper-", "", 1)

            last_exc = None
            for attempt in range(1, 4):
                try:
                    logger.info(f"下载faster-whisper模型({attempt}/3): {size_or_id}")
                    downloaded = Path(
                        fw_download_model(
                            size_or_id=size_or_id,
                            output_dir=str(target_dir),
                            local_files_only=False,
                        )
                    )
                    if downloaded != target_dir and downloaded.exists():
                        if target_dir.exists():
                            shutil.rmtree(target_dir, ignore_errors=True)
                        shutil.copytree(downloaded, target_dir, dirs_exist_ok=True)
                    return _mark_downloaded(target_dir)
                except Exception as exc:
                    last_exc = exc
                    logger.warning(f"下载faster-whisper失败({attempt}/3): {exc}")
                    if attempt < 3:
                        time.sleep(attempt * 1.5)
            raise RuntimeError(f"下载faster-whisper模型失败: {model.name}") from last_exc

        try:
            from huggingface_hub import snapshot_download, hf_hub_download
        except ImportError as exc:
            raise ImportError("请安装huggingface_hub: pip install huggingface_hub") from exc

        target_dir.mkdir(parents=True, exist_ok=True)
        last_exc = None
        for attempt in range(1, 4):
            try:
                if model.files:
                    repo_id = model.url
                    for file_path in model.files:
                        logger.info(f"下载文件({attempt}/3): {file_path}")
                        hf_hub_download(
                            repo_id=repo_id,
                            filename=file_path,
                            local_dir=target_dir,
                            local_dir_use_symlinks=False,
                            local_files_only=False,
                        )
                else:
                    logger.info(f"下载仓库({attempt}/3): {model.url}")
                    snapshot_download(
                        repo_id=model.url,
                        local_dir=target_dir,
                        local_dir_use_symlinks=False,
                        local_files_only=False,
                    )
                return _mark_downloaded(target_dir)
            except Exception as exc:
                last_exc = exc
                logger.warning(f"HuggingFace下载失败({attempt}/3): {model.name} -> {exc}")
                if attempt < 3:
                    time.sleep(attempt * 1.5)

        raise RuntimeError(f"下载模型失败: {model.name}") from last_exc
    
    def _download_whisper(
        self,
        model: ModelInfo,
        target_dir: Path,
        progress_callback: Optional[callable],
    ) -> Path:
        """下载Whisper模型"""
        try:
            import whisper
            
            logger.info(f"下载Whisper模型: {model.name}")
            
            # Whisper会自动下载
            whisper_model = whisper.load_model(model.name.replace("whisper-", ""))
            
            target_dir.mkdir(parents=True, exist_ok=True)
            
            # 更新缓存
            self._cache[model.name] = {
                "downloaded": True,
                "path": str(target_dir),
            }
            self._save_cache()
            
            logger.info(f"模型 {model.name} 下载完成")
            return target_dir
            
        except ImportError:
            raise ImportError("请安装openai-whisper: pip install openai-whisper")

    def _download_direct(
        self,
        model: ModelInfo,
        target_dir: Path,
        progress_callback: Optional[callable],
    ) -> Path:
        """从直链下载并解压模型"""
        if not model.url:
            raise ValueError(f"模型 {model.name} 缺少下载URL")

        target_dir.mkdir(parents=True, exist_ok=True)
        zip_path = target_dir.parent / f"{model.name}.zip"

        logger.info(f"下载模型文件: {model.url}")
        with requests.get(model.url, stream=True, timeout=60) as response:
            response.raise_for_status()
            total_size = int(response.headers.get("content-length", 0))
            downloaded = 0
            with open(zip_path, "wb") as f:
                for chunk in response.iter_content(chunk_size=1024 * 1024):
                    if not chunk:
                        continue
                    f.write(chunk)
                    downloaded += len(chunk)
                    if progress_callback and total_size > 0:
                        progress_callback(downloaded / total_size)

        logger.info(f"解压模型: {zip_path}")
        with zipfile.ZipFile(zip_path, "r") as zf:
            zf.extractall(target_dir.parent)

        # Vosk包通常会解压出同名目录，确保返回目录正确
        extracted_dir = target_dir
        if not extracted_dir.exists():
            candidates = [p for p in target_dir.parent.iterdir() if p.is_dir() and p.name.startswith(model.name)]
            if candidates:
                extracted_dir = candidates[0]

        if zip_path.exists():
            zip_path.unlink()

        self._cache[model.name] = {
            "downloaded": True,
            "path": str(extracted_dir),
            "source": model.source,
        }
        self._save_cache()

        logger.info(f"模型 {model.name} 下载完成")
        return extracted_dir

    def _download_argos_package(
        self,
        model: ModelInfo,
        target_dir: Path,
        progress_callback: Optional[callable],
    ) -> Path:
        """下载并安装Argos语言包"""
        try:
            import argostranslate.package as argos_package
        except ImportError as exc:
            raise ImportError("请安装argostranslate: pip install argostranslate") from exc

        if not model.url or "-" not in model.url:
            raise ValueError(f"Argos模型URL格式错误: {model.url}")

        source_lang, target_lang = model.url.split("-", maxsplit=1)

        installed_pkg = None
        try:
            installed_pkg = next(
                (
                    p
                    for p in argos_package.get_installed_packages()
                    if getattr(p, "from_code", "") == source_lang
                    and getattr(p, "to_code", "") == target_lang
                    and getattr(p, "type", "translate") == "translate"
                ),
                None,
            )
        except Exception as exc:
            logger.debug(f"读取Argos已安装包失败，继续下载流程: {exc}")

        if installed_pkg is not None:
            logger.info(f"检测到已安装Argos语言包，复用: {source_lang}->{target_lang}")
            download_path = getattr(installed_pkg, "package_path", "")
        else:
            logger.info(f"更新Argos包索引并查找语言包: {source_lang}->{target_lang}")
            update_error = None
            for attempt in range(1, 4):
                try:
                    argos_package.update_package_index()
                    update_error = None
                    break
                except Exception as exc:
                    update_error = exc
                    logger.warning(f"更新Argos包索引失败({attempt}/3): {exc}")
                    if attempt < 3:
                        time.sleep(attempt * 1.5)
            if update_error is not None:
                raise RuntimeError("更新Argos包索引失败，请检查网络后重试") from update_error

            available = argos_package.get_available_packages()
            pkg = next(
                (
                    p
                    for p in available
                    if p.from_code == source_lang and p.to_code == target_lang
                ),
                None,
            )
            if pkg is None:
                raise ValueError(f"未找到Argos语言包: {source_lang}->{target_lang}")

            download_error = None
            download_path = None
            for attempt in range(1, 4):
                try:
                    download_path = pkg.download()
                    download_error = None
                    break
                except Exception as exc:
                    download_error = exc
                    logger.warning(f"下载Argos语言包失败({attempt}/3): {exc}")
                    if attempt < 3:
                        time.sleep(attempt * 1.5)

            if download_error is not None or not download_path:
                raise RuntimeError(
                    f"下载Argos语言包失败: {source_lang}->{target_lang}，请检查网络后重试"
                ) from download_error

            argos_package.install_from_path(download_path)

        # onefile 场景下导入 argostranslate.translate 可能触发可选依赖异常（如 _C）。
        # 这里仅用于写入本地标记，直接记录当前语言对即可，避免误报。
        installed_languages: List[str] = sorted({source_lang, target_lang})

        # 写入本地标记目录，便于统一缓存管理
        target_dir.mkdir(parents=True, exist_ok=True)
        marker = {
            "model": model.name,
            "source_lang": source_lang,
            "target_lang": target_lang,
            "download_path": str(download_path),
            "installed_languages": installed_languages,
        }
        with open(target_dir / "installed.json", "w", encoding="utf-8") as f:
            json.dump(marker, f, ensure_ascii=False, indent=2)

        self._cache[model.name] = {
            "downloaded": True,
            "path": str(target_dir),
            "source": model.source,
        }
        self._save_cache()

        logger.info(f"Argos模型 {model.name} 安装完成")
        return target_dir
    
    def download_faster_whisper(
        self,
        model_size: str = "base",
        target_dir: Optional[Path] = None,
    ) -> Path:
        """下载faster-whisper模型"""
        try:
            from faster_whisper import WhisperModel
            
            model_name = f"faster-whisper-{model_size}"
            target_dir = target_dir or self.models_dir / "asr" / model_name
            target_dir.mkdir(parents=True, exist_ok=True)
            
            logger.info(f"下载faster-whisper模型: {model_size}")
            
            # 首次加载会自动下载
            WhisperModel(model_size, download_root=str(target_dir))
            
            self._cache[model_name] = {
                "downloaded": True,
                "path": str(target_dir),
            }
            self._save_cache()
            
            logger.info(f"模型 {model_name} 下载完成")
            return target_dir
            
        except ImportError:
            raise ImportError("请安装faster-whisper: pip install faster-whisper")
    
    def delete_model(self, name: str) -> bool:
        """删除模型"""
        if name not in self.AVAILABLE_MODELS:
            return False
        
        model = self.AVAILABLE_MODELS[name]
        model_dir = self.models_dir / model.type / name
        
        if not model_dir.exists():
            return False
        
        # 删除目录
        shutil.rmtree(model_dir)
        
        # 更新缓存
        if name in self._cache:
            del self._cache[name]
            self._save_cache()
        
        logger.info(f"模型 {name} 已删除")
        return True


def download_all_required():
    """下载所有必需模型"""
    downloader = ModelDownloader()
    
    # 下载默认模型
    models_to_download = [
        ("vosk-model-small-en-us-0.15", "asr"),
        ("argos-en-zh", "mt"),
    ]
    
    for model_name, model_type in models_to_download:
        if not downloader.is_downloaded(model_name):
            print(f"下载 {model_name}...")
            try:
                downloader.download_model(model_name)
                print(f"✓ {model_name} 下载完成")
            except Exception as e:
                print(f"✗ {model_name} 下载失败: {e}")
        else:
            print(f"✓ {model_name} 已存在")


def main():
    """命令行入口"""
    import argparse
    
    parser = argparse.ArgumentParser(description="模型下载工具")
    parser.add_argument("command", choices=["list", "download", "delete", "download-all"])
    parser.add_argument("--model", "-m", help="模型名称")
    parser.add_argument("--type", "-t", choices=["asr", "mt", "tts"], help="模型类型")
    parser.add_argument("--force", "-f", action="store_true", help="强制重新下载")
    
    args = parser.parse_args()
    
    downloader = ModelDownloader()
    
    if args.command == "list":
        models = downloader.list_available(args.type)
        print("\n可用模型:")
        print("-" * 60)
        for model in models:
            status = "✓ 已下载" if downloader.is_downloaded(model.name) else "○ 未下载"
            size = f"{model.size_mb}MB" if model.size_mb else "未知"
            print(f"  {status}  {model.name:30} ({model.type:3}) {size:>10}")
        print()
    
    elif args.command == "download":
        if not args.model:
            print("请指定模型名称: --model <name>")
            return
        downloader.download_model(args.model, force=args.force)
    
    elif args.command == "delete":
        if not args.model:
            print("请指定模型名称: --model <name>")
            return
        downloader.delete_model(args.model)
    
    elif args.command == "download-all":
        download_all_required()


if __name__ == "__main__":
    main()
