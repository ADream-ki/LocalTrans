"""
AudioDeviceViewModel - 音频设备 ViewModel

生产级 I/O 选择与深度控制入口（设备选择 + I/O 档位 + 缓冲/增益）。
"""

from typing import Optional, List, Dict, Any

from PySide6.QtCore import QObject, Signal, Property, Slot
from loguru import logger

from localtrans.services.audio_device_service import AudioDeviceService
from localtrans.services.audio_io_service import AudioIOService


class AudioDeviceViewModel(QObject):
    """音频设备 ViewModel。"""

    inputDevicesChanged = Signal()
    outputDevicesChanged = Signal()
    inputDeviceChanged = Signal(str)
    outputDeviceChanged = Signal(str)
    ioChanged = Signal()
    errorOccurred = Signal(str)

    def __init__(self, parent: Optional[QObject] = None):
        super().__init__(parent)
        self._service = AudioDeviceService.get_instance()
        self._io_service = AudioIOService.get_instance()
        self._input_devices: List[Dict[str, Any]] = []
        self._output_devices: List[Dict[str, Any]] = []
        self._selected_input: str = ""
        self._selected_output: str = ""
        self._load_audio_controls()
        self._refresh_devices()

    def _load_audio_controls(self) -> None:
        state = self._io_service.get_control_state()
        self._io_profile = str(state["io_profile"])
        self._output_mode = str(state["output_mode"])
        self._io_buffer_ms = int(state["io_buffer_ms"])
        self._monitoring_enabled = bool(state["monitoring_enabled"])
        self._input_gain_db = float(state["input_gain_db"])
        self._output_gain_db = float(state["output_gain_db"])

    def _persist_audio_controls(self) -> None:
        self._io_service.update_control_state(
            io_profile=self._io_profile,
            output_mode=self._output_mode,
            io_buffer_ms=self._io_buffer_ms,
            monitoring_enabled=self._monitoring_enabled,
            input_gain_db=self._input_gain_db,
            output_gain_db=self._output_gain_db,
        )

    def _refresh_devices(self) -> None:
        try:
            self._service.refresh()
            self._input_devices = [
                {
                    "id": str(dev.id),
                    "name": dev.name,
                    "channels": dev.channels,
                    "sample_rate": dev.sample_rate,
                    "is_default": dev.is_default,
                }
                for dev in self._service.input_devices
            ]
            self._output_devices = [
                {
                    "id": str(dev.id),
                    "name": dev.name,
                    "channels": dev.channels,
                    "sample_rate": dev.sample_rate,
                    "is_default": dev.is_default,
                }
                for dev in self._service.output_devices
            ]

            selected_input = self._service.selected_input
            selected_output = self._service.selected_output
            self._selected_input = str(selected_input.id) if selected_input else ""
            self._selected_output = str(selected_output.id) if selected_output else ""

            self.inputDevicesChanged.emit()
            self.outputDevicesChanged.emit()
            self.inputDeviceChanged.emit(self._selected_input)
            self.outputDeviceChanged.emit(self._selected_output)
            logger.debug(f"发现 {len(self._input_devices)} 个输入设备, {len(self._output_devices)} 个输出设备")
        except Exception as e:
            logger.exception(f"枚举音频设备失败: {e}")
            self.errorOccurred.emit(str(e))

    @Slot(result=list)
    def getInputDevices(self) -> List[Dict[str, Any]]:
        return self._input_devices

    @Slot(result=list)
    def getOutputDevices(self) -> List[Dict[str, Any]]:
        return self._output_devices

    @Slot()
    def refresh(self):
        self._refresh_devices()

    @Slot(str)
    def selectInput(self, device_id: str):
        try:
            dev_id = int(device_id)
        except Exception:
            return
        if self._service.select_input(dev_id):
            self._selected_input = str(dev_id)
            self.inputDeviceChanged.emit(self._selected_input)

    @Slot(str)
    def selectOutput(self, device_id: str):
        try:
            dev_id = int(device_id)
        except Exception:
            return
        if self._service.select_output(dev_id):
            self._selected_output = str(dev_id)
            self.outputDeviceChanged.emit(self._selected_output)

    @Property(str, notify=inputDeviceChanged)
    def selectedInput(self) -> str:
        return self._selected_input

    @Property(str, notify=outputDeviceChanged)
    def selectedOutput(self) -> str:
        return self._selected_output

    @Property(str, notify=ioChanged)
    def ioProfile(self) -> str:
        return self._io_profile

    @ioProfile.setter
    def ioProfile(self, value: str):
        value = (value or "balanced").strip().lower()
        if value not in {"realtime", "balanced", "studio"}:
            value = "balanced"
        if self._io_profile != value:
            self._io_profile = value
            self._persist_audio_controls()
            self.ioChanged.emit()

    @Property(str, notify=ioChanged)
    def outputMode(self) -> str:
        return self._output_mode

    @outputMode.setter
    def outputMode(self, value: str):
        value = (value or "virtual").strip().lower()
        if value not in {"virtual", "device", "system"}:
            value = "virtual"
        if self._output_mode != value:
            self._output_mode = value
            self._persist_audio_controls()
            self.ioChanged.emit()

    @Property(int, notify=ioChanged)
    def ioBufferMs(self) -> int:
        return self._io_buffer_ms

    @ioBufferMs.setter
    def ioBufferMs(self, value: int):
        value = int(max(20, min(300, value)))
        if self._io_buffer_ms != value:
            self._io_buffer_ms = value
            self._persist_audio_controls()
            self.ioChanged.emit()

    @Property(bool, notify=ioChanged)
    def monitoringEnabled(self) -> bool:
        return self._monitoring_enabled

    @monitoringEnabled.setter
    def monitoringEnabled(self, value: bool):
        new_val = bool(value)
        if self._monitoring_enabled != new_val:
            self._monitoring_enabled = new_val
            self._persist_audio_controls()
            self.ioChanged.emit()

    @Property(float, notify=ioChanged)
    def inputGainDb(self) -> float:
        return float(self._input_gain_db)

    @inputGainDb.setter
    def inputGainDb(self, value: float):
        v = float(max(-24.0, min(24.0, value)))
        if self._input_gain_db != v:
            self._input_gain_db = v
            self._persist_audio_controls()
            self.ioChanged.emit()

    @Property(float, notify=ioChanged)
    def outputGainDb(self) -> float:
        return float(self._output_gain_db)

    @outputGainDb.setter
    def outputGainDb(self, value: float):
        v = float(max(-24.0, min(24.0, value)))
        if self._output_gain_db != v:
            self._output_gain_db = v
            self._persist_audio_controls()
            self.ioChanged.emit()
