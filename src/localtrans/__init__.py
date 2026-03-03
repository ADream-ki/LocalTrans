"""
LocalTrans - AI实时翻译软件
端侧本地部署的实时翻译工具
"""

__version__ = "1.0.0"
__author__ = "iFlow Team"

from localtrans.config import settings
from localtrans.pipeline import TranslationPipeline

__all__ = ["settings", "TranslationPipeline", "__version__"]
