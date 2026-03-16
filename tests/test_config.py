"""测试配置模块"""

import pytest
from pathlib import Path

from localtrans.config import settings, Settings
from localtrans.config.models import AudioConfig, ASRConfig, MTConfig, TTSConfig, LanguagePair


class TestSettings:
    """配置测试"""
    
    def test_default_settings(self):
        """测试默认配置"""
        assert settings.app_name == "LocalTrans"
        assert settings.audio.sample_rate == 16000
        assert settings.asr.model_type == "vosk"
        assert settings.mt.model_type == "argos-ct2"
    
    def test_audio_config(self):
        """测试音频配置"""
        config = AudioConfig()
        
        assert config.sample_rate == 16000
        assert config.channels == 1
        assert config.input_device_id == -1
        assert config.output_device_id == -1
        assert config.output_mode in {"virtual", "device", "system"}
        assert config.io_profile in {"realtime", "balanced", "studio"}
    
    def test_asr_config(self):
        """测试ASR配置"""
        config = ASRConfig()
        
        assert config.model_type == "vosk"
        assert config.model_size == "base"
        assert config.model_name is None
        assert config.vad_filter is False

    def test_language_pair_default(self):
        """测试默认语言方向"""
        pair = LanguagePair()
        assert pair.source == "zh"
        assert pair.target == "en"
    
    def test_mt_config(self):
        """测试MT配置"""
        config = MTConfig()
        
        assert config.model_type == "argos-ct2"
        assert config.source_lang == "zh"
        assert config.target_lang == "en"
    
    def test_tts_config(self):
        """测试TTS配置"""
        config = TTSConfig()
        
        assert config.engine == "pyttsx3"
        assert config.stream_enabled is True
    
    def test_directories_created(self):
        """测试目录创建"""
        assert settings.data_dir.exists()
        assert settings.models_dir.exists()
        assert settings.logs_dir.exists()

    def test_load_persisted_config(self):
        """测试从 config.json 恢复配置"""
        import shutil

        data_dir = Path(".pytest_tmp_settings") / "localtrans-data"
        if data_dir.parent.exists():
            shutil.rmtree(data_dir.parent)
        data_dir.mkdir(parents=True, exist_ok=True)

        try:
            config_file = data_dir / "config.json"
            config_file.write_text(
                """
{
  "mt": {
    "model_type": "loci-enhanced",
    "source_lang": "en",
    "target_lang": "ja"
  }
}
""".strip(),
                encoding="utf-8",
            )

            loaded = Settings(data_dir=data_dir)
            assert loaded.mt.model_type == "loci-enhanced"
            assert loaded.mt.source_lang == "en"
            assert loaded.mt.target_lang == "ja"
        finally:
            if data_dir.parent.exists():
                shutil.rmtree(data_dir.parent)
