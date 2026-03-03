"""MT机器翻译模块"""

from localtrans.mt.engine import MTEngine, TranslationResult
from localtrans.mt.term_bank import TermBankManager

__all__ = ["MTEngine", "TranslationResult", "TermBankManager"]
