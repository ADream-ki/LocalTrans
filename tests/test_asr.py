"""测试ASR模块"""

import pytest
import numpy as np
import shutil
import sys
import tempfile
import time
from unittest.mock import Mock, patch, MagicMock
from pathlib import Path
from textwrap import dedent

from localtrans.asr.engine import (
    ASREngine,
    FunASRBackend,
    Qwen3ASRBackend,
    SherpaOnnxBackend,
    TranscriptionResult,
)
from localtrans.asr.funasr_direct import FunASRDirectRuntime
from localtrans.asr.stream import StreamingASR, StreamState
from localtrans.config import ASRConfig


def _reset_funasr_runtime() -> None:
    FunASRDirectRuntime._CORE_BOOTSTRAPPED = False
    for module_name in list(sys.modules):
        if (
            module_name == "funasr"
            or module_name.startswith("funasr.")
            or module_name.startswith("_localtrans_funasr_")
            or module_name.startswith("torchaudio")
        ):
            sys.modules.pop(module_name, None)


def _write_fake_funasr_package(base_dir: Path) -> Path:
    package_dir = base_dir / "funasr"
    (package_dir / "train_utils").mkdir(parents=True, exist_ok=True)
    (package_dir / "utils").mkdir(parents=True, exist_ok=True)

    (package_dir / "register.py").write_text(
        dedent(
            """
            class _Tables:
                def __init__(self):
                    self.model_classes = {}
                    self.frontend_classes = {}
                    self.tokenizer_classes = {}

                def register(self, table_name, key=None):
                    def decorator(cls):
                        getattr(self, table_name)[key or cls.__name__] = cls
                        return cls
                    return decorator


            tables = _Tables()
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (package_dir / "train_utils" / "set_all_random_seed.py").write_text(
        "def set_all_random_seed(seed):\n    return None\n",
        encoding="utf-8",
    )
    (package_dir / "train_utils" / "load_pretrained_model.py").write_text(
        "def load_pretrained_model(**kwargs):\n    return None\n",
        encoding="utf-8",
    )
    (package_dir / "utils" / "postprocess_utils.py").write_text(
        "def rich_transcription_postprocess(text):\n    return text\n",
        encoding="utf-8",
    )
    return package_dir


def _write_fake_funasr_model(model_dir: Path) -> Path:
    model_dir.mkdir(parents=True, exist_ok=True)
    (model_dir / "config.yaml").write_text(
        dedent(
            """
            model: UnitTestModel
            tokenizer: UnitTestTokenizer
            frontend: UnitTestFrontend
            tokenizer_conf:
              token_list:
                - <blank>
                - 你
                - 好
                - 世
                - 界
            frontend_conf:
              output_dim: 80
            model_conf:
              prefix: ""
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (model_dir / "model.py").write_text(
        dedent(
            """
            import torch
            from funasr.register import tables


            @tables.register("tokenizer_classes", "UnitTestTokenizer")
            class UnitTestTokenizer:
                def __init__(self, token_list=None, **kwargs):
                    self.token_list = token_list or ["<blank>", "你", "好", "世", "界"]


            @tables.register("frontend_classes", "UnitTestFrontend")
            class UnitTestFrontend:
                def __init__(self, output_dim=80, **kwargs):
                    self._output_dim = int(output_dim)

                def output_size(self):
                    return self._output_dim


            @tables.register("model_classes", "UnitTestModel")
            class UnitTestModel(torch.nn.Module):
                def __init__(self, prefix="", **kwargs):
                    super().__init__()
                    self.prefix = prefix

                def inference(self, audio, key=None, tokenizer=None, frontend=None, **kwargs):
                    audio_len = len(audio)
                    if audio_len >= 3200:
                        text = f"{self.prefix}你好世界".strip()
                    elif audio_len >= 1600:
                        text = f"{self.prefix}你好".strip()
                    else:
                        text = f"{self.prefix}你".strip()
                    return [
                        {
                            "text": text,
                            "timestamp": [[0, int(audio_len)]],
                            "words": [{"word": text, "probability": 0.9}],
                        }
                    ], {"frontend_size": frontend.output_size() if frontend else None}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    return model_dir


def _make_local_temp_dir() -> Path:
    root = Path.cwd() / ".pytest_tmp"
    root.mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(prefix="funasr-test-", dir=str(root)))


class TestTranscriptionResult:
    """转录结果测试"""
    
    def test_duration(self):
        """测试时长计算"""
        result = TranscriptionResult(
            text="Hello world",
            language="en",
            confidence=0.95,
            start_time=0.0,
            end_time=2.5,
        )
        
        assert result.duration == 2.5


class TestASREngine:
    """ASR引擎测试"""
    
    def test_backend_types(self):
        """测试后端类型"""
        assert "faster-whisper" in ASREngine.BACKENDS
        assert "whisper" in ASREngine.BACKENDS
        assert "funasr" in ASREngine.BACKENDS
        assert "qwen3-asr" in ASREngine.BACKENDS
        assert "sherpa-onnx" in ASREngine.BACKENDS
        assert "wenet" in ASREngine.BACKENDS
    
    @patch('localtrans.asr.engine.FasterWhisperBackend')
    def test_engine_init(self, mock_backend):
        """测试引擎初始化"""
        config = ASRConfig(model_type="faster-whisper", model_size="tiny")
        
        # 不实际加载模型，只测试配置
        mock_backend.return_value = Mock()
        
        with patch.object(ASREngine, '_load_backend'):
            engine = ASREngine(config)
            engine._backend = mock_backend.return_value
            
            assert engine.config.model_type == "faster-whisper"

    def test_wenet_ctc_recognizer_builder(self):
        """测试sherpa-onnx wenet-ctc识别器构造参数"""

        class FakeOfflineRecognizer:
            called_kwargs = None

            @staticmethod
            def from_wenet_ctc(**kwargs):
                FakeOfflineRecognizer.called_kwargs = kwargs
                return object()

        class FakeSherpaModule:
            OfflineRecognizer = FakeOfflineRecognizer

        backend = SherpaOnnxBackend.__new__(SherpaOnnxBackend)
        backend.config = ASRConfig(model_type="wenet", device="cpu")
        backend._sample_rate = 16000
        backend._find_first_existing = (
            lambda base, candidates: Path("tokens.txt")
            if "tokens.txt" in candidates
            else Path("model.onnx")
        )

        recognizer = backend._build_wenet_ctc_recognizer(FakeSherpaModule, Path("."))

        assert recognizer is not None
        assert FakeOfflineRecognizer.called_kwargs is not None
        assert FakeOfflineRecognizer.called_kwargs["tokens"] == "tokens.txt"
        assert FakeOfflineRecognizer.called_kwargs["model"] == "model.onnx"


class TestFunASRDirectRuntime:
    """FunASR直连加载测试"""

    def test_detect_funasr_package_dir_uses_cached_module_path(self):
        package_dir = Path.cwd() / "tests"
        original = sys.modules.get("funasr")
        fake_module = type(sys)("funasr")
        fake_module.__path__ = [str(package_dir)]
        fake_module.__spec__ = None
        sys.modules["funasr"] = fake_module

        try:
            detected = FunASRDirectRuntime._detect_funasr_package_dir()
        finally:
            if original is not None:
                sys.modules["funasr"] = original
            else:
                sys.modules.pop("funasr", None)

        assert detected == package_dir.resolve()

    def test_direct_runtime_loads_local_model_dir(self, monkeypatch):
        work_dir = _make_local_temp_dir()
        package_dir = _write_fake_funasr_package(work_dir / "fake_pkg")
        model_dir = _write_fake_funasr_model(work_dir / "fake_model")

        def _fake_detect(_cls):
            return package_dir

        _reset_funasr_runtime()
        monkeypatch.setattr(
            FunASRDirectRuntime,
            "_detect_funasr_package_dir",
            classmethod(_fake_detect),
        )

        try:
            runtime = FunASRDirectRuntime(str(model_dir), device="cpu", language="zh")
            result = runtime.infer(np.ones(3200, dtype=np.float32))
        finally:
            _reset_funasr_runtime()
            shutil.rmtree(work_dir, ignore_errors=True)

        assert result["text"] == "你好世界"
        assert result["timestamps"] == [[0, 3200]]
        assert result["words"][0]["probability"] == pytest.approx(0.9)
        assert result["meta"]["frontend_size"] == 80

    @pytest.mark.parametrize("backend_cls", [FunASRBackend, Qwen3ASRBackend])
    def test_funasr_backends_use_direct_runtime(self, monkeypatch, backend_cls):
        work_dir = _make_local_temp_dir()
        package_dir = _write_fake_funasr_package(work_dir / "fake_pkg")
        model_dir = _write_fake_funasr_model(work_dir / "fake_model")

        def _fake_detect(_cls):
            return package_dir

        _reset_funasr_runtime()
        monkeypatch.setattr(
            FunASRDirectRuntime,
            "_detect_funasr_package_dir",
            classmethod(_fake_detect),
        )

        try:
            backend = backend_cls(
                ASRConfig(
                    model_type="qwen3-asr" if backend_cls is Qwen3ASRBackend else "funasr",
                    model_path=model_dir,
                    language="zh",
                    device="cpu",
                    word_timestamps=True,
                )
            )
            result = backend.transcribe(np.ones(3200, dtype=np.float32))
        finally:
            _reset_funasr_runtime()
            shutil.rmtree(work_dir, ignore_errors=True)

        assert result.text == "你好世界"
        assert result.confidence == pytest.approx(0.9)
        assert result.words == [{"word": "你好世界", "probability": 0.9}]
        assert result.end_time == pytest.approx(0.2)


class TestStreamingASR:
    """流式ASR测试"""
    
    def test_init(self):
        """测试初始化"""
        mock_engine = Mock()
        streaming = StreamingASR(asr_engine=mock_engine)
        
        assert streaming.state == StreamState.IDLE
        assert streaming.buffer_duration == 2.0
        assert streaming.overlap_duration == 0.5
    
    def test_put_audio_when_not_running(self):
        """测试非运行状态下添加音频"""
        mock_engine = Mock()
        streaming = StreamingASR(asr_engine=mock_engine)
        
        audio = np.random.randint(-1000, 1000, size=1600, dtype=np.int16)
        streaming.put_audio(audio)
        
        # 队列应该为空，因为没有运行
        assert streaming._audio_queue.empty()

    def test_buffer_does_not_grow_unbounded_after_transcribe_error(self):
        """转录异常后应推进窗口，避免缓冲无限增长"""
        call_lengths = []

        class FailingEngine:
            def transcribe(self, audio):
                call_lengths.append(len(audio))
                raise RuntimeError("mock transcribe failure")

        streaming = StreamingASR(
            asr_engine=FailingEngine(),
            buffer_duration=0.1,
            overlap_duration=0.05,
        )
        streaming.start()

        try:
            chunk = np.random.randint(-1000, 1000, size=1600, dtype=np.int16)
            for _ in range(16):
                streaming.put_audio(chunk)
                time.sleep(0.01)
            time.sleep(0.2)
        finally:
            streaming.stop()

        assert len(call_lengths) >= 3
        # 正常推进时每次处理片段长度应保持在小窗口范围，不会线性失控增长
        assert max(call_lengths) < 16000

    def test_vad_skips_silence_before_speech(self):
        """VAD开启时，纯静音不应触发ASR调用"""
        class CountingEngine:
            def __init__(self):
                self.calls = 0

            def transcribe(self, audio):
                self.calls += 1
                return TranscriptionResult(
                    text="测试",
                    language="zh",
                    confidence=1.0,
                    start_time=0.0,
                    end_time=len(audio) / 16000.0,
                )

        engine = CountingEngine()
        streaming = StreamingASR(
            asr_engine=engine,
            buffer_duration=0.2,
            overlap_duration=0.0,
            min_buffer_duration=0.1,
            max_buffer_duration=0.2,
            vad_enabled=True,
            vad_energy_threshold=0.05,
            vad_silence_duration=0.05,
        )
        streaming.start()
        try:
            silence = np.zeros(800, dtype=np.int16)
            for _ in range(6):
                streaming.put_audio(silence)
                time.sleep(0.01)
            time.sleep(0.1)
            assert engine.calls == 0

            speech = np.full(2000, 22000, dtype=np.int16)
            streaming.put_audio(speech)
            streaming.put_audio(speech)
            time.sleep(0.2)
            assert engine.calls >= 1
        finally:
            streaming.stop()

    def test_streaming_marks_partial_and_final_results(self):
        """连续语音应先产出partial，再在空闲时产出final"""

        class EchoEngine:
            def __init__(self):
                self.calls = 0

            def transcribe(self, audio):
                self.calls += 1
                return TranscriptionResult(
                    text="你好世界",
                    language="zh",
                    confidence=1.0,
                    start_time=0.0,
                    end_time=len(audio) / 16000.0,
                )

        engine = EchoEngine()
        streaming = StreamingASR(
            asr_engine=engine,
            buffer_duration=0.1,
            overlap_duration=0.0,
            min_buffer_duration=0.08,
            max_buffer_duration=0.4,
            streaming_mode="managed",
            vad_enabled=False,
            partial_decode_interval=0.08,
        )
        streaming.start()
        try:
            speech = np.full(800, 22000, dtype=np.int16)
            streaming.put_audio(speech)
            streaming.put_audio(speech)
            time.sleep(0.3)

            results = []
            while True:
                item = streaming.get_result(timeout=0.05)
                if item is None:
                    break
                results.append(item)
        finally:
            streaming.stop()

        assert engine.calls >= 2
        assert any(result.is_final is False for result in results)
        assert results[-1].is_final is True

    def test_legacy_mode_keeps_final_only_results(self):
        """legacy 模式保留原始切窗方案，只产出 final 结果。"""

        class EchoEngine:
            def __init__(self):
                self.calls = 0

            def transcribe(self, audio):
                self.calls += 1
                return TranscriptionResult(
                    text="legacy",
                    language="en",
                    confidence=1.0,
                    start_time=0.0,
                    end_time=len(audio) / 16000.0,
                )

        engine = EchoEngine()
        streaming = StreamingASR(
            asr_engine=engine,
            buffer_duration=0.1,
            overlap_duration=0.0,
            min_buffer_duration=0.05,
            max_buffer_duration=0.12,
            streaming_mode="legacy",
            vad_enabled=False,
        )
        streaming.start()
        try:
            speech = np.full(800, 22000, dtype=np.int16)
            streaming.put_audio(speech)
            streaming.put_audio(speech)
            time.sleep(0.2)

            results = []
            while True:
                item = streaming.get_result(timeout=0.05)
                if item is None:
                    break
                results.append(item)
        finally:
            streaming.stop()

        assert engine.calls >= 1
        assert results
        assert all(result.is_final is True for result in results)
