"""测试流水线模块"""

import pytest
import time
from unittest.mock import Mock, patch, MagicMock
import numpy as np

from localtrans.asr.engine import TranscriptionResult
from localtrans.pipeline.translator import TranslationPipeline, PipelineState, PipelineMetrics
from localtrans.pipeline.realtime import (
    BidirectionalRealtimeSession,
    RealtimePipeline,
    RealtimeConfig,
    SessionOrchestrator,
)


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
        assert config.asr_streaming_mode == "legacy"
        assert config.asr_vad_mode == "webrtc"
    
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
        assert pipeline.config.asr_streaming_mode == "legacy"

    def test_streaming_mode_normalization(self):
        """非法流式模式应回退到 legacy。"""
        config = RealtimeConfig(asr_streaming_mode="unknown", asr_vad_mode="???")
        assert config.asr_streaming_mode == "legacy"
        assert config.asr_vad_mode == "webrtc"

    def test_funasr_managed_defaults_are_more_conservative(self, monkeypatch):
        """FunASR 在 managed 模式下应自动提高稳定性阈值，避免碎片误识别。"""
        from localtrans.config import settings

        monkeypatch.setattr(settings.asr, "model_type", "funasr")
        config = RealtimeConfig(asr_streaming_mode="managed", stream_profile="realtime")

        assert config.asr_buffer_duration >= 0.9
        assert config.asr_min_buffer_duration >= 0.55
        assert config.asr_max_buffer_duration >= 1.2
        assert config.asr_partial_decode_interval >= 0.45
        assert config.stream_flush_interval >= 0.45
        assert config.stream_agreement >= 3
        assert config.translation_batch_chars >= 60
        assert config.max_translation_queue >= 4

    def test_funasr_managed_quality_profile_keeps_conservative_defaults(self, monkeypatch):
        """quality 档位下也应继续保留 FunASR managed 的保守阈值。"""
        from localtrans.config import settings

        monkeypatch.setattr(settings.asr, "model_type", "funasr")
        config = RealtimeConfig(asr_streaming_mode="managed", stream_profile="quality")

        assert config.asr_buffer_duration >= 0.9
        assert config.asr_min_buffer_duration >= 0.55
        assert config.asr_max_buffer_duration >= 1.2
        assert config.asr_partial_decode_interval >= 0.45
        assert config.stream_flush_interval >= 0.45
        assert config.stream_agreement >= 3

    def test_explicit_backend_type_applies_conservative_defaults(self):
        """显式传入 backend type 时，不应依赖全局 settings.asr。"""
        config = RealtimeConfig(
            asr_streaming_mode="managed",
            stream_profile="quality",
            asr_backend_type="funasr",
        )
        assert config.asr_buffer_duration >= 0.9
        assert config.stream_agreement >= 3

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

    def test_agreement_timeout_still_emits(self):
        """agreement>1时，候选超时应放行，避免界面长期无输出"""
        pipeline = RealtimePipeline(
            RealtimeConfig(
                source_lang="zh",
                stream_profile="balanced",
                stream_agreement=2,
                stream_flush_interval=0.25,
                drop_hallucination=False,
            )
        )
        with patch("localtrans.pipeline.realtime.time.time", side_effect=[0.0, 0.8]):
            with pipeline._text_state_lock:
                first = pipeline._should_emit_segment_locked("你好")
                second = pipeline._should_emit_segment_locked("今天")
        assert first is False
        assert second is True

    def test_final_transcription_flushes_pending_segment(self):
        """final ASR结果应立即冲刷pending文本，避免尾句滞留"""
        pipeline = RealtimePipeline(
            RealtimeConfig(source_lang="zh", enable_tts=False, drop_hallucination=False)
        )
        pipeline._running = True

        with patch.object(pipeline, "_enqueue_translation_text") as mock_enqueue:
            pipeline._on_transcription(
                TranscriptionResult(
                    text="你好世界",
                    language="zh",
                    confidence=0.9,
                    start_time=0.0,
                    end_time=0.4,
                    is_final=True,
                )
            )

        assert mock_enqueue.called
        assert pipeline._last_asr_text == ""

    def test_funasr_partial_revisions_only_commit_stable_prefix(self, monkeypatch):
        """FunASR managed partial 反复重解码时，只提交稳定前缀，尾句留给 final。"""
        from localtrans.config import settings

        monkeypatch.setattr(settings.asr, "model_type", "funasr")
        pipeline = RealtimePipeline(
            RealtimeConfig(
                source_lang="zh",
                enable_tts=False,
                drop_hallucination=False,
                asr_streaming_mode="managed",
                stream_min_chars=4,
                stream_max_chars=20,
                stream_flush_interval=0.05,
            )
        )
        pipeline._running = True
        pipeline._last_source_emit_ts = 0.0

        with patch.object(pipeline, "_enqueue_translation_text") as mock_enqueue:
            with patch("localtrans.pipeline.realtime.time.time", side_effect=[1.0, 1.1, 1.2, 1.3, 1.4, 1.5]):
                pipeline._on_transcription(
                    TranscriptionResult(
                        text="开放时间岛上。",
                        language="zh",
                        confidence=0.9,
                        start_time=0.0,
                        end_time=0.3,
                        is_final=False,
                    )
                )
                pipeline._on_transcription(
                    TranscriptionResult(
                        text="开放时间早上9点。",
                        language="zh",
                        confidence=0.9,
                        start_time=0.0,
                        end_time=0.6,
                        is_final=False,
                    )
                )
                pipeline._on_transcription(
                    TranscriptionResult(
                        text="开放时间早上9点至下午5点。",
                        language="zh",
                        confidence=0.9,
                        start_time=0.0,
                        end_time=1.0,
                        is_final=True,
                    )
                )

        emitted = [call.args[0] for call in mock_enqueue.call_args_list]
        assert emitted == ["开放时间", "早上9点至下午5点。"]

    def test_bidirectional_session_rolls_back_on_start_failure(self):
        forward = Mock()
        reverse = Mock()
        forward.start.return_value = True
        forward.is_running = True
        reverse.start.return_value = False
        reverse.is_running = False

        session = BidirectionalRealtimeSession(forward, reverse)

        assert session.start() is False
        forward.stop.assert_called_once()

    def test_runtime_summary_reports_direction_and_output_mode(self):
        pipeline = RealtimePipeline(
            RealtimeConfig(
                source_lang="zh",
                target_lang="en",
                enable_tts=True,
                output_to_virtual_device=False,
                input_device_id=9,
                direction_label="我→对方",
            )
        )

        summary = pipeline.get_runtime_summary()

        assert summary["direction"] == "我→对方"
        assert summary["source_lang"] == "zh"
        assert summary["target_lang"] == "en"
        assert summary["input_device_id"] == 9
        assert summary["output_mode"] == "speaker"
        assert summary["asr_backend"] == pipeline._asr_config.model_type

    def test_bidirectional_session_exposes_runtime_summaries(self):
        forward = RealtimePipeline(
            RealtimeConfig(source_lang="zh", target_lang="en", direction_label="我→对方")
        )
        reverse = RealtimePipeline(
            RealtimeConfig(source_lang="en", target_lang="zh", direction_label="对方→我")
        )

        session = BidirectionalRealtimeSession(forward, reverse)
        summaries = session.get_runtime_summaries()

        assert len(summaries) == 2
        assert summaries[0]["direction"] == "我→对方"
        assert summaries[1]["direction"] == "对方→我"

    def test_session_orchestrator_wraps_two_directions(self):
        forward = RealtimePipeline(
            RealtimeConfig(source_lang="zh", target_lang="en", direction_label="我→对方")
        )
        reverse = RealtimePipeline(
            RealtimeConfig(source_lang="en", target_lang="zh", direction_label="对方→我")
        )

        orchestrator = SessionOrchestrator({"forward": forward, "reverse": reverse})

        assert orchestrator.direction_names == ["forward", "reverse"]
        summaries = orchestrator.get_runtime_summaries()
        assert len(summaries) == 2
        assert summaries[0]["direction"] == "我→对方"
        assert summaries[1]["direction"] == "对方→我"

    def test_session_orchestrator_feedback_guard_blocks_cross_direction_echo(self):
        forward = RealtimePipeline(
            RealtimeConfig(source_lang="zh", target_lang="en", direction_label="我→对方")
        )
        reverse = RealtimePipeline(
            RealtimeConfig(source_lang="en", target_lang="zh", direction_label="对方→我")
        )

        orchestrator = SessionOrchestrator({"forward": forward, "reverse": reverse})
        # 通过方向A回调注入最近输出，模拟A路TTS内容被B路回录。
        forward._result_callback({"translation": "Hello, this is a test sentence."})

        assert reverse.should_accept_source_text("hello this is a test sentence") is False
        assert reverse.should_accept_source_text("completely different content") is True

    def test_session_orchestrator_auto_restarts_failed_direction(self):
        class FakePipeline:
            def __init__(self, label: str):
                self.label = label
                self.is_running = False
                self.start_calls = 0

            def start(self):
                self.start_calls += 1
                self.is_running = True
                return True

            def stop(self):
                self.is_running = False

            def get_history(self, limit=50):
                return []

            def get_runtime_summary(self):
                return {
                    "direction": self.label,
                    "source_lang": "zh",
                    "target_lang": "en",
                    "input_device_id": 0,
                    "output_mode": "speaker",
                    "output_device_id": None,
                    "stream_profile": "realtime",
                    "asr_streaming_mode": "legacy",
                    "asr_vad_mode": "webrtc",
                    "asr_backend": "funasr",
                    "asr_model": "FunAudioLLM/SenseVoiceSmall",
                    "mt_backend": "argos-ct2",
                    "mt_model": "argos-zh-en",
                    "tts_enabled": False,
                    "tts_engine": "piper",
                    "tts_model": "",
                    "is_running": self.is_running,
                }

            def set_source_acceptor(self, acceptor):
                self._acceptor = acceptor

        forward = FakePipeline("我→对方")
        reverse = FakePipeline("对方→我")

        orchestrator = SessionOrchestrator(
            {"forward": forward, "reverse": reverse},
            restart_check_interval_s=0.05,
            restart_cooldown_s=0.01,
            restart_max_attempts=3,
        )
        assert orchestrator.start() is True
        assert forward.start_calls == 1
        assert reverse.start_calls == 1

        reverse.is_running = False
        deadline = time.time() + 1.0
        while reverse.start_calls < 2 and time.time() < deadline:
            time.sleep(0.05)

        orchestrator.stop()
        assert reverse.start_calls >= 2
