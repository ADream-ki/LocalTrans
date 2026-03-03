"""测试MT模块"""

import pytest
from unittest.mock import Mock, patch, MagicMock

from localtrans.mt.engine import MTEngine, TranslationResult
from localtrans.mt.term_bank import TermBankManager
from localtrans.config import MTConfig


class TestTranslationResult:
    """翻译结果测试"""
    
    def test_result_creation(self):
        """测试结果创建"""
        result = TranslationResult(
            source_text="Hello",
            translated_text="你好",
            source_lang="en",
            target_lang="zh",
        )
        
        assert result.source_text == "Hello"
        assert result.translated_text == "你好"


class TestMTEngine:
    """MT引擎测试"""
    
    def test_backend_types(self):
        """测试后端类型"""
        assert "nllb" in MTEngine.BACKENDS
        assert "nllb-ct2" in MTEngine.BACKENDS
        assert "argos-ct2" in MTEngine.BACKENDS
        assert "argos" in MTEngine.BACKENDS
    
    def test_lang_code_map(self):
        """测试语言代码映射"""
        from localtrans.mt.engine import NLLBBackend
        
        lang_map = NLLBBackend.LANG_CODE_MAP
        
        assert lang_map["en"] == "eng_Latn"
        assert lang_map["zh"] == "zho_Hans"
        assert lang_map["ja"] == "jpn_Jpan"
    
    @patch('localtrans.mt.engine.NLLBBackend')
    def test_engine_with_term_bank(self, mock_backend):
        """测试术语库集成"""
        mock_backend.return_value = Mock()
        mock_backend.return_value.translate = Mock(return_value=TranslationResult(
            source_text="API",
            translated_text="API",
            source_lang="en",
            target_lang="zh",
        ))
        
        term_bank = TermBankManager()
        term_bank.add_term("API", "应用程序接口")
        
        with patch.object(MTEngine, '_load_backend'):
            engine = MTEngine()
            engine._backend = mock_backend.return_value
            engine._term_bank = term_bank
            
            # 测试翻译
            result = engine.translate("API")
            
            # 术语应该被应用
            assert "应用程序接口" in result.translated_text


class TestTermBankIntegration:
    """术语库集成测试"""
    
    def test_term_bank_apply(self):
        """测试术语库应用"""
        manager = TermBankManager()
        manager.add_term("API", "应用程序接口")
        manager.add_term("SDK", "软件开发工具包")
        
        text = "Use our API and SDK to build apps."
        result = manager.apply(text)
        
        assert "应用程序接口" in result
        assert "软件开发工具包" in result
