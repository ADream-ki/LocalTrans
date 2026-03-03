"""测试初始化"""

import pytest
from localtrans import __version__, settings, TranslationPipeline


class TestInit:
    """初始化测试"""
    
    def test_version(self):
        """测试版本"""
        assert __version__ == "1.0.0"
    
    def test_settings_import(self):
        """测试配置导入"""
        assert settings is not None
        assert settings.app_name == "LocalTrans"
    
    def test_pipeline_import(self):
        """测试流水线导入"""
        assert TranslationPipeline is not None
