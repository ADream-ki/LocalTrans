"""测试配置"""

import pytest
from pathlib import Path
import sys

# 添加src路径
src_path = Path(__file__).parent.parent / "src"
sys.path.insert(0, str(src_path))


def pytest_addoption(parser):
    parser.addoption(
        "--run-real-audio",
        action="store_true",
        default=False,
        help="run real audio integration tests",
    )


def pytest_configure(config):
    config.addinivalue_line("markers", "real_audio: mark real-audio integration tests")


def pytest_collection_modifyitems(config, items):
    if config.getoption("--run-real-audio"):
        return
    skip_marker = pytest.mark.skip(reason="need --run-real-audio option to run")
    for item in items:
        if "real_audio" in item.keywords:
            item.add_marker(skip_marker)
