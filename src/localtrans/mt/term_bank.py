"""
术语库管理
支持自定义术语导入和应用
"""

import json
from pathlib import Path
from typing import Dict, List, Optional
from dataclasses import dataclass, field, asdict

from loguru import logger

from localtrans.config.models import TermBank as TermBankConfig


@dataclass
class TermEntry:
    """术语条目"""
    source: str
    target: str
    context: Optional[str] = None
    category: Optional[str] = None
    priority: int = 0  # 优先级，用于多匹配时选择


class TermBankManager:
    """
    术语库管理器
    支持导入、查询、应用术语
    """
    
    def __init__(self, filepath: Optional[Path] = None):
        self.filepath = filepath
        self._terms: Dict[str, TermEntry] = {}
        self._sorted_keys: List[str] = []  # 按长度排序的key，优先匹配长术语
        
        if filepath and filepath.exists():
            self.load(filepath)
        
        logger.info(f"术语库初始化: {len(self._terms)}个术语")
    
    def add_term(
        self,
        source: str,
        target: str,
        context: Optional[str] = None,
        category: Optional[str] = None,
        priority: int = 0,
    ) -> None:
        """添加术语"""
        entry = TermEntry(
            source=source,
            target=target,
            context=context,
            category=category,
            priority=priority,
        )
        self._terms[source] = entry
        self._update_sorted_keys()
        logger.debug(f"添加术语: {source} -> {target}")
    
    def remove_term(self, source: str) -> bool:
        """移除术语"""
        if source in self._terms:
            del self._terms[source]
            self._update_sorted_keys()
            return True
        return False
    
    def lookup(self, source: str) -> Optional[TermEntry]:
        """查找术语"""
        return self._terms.get(source)
    
    def _update_sorted_keys(self) -> None:
        """更新排序后的key列表"""
        self._sorted_keys = sorted(self._terms.keys(), key=len, reverse=True)
    
    def apply(self, text: str) -> str:
        """
        应用术语翻译到文本
        优先匹配更长的术语
        """
        for source in self._sorted_keys:
            if source in text:
                entry = self._terms[source]
                text = text.replace(source, entry.target)
        return text
    
    def apply_with_highlight(self, text: str) -> tuple:
        """
        应用术语并返回高亮信息
        返回: (翻译后文本, 匹配的术语列表)
        """
        matched = []
        for source in self._sorted_keys:
            if source in text:
                entry = self._terms[source]
                text = text.replace(source, entry.target)
                matched.append((source, entry.target))
        return text, matched
    
    def load(self, filepath: Path) -> None:
        """从文件加载术语库"""
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                data = json.load(f)
            
            # 支持多种格式
            if isinstance(data, dict):
                if "entries" in data:
                    # 完整格式
                    entries = data["entries"]
                else:
                    # 简单映射格式
                    entries = data
            elif isinstance(data, list):
                # 列表格式
                entries = {item["source"]: item for item in data}
            else:
                raise ValueError(f"不支持的术语库格式: {type(data)}")
            
            self._terms.clear()
            for source, item in entries.items():
                if isinstance(item, dict):
                    self.add_term(
                        source=source,
                        target=item.get("target", item.get("translation", "")),
                        context=item.get("context"),
                        category=item.get("category"),
                        priority=item.get("priority", 0),
                    )
                elif isinstance(item, str):
                    self.add_term(source=source, target=item)
            
            self._update_sorted_keys()
            logger.info(f"从 {filepath} 加载 {len(self._terms)} 个术语")
            
        except Exception as e:
            logger.error(f"加载术语库失败: {e}")
    
    def save(self, filepath: Optional[Path] = None) -> None:
        """保存术语库到文件"""
        filepath = filepath or self.filepath
        if not filepath:
            raise ValueError("未指定保存路径")
        
        data = {
            "name": "term_bank",
            "version": "1.0",
            "entries": {
                source: asdict(entry)
                for source, entry in self._terms.items()
            }
        }
        
        filepath.parent.mkdir(parents=True, exist_ok=True)
        
        with open(filepath, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=2)
        
        logger.info(f"术语库已保存到 {filepath}")
    
    def import_from_csv(self, filepath: Path, delimiter: str = ',') -> int:
        """从CSV导入术语"""
        import csv
        
        count = 0
        with open(filepath, 'r', encoding='utf-8') as f:
            reader = csv.reader(f, delimiter=delimiter)
            for row in reader:
                if len(row) >= 2:
                    self.add_term(
                        source=row[0].strip(),
                        target=row[1].strip(),
                        context=row[2].strip() if len(row) > 2 else None,
                        category=row[3].strip() if len(row) > 3 else None,
                    )
                    count += 1
        
        logger.info(f"从CSV导入 {count} 个术语")
        return count
    
    def export_to_csv(self, filepath: Path, delimiter: str = ',') -> None:
        """导出术语到CSV"""
        import csv
        
        with open(filepath, 'w', encoding='utf-8', newline='') as f:
            writer = csv.writer(f, delimiter=delimiter)
            for entry in self._terms.values():
                writer.writerow([
                    entry.source,
                    entry.target,
                    entry.context or '',
                    entry.category or '',
                ])
        
        logger.info(f"术语库已导出到 {filepath}")
    
    def clear(self) -> None:
        """清空术语库"""
        self._terms.clear()
        self._sorted_keys.clear()
    
    def get_all(self) -> Dict[str, TermEntry]:
        """获取所有术语"""
        return self._terms.copy()
    
    def search(self, query: str) -> List[TermEntry]:
        """搜索术语"""
        query = query.lower()
        return [
            entry for source, entry in self._terms.items()
            if query in source.lower() or query in entry.target.lower()
        ]
    
    def get_categories(self) -> List[str]:
        """获取所有分类"""
        categories = set()
        for entry in self._terms.values():
            if entry.category:
                categories.add(entry.category)
        return sorted(categories)
    
    def __len__(self) -> int:
        return len(self._terms)
    
    def __contains__(self, source: str) -> bool:
        return source in self._terms
