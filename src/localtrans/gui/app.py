"""
主窗口GUI
使用PyQt6实现
"""

from __future__ import annotations

import json
import sys
import traceback
import importlib.util
import copy
from pathlib import Path
from typing import Optional

from loguru import logger

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtGui import QAction, QFont
from PyQt6.QtWidgets import (
    QApplication,
    QCheckBox,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFileDialog,
    QFormLayout,
    QFrame,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMainWindow,
    QMenu,
    QMessageBox,
    QPushButton,
    QSplitter,
    QSystemTrayIcon,
    QTabWidget,
    QTextEdit,
    QVBoxLayout,
    QWidget,
    QDoubleSpinBox,
    QSpinBox,
    QSizePolicy,
)

from localtrans import __version__
from localtrans.audio import AudioCapturer, VirtualAudioDevice
from localtrans.config import ASRConfig, MTConfig, TTSConfig, settings
from localtrans.mt import TermBankManager
from localtrans.pipeline.realtime import (
    RealtimeConfig,
    RealtimePipeline,
    SessionOrchestrator,
)
from localtrans.utils import ModelDownloader


class MainWindow(QMainWindow):
    """主窗口"""

    translation_signal = pyqtSignal(dict)

    LANG_OPTIONS = [
        ("英语", "en"),
        ("中文", "zh"),
        ("日语", "ja"),
        ("韩语", "ko"),
        ("法语", "fr"),
        ("德语", "de"),
    ]
    DEFAULT_ARGOS_MODELS = {
        ("zh", "en"): "argos-zh-en",
        ("en", "zh"): "argos-en-zh",
    }
    DEFAULT_PIPER_MODELS = {
        "zh": "piper-zh_CN-huayan",
        "en": "piper-en_US-lessac",
    }

    def __init__(self):
        super().__init__()

        self._pipeline: Optional[RealtimePipeline | SessionOrchestrator] = None
        self._config_path = settings.data_dir / "gui_config.json"
        self._config_tab: Optional[QWidget] = None
        self._model_downloader = ModelDownloader()

        self._init_ui()
        self.translation_signal.connect(self._append_result)
        self._load_device_list()
        self._load_downloadable_models()
        if self._config_path.exists():
            self._load_gui_config(silent=True)
        else:
            self._apply_bootstrap_runtime_defaults()
        self._check_environment()

    def _init_ui(self) -> None:
        """初始化UI"""
        self.setWindowTitle(f"LocalTrans - AI实时翻译 v{__version__}")
        self.setMinimumSize(980, 680)
        self.resize(1180, 760)

        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        main_layout = QVBoxLayout(central_widget)

        main_layout.addWidget(self._create_run_panel())

        tabs = QTabWidget()
        tabs.addTab(self._create_result_tab(), "实时结果")
        self._config_tab = self._create_config_tab()
        tabs.addTab(self._config_tab, "运行配置")
        main_layout.addWidget(tabs, stretch=1)

        self.statusBar().showMessage("就绪")
        self._create_menu()
        self._create_tray()

    def _create_run_panel(self) -> QGroupBox:
        """顶部运行控制面板"""
        group = QGroupBox("翻译控制")
        layout = QVBoxLayout(group)
        controls_layout = QHBoxLayout()

        controls_layout.addWidget(QLabel("源语言:"))
        self.source_lang_combo = QComboBox()
        self._populate_language_combo(self.source_lang_combo, default_code="zh")
        controls_layout.addWidget(self.source_lang_combo)

        controls_layout.addWidget(QLabel("目标语言:"))
        self.target_lang_combo = QComboBox()
        self._populate_language_combo(self.target_lang_combo, default_code="en")
        controls_layout.addWidget(self.target_lang_combo)

        self.tts_checkbox = QCheckBox("启用语音合成")
        self.tts_checkbox.setChecked(True)
        controls_layout.addWidget(self.tts_checkbox)

        controls_layout.addStretch()

        self.start_btn = QPushButton("▶ 开始翻译")
        self.start_btn.setMinimumWidth(140)
        self.start_btn.clicked.connect(self._toggle_translation)
        controls_layout.addWidget(self.start_btn)

        self.session_info_label = QLabel("当前会话: 未启动")
        self.session_info_label.setWordWrap(True)
        self.session_info_label.setStyleSheet("color: #455A64; padding-top: 4px;")

        layout.addLayout(controls_layout)
        layout.addWidget(self.session_info_label)

        return group

    def _create_result_tab(self) -> QWidget:
        """实时结果页面"""
        container = QWidget()
        layout = QVBoxLayout(container)

        splitter = QSplitter(Qt.Orientation.Vertical)

        source_frame = QFrame()
        source_layout = QVBoxLayout(source_frame)
        source_layout.setContentsMargins(0, 0, 0, 0)
        source_label = QLabel("识别原文")
        source_label.setStyleSheet("font-weight: bold; color: #666;")
        source_layout.addWidget(source_label)
        self.source_text = QTextEdit()
        self.source_text.setReadOnly(True)
        self.source_text.setFont(QFont("Microsoft YaHei", 11))
        self.source_text.setStyleSheet("background-color: #fafafa;")
        source_layout.addWidget(self.source_text)
        splitter.addWidget(source_frame)

        target_frame = QFrame()
        target_layout = QVBoxLayout(target_frame)
        target_layout.setContentsMargins(0, 0, 0, 0)
        target_label = QLabel("翻译结果")
        target_label.setStyleSheet("font-weight: bold; color: #1565C0;")
        target_layout.addWidget(target_label)
        self.target_text = QTextEdit()
        self.target_text.setReadOnly(True)
        self.target_text.setFont(QFont("Microsoft YaHei", 12))
        self.target_text.setStyleSheet("background-color: #f0f7ff;")
        target_layout.addWidget(self.target_text)
        splitter.addWidget(target_frame)

        splitter.setSizes([230, 310])
        layout.addWidget(splitter)

        btn_layout = QHBoxLayout()
        self.clear_btn = QPushButton("清空结果")
        self.clear_btn.clicked.connect(self._clear_display)
        btn_layout.addWidget(self.clear_btn)
        btn_layout.addStretch()
        layout.addLayout(btn_layout)

        return container

    def _create_config_tab(self) -> QWidget:
        """运行配置页面"""
        tab = QWidget()
        layout = QVBoxLayout(tab)

        model_group = QGroupBox("模型配置")
        model_form = QFormLayout(model_group)

        self.asr_backend_combo = QComboBox()
        self.asr_backend_combo.addItem("vosk", "vosk")
        self.asr_backend_combo.addItem("faster-whisper", "faster-whisper")
        if self._is_asr_backend_available("whisper"):
            self.asr_backend_combo.addItem("whisper", "whisper")
        if self._is_asr_backend_available("funasr"):
            self.asr_backend_combo.addItem("funasr", "funasr")
        if self._is_asr_backend_available("qwen3-asr"):
            self.asr_backend_combo.addItem("qwen3-asr", "qwen3-asr")
        if self._is_asr_backend_available("sherpa-onnx"):
            self.asr_backend_combo.addItem("sherpa-onnx", "sherpa-onnx")
        if self._is_asr_backend_available("wenet"):
            self.asr_backend_combo.addItem("wenet", "wenet")
        if not self._set_combo_data(self.asr_backend_combo, settings.asr.model_type):
            self._set_combo_data(self.asr_backend_combo, "faster-whisper")
        model_form.addRow("ASR后端", self.asr_backend_combo)

        self.asr_model_combo = QComboBox()
        self.asr_model_combo.setEditable(True)
        for size in (
            "tiny",
            "base",
            "small",
            "medium",
            "large",
            "large-v3",
            "large-v3-turbo",
            "distil-large-v3",
            "turbo",
            "FunAudioLLM/SenseVoiceSmall",
            "Qwen/Qwen3-ASR-0.6B",
            "Qwen/Qwen3-ASR-1.7B",
            "sherpa-onnx-zh-en-zipformer",
            "wenet-u2pp-cn",
        ):
            self.asr_model_combo.addItem(size, size)
        self._set_combo_data(self.asr_model_combo, settings.asr.model_size)
        model_form.addRow("ASR模型大小", self.asr_model_combo)

        self.direct_asr_translate_checkbox = QCheckBox("Whisper中译英直出（跳过MT）")
        self.direct_asr_translate_checkbox.setChecked(False)
        model_form.addRow("ASR直译", self.direct_asr_translate_checkbox)

        self.mt_backend_combo = QComboBox()
        self.mt_backend_combo.addItem("argos-ct2", "argos-ct2")
        self.mt_backend_combo.addItem("argos", "argos")
        self.mt_backend_combo.addItem("nllb", "nllb")
        self.mt_backend_combo.addItem("nllb-ct2", "nllb-ct2")
        self.mt_backend_combo.addItem("marian", "marian")
        self._set_combo_data(self.mt_backend_combo, settings.mt.model_type)
        model_form.addRow("MT后端", self.mt_backend_combo)

        self.mt_model_name_edit = QLineEdit(settings.mt.model_name)
        model_form.addRow("MT模型名称/路径", self.mt_model_name_edit)

        self.tts_engine_combo = QComboBox()
        self.tts_engine_combo.addItem("pyttsx3", "pyttsx3")
        self.tts_engine_combo.addItem("piper", "piper")
        self.tts_engine_combo.addItem("coqui", "coqui")
        self.tts_engine_combo.addItem("edge-tts", "edge-tts")
        self._set_combo_data(self.tts_engine_combo, settings.tts.engine)
        model_form.addRow("TTS引擎", self.tts_engine_combo)

        self.tts_model_name_edit = QLineEdit(settings.tts.model_name or "")
        model_form.addRow("TTS模型名称/路径", self.tts_model_name_edit)
        layout.addWidget(model_group)

        download_group = QGroupBox("模型下载与自动配置")
        download_form = QFormLayout(download_group)
        download_form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.AllNonFixedFieldsGrow)
        download_form.setRowWrapPolicy(QFormLayout.RowWrapPolicy.DontWrapRows)

        self.asr_download_combo = QComboBox()
        download_form.addRow("ASR可下载模型", self.asr_download_combo)

        self.mt_download_combo = QComboBox()
        download_form.addRow("MT可下载模型", self.mt_download_combo)

        self.tts_download_combo = QComboBox()
        download_form.addRow("TTS可下载模型", self.tts_download_combo)

        self.auto_download_checkbox = QCheckBox("应用/启动时自动下载并配置所选模型")
        self.auto_download_checkbox.setChecked(True)
        download_form.addRow("自动下载", self.auto_download_checkbox)

        download_btn_row = QHBoxLayout()
        download_btn_row.setContentsMargins(0, 0, 0, 0)
        download_btn_row.setSpacing(8)
        self.refresh_models_btn = QPushButton("刷新模型状态")
        self.refresh_models_btn.setMinimumWidth(120)
        self.refresh_models_btn.clicked.connect(self._load_downloadable_models)
        download_btn_row.addWidget(self.refresh_models_btn)

        self.download_selected_btn = QPushButton("下载并应用所选模型")
        self.download_selected_btn.setMinimumWidth(180)
        self.download_selected_btn.clicked.connect(lambda: self._ensure_selected_models_ready(show_dialog=True))
        download_btn_row.addWidget(self.download_selected_btn)
        download_btn_row.addStretch()
        download_form.addRow("操作", self._wrap_layout(download_btn_row))

        layout.addWidget(download_group)

        audio_group = QGroupBox("音频与延迟")
        audio_form = QFormLayout(audio_group)
        audio_form.setFieldGrowthPolicy(QFormLayout.FieldGrowthPolicy.AllNonFixedFieldsGrow)
        audio_form.setRowWrapPolicy(QFormLayout.RowWrapPolicy.DontWrapRows)

        device_row = QHBoxLayout()
        device_row.setContentsMargins(0, 0, 0, 0)
        device_row.setSpacing(8)
        self.input_device_combo = QComboBox()
        self.input_device_combo.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.input_device_combo.setMinimumWidth(300)
        device_row.addWidget(self.input_device_combo, stretch=1)
        refresh_btn = QPushButton("刷新")
        refresh_btn.setMinimumWidth(80)
        refresh_btn.clicked.connect(self._load_device_list)
        device_row.addWidget(refresh_btn)
        device_widget = QWidget()
        device_widget.setLayout(device_row)
        audio_form.addRow("输入设备", device_widget)

        self.output_virtual_checkbox = QCheckBox("输出到虚拟设备")
        self.output_virtual_checkbox.setChecked(True)
        audio_form.addRow("输出路由", self.output_virtual_checkbox)

        self.bidirectional_checkbox = QCheckBox("启用双向翻译")
        self.bidirectional_checkbox.setChecked(False)
        audio_form.addRow("会话模式", self.bidirectional_checkbox)

        self.reverse_input_device_combo = QComboBox()
        self.reverse_input_device_combo.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
        self.reverse_input_device_combo.setMinimumWidth(300)
        audio_form.addRow("对方语音设备", self.reverse_input_device_combo)

        self.runtime_profile_combo = QComboBox()
        self.runtime_profile_combo.addItem("实时优先 (<1s)", "realtime")
        self.runtime_profile_combo.addItem("平衡模式", "balanced")
        self.runtime_profile_combo.addItem("质量优先", "quality")
        self._set_combo_data(self.runtime_profile_combo, "realtime")
        audio_form.addRow("运行模式", self.runtime_profile_combo)

        self.asr_streaming_mode_combo = QComboBox()
        self.asr_streaming_mode_combo.addItem("原始切窗方案", "legacy")
        self.asr_streaming_mode_combo.addItem("本地状态机方案", "managed")
        self._set_combo_data(self.asr_streaming_mode_combo, "legacy")
        audio_form.addRow("ASR流式方案", self.asr_streaming_mode_combo)

        self.asr_vad_mode_combo = QComboBox()
        self.asr_vad_mode_combo.addItem("WebRTC VAD", "webrtc")
        self.asr_vad_mode_combo.addItem("能量阈值", "energy")
        self.asr_vad_mode_combo.addItem("Silero VAD", "silero")
        self._set_combo_data(self.asr_vad_mode_combo, "energy")
        audio_form.addRow("流式VAD", self.asr_vad_mode_combo)

        self.asr_buffer_spin = QDoubleSpinBox()
        self.asr_buffer_spin.setRange(0.3, 8.0)
        self.asr_buffer_spin.setSingleStep(0.1)
        self.asr_buffer_spin.setValue(0.6)
        audio_form.addRow("ASR缓冲时长(s)", self.asr_buffer_spin)

        self.asr_overlap_spin = QDoubleSpinBox()
        self.asr_overlap_spin.setRange(0.0, 2.0)
        self.asr_overlap_spin.setSingleStep(0.05)
        self.asr_overlap_spin.setValue(0.05)
        audio_form.addRow("ASR重叠时长(s)", self.asr_overlap_spin)

        self.asr_partial_step_spin = QDoubleSpinBox()
        self.asr_partial_step_spin.setRange(0.05, 3.0)
        self.asr_partial_step_spin.setSingleStep(0.05)
        self.asr_partial_step_spin.setValue(0.25)
        audio_form.addRow("Partial步长(s)", self.asr_partial_step_spin)

        self.max_queue_spin = QSpinBox()
        self.max_queue_spin.setRange(1, 100)
        self.max_queue_spin.setValue(2)
        audio_form.addRow("最大翻译队列", self.max_queue_spin)

        layout.addWidget(audio_group)

        btn_row = QHBoxLayout()
        self.fast_preset_btn = QPushButton("极速(<1s)预设")
        self.fast_preset_btn.clicked.connect(self._apply_ultra_low_latency_preset)
        btn_row.addWidget(self.fast_preset_btn)

        self.accuracy_preset_btn = QPushButton("高准确率预设")
        self.accuracy_preset_btn.clicked.connect(self._apply_high_accuracy_preset)
        btn_row.addWidget(self.accuracy_preset_btn)

        self.cn_asr_preset_btn = QPushButton("中文ASR增强预设")
        self.cn_asr_preset_btn.clicked.connect(self._apply_cn_asr_enhanced_preset)
        btn_row.addWidget(self.cn_asr_preset_btn)

        self.en_zh_voice_preset_btn = QPushButton("英转中语音预设")
        self.en_zh_voice_preset_btn.clicked.connect(self._apply_en_to_zh_voice_preset)
        btn_row.addWidget(self.en_zh_voice_preset_btn)

        self.load_cfg_btn = QPushButton("加载配置")
        self.load_cfg_btn.clicked.connect(lambda: self._load_gui_config(silent=False))
        btn_row.addWidget(self.load_cfg_btn)

        self.save_cfg_btn = QPushButton("保存配置")
        self.save_cfg_btn.clicked.connect(self._save_gui_config)
        btn_row.addWidget(self.save_cfg_btn)

        self.apply_cfg_btn = QPushButton("应用到当前会话")
        self.apply_cfg_btn.clicked.connect(self._apply_runtime_settings)
        btn_row.addWidget(self.apply_cfg_btn)
        btn_row.addStretch()
        layout.addLayout(btn_row)

        layout.addStretch()
        return tab

    def _apply_ultra_low_latency_preset(self) -> None:
        """一键应用低延迟预设（中->英）"""
        self._set_combo_data(self.source_lang_combo, "zh")
        self._set_combo_data(self.target_lang_combo, "en")
        self.tts_checkbox.setChecked(True)

        self._set_combo_data(self.asr_backend_combo, "faster-whisper")
        self._set_combo_data(self.asr_model_combo, "small")
        self.direct_asr_translate_checkbox.setChecked(False)
        self._set_combo_data(self.mt_backend_combo, "argos-ct2")
        self.mt_model_name_edit.setText("argos-zh-en")
        self._set_combo_data(self.tts_engine_combo, "piper")
        self.tts_model_name_edit.setText("piper-en_US-lessac")

        self._set_combo_data(self.asr_download_combo, "faster-whisper-small")
        self._set_combo_data(self.mt_download_combo, "argos-zh-en")
        self._set_combo_data(self.tts_download_combo, "piper-en_US-lessac")
        self.auto_download_checkbox.setChecked(True)

        self.output_virtual_checkbox.setChecked(True)
        self._set_combo_data(self.runtime_profile_combo, "realtime")
        self._set_combo_data(self.asr_streaming_mode_combo, "legacy")
        self._set_combo_data(self.asr_vad_mode_combo, "energy")
        self.asr_buffer_spin.setValue(0.45)
        self.asr_overlap_spin.setValue(0.06)
        self.asr_partial_step_spin.setValue(0.2)
        self.max_queue_spin.setValue(1)

        self.statusBar().showMessage("已应用极速(<1s)预设，建议直接点击开始翻译验证", 4000)

    def _apply_high_accuracy_preset(self) -> None:
        """一键应用高准确率预设（中->英）"""
        self._set_combo_data(self.source_lang_combo, "zh")
        self._set_combo_data(self.target_lang_combo, "en")
        self.tts_checkbox.setChecked(True)

        medium_dir = settings.models_dir / "asr" / "faster-whisper-medium"
        model_size = "medium" if medium_dir.exists() else "small"
        self._set_combo_data(self.asr_backend_combo, "faster-whisper")
        self._set_combo_data(self.asr_model_combo, model_size)
        self.direct_asr_translate_checkbox.setChecked(False)
        self._set_combo_data(self.mt_backend_combo, "argos-ct2")
        self.mt_model_name_edit.setText("argos-zh-en")
        self._set_combo_data(self.tts_engine_combo, "piper")
        self.tts_model_name_edit.setText("piper-en_US-lessac")

        self._set_combo_data(self.asr_download_combo, f"faster-whisper-{model_size}")
        self._set_combo_data(self.mt_download_combo, "argos-zh-en")
        self._set_combo_data(self.tts_download_combo, "piper-en_US-lessac")
        self.auto_download_checkbox.setChecked(True)

        self.output_virtual_checkbox.setChecked(True)
        self._set_combo_data(self.runtime_profile_combo, "quality")
        self._set_combo_data(self.asr_streaming_mode_combo, "managed")
        self._set_combo_data(self.asr_vad_mode_combo, "webrtc")
        self.asr_buffer_spin.setValue(0.85)
        self.asr_overlap_spin.setValue(0.12)
        self.asr_partial_step_spin.setValue(0.3)
        self.max_queue_spin.setValue(2)

        self.statusBar().showMessage("已应用高准确率预设（建议首次先测试30秒）", 4000)

    def _apply_cn_asr_enhanced_preset(self) -> None:
        """中文识别增强预设：优先SenseVoice，其次faster-whisper-medium/small。"""
        self._set_combo_data(self.source_lang_combo, "zh")
        self._set_combo_data(self.target_lang_combo, "en")
        self.tts_checkbox.setChecked(True)
        self.direct_asr_translate_checkbox.setChecked(False)
        self._set_combo_data(self.mt_backend_combo, "argos-ct2")
        self.mt_model_name_edit.setText("argos-zh-en")
        self._set_combo_data(self.tts_engine_combo, "piper")
        self.tts_model_name_edit.setText("piper-en_US-lessac")
        self._set_combo_data(self.mt_download_combo, "argos-zh-en")
        self._set_combo_data(self.tts_download_combo, "piper-en_US-lessac")
        self.auto_download_checkbox.setChecked(True)
        self.output_virtual_checkbox.setChecked(True)
        self._set_combo_data(self.runtime_profile_combo, "quality")
        self._set_combo_data(self.asr_streaming_mode_combo, "managed")
        self._set_combo_data(self.asr_vad_mode_combo, "energy")
        self.asr_buffer_spin.setValue(2.6)
        self.asr_overlap_spin.setValue(0.12)
        self.asr_partial_step_spin.setValue(0.45)
        self.max_queue_spin.setValue(4)

        if self._is_asr_backend_available("funasr"):
            self._set_combo_data(self.asr_backend_combo, "funasr")
            self._set_combo_data(self.asr_model_combo, "FunAudioLLM/SenseVoiceSmall")
            self._set_combo_data(self.asr_download_combo, "funasr-sensevoice-small")
            self.statusBar().showMessage("已应用中文ASR增强预设（SenseVoice，managed 2.6s 提升准确率）", 4500)
            return

        medium_dir = settings.models_dir / "asr" / "faster-whisper-medium"
        model_size = "medium" if medium_dir.exists() else "small"
        self._set_combo_data(self.asr_streaming_mode_combo, "legacy")
        self._set_combo_data(self.asr_backend_combo, "faster-whisper")
        self._set_combo_data(self.asr_model_combo, model_size)
        self._set_combo_data(self.asr_download_combo, f"faster-whisper-{model_size}")
        self.statusBar().showMessage("已应用中文ASR增强预设（faster-whisper）", 4500)

    def _apply_en_to_zh_voice_preset(self) -> None:
        """英文语音转中文语音预设：优先 SenseVoice，其次 faster-whisper。"""
        self._set_combo_data(self.source_lang_combo, "en")
        self._set_combo_data(self.target_lang_combo, "zh")
        self.tts_checkbox.setChecked(True)
        self.direct_asr_translate_checkbox.setChecked(False)
        self._set_combo_data(self.mt_backend_combo, "argos-ct2")
        self.mt_model_name_edit.setText("argos-en-zh")
        self._set_combo_data(self.tts_engine_combo, "piper")
        self.tts_model_name_edit.setText("piper-zh_CN-huayan")
        self._set_combo_data(self.mt_download_combo, "argos-en-zh")
        self._set_combo_data(self.tts_download_combo, "piper-zh_CN-huayan")
        self.auto_download_checkbox.setChecked(True)
        self.output_virtual_checkbox.setChecked(True)
        self._set_combo_data(self.runtime_profile_combo, "quality")
        self._set_combo_data(self.asr_streaming_mode_combo, "managed")
        self._set_combo_data(self.asr_vad_mode_combo, "energy")
        self.asr_buffer_spin.setValue(2.6)
        self.asr_overlap_spin.setValue(0.12)
        self.asr_partial_step_spin.setValue(0.45)
        self.max_queue_spin.setValue(4)

        if self._is_asr_backend_available("funasr"):
            self._set_combo_data(self.asr_backend_combo, "funasr")
            self._set_combo_data(self.asr_model_combo, "FunAudioLLM/SenseVoiceSmall")
            self._set_combo_data(self.asr_download_combo, "funasr-sensevoice-small")
            self.statusBar().showMessage("已应用英转中语音预设（SenseVoice managed 2.6s + Argos + Piper）", 4500)
            return

        medium_dir = settings.models_dir / "asr" / "faster-whisper-medium"
        model_size = "medium" if medium_dir.exists() else "small"
        self._set_combo_data(self.asr_streaming_mode_combo, "legacy")
        self._set_combo_data(self.asr_backend_combo, "faster-whisper")
        self._set_combo_data(self.asr_model_combo, model_size)
        self._set_combo_data(self.asr_download_combo, f"faster-whisper-{model_size}")
        self.statusBar().showMessage("已应用英转中语音预设（faster-whisper + Argos + Piper）", 4500)

    def _apply_bootstrap_runtime_defaults(self) -> None:
        """首次启动（无保存配置）时使用可落地的默认策略。"""
        self._set_combo_data(self.source_lang_combo, "zh")
        self._set_combo_data(self.target_lang_combo, "en")
        self.tts_checkbox.setChecked(True)
        self.direct_asr_translate_checkbox.setChecked(False)

        fw_small_dir = settings.models_dir / "asr" / "faster-whisper-small"
        funasr_dir = settings.models_dir / "asr" / "funasr-sensevoice-small"
        vosk_cn_dir = settings.models_dir / "asr" / "vosk-model-small-cn-0.22"
        if funasr_dir.exists() and self._is_asr_backend_available("funasr"):
            self._set_combo_data(self.asr_backend_combo, "funasr")
            self._set_combo_data(self.asr_model_combo, "FunAudioLLM/SenseVoiceSmall")
            self._set_combo_data(self.asr_download_combo, "funasr-sensevoice-small")
            self._set_combo_data(self.runtime_profile_combo, "quality")
            self._set_combo_data(self.asr_streaming_mode_combo, "managed")
            self._set_combo_data(self.asr_vad_mode_combo, "energy")
            self.asr_buffer_spin.setValue(2.6)
            self.asr_overlap_spin.setValue(0.12)
            self.asr_partial_step_spin.setValue(0.45)
            self.max_queue_spin.setValue(4)
        elif fw_small_dir.exists():
            self._set_combo_data(self.asr_backend_combo, "faster-whisper")
            self._set_combo_data(self.asr_model_combo, "small")
            self._set_combo_data(self.asr_download_combo, "faster-whisper-small")
        elif vosk_cn_dir.exists():
            self._set_combo_data(self.asr_backend_combo, "vosk")
            self._set_combo_data(self.asr_model_combo, "vosk-model-small-cn-0.22")
            self._set_combo_data(self.asr_download_combo, "vosk-model-small-cn-0.22")

        self._set_combo_data(self.mt_backend_combo, "argos-ct2")
        self.mt_model_name_edit.setText("argos-zh-en")
        self._set_combo_data(self.mt_download_combo, "argos-zh-en")
        self._set_combo_data(self.tts_engine_combo, "piper")
        self.tts_model_name_edit.setText("piper-en_US-lessac")
        self._set_combo_data(self.tts_download_combo, "piper-en_US-lessac")
        self.auto_download_checkbox.setChecked(True)
        self.output_virtual_checkbox.setChecked(True)
        if funasr_dir.exists() and self._is_asr_backend_available("funasr"):
            logger.info("未发现GUI配置文件，已应用启动默认配置(zh->en, funasr managed quality)")
            return

        if not (funasr_dir.exists() and self._is_asr_backend_available("funasr")):
            self._set_combo_data(self.runtime_profile_combo, "balanced")
            self._set_combo_data(self.asr_streaming_mode_combo, "legacy")
            self._set_combo_data(self.asr_vad_mode_combo, "energy")
            self.asr_buffer_spin.setValue(0.65)
            self.asr_overlap_spin.setValue(0.08)
            self.asr_partial_step_spin.setValue(0.25)
            self.max_queue_spin.setValue(2)
        logger.info("未发现GUI配置文件，已应用启动默认配置(zh->en, balanced)")

    def _create_menu(self) -> None:
        """创建菜单"""
        menubar = self.menuBar()

        file_menu = menubar.addMenu("文件(&F)")

        import_action = QAction("导入术语库...", self)
        import_action.triggered.connect(self._import_term_bank)
        file_menu.addAction(import_action)

        save_cfg_action = QAction("保存界面配置", self)
        save_cfg_action.triggered.connect(self._save_gui_config)
        file_menu.addAction(save_cfg_action)

        load_cfg_action = QAction("加载界面配置", self)
        load_cfg_action.triggered.connect(lambda: self._load_gui_config(silent=False))
        file_menu.addAction(load_cfg_action)

        file_menu.addSeparator()

        exit_action = QAction("退出", self)
        exit_action.triggered.connect(self.close)
        file_menu.addAction(exit_action)

        tools_menu = menubar.addMenu("工具(&T)")
        devices_action = QAction("音频设备...", self)
        devices_action.triggered.connect(self._show_devices)
        tools_menu.addAction(devices_action)

        help_menu = menubar.addMenu("帮助(&H)")
        about_action = QAction("关于", self)
        about_action.triggered.connect(self._show_about)
        help_menu.addAction(about_action)

    def _create_tray(self) -> None:
        """创建系统托盘"""
        self.tray_icon = QSystemTrayIcon(self)
        self.tray_icon.setToolTip("LocalTrans")

        tray_menu = QMenu()
        show_action = QAction("显示主窗口", self)
        show_action.triggered.connect(self.show)
        tray_menu.addAction(show_action)
        tray_menu.addSeparator()
        quit_action = QAction("退出", self)
        quit_action.triggered.connect(self._quit_app)
        tray_menu.addAction(quit_action)

        self.tray_icon.setContextMenu(tray_menu)
        self.tray_icon.activated.connect(self._on_tray_activated)

    def _populate_language_combo(self, combo: QComboBox, default_code: str) -> None:
        """填充语言下拉框"""
        combo.clear()
        for label, code in self.LANG_OPTIONS:
            combo.addItem(f"{label} ({code})", code)
        self._set_combo_data(combo, default_code)

    @staticmethod
    def _set_combo_data(combo: QComboBox, value) -> bool:
        """按 userData 设置下拉选中项"""
        for idx in range(combo.count()):
            if combo.itemData(idx) == value:
                combo.setCurrentIndex(idx)
                return True
        if combo.isEditable() and value is not None:
            combo.setEditText(str(value))
            return True
        return False

    @staticmethod
    def _module_available(module_name: str) -> bool:
        try:
            return importlib.util.find_spec(module_name) is not None
        except Exception:
            return False

    def _is_asr_backend_available(self, backend: str) -> bool:
        backend = (backend or "").lower()
        if backend in {"vosk", "faster-whisper"}:
            return True
        if backend == "whisper":
            # 打包版默认不包含 openai-whisper，避免用户误选后回退低精度后端
            if getattr(sys, "frozen", False):
                return False
            return self._module_available("whisper")
        if backend == "funasr":
            return self._module_available("funasr")
        if backend == "qwen3-asr":
            return self._module_available("funasr")
        if backend == "sherpa-onnx":
            return self._module_available("sherpa_onnx")
        if backend == "wenet":
            return self._module_available("sherpa_onnx")
        return False

    @staticmethod
    def _wrap_layout(layout: QHBoxLayout) -> QWidget:
        layout.setContentsMargins(0, 0, 0, 0)
        widget = QWidget()
        widget.setLayout(layout)
        return widget

    def _model_label(self, model_name: str) -> str:
        status = "[OK]" if self._model_downloader.is_downloaded(model_name) else "[  ]"
        return f"{status} {model_name}"

    def _load_downloadable_models(self) -> None:
        """加载可下载模型列表"""
        if not hasattr(self, "asr_download_combo"):
            return

        selected_asr = self.asr_download_combo.currentData()
        selected_mt = self.mt_download_combo.currentData()
        selected_tts = self.tts_download_combo.currentData()

        self.asr_download_combo.clear()
        self.mt_download_combo.clear()
        self.tts_download_combo.clear()

        self.asr_download_combo.addItem("不使用（保持当前）", "")
        self.mt_download_combo.addItem("不使用（保持当前）", "")
        self.tts_download_combo.addItem("不使用（保持当前）", "")

        for model in self._model_downloader.list_available("asr"):
            if model.name.startswith("whisper-") and not self._is_asr_backend_available("whisper"):
                continue
            if model.name.startswith("funasr-") and not self._is_asr_backend_available("funasr"):
                continue
            if model.name.startswith("qwen3-asr-") and not self._is_asr_backend_available("qwen3-asr"):
                continue
            if model.name.startswith("sherpa-onnx-") and not self._is_asr_backend_available("sherpa-onnx"):
                continue
            if model.name.startswith("wenet-") and not self._is_asr_backend_available("wenet"):
                continue
            self.asr_download_combo.addItem(self._model_label(model.name), model.name)
        for model in self._model_downloader.list_available("mt"):
            self.mt_download_combo.addItem(self._model_label(model.name), model.name)
        for model in self._model_downloader.list_available("tts"):
            self.tts_download_combo.addItem(self._model_label(model.name), model.name)

        self._set_combo_data(self.asr_download_combo, selected_asr or "")
        self._set_combo_data(self.mt_download_combo, selected_mt or "")
        self._set_combo_data(self.tts_download_combo, selected_tts or "")

    def _selected_download_models(self) -> list[str]:
        selected = [
            self.asr_download_combo.currentData(),
            self.mt_download_combo.currentData(),
            self.tts_download_combo.currentData(),
        ]
        # 保持顺序去重
        return [m for m in dict.fromkeys(selected) if m]

    def _resolve_piper_onnx(self, model_dir: Optional[Path]) -> Optional[Path]:
        if not model_dir or not model_dir.exists():
            return None
        candidates = sorted(model_dir.rglob("*.onnx"))
        return candidates[0] if candidates else None

    def _apply_downloaded_model_to_settings(self, model_name: str, model_path: Optional[Path]) -> None:
        """将下载模型自动映射到当前配置"""
        if model_name.startswith("vosk-model"):
            settings.asr.model_type = "vosk"
            settings.asr.model_name = None
            settings.asr.model_size = model_name
            settings.asr.model_path = model_path
            self._set_combo_data(self.asr_backend_combo, "vosk")
            self._set_combo_data(self.asr_model_combo, model_name)
            return

        if model_name.startswith("whisper-"):
            settings.asr.model_type = "whisper"
            settings.asr.model_size = model_name.replace("whisper-", "", 1)
            settings.asr.model_name = None
            settings.asr.model_path = None
            self._set_combo_data(self.asr_backend_combo, "whisper")
            self._set_combo_data(self.asr_model_combo, settings.asr.model_size)
            return

        if model_name.startswith("faster-whisper-"):
            settings.asr.model_type = "faster-whisper"
            settings.asr.model_size = model_name.replace("faster-whisper-", "", 1)
            settings.asr.model_name = None
            settings.asr.model_path = model_path
            self._set_combo_data(self.asr_backend_combo, "faster-whisper")
            self._set_combo_data(self.asr_model_combo, settings.asr.model_size)
            return

        if model_name.startswith("funasr-"):
            settings.asr.model_type = "funasr"
            settings.asr.model_name = str(model_path) if model_path else "FunAudioLLM/SenseVoiceSmall"
            settings.asr.model_size = settings.asr.model_name
            settings.asr.model_path = model_path
            self._set_combo_data(self.asr_backend_combo, "funasr")
            self._set_combo_data(self.asr_model_combo, settings.asr.model_size)
            return

        if model_name.startswith("qwen3-asr-"):
            settings.asr.model_type = "qwen3-asr"
            if "1.7" in model_name:
                default_ref = "Qwen/Qwen3-ASR-1.7B"
            else:
                default_ref = "Qwen/Qwen3-ASR-0.6B"
            settings.asr.model_name = default_ref
            settings.asr.model_size = default_ref
            settings.asr.model_path = model_path if model_path and model_path.exists() else None
            self._set_combo_data(self.asr_backend_combo, "qwen3-asr")
            self._set_combo_data(self.asr_model_combo, default_ref)
            return

        if model_name.startswith("sherpa-onnx-"):
            settings.asr.model_type = "sherpa-onnx"
            settings.asr.model_name = str(model_path) if model_path else model_name
            settings.asr.model_size = model_name
            settings.asr.model_path = model_path
            self._set_combo_data(self.asr_backend_combo, "sherpa-onnx")
            self._set_combo_data(self.asr_model_combo, settings.asr.model_size)
            return

        if model_name.startswith("wenet-"):
            settings.asr.model_type = "wenet"
            settings.asr.model_name = "wenet-u2pp-cn"
            settings.asr.model_size = "wenet-u2pp-cn"
            settings.asr.model_path = model_path if model_path and model_path.exists() else None
            self._set_combo_data(self.asr_backend_combo, "wenet")
            self._set_combo_data(self.asr_model_combo, "wenet-u2pp-cn")
            return

        if model_name.startswith("argos-"):
            settings.mt.model_type = "argos-ct2"
            settings.mt.model_name = model_name
            settings.mt.model_path = None
            self._set_combo_data(self.mt_backend_combo, "argos-ct2")
            self.mt_model_name_edit.setText(model_name)
            return

        if model_name.startswith("nllb-"):
            settings.mt.model_type = "nllb"
            settings.mt.model_name = model_name
            settings.mt.model_path = model_path
            self._set_combo_data(self.mt_backend_combo, "nllb")
            self.mt_model_name_edit.setText(model_name)
            return

        if model_name.startswith("piper-"):
            settings.tts.engine = "piper"
            settings.tts.model_name = model_name
            settings.tts.model_path = self._resolve_piper_onnx(model_path)
            self._set_combo_data(self.tts_engine_combo, "piper")
            if settings.tts.model_path:
                self.tts_model_name_edit.setText(str(settings.tts.model_path))
            else:
                self.tts_model_name_edit.setText(model_name)

    def _ensure_selected_models_ready(self, show_dialog: bool = False) -> bool:
        """下载并应用当前选中的模型"""
        selected = self._selected_download_models()
        if not selected:
            if show_dialog:
                QMessageBox.information(self, "提示", "未选择需要下载的模型。")
            return True

        downloaded_or_ready: list[str] = []
        failed: list[tuple[str, str]] = []
        degraded: list[str] = []

        for model_name in selected:
            try:
                self.statusBar().showMessage(f"准备模型: {model_name} ...")
                QApplication.processEvents()

                if self._model_downloader.is_downloaded(model_name):
                    model_path = self._model_downloader.get_model_path(model_name)
                else:
                    model_path = self._model_downloader.download_model(model_name)

                self._apply_downloaded_model_to_settings(model_name, model_path)
                downloaded_or_ready.append(model_name)
            except Exception as exc:
                failed.append((model_name, str(exc)))
                logger.error(f"模型准备失败 {model_name}: {exc}")

        self._load_downloadable_models()

        remaining_failed: list[str] = []
        for model_name, reason in failed:
            handled = False

            # 高准确率常用链路：medium 下载失败时自动降级 small，避免阻断启动。
            if (
                model_name == "faster-whisper-medium"
                and settings.asr.model_type == "faster-whisper"
                and str(settings.asr.model_size).lower() == "medium"
            ):
                fw_small = settings.models_dir / "asr" / "faster-whisper-small"
                if fw_small.exists():
                    settings.asr.model_size = "small"
                    settings.asr.model_path = fw_small
                    self._set_combo_data(self.asr_backend_combo, "faster-whisper")
                    self._set_combo_data(self.asr_model_combo, "small")
                    self._set_combo_data(self.asr_download_combo, "faster-whisper-small")
                    degraded.append("faster-whisper-medium -> faster-whisper-small")
                    handled = True

            if not handled:
                remaining_failed.append(f"{model_name}: {reason}")

        if remaining_failed:
            if show_dialog:
                QMessageBox.warning(self, "模型准备失败", "\n".join(remaining_failed))
            self.statusBar().showMessage("模型准备失败", 4000)
            return False

        if degraded:
            logger.warning(f"模型自动降级: {', '.join(degraded)}")
            if show_dialog:
                QMessageBox.information(self, "提示", "部分模型下载失败，已自动降级：\n" + "\n".join(degraded))
            self.statusBar().showMessage("模型下载失败，已自动降级到可用模型", 4500)

        if degraded:
            self.statusBar().showMessage("模型部分下载失败，已自动降级并完成配置", 4500)
        else:
            self.statusBar().showMessage("模型已自动下载并配置完成", 4000)
        if show_dialog:
            QMessageBox.information(
                self,
                "完成",
                "以下模型已就绪并应用：\n" + "\n".join(downloaded_or_ready),
            )
        return True

    def _load_device_list(self) -> None:
        """加载输入设备列表"""
        selected = self.input_device_combo.currentData() if hasattr(self, "input_device_combo") else None
        reverse_selected = (
            self.reverse_input_device_combo.currentData()
            if hasattr(self, "reverse_input_device_combo")
            else None
        )
        self.input_device_combo.clear()
        self.input_device_combo.addItem("系统默认 / 自动选择", None)
        if hasattr(self, "reverse_input_device_combo"):
            self.reverse_input_device_combo.clear()
            self.reverse_input_device_combo.addItem("自动选择(会议回录)", None)

        try:
            capturer = AudioCapturer()
            for dev in capturer.list_devices():
                label = f"[{dev['id']}] {dev['name']} ({dev['channels']}ch/{int(dev['sample_rate'])}Hz)"
                self.input_device_combo.addItem(label, dev["id"])
                if hasattr(self, "reverse_input_device_combo"):
                    self.reverse_input_device_combo.addItem(label, dev["id"])
        except Exception as exc:
            logger.error(f"加载设备列表失败: {exc}")

        if selected is not None:
            self._set_combo_data(self.input_device_combo, selected)
        if reverse_selected is not None and hasattr(self, "reverse_input_device_combo"):
            self._set_combo_data(self.reverse_input_device_combo, reverse_selected)

    def _check_environment(self) -> None:
        """检查环境"""
        if not VirtualAudioDevice.check_vb_cable_installed():
            reply = QMessageBox.question(
                self,
                "虚拟声卡未安装",
                "未检测到虚拟声卡(VB-Cable)。\n\n是否查看安装指南？",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            )

            if reply == QMessageBox.StandardButton.Yes:
                QMessageBox.information(self, "安装指南", VirtualAudioDevice.get_installation_guide())

    def _collect_gui_config(self) -> dict:
        """采集当前界面配置"""
        return {
            "config_version": 6,
            "source_lang": self.source_lang_combo.currentData(),
            "target_lang": self.target_lang_combo.currentData(),
            "enable_tts": self.tts_checkbox.isChecked(),
            "asr_backend": self.asr_backend_combo.currentData(),
            "asr_model_size": self.asr_model_combo.currentData() or self.asr_model_combo.currentText().strip(),
            "direct_asr_translate": self.direct_asr_translate_checkbox.isChecked(),
            "mt_backend": self.mt_backend_combo.currentData(),
            "mt_model_name": self.mt_model_name_edit.text().strip(),
            "tts_engine": self.tts_engine_combo.currentData(),
            "tts_model_name": self.tts_model_name_edit.text().strip(),
            "asr_download_model": self.asr_download_combo.currentData(),
            "mt_download_model": self.mt_download_combo.currentData(),
            "tts_download_model": self.tts_download_combo.currentData(),
            "auto_download_models": self.auto_download_checkbox.isChecked(),
            "input_device_id": self.input_device_combo.currentData(),
            "bidirectional_enabled": self.bidirectional_checkbox.isChecked(),
            "reverse_input_device_id": self.reverse_input_device_combo.currentData(),
            "output_to_virtual_device": self.output_virtual_checkbox.isChecked(),
            "runtime_profile": self.runtime_profile_combo.currentData(),
            "asr_streaming_mode": self.asr_streaming_mode_combo.currentData(),
            "asr_vad_mode": self.asr_vad_mode_combo.currentData(),
            "asr_buffer_duration": self.asr_buffer_spin.value(),
            "asr_overlap": self.asr_overlap_spin.value(),
            "asr_partial_decode_interval": self.asr_partial_step_spin.value(),
            "max_translation_queue": self.max_queue_spin.value(),
        }

    def _apply_gui_config(self, cfg: dict) -> None:
        """应用界面配置"""
        config_version = int(cfg.get("config_version", 1))
        asr_backend = cfg.get("asr_backend", settings.asr.model_type)
        asr_model_size = cfg.get("asr_model_size", "base")
        asr_download_model = cfg.get("asr_download_model", "")

        # v4之前默认偏向vosk，历史配置在部分机器会出现ASR队列拥塞；迁移到 faster-whisper-small。
        if config_version < 4 and asr_backend == "vosk":
            fw_small_dir = settings.models_dir / "asr" / "faster-whisper-small"
            if fw_small_dir.exists():
                asr_backend = "faster-whisper"
                asr_model_size = "small"
                if not asr_download_model or str(asr_download_model).startswith("vosk-model"):
                    asr_download_model = "faster-whisper-small"

        self._set_combo_data(self.source_lang_combo, cfg.get("source_lang", "zh"))
        self._set_combo_data(self.target_lang_combo, cfg.get("target_lang", "en"))
        self.tts_checkbox.setChecked(bool(cfg.get("enable_tts", True)))

        if not self._set_combo_data(self.asr_backend_combo, asr_backend):
            fallback_backend = "faster-whisper" if self._is_asr_backend_available("faster-whisper") else "vosk"
            self._set_combo_data(self.asr_backend_combo, fallback_backend)
        self._set_combo_data(self.asr_model_combo, asr_model_size)
        self.direct_asr_translate_checkbox.setChecked(bool(cfg.get("direct_asr_translate", False)))
        self._set_combo_data(self.mt_backend_combo, cfg.get("mt_backend", settings.mt.model_type))
        self.mt_model_name_edit.setText(cfg.get("mt_model_name", settings.mt.model_name))
        self._set_combo_data(self.tts_engine_combo, cfg.get("tts_engine", settings.tts.engine))
        self.tts_model_name_edit.setText(cfg.get("tts_model_name", settings.tts.model_name or ""))
        self._set_combo_data(self.asr_download_combo, asr_download_model)
        self._set_combo_data(self.mt_download_combo, cfg.get("mt_download_model", ""))
        self._set_combo_data(self.tts_download_combo, cfg.get("tts_download_model", ""))
        self.auto_download_checkbox.setChecked(bool(cfg.get("auto_download_models", True)))

        self._set_combo_data(self.input_device_combo, cfg.get("input_device_id"))
        self.bidirectional_checkbox.setChecked(bool(cfg.get("bidirectional_enabled", False)))
        self._set_combo_data(self.reverse_input_device_combo, cfg.get("reverse_input_device_id"))
        self.output_virtual_checkbox.setChecked(bool(cfg.get("output_to_virtual_device", True)))
        self._set_combo_data(self.runtime_profile_combo, cfg.get("runtime_profile", "realtime"))
        self._set_combo_data(self.asr_streaming_mode_combo, cfg.get("asr_streaming_mode", "legacy"))
        default_vad_mode = "energy" if self.asr_streaming_mode_combo.currentData() == "legacy" else "webrtc"
        self._set_combo_data(self.asr_vad_mode_combo, cfg.get("asr_vad_mode", default_vad_mode))
        asr_buffer = float(cfg.get("asr_buffer_duration", 0.6))
        asr_overlap = float(cfg.get("asr_overlap", 0.05))
        asr_partial_step = float(cfg.get("asr_partial_decode_interval", 0.25))
        max_queue = int(cfg.get("max_translation_queue", 2))

        # 兼容旧配置：如果仍是历史默认慢参数，自动迁移为低延迟参数
        if (
            config_version < 2
            and abs(asr_buffer - 1.5) < 1e-6
            and abs(asr_overlap - 0.3) < 1e-6
            and max_queue == 10
        ):
            asr_buffer = 0.6
            asr_overlap = 0.05
            max_queue = 2

        self.asr_buffer_spin.setValue(asr_buffer)
        self.asr_overlap_spin.setValue(asr_overlap)
        self.asr_partial_step_spin.setValue(asr_partial_step)
        self.max_queue_spin.setValue(max_queue)

    @staticmethod
    def _runtime_profile_tuning(
        profile: str,
        asr_buffer_duration: float,
    ) -> dict:
        profile = (profile or "realtime").lower()
        if profile == "quality":
            return {
                "stream_flush_interval": 0.32,
                "stream_min_chars": 4,
                "stream_max_chars": 22,
                "stream_agreement": 2,
                "translation_batch_chars": 64,
                "tts_merge_chars": 180,
                "asr_beam_size": 3,
                "asr_backend_vad_filter": True,
                "asr_vad_enabled": True,
                "asr_vad_energy_threshold": 0.008,
                "asr_vad_silence_duration": 0.24,
                "asr_min_buffer_duration": max(0.45, asr_buffer_duration * 0.7),
                "asr_max_buffer_duration": max(asr_buffer_duration, 1.0),
                "min_asr_confidence": 0.10,
                "min_cjk_ratio": 0.15,
                "drop_hallucination": True,
            }
        if profile == "balanced":
            return {
                "stream_flush_interval": 0.25,
                "stream_min_chars": 3,
                "stream_max_chars": 16,
                "stream_agreement": 2,
                "translation_batch_chars": 42,
                "tts_merge_chars": 120,
                "asr_beam_size": 2,
                "asr_backend_vad_filter": True,
                "asr_vad_enabled": True,
                "asr_vad_energy_threshold": 0.01,
                "asr_vad_silence_duration": 0.2,
                "asr_min_buffer_duration": max(0.35, asr_buffer_duration * 0.6),
                "asr_max_buffer_duration": max(asr_buffer_duration, 0.8),
                "min_asr_confidence": 0.08,
                "min_cjk_ratio": 0.1,
                "drop_hallucination": True,
            }
        return {
            "stream_flush_interval": 0.22,
            "stream_min_chars": 3,
            "stream_max_chars": 14,
            "stream_agreement": 1,
            "translation_batch_chars": 28,
            "tts_merge_chars": 80,
            "asr_beam_size": 2,
            "asr_backend_vad_filter": True,
            "asr_vad_enabled": True,
            "asr_vad_energy_threshold": 0.015,
            "asr_vad_silence_duration": 0.2,
            "asr_min_buffer_duration": max(0.32, asr_buffer_duration * 0.65),
            "asr_max_buffer_duration": max(asr_buffer_duration, 0.7),
            "min_asr_confidence": 0.06,
            "min_cjk_ratio": 0.1,
            "drop_hallucination": True,
        }

    def _save_gui_config(self) -> None:
        """保存界面配置到本地"""
        cfg = self._collect_gui_config()
        try:
            self._config_path.parent.mkdir(parents=True, exist_ok=True)
            with self._config_path.open("w", encoding="utf-8") as fh:
                json.dump(cfg, fh, ensure_ascii=False, indent=2)
            self.statusBar().showMessage(f"配置已保存: {self._config_path}", 3000)
        except Exception as exc:
            logger.error(f"保存配置失败: {exc}")
            QMessageBox.warning(self, "保存失败", str(exc))

    def _load_gui_config(self, silent: bool = True) -> None:
        """从本地加载界面配置"""
        if not self._config_path.exists():
            if not silent:
                QMessageBox.information(self, "提示", "尚未发现已保存的配置文件。")
            return

        try:
            with self._config_path.open("r", encoding="utf-8") as fh:
                cfg = json.load(fh)
            self._apply_gui_config(cfg)
            self.statusBar().showMessage(f"配置已加载: {self._config_path}", 3000)
        except Exception as exc:
            logger.error(f"加载配置失败: {exc}")
            QMessageBox.warning(self, "加载失败", str(exc))

    def _apply_runtime_settings(self) -> RealtimeConfig:
        """把UI配置应用到运行时 settings，并返回流水线配置"""
        source_lang = self.source_lang_combo.currentData()
        settings.asr.model_type = self.asr_backend_combo.currentData()
        asr_model_value = self.asr_model_combo.currentData() or self.asr_model_combo.currentText().strip()

        if not self._is_asr_backend_available(settings.asr.model_type):
            requested_backend = settings.asr.model_type
            fallback_backend = "faster-whisper" if self._is_asr_backend_available("faster-whisper") else "vosk"
            settings.asr.model_type = fallback_backend
            self._set_combo_data(self.asr_backend_combo, fallback_backend)
            if fallback_backend == "faster-whisper":
                asr_model_value = "small"
                self._set_combo_data(self.asr_model_combo, asr_model_value)
                if self.auto_download_checkbox.isChecked() and not self.asr_download_combo.currentData():
                    self._set_combo_data(self.asr_download_combo, "faster-whisper-small")
            else:
                src = (source_lang or "").lower()
                asr_model_value = "vosk-model-small-cn-0.22" if src.startswith("zh") else "vosk-model-small-en-us-0.15"
                self._set_combo_data(self.asr_model_combo, asr_model_value)
                if self.auto_download_checkbox.isChecked():
                    self._set_combo_data(self.asr_download_combo, asr_model_value)
            logger.warning(f"ASR后端不可用({requested_backend})，已自动切换为 {fallback_backend}")
            self.statusBar().showMessage(f"ASR后端 {requested_backend} 不可用，已切换为 {fallback_backend}", 4500)

        if settings.asr.model_type == "vosk":
            src = (source_lang or "").lower()
            if src.startswith("zh"):
                recommended_vosk_model = "vosk-model-small-cn-0.22"
            elif src.startswith("en"):
                recommended_vosk_model = "vosk-model-small-en-us-0.15"
            else:
                recommended_vosk_model = "vosk-model-small-en-us-0.15"

            looks_like_path = (
                bool(asr_model_value)
                and ("\\" in asr_model_value or "/" in asr_model_value)
            )
            if not asr_model_value or asr_model_value in {"tiny", "base", "small", "medium", "large"}:
                asr_model_value = recommended_vosk_model
            elif (src.startswith("zh") and ("cn" not in asr_model_value and "zh" not in asr_model_value)) and not looks_like_path:
                asr_model_value = recommended_vosk_model
            elif (src.startswith("en") and "en" not in asr_model_value) and not looks_like_path:
                asr_model_value = recommended_vosk_model

            self._set_combo_data(self.asr_model_combo, asr_model_value)
            if self.auto_download_checkbox.isChecked():
                self._set_combo_data(self.asr_download_combo, asr_model_value)

        settings.asr.model_size = asr_model_value
        asr_path_candidate = Path(asr_model_value) if asr_model_value else None
        if settings.asr.model_type == "funasr":
            settings.asr.model_name = asr_model_value or "FunAudioLLM/SenseVoiceSmall"
            if asr_path_candidate and asr_path_candidate.exists():
                settings.asr.model_path = asr_path_candidate
            elif settings.asr.model_path and not Path(settings.asr.model_path).exists():
                settings.asr.model_path = None
        elif settings.asr.model_type == "qwen3-asr":
            model_ref = asr_model_value or "Qwen/Qwen3-ASR-0.6B"
            settings.asr.model_size = model_ref

            alias_candidates = [str(model_ref)]
            model_ref_lower = str(model_ref).lower()
            if any(key in model_ref_lower for key in {"1.7", "1_7", "1-7"}):
                alias_candidates.append("qwen3-asr-1.7b")
            else:
                alias_candidates.append("qwen3-asr-0.6b")

            resolved_path = None
            if asr_path_candidate and asr_path_candidate.exists():
                resolved_path = asr_path_candidate
            else:
                for alias in alias_candidates:
                    local_path = settings.models_dir / "asr" / alias
                    if local_path.exists():
                        resolved_path = local_path
                        break

            settings.asr.model_path = resolved_path
            settings.asr.model_name = str(resolved_path) if resolved_path else str(model_ref)
        elif settings.asr.model_type == "sherpa-onnx":
            settings.asr.model_name = asr_model_value or "sherpa-onnx-zh-en-zipformer"
            if asr_path_candidate and asr_path_candidate.exists():
                settings.asr.model_path = asr_path_candidate
            else:
                local_sherpa = settings.models_dir / "asr" / asr_model_value
                settings.asr.model_path = local_sherpa if local_sherpa.exists() else None
        elif settings.asr.model_type == "wenet":
            model_name = asr_model_value or "wenet-u2pp-cn"
            settings.asr.model_name = model_name
            settings.asr.model_size = model_name

            resolved_path = None
            if asr_path_candidate and asr_path_candidate.exists():
                resolved_path = asr_path_candidate
            else:
                for local_name in (model_name, "wenet-u2pp-cn"):
                    local_path = settings.models_dir / "asr" / local_name
                    if local_path.exists():
                        resolved_path = local_path
                        break

            settings.asr.model_path = resolved_path
        else:
            settings.asr.model_name = None
            if asr_path_candidate and asr_path_candidate.exists():
                settings.asr.model_path = asr_path_candidate
            elif settings.asr.model_type == "faster-whisper":
                local_fw = settings.models_dir / "asr" / f"faster-whisper-{asr_model_value}"
                settings.asr.model_path = local_fw if local_fw.exists() else None
            elif settings.asr.model_type == "vosk":
                local_vosk = settings.models_dir / "asr" / asr_model_value
                settings.asr.model_path = local_vosk if local_vosk.exists() else None
            else:
                settings.asr.model_path = None

        settings.mt.model_type = self.mt_backend_combo.currentData()
        mt_model = self.mt_model_name_edit.text().strip()
        if mt_model:
            settings.mt.model_name = mt_model

        settings.tts.engine = self.tts_engine_combo.currentData()
        tts_model = self.tts_model_name_edit.text().strip()
        settings.tts.model_name = tts_model or None
        settings.tts.language = self.target_lang_combo.currentData()

        if self.auto_download_checkbox.isChecked():
            if not self._ensure_selected_models_ready(show_dialog=False):
                logger.warning("部分模型下载/配置失败，继续使用本地已安装模型启动")
                self.statusBar().showMessage("部分模型下载失败，已使用本地可用模型继续", 4500)

        profile = self.runtime_profile_combo.currentData() or "realtime"
        asr_buffer = float(self.asr_buffer_spin.value())
        tuning = self._runtime_profile_tuning(profile, asr_buffer)
        direct_asr_translate = bool(self.direct_asr_translate_checkbox.isChecked())
        if direct_asr_translate and settings.asr.model_type not in {"whisper", "faster-whisper"}:
            logger.warning("ASR直译仅支持 whisper/faster-whisper，自动关闭")
            direct_asr_translate = False
        if direct_asr_translate and (self.target_lang_combo.currentData() or "").lower() != "en":
            logger.warning("ASR直译当前仅支持目标英语，自动关闭")
            direct_asr_translate = False

        settings.asr.task = "translate" if direct_asr_translate else "transcribe"
        settings.asr.beam_size = int(tuning["asr_beam_size"])
        settings.asr.vad_filter = bool(tuning["asr_backend_vad_filter"])

        realtime_config = RealtimeConfig(
            source_lang=source_lang,
            target_lang=self.target_lang_combo.currentData(),
            enable_tts=self.tts_checkbox.isChecked(),
            direct_asr_translate=direct_asr_translate,
            output_to_virtual_device=self.output_virtual_checkbox.isChecked(),
            asr_streaming_mode=self.asr_streaming_mode_combo.currentData() or "legacy",
            asr_vad_mode=self.asr_vad_mode_combo.currentData() or "webrtc",
            asr_buffer_duration=asr_buffer,
            asr_overlap=float(self.asr_overlap_spin.value()),
            asr_partial_decode_interval=float(self.asr_partial_step_spin.value()),
            max_translation_queue=int(self.max_queue_spin.value()),
            stream_profile=profile,
            stream_flush_interval=float(tuning["stream_flush_interval"]),
            stream_min_chars=int(tuning["stream_min_chars"]),
            stream_max_chars=int(tuning["stream_max_chars"]),
            stream_agreement=int(tuning["stream_agreement"]),
            translation_batch_chars=int(tuning["translation_batch_chars"]),
            tts_merge_chars=int(tuning["tts_merge_chars"]),
            asr_vad_enabled=bool(tuning["asr_vad_enabled"]),
            asr_vad_energy_threshold=float(tuning["asr_vad_energy_threshold"]),
            asr_vad_silence_duration=float(tuning["asr_vad_silence_duration"]),
            asr_min_buffer_duration=float(tuning["asr_min_buffer_duration"]),
            asr_max_buffer_duration=float(tuning["asr_max_buffer_duration"]),
            min_asr_confidence=float(tuning["min_asr_confidence"]),
            min_cjk_ratio=float(tuning["min_cjk_ratio"]),
            drop_hallucination=bool(tuning["drop_hallucination"]),
            input_device_id=self.input_device_combo.currentData(),
        )
        logger.info(
            "运行配置: "
            f"src={realtime_config.source_lang}, tgt={realtime_config.target_lang}, "
            f"asr={settings.asr.model_type}/{settings.asr.model_size}, asr_path={settings.asr.model_path}, "
            f"streaming={realtime_config.asr_streaming_mode}/{realtime_config.asr_vad_mode}, "
            f"mt={settings.mt.model_type}/{settings.mt.model_name}, "
            f"tts={settings.tts.engine}/{settings.tts.model_name}, "
            f"profile={realtime_config.stream_profile}, input_device={realtime_config.input_device_id}"
        )
        self.statusBar().showMessage(f"配置已应用（{profile}）", 2500)
        return realtime_config

    @classmethod
    def _default_mt_model_name(cls, source_lang: str, target_lang: str) -> Optional[str]:
        return cls.DEFAULT_ARGOS_MODELS.get(((source_lang or "").lower(), (target_lang or "").lower()))

    @classmethod
    def _default_piper_model_name(cls, language: str) -> Optional[str]:
        return cls.DEFAULT_PIPER_MODELS.get((language or "").lower())

    def _ensure_bidirectional_models_ready(self, source_lang: str, target_lang: str) -> tuple[Optional[str], Optional[str]]:
        reverse_mt_model = None
        reverse_tts_model = None

        mt_backend = str(settings.mt.model_type or "").lower()
        if mt_backend in {"argos-ct2", "argos"}:
            reverse_mt_model = self._default_mt_model_name(target_lang, source_lang)
            if reverse_mt_model and self.auto_download_checkbox.isChecked():
                try:
                    if not self._model_downloader.is_downloaded(reverse_mt_model):
                        self.statusBar().showMessage(f"正在准备双向翻译模型: {reverse_mt_model}", 3000)
                        self._model_downloader.download_model(reverse_mt_model)
                except Exception as exc:
                    logger.warning(f"准备双向MT模型失败({reverse_mt_model}): {exc}")

        tts_engine = str(settings.tts.engine or "").lower()
        if tts_engine == "piper":
            reverse_tts_model = self._default_piper_model_name(source_lang)
            if reverse_tts_model and self.auto_download_checkbox.isChecked():
                try:
                    if not self._model_downloader.is_downloaded(reverse_tts_model):
                        self.statusBar().showMessage(f"正在准备双向语音模型: {reverse_tts_model}", 3000)
                        self._model_downloader.download_model(reverse_tts_model)
                except Exception as exc:
                    logger.warning(f"准备双向TTS模型失败({reverse_tts_model}): {exc}")

        return reverse_mt_model, reverse_tts_model

    def _create_direction_pipeline(
        self,
        base_config: RealtimeConfig,
        *,
        source_lang: str,
        target_lang: str,
        input_device_id: Optional[int],
        output_to_virtual_device: bool,
        direction_label: str,
        mt_model_name: Optional[str] = None,
        tts_model_name: Optional[str] = None,
    ) -> RealtimePipeline:
        pipeline_config = copy.deepcopy(base_config)
        pipeline_config.source_lang = source_lang
        pipeline_config.target_lang = target_lang
        pipeline_config.input_device_id = input_device_id
        pipeline_config.output_to_virtual_device = output_to_virtual_device
        pipeline_config.direction_label = direction_label
        pipeline_config.asr_backend_type = settings.asr.model_type

        asr_config = settings.asr.model_copy(deep=True)
        asr_config.language = source_lang

        mt_config = settings.mt.model_copy(deep=True)
        mt_config.source_lang = source_lang
        mt_config.target_lang = target_lang
        if mt_model_name:
            mt_config.model_name = mt_model_name

        tts_config = settings.tts.model_copy(deep=True)
        tts_config.language = target_lang
        if tts_model_name is not None:
            tts_config.model_name = tts_model_name or None

        return RealtimePipeline(
            config=pipeline_config,
            result_callback=lambda item: self.translation_signal.emit(item),
            asr_config=asr_config,
            mt_config=mt_config,
            tts_config=tts_config,
        )

    def _set_session_info_text(self, text: str) -> None:
        if hasattr(self, "session_info_label") and self.session_info_label is not None:
            self.session_info_label.setText(text)

    @staticmethod
    def _format_runtime_summary(summary: dict) -> str:
        direction = str(summary.get("direction", "")).strip()
        prefix = f"{direction}: " if direction else ""

        input_device = summary.get("input_device_id")
        input_label = f"[{input_device}]" if input_device is not None else "自动"

        output_mode = str(summary.get("output_mode", "") or "speaker")
        output_device_id = summary.get("output_device_id")
        if output_mode == "virtual-device":
            output_label = f"虚拟声卡[{output_device_id}]"
        elif output_mode == "speaker-fallback":
            output_label = "默认扬声器(虚拟声卡不可用)"
        elif output_mode == "disabled":
            output_label = "关闭"
        else:
            output_label = "默认扬声器"

        asr_model = str(summary.get("asr_model", "") or "-")
        mt_model = str(summary.get("mt_model", "") or "-")
        if summary.get("tts_enabled"):
            tts_label = f"{summary.get('tts_engine')}/{summary.get('tts_model') or '-'}"
        else:
            tts_label = "off"

        return (
            f"{prefix}{summary.get('source_lang')}->{summary.get('target_lang')} | "
            f"输入{input_label} | 输出{output_label} | "
            f"ASR {summary.get('asr_backend')}/{asr_model} | "
            f"MT {summary.get('mt_backend')}/{mt_model} | "
            f"TTS {tts_label}"
        )

    def _update_session_runtime_info(self, runner: Optional[RealtimePipeline | SessionOrchestrator]) -> None:
        if runner is None:
            MainWindow._set_session_info_text(self, "当前会话: 未启动")
            return

        if isinstance(runner, SessionOrchestrator):
            summaries = runner.get_runtime_summaries()
        else:
            summaries = [runner.get_runtime_summary()]

        if not summaries:
            MainWindow._set_session_info_text(self, "当前会话: 未启动")
            return

        lines = [MainWindow._format_runtime_summary(summary) for summary in summaries]
        MainWindow._set_session_info_text(self, "当前会话: " + " | ".join(lines))

    def _toggle_translation(self) -> None:
        """切换翻译状态"""
        if self._pipeline and self._pipeline.is_running:
            self._stop_translation()
        else:
            self._start_translation()

    def _start_translation(self) -> None:
        """开始翻译"""
        try:
            self.statusBar().showMessage("正在启动...")
            realtime_config = self._apply_runtime_settings()

            if self.bidirectional_checkbox.isChecked():
                if realtime_config.input_device_id is None:
                    QMessageBox.information(
                        self,
                        "需要本地麦克风",
                        "双向模式下，请在“输入设备”里显式选择你的麦克风；\n"
                        "“系统默认 / 自动选择”会优先探测会议回录设备，不适合本地方向。",
                    )
                    return

                reverse_mt_model, reverse_tts_model = self._ensure_bidirectional_models_ready(
                    realtime_config.source_lang,
                    realtime_config.target_lang,
                )
                forward_pipeline = self._create_direction_pipeline(
                    realtime_config,
                    source_lang=realtime_config.source_lang,
                    target_lang=realtime_config.target_lang,
                    input_device_id=realtime_config.input_device_id,
                    output_to_virtual_device=bool(realtime_config.output_to_virtual_device),
                    direction_label="我→对方",
                )
                reverse_pipeline = self._create_direction_pipeline(
                    realtime_config,
                    source_lang=realtime_config.target_lang,
                    target_lang=realtime_config.source_lang,
                    input_device_id=self.reverse_input_device_combo.currentData(),
                    output_to_virtual_device=False,
                    direction_label="对方→我",
                    mt_model_name=reverse_mt_model,
                    tts_model_name=reverse_tts_model,
                )
                self._pipeline = SessionOrchestrator(
                    {
                        "forward": forward_pipeline,
                        "reverse": reverse_pipeline,
                    }
                )
            else:
                self._pipeline = self._create_direction_pipeline(
                    realtime_config,
                    source_lang=realtime_config.source_lang,
                    target_lang=realtime_config.target_lang,
                    input_device_id=realtime_config.input_device_id,
                    output_to_virtual_device=bool(realtime_config.output_to_virtual_device),
                    direction_label="",
                )

            if self._pipeline.start():
                MainWindow._update_session_runtime_info(self, self._pipeline)
                self.start_btn.setText("■ 停止翻译")
                self.start_btn.setStyleSheet("background-color: #f44336; color: white;")
                if self.bidirectional_checkbox.isChecked():
                    self.statusBar().showMessage("双向翻译中...")
                else:
                    self.statusBar().showMessage("翻译中...")

                self.source_lang_combo.setEnabled(False)
                self.target_lang_combo.setEnabled(False)
                self.tts_checkbox.setEnabled(False)
                if self._config_tab:
                    self._config_tab.setEnabled(False)
            else:
                MainWindow._update_session_runtime_info(self, None)
                self.statusBar().showMessage("启动失败")
                QMessageBox.warning(self, "错误", "翻译启动失败，请检查日志。")

        except Exception as exc:
            MainWindow._update_session_runtime_info(self, None)
            logger.error(f"启动失败: {exc}")
            QMessageBox.critical(self, "错误", f"启动失败: {exc}")

    def _stop_translation(self) -> None:
        """停止翻译"""
        if self._pipeline:
            self._pipeline.stop()
            self._pipeline = None

        self.start_btn.setText("▶ 开始翻译")
        self.start_btn.setStyleSheet("")
        MainWindow._update_session_runtime_info(self, None)
        self.statusBar().showMessage("已停止")

        self.source_lang_combo.setEnabled(True)
        self.target_lang_combo.setEnabled(True)
        self.tts_checkbox.setEnabled(True)
        if self._config_tab:
            self._config_tab.setEnabled(True)

    def _append_result(self, item: dict) -> None:
        """将流水线结果追加到界面"""
        source = item.get("source", "").strip()
        translation = item.get("translation", "").strip()
        direction = str(item.get("direction", "")).strip()
        prefix = f"[{direction}] " if direction else ""
        if source:
            self.source_text.append(f"{prefix}{source}")
        if translation:
            self.target_text.append(f"{prefix}{translation}")

    def _clear_display(self) -> None:
        """清空显示"""
        self.source_text.clear()
        self.target_text.clear()

    def _import_term_bank(self) -> None:
        """导入术语库"""
        filepath, _ = QFileDialog.getOpenFileName(
            self,
            "导入术语库",
            str(settings.data_dir),
            "JSON文件;;CSV文件;;所有文件",
        )

        if not filepath:
            return

        try:
            term_bank = TermBankManager()
            path = Path(filepath)

            if path.suffix.lower() == ".json":
                term_bank.load(path)
            else:
                term_bank.import_from_csv(path)

            QMessageBox.information(self, "导入成功", f"已导入 {len(term_bank)} 个术语")
        except Exception as exc:
            QMessageBox.warning(self, "导入失败", str(exc))

    def _show_devices(self) -> None:
        """显示音频设备"""
        dialog = QDialog(self)
        dialog.setWindowTitle("音频设备")
        dialog.setMinimumSize(620, 360)

        layout = QVBoxLayout(dialog)
        layout.addWidget(QLabel("可用输入设备:"))

        device_list = QListWidget()
        capturer = AudioCapturer()
        for dev in capturer.list_devices():
            device_list.addItem(
                QListWidgetItem(
                    f"[{dev['id']}] {dev['name']} | channels={dev['channels']} | sample_rate={int(dev['sample_rate'])}"
                )
            )
        layout.addWidget(device_list)

        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Close)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        dialog.exec()

    def _show_about(self) -> None:
        """显示关于对话框"""
        QMessageBox.about(
            self,
            "关于 LocalTrans",
            f"<h3>LocalTrans - AI实时翻译</h3>"
            f"<p>版本: {__version__}</p>"
            f"<p>支持端侧部署、可配置模型与音频链路。</p>"
            f"<p>GUI: PyQt6</p>",
        )

    def _on_tray_activated(self, reason) -> None:
        """托盘图标激活"""
        if reason == QSystemTrayIcon.ActivationReason.DoubleClick:
            self.show()

    def _quit_app(self) -> None:
        """退出应用"""
        self._stop_translation()
        QApplication.quit()

    def closeEvent(self, event) -> None:  # noqa: N802
        """关闭事件"""
        reply = QMessageBox.question(
            self,
            "确认退出",
            "确定要退出吗？",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
        )

        if reply == QMessageBox.StandardButton.Yes:
            self._stop_translation()
            event.accept()
        else:
            event.ignore()


def run_gui() -> None:
    """运行GUI应用"""
    # GUI模式下默认没有控制台，显式写入日志文件便于排障
    logger.remove()
    logger.add(
        settings.logs_dir / "localtrans_gui_{time}.log",
        level="DEBUG",
        rotation="10 MB",
        retention="7 days",
        encoding="utf-8",
    )
    try:
        app = QApplication(sys.argv)
        app.setApplicationName("LocalTrans")
        app.setApplicationVersion(__version__)
        app.setStyle("Fusion")

        window = MainWindow()
        window.show()

        sys.exit(app.exec())
    except Exception:
        logger.exception("GUI启动失败")
        fallback = settings.logs_dir / "localtrans_gui_bootstrap_error.log"
        try:
            fallback.write_text(traceback.format_exc(), encoding="utf-8")
        except Exception:
            pass
        raise


if __name__ == "__main__":
    run_gui()
