"""
SessionViewModel - 会话视图模型

管理实时翻译会话的状态和控制。
"""

import time
from typing import Optional, List, Dict, Any
from dataclasses import dataclass
from enum import Enum

from PySide6.QtCore import QObject, Signal, Property, Slot, QTimer

from loguru import logger


class SessionState(Enum):
    """会话状态"""

    IDLE = "idle"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    ERROR = "error"


@dataclass
class TranslationSegment:
    """翻译段落"""

    source_text: str
    translated_text: str
    source_lang: str
    target_lang: str
    is_enhanced: bool = False
    direction: str = ""


class SessionViewModel(QObject):
    """会话视图模型。"""

    stateChanged = Signal()
    sourceLangChanged = Signal()
    targetLangChanged = Signal()
    currentTranscriptionChanged = Signal()
    currentTranslationChanged = Signal()
    historyChanged = Signal()
    errorOccurred = Signal(str)
    metricsChanged = Signal()
    runtimeSummaryChanged = Signal()
    routingChanged = Signal()

    def __init__(self, parent: Optional[QObject] = None):
        super().__init__(parent)

        self._state = SessionState.IDLE
        from localtrans.config import settings

        self._source_lang = settings.mt.source_lang
        self._target_lang = settings.mt.target_lang

        self._current_transcription = ""
        self._current_translation = ""
        self._is_enhanced = False

        self._history: List[TranslationSegment] = []
        self._max_history = 100
        self._history_page_size = 30
        self._visible_history_count = self._history_page_size

        self._estimated_latency_ms = 0.0
        self._session_started_ts = 0.0

        self._runtime_summary: Dict[str, Any] = {}
        self._runtime_summaries: List[Dict[str, Any]] = []
        self._pipeline_summary = ""

        self._pipeline = None
        self._orchestrator = None

        self._peer_input_device_id = ""
        self._peer_output_device_id = ""
        self._self_input_device_id = ""
        self._self_output_device_id = ""
        self._init_default_routes()

        self._status_timer = QTimer(self)
        self._status_timer.timeout.connect(self._update_status)
        self._status_timer.setInterval(500)

    @Property(str, notify=stateChanged)
    def state(self) -> str:
        return self._state.value

    @Property(str, notify=sourceLangChanged)
    def sourceLang(self) -> str:
        return self._source_lang

    @sourceLang.setter
    def sourceLang(self, value: str):
        if self._source_lang != value:
            self._source_lang = value
            self.sourceLangChanged.emit()

    @Property(str, notify=targetLangChanged)
    def targetLang(self) -> str:
        return self._target_lang

    @targetLang.setter
    def targetLang(self, value: str):
        if self._target_lang != value:
            self._target_lang = value
            self.targetLangChanged.emit()

    @Property(str, notify=currentTranscriptionChanged)
    def currentTranscription(self) -> str:
        return self._current_transcription

    @Property(str, notify=currentTranslationChanged)
    def currentTranslation(self) -> str:
        return self._current_translation

    @Property(bool, notify=currentTranslationChanged)
    def isEnhanced(self) -> bool:
        return self._is_enhanced

    @Property(list, notify=historyChanged)
    def history(self) -> List[dict]:
        visible = max(1, self._visible_history_count)
        return [
            {
                "source": seg.source_text,
                "translation": seg.translated_text,
                "sourceLang": seg.source_lang,
                "targetLang": seg.target_lang,
                "isEnhanced": seg.is_enhanced,
                "direction": seg.direction,
            }
            for seg in self._history[-visible:]
        ]

    @Property(bool, notify=historyChanged)
    def hasMoreHistory(self) -> bool:
        return len(self._history) > self._visible_history_count

    @Property(int, notify=historyChanged)
    def visibleHistoryCount(self) -> int:
        return min(len(self._history), self._visible_history_count)

    @Property(float, notify=metricsChanged)
    def estimatedLatencyMs(self) -> float:
        return float(self._estimated_latency_ms)

    @Property(str, notify=metricsChanged)
    def sessionDuration(self) -> str:
        if self._session_started_ts <= 0:
            return "00:00"
        elapsed = int(max(0.0, time.time() - self._session_started_ts))
        minutes = elapsed // 60
        seconds = elapsed % 60
        return f"{minutes:02d}:{seconds:02d}"

    @Property(str, notify=runtimeSummaryChanged)
    def pipelineSummary(self) -> str:
        return self._pipeline_summary

    @Property(bool, notify=stateChanged)
    def isRunning(self) -> bool:
        return self._state == SessionState.RUNNING

    @Property(bool, notify=stateChanged)
    def isIdle(self) -> bool:
        return self._state == SessionState.IDLE

    @Property(str, notify=routingChanged)
    def peerInputDeviceId(self) -> str:
        return self._peer_input_device_id

    @peerInputDeviceId.setter
    def peerInputDeviceId(self, value: str):
        self._set_route_value("peer_input", value)

    @Property(str, notify=routingChanged)
    def peerOutputDeviceId(self) -> str:
        return self._peer_output_device_id

    @peerOutputDeviceId.setter
    def peerOutputDeviceId(self, value: str):
        self._set_route_value("peer_output", value)

    @Property(str, notify=routingChanged)
    def selfInputDeviceId(self) -> str:
        return self._self_input_device_id

    @selfInputDeviceId.setter
    def selfInputDeviceId(self, value: str):
        self._set_route_value("self_input", value)

    @Property(str, notify=routingChanged)
    def selfOutputDeviceId(self) -> str:
        return self._self_output_device_id

    @selfOutputDeviceId.setter
    def selfOutputDeviceId(self, value: str):
        self._set_route_value("self_output", value)

    @Slot()
    def startSession(self):
        """开始双向翻译会话。"""
        if self._state != SessionState.IDLE:
            logger.warning(f"无法启动：当前状态为 {self._state}")
            return

        self._setState(SessionState.STARTING)

        try:
            from localtrans.pipeline.realtime import RealtimePipeline, RealtimeConfig, SessionOrchestrator
            from localtrans.config import settings
            from localtrans.services.audio_io_service import AudioIOService

            io_opts = AudioIOService.get_instance().build_runtime_options()

            peer_cfg = self._build_direction_config(
                RealtimeConfig,
                source_lang=self._target_lang,
                target_lang=self._source_lang,
                input_device_id=self._parse_device_id(self._peer_input_device_id),
                output_device_id=self._parse_device_id(self._peer_output_device_id),
                direction_label="对方→我",
                io_opts=io_opts,
            )
            self_cfg = self._build_direction_config(
                RealtimeConfig,
                source_lang=self._source_lang,
                target_lang=self._target_lang,
                input_device_id=self._parse_device_id(self._self_input_device_id),
                output_device_id=self._parse_device_id(self._self_output_device_id),
                direction_label="我→对方",
                io_opts=io_opts,
            )

            peer_pipeline = RealtimePipeline(
                config=peer_cfg,
                asr_config=settings.asr,
                mt_config=settings.mt,
                tts_config=settings.tts,
                result_callback=self._on_pipeline_result,
            )
            self_pipeline = RealtimePipeline(
                config=self_cfg,
                asr_config=settings.asr,
                mt_config=settings.mt,
                tts_config=settings.tts,
                result_callback=self._on_pipeline_result,
            )

            self._orchestrator = SessionOrchestrator(
                {"peer_to_me": peer_pipeline, "me_to_peer": self_pipeline},
                share_mt_engine=True,
                share_tts_engine=True,
                restart_enabled=True,
            )
            if not self._orchestrator.start():
                raise RuntimeError("双向会话启动失败，请检查 A/B/C/D 路由是否可用")

            self._pipeline = None
            self._setState(SessionState.RUNNING)
            self._session_started_ts = time.time()
            self._update_runtime_summary()
            self._status_timer.start()
            logger.info("双向会话已启动")

        except Exception as e:
            logger.exception(f"启动会话失败: {e}")
            self._setState(SessionState.ERROR)
            self.errorOccurred.emit(str(e))

    @Slot()
    def stopSession(self):
        """停止翻译会话。"""
        if self._state != SessionState.RUNNING:
            return

        self._setState(SessionState.STOPPING)
        self._status_timer.stop()

        try:
            if self._orchestrator:
                self._orchestrator.stop()
                self._orchestrator = None
            if self._pipeline:
                self._pipeline.stop()
                self._pipeline = None

            self._setState(SessionState.IDLE)
            self._session_started_ts = 0.0
            self._estimated_latency_ms = 0.0
            self._runtime_summary = {}
            self._runtime_summaries = []
            self._pipeline_summary = ""
            self.metricsChanged.emit()
            self.runtimeSummaryChanged.emit()
            logger.info("会话已停止")

        except Exception as e:
            logger.exception(f"停止会话失败: {e}")
            self._setState(SessionState.ERROR)

    @Slot()
    def swapLanguages(self):
        self._source_lang, self._target_lang = self._target_lang, self._source_lang
        self.sourceLangChanged.emit()
        self.targetLangChanged.emit()

    @Slot()
    def clearHistory(self):
        self._history.clear()
        self._visible_history_count = self._history_page_size
        self.historyChanged.emit()

    @Slot()
    def loadMoreHistory(self):
        if self.hasMoreHistory:
            self._visible_history_count += self._history_page_size
            self.historyChanged.emit()

    @Slot(result=str)
    def exportHistory(self) -> str:
        import json

        return json.dumps(self.history, ensure_ascii=False, indent=2)

    @Slot(str)
    def setPeerInputDeviceId(self, value: str):
        self.peerInputDeviceId = value

    @Slot(str)
    def setPeerOutputDeviceId(self, value: str):
        self.peerOutputDeviceId = value

    @Slot(str)
    def setSelfInputDeviceId(self, value: str):
        self.selfInputDeviceId = value

    @Slot(str)
    def setSelfOutputDeviceId(self, value: str):
        self.selfOutputDeviceId = value

    def _setState(self, state: SessionState):
        if self._state != state:
            self._state = state
            self.stateChanged.emit()

    def _on_pipeline_result(self, item: Dict[str, Any]) -> None:
        source = str(item.get("source", "") or "")
        translated = str(item.get("translation", "") or "")
        if not source and not translated:
            return

        direction = str(item.get("direction", "") or "")
        self._current_transcription = source
        self._current_translation = translated
        self._is_enhanced = bool(item.get("is_enhanced", False))
        self.currentTranscriptionChanged.emit()
        self.currentTranslationChanged.emit()

        latency = float(item.get("latency_ms", 0.0) or 0.0)
        if latency > 0:
            self._estimated_latency_ms = latency
            self.metricsChanged.emit()

        segment = TranslationSegment(
            source_text=source,
            translated_text=translated,
            source_lang=self._source_lang if direction == "我→对方" else self._target_lang,
            target_lang=self._target_lang if direction == "我→对方" else self._source_lang,
            is_enhanced=self._is_enhanced,
            direction=direction,
        )
        self._history.append(segment)
        if len(self._history) > self._max_history:
            self._history = self._history[-self._max_history :]
        self.historyChanged.emit()

    def _update_status(self):
        if self._orchestrator or self._pipeline:
            self._update_runtime_summary()
        self.metricsChanged.emit()

    def _update_runtime_summary(self):
        summary_text = ""
        if self._orchestrator and hasattr(self._orchestrator, "get_runtime_summaries"):
            try:
                self._runtime_summaries = self._orchestrator.get_runtime_summaries() or []
            except Exception:
                self._runtime_summaries = []
        elif self._pipeline and hasattr(self._pipeline, "get_runtime_summary"):
            try:
                self._runtime_summaries = [self._pipeline.get_runtime_summary() or {}]
            except Exception:
                self._runtime_summaries = []

        if self._runtime_summaries:
            parts: List[str] = []
            for one in self._runtime_summaries:
                direction = str(one.get("direction", "") or one.get("direction_name", ""))
                in_id = str(one.get("input_device_id", ""))
                out_id = str(one.get("output_device_id", ""))
                mode = str(one.get("output_mode", ""))
                parts.append(f"{direction}: in{in_id}->out{out_id}({mode})")
            summary_text = " || ".join(parts)

        if self._pipeline_summary != summary_text:
            self._pipeline_summary = summary_text
            self.runtimeSummaryChanged.emit()

    def _init_default_routes(self) -> None:
        try:
            from localtrans.services.audio_io_service import AudioIOService

            io_opts = AudioIOService.get_instance().build_runtime_options()
            default_in = "" if io_opts.input_device_id is None else str(io_opts.input_device_id)
            default_out = "" if io_opts.output_device_id is None else str(io_opts.output_device_id)
            self._peer_input_device_id = default_in
            self._peer_output_device_id = default_out
            self._self_input_device_id = default_in
            self._self_output_device_id = default_out
        except Exception:
            self._peer_input_device_id = ""
            self._peer_output_device_id = ""
            self._self_input_device_id = ""
            self._self_output_device_id = ""
        self.routingChanged.emit()

    def _set_route_value(self, route_name: str, value: str) -> None:
        normalized = str(value or "").strip()
        changed = False
        if route_name == "peer_input" and self._peer_input_device_id != normalized:
            self._peer_input_device_id = normalized
            changed = True
        elif route_name == "peer_output" and self._peer_output_device_id != normalized:
            self._peer_output_device_id = normalized
            changed = True
        elif route_name == "self_input" and self._self_input_device_id != normalized:
            self._self_input_device_id = normalized
            changed = True
        elif route_name == "self_output" and self._self_output_device_id != normalized:
            self._self_output_device_id = normalized
            changed = True

        if changed:
            self.routingChanged.emit()

    @staticmethod
    def _parse_device_id(value: str) -> Optional[int]:
        text = str(value or "").strip()
        if text == "":
            return None
        try:
            return int(text)
        except Exception:
            return None

    @staticmethod
    def _build_direction_config(
        cfg_cls,
        *,
        source_lang: str,
        target_lang: str,
        input_device_id: Optional[int],
        output_device_id: Optional[int],
        direction_label: str,
        io_opts,
    ):
        from localtrans.config import settings

        output_mode = "device" if output_device_id is not None else str(io_opts.output_mode or "system")
        return cfg_cls(
            source_lang=source_lang,
            target_lang=target_lang,
            enable_tts=bool(settings.tts.stream_enabled),
            output_mode=output_mode,
            output_device_id=output_device_id,
            input_device_id=input_device_id,
            io_buffer_ms=int(io_opts.io_buffer_ms),
            input_gain_db=float(io_opts.input_gain_db),
            output_gain_db=float(io_opts.output_gain_db),
            direction_label=direction_label,
        )
