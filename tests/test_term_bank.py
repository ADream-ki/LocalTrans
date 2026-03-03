"""测试术语库管理"""

import pytest
from pathlib import Path
import tempfile
import json

from localtrans.mt.term_bank import TermBankManager, TermEntry


class TestTermBankManager:
    """术语库管理器测试"""
    
    def test_add_term(self):
        """测试添加术语"""
        manager = TermBankManager()
        
        manager.add_term("hello", "你好")
        
        assert len(manager) == 1
        assert "hello" in manager
    
    def test_lookup_term(self):
        """测试查找术语"""
        manager = TermBankManager()
        manager.add_term("hello", "你好", context="greeting")
        
        entry = manager.lookup("hello")
        
        assert entry is not None
        assert entry.target == "你好"
        assert entry.context == "greeting"
    
    def test_apply_translation(self):
        """测试应用术语翻译"""
        manager = TermBankManager()
        manager.add_term("API", "应用程序接口")
        manager.add_term("SDK", "软件开发工具包")
        
        text = "Please use our API and SDK."
        result = manager.apply(text)
        
        assert "应用程序接口" in result
        assert "软件开发工具包" in result
    
    def test_priority_matching(self):
        """测试优先匹配长术语"""
        manager = TermBankManager()
        manager.add_term("test", "测试")
        manager.add_term("test case", "测试用例")
        
        text = "This is a test case."
        result = manager.apply(text)
        
        assert "测试用例" in result
        assert result.count("测试") == 1
    
    def test_save_and_load(self):
        """测试保存和加载"""
        with tempfile.TemporaryDirectory() as tmpdir:
            filepath = Path(tmpdir) / "terms.json"
            
            # 创建并保存
            manager = TermBankManager()
            manager.add_term("hello", "你好")
            manager.add_term("world", "世界")
            manager.save(filepath)
            
            # 加载
            manager2 = TermBankManager(filepath)
            
            assert len(manager2) == 2
            assert manager2.lookup("hello").target == "你好"
    
    def test_import_csv(self):
        """测试CSV导入"""
        with tempfile.TemporaryDirectory() as tmpdir:
            csv_path = Path(tmpdir) / "terms.csv"
            
            # 创建CSV文件
            with open(csv_path, 'w', encoding='utf-8') as f:
                f.write("hello,你好,问候词,greeting\n")
                f.write("world,世界,,\n")
            
            manager = TermBankManager()
            count = manager.import_from_csv(csv_path)
            
            assert count == 2
            assert manager.lookup("hello").target == "你好"
            assert manager.lookup("hello").category == "greeting"
    
    def test_search(self):
        """测试搜索"""
        manager = TermBankManager()
        manager.add_term("hello", "你好")
        manager.add_term("help", "帮助")
        manager.add_term("world", "世界")
        
        results = manager.search("hel")
        
        assert len(results) == 2
    
    def test_categories(self):
        """测试分类"""
        manager = TermBankManager()
        manager.add_term("API", "应用程序接口", category="technical")
        manager.add_term("SDK", "软件开发工具包", category="technical")
        manager.add_term("hello", "你好", category="greeting")
        
        categories = manager.get_categories()
        
        assert "technical" in categories
        assert "greeting" in categories
