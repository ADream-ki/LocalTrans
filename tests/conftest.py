"""测试配置"""

import pytest
from pathlib import Path
import sys

# 添加src路径
src_path = Path(__file__).parent.parent / "src"
sys.path.insert(0, str(src_path))
