"""测试模型下载器配置"""

import shutil
import uuid
from pathlib import Path

from localtrans.utils.model_downloader import ModelDownloader


class TestModelDownloaderRegistry:
    """模型注册表测试"""

    def test_asr_registry_contains_qwen3_and_wenet(self):
        asr_names = {
            model.name
            for model in ModelDownloader.AVAILABLE_MODELS.values()
            if model.type == "asr"
        }

        assert "qwen3-asr-0.6b" in asr_names
        assert "qwen3-asr-1.7b" in asr_names
        assert "wenet-u2pp-cn" in asr_names

    def test_qwen3_and_wenet_model_sources(self):
        sensevoice = ModelDownloader.AVAILABLE_MODELS["funasr-sensevoice-small"]
        qwen_06 = ModelDownloader.AVAILABLE_MODELS["qwen3-asr-0.6b"]
        qwen_17 = ModelDownloader.AVAILABLE_MODELS["qwen3-asr-1.7b"]
        wenet = ModelDownloader.AVAILABLE_MODELS["wenet-u2pp-cn"]

        assert sensevoice.source == "huggingface"
        assert sensevoice.url == "FunAudioLLM/SenseVoiceSmall"
        assert qwen_06.source == "huggingface"
        assert qwen_17.source == "huggingface"
        assert wenet.source == "direct"


class TestModelDownloaderHubSelection:
    """模型下载Hub选择逻辑测试"""

    def test_modelscope_mode_uses_modelscope_only(self, monkeypatch):
        base = self._create_test_base_dir()
        try:
            downloader = ModelDownloader(models_dir=base / "models")
            model = ModelDownloader.AVAILABLE_MODELS["funasr-sensevoice-small"]
            target_dir = base / "out-modelscope"

            called = {"hf": False, "ms": False}

            def fake_modelscope_download(_model, _target_dir, _progress_callback, mark_downloaded):
                called["ms"] = True
                Path(_target_dir).mkdir(parents=True, exist_ok=True)
                return mark_downloaded(Path(_target_dir))

            monkeypatch.setenv("LOCALTRANS_MODEL_HUB", "modelscope")
            monkeypatch.setattr(downloader, "_download_from_modelscope", fake_modelscope_download)

            result = downloader._download_from_huggingface(model, target_dir, None)

            assert result == target_dir
            assert called["ms"] is True
            assert called["hf"] is False
        finally:
            shutil.rmtree(base, ignore_errors=True)

    def test_auto_mode_falls_back_to_modelscope_when_hf_import_missing(self, monkeypatch):
        base = self._create_test_base_dir()
        try:
            downloader = ModelDownloader(models_dir=base / "models")
            model = ModelDownloader.AVAILABLE_MODELS["funasr-sensevoice-small"]
            target_dir = base / "out-auto-fallback"

            called = {"ms": False}
            real_import = __import__

            def fake_import(name, *args, **kwargs):
                if name == "huggingface_hub":
                    raise ImportError("mock missing huggingface_hub")
                return real_import(name, *args, **kwargs)

            def fake_modelscope_download(_model, _target_dir, _progress_callback, mark_downloaded):
                called["ms"] = True
                Path(_target_dir).mkdir(parents=True, exist_ok=True)
                return mark_downloaded(Path(_target_dir))

            monkeypatch.setenv("LOCALTRANS_MODEL_HUB", "auto")
            monkeypatch.setattr("builtins.__import__", fake_import)
            monkeypatch.setattr(downloader, "_download_from_modelscope", fake_modelscope_download)

            result = downloader._download_from_huggingface(model, target_dir, None)

            assert result == target_dir
            assert called["ms"] is True
        finally:
            shutil.rmtree(base, ignore_errors=True)

    @staticmethod
    def _create_test_base_dir() -> Path:
        root = Path.home() / ".localtrans" / "test-tmp"
        root.mkdir(parents=True, exist_ok=True)
        base = root / f"downloader-{uuid.uuid4().hex}"
        base.mkdir(parents=True, exist_ok=True)
        return base
