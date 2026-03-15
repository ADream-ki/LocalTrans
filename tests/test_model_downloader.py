"""测试模型下载器配置"""

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
