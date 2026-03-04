"""测试流水线模块"""

import pytest
from unittest.mock import Mock, patch, MagicMock
import numpy as np

from localtrans.pipeline.translator import TranslationPipeline, PipelineState, PipelineMetrics
from localtrans.pipeline.realtime import RealtimePipeline, RealtimeConfig


class TestPipelineMetrics:
    """流水线指标测试"""
    
    def test_metrics_init(self):
        """测试指标初始化"""
        metrics = PipelineMetrics()
        
        assert metrics.asr_latency_ms == 0.0
        assert metrics.mt_latency_ms == 0.0
        assert metrics.tts_latency_ms == 0.0
        assert metrics.total_latency_ms == 0.0
        assert metrics.audio_chunks_processed == 0


class TestTranslationPipeline:
    """翻译流水线测试"""
    
    def test_init(self):
        """测试初始化"""
        pipeline = TranslationPipeline(
            source_lang="en",
            target_lang="zh",
            enable_tts=False,
        )
        
        assert pipeline.source_lang == "en"
        assert pipeline.target_lang == "zh"
        assert pipeline.enable_tts is False
        assert pipeline.state == PipelineState.IDLE
    
    def test_metrics(self):
        """测试指标"""
        pipeline = TranslationPipeline(enable_tts=False)
        
        assert isinstance(pipeline.metrics, PipelineMetrics)


class TestRealtimePipeline:
    """实时流水线测试"""
    
    def test_config(self):
        """测试配置"""
        config = RealtimeConfig(
            source_lang="en",
            target_lang="zh",
            enable_tts=True,
        )
        
        assert config.source_lang == "en"
        assert config.target_lang == "zh"
        assert config.enable_tts is True
        assert config.direct_asr_translate is False
        assert config.stream_profile == "realtime"
        assert config.stream_agreement >= 1
    
    def test_pipeline_init(self):
        """测试初始化"""
        pipeline = RealtimePipeline()
        
        assert not pipeline.is_running

    def test_profile_quality_defaults(self):
        """测试质量模式参数调整"""
        config = RealtimeConfig(stream_profile="quality")
        assert config.stream_agreement >= 2
        assert config.translation_batch_chars >= 64
        assert config.tts_merge_chars >= 180
    
    def test_create_pipeline_helper(self):
        """测试创建辅助函数"""
        from localtrans.pipeline.realtime import create_pipeline
        
        pipeline = create_pipeline(
            source_lang="en",
            target_lang="zh",
            enable_tts=False,
        )
        
        assert pipeline.config.source_lang == "en"
        assert pipeline.config.target_lang == "zh"

    def test_sanitize_hallucination_phrase(self):
        """测试幻听短语过滤"""
        pipeline = RealtimePipeline(
            RealtimeConfig(source_lang="zh", drop_hallucination=True, min_asr_confidence=0.0)
        )
        cleaned = pipeline._sanitize_source_segment("字幕by索兰娅 感谢观看", "zh", 0.9)
        assert cleaned == ""

    def test_drop_low_confidence_short_segment(self):
        """测试低置信度短片段丢弃"""
        pipeline = RealtimePipeline(
            RealtimeConfig(source_lang="zh", min_asr_confidence=0.3, drop_hallucination=False)
        )
        cleaned = pipeline._sanitize_source_segment("好的", "zh", 0.05)
        assert cleaned == ""
