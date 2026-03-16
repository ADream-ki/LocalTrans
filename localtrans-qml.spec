# -*- mode: python ; coding: utf-8 -*-
"""
PyInstaller 打包配置文件 - QML GUI 版

支持：
- PySide6 + QML
- Loci 动态库
- 跨平台打包
"""

from pathlib import Path
import importlib.util
from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

project_root = Path(SPECPATH)

datas = []
binaries = []

# === Loci 动态库 ===
loci_native_dir = project_root / "src" / "localtrans" / "native" / "loci"
if loci_native_dir.exists():
    # 收集 Loci 动态库
    for dll in loci_native_dir.glob("*.dll"):
        binaries.append((str(dll), "localtrans/native/loci"))
    for so in loci_native_dir.glob("*.so"):
        binaries.append((str(so), "localtrans/native/loci"))
    for dylib in loci_native_dir.glob("*.dylib"):
        binaries.append((str(dylib), "localtrans/native/loci"))
    
    # 收集头文件（可选）
    for h in loci_native_dir.glob("*.h"):
        datas.append((str(h), "localtrans/native/loci"))

# === QML 资源 ===
qml_dir = project_root / "src" / "localtrans" / "ui" / "qml"
if qml_dir.exists():
    datas.append((str(qml_dir), "localtrans/ui/qml"))
    
# QML 主题目录
qml_theme_dir = project_root / "src" / "localtrans" / "ui" / "qml" / "theme"
if qml_theme_dir.exists():
    datas.append((str(qml_theme_dir), "localtrans/ui/qml/theme"))

# PySide6 QML 插件
datas += collect_data_files("PySide6", include_py_files=False)

# === 其他依赖 ===
datas += collect_data_files("vosk")
datas += collect_data_files("argostranslate")
datas += collect_data_files("piper")
datas += collect_data_files("faster_whisper")

if importlib.util.find_spec("funasr") is not None:
    datas += collect_data_files("funasr")

binaries += collect_dynamic_libs("vosk")
binaries += collect_dynamic_libs("ctranslate2")
binaries += collect_dynamic_libs("sentencepiece")
binaries += collect_dynamic_libs("piper")

if importlib.util.find_spec("sherpa_onnx") is not None:
    binaries += collect_dynamic_libs("sherpa_onnx")

# 类型标记文件
config_files = [
    ("src/localtrans/py.typed", "localtrans"),
]
for src, dst in config_files:
    if (project_root / src).exists():
        datas.append((src, dst))

# === 隐藏导入 ===
hiddenimports = [
    # LocalTrans 核心模块
    "localtrans",
    "localtrans.config",
    "localtrans.config.settings",
    "localtrans.config.models",
    
    # Loci 模块
    "localtrans.loci",
    "localtrans.loci.types",
    "localtrans.loci.runtime",
    
    # Services
    "localtrans.services",
    "localtrans.services.loci",
    "localtrans.services.loci.adapter",
    
    # UI 模块
    "localtrans",
    "localtrans.ui",
    "localtrans.ui.main",
    "localtrans.ui.viewmodels",
    "localtrans.ui.viewmodels.session_vm",
    "localtrans.ui.viewmodels.settings_vm",
    "localtrans.ui.viewmodels.model_vm",
    "localtrans.ui.viewmodels.audio_device_vm",
    "localtrans.ui.viewmodels.platform_vm",
    "localtrans.ui.bridge",
    "localtrans.ui.bridge.qt_bridge",
    
    # 音频模块
    "localtrans.audio",
    "localtrans.audio.capturer",
    "localtrans.audio.virtual_device",
    "localtrans.audio.router",
    "localtrans.audio.utils",
    "localtrans.audio.utils.vad",
    "localtrans.audio.utils.resampler",
    
    # ASR 模块
    "localtrans.asr",
    "localtrans.asr.engine",
    "localtrans.asr.funasr_direct",
    "localtrans.asr.stream",
    
    # MT 模块
    "localtrans.mt",
    "localtrans.mt.engine",
    "localtrans.mt.term_bank",
    
    # TTS 模块
    "localtrans.tts",
    "localtrans.tts.engine",
    "localtrans.tts.stream",
    
    # Pipeline
    "localtrans.pipeline",
    "localtrans.pipeline.translator",
    "localtrans.pipeline.realtime",
    
    # Utils
    "localtrans.utils",
    "localtrans.utils.monitor",
    "localtrans.utils.model_downloader",
    
    # PySide6
    "PySide6",
    "PySide6.QtCore",
    "PySide6.QtGui",
    "PySide6.QtWidgets",
    "PySide6.QtQml",
    "PySide6.QtQuick",
    "PySide6.QtQuickControls2",
    "PySide6.QtNetwork",
    
    # 其他依赖
    "pyttsx3",
    "pyttsx3.drivers",
    "pyttsx3.drivers.sapi5",
    "piper",
    "piper.voice",
    "piper.config",
    "faster_whisper",
    "faster_whisper.audio",
    "faster_whisper.vad",
    "faster_whisper.transcribe",
    "vosk",
    "vosk.vosk_cffi",
    "ctranslate2",
    "sentencepiece",
    "omegaconf",
    "omegaconf.grammar.gen.OmegaConfGrammarLexer",
    "omegaconf.grammar.gen.OmegaConfGrammarParser",
    "omegaconf.grammar.gen.OmegaConfGrammarParserVisitor",
    "kaldiio",
    "librosa",
    "argostranslate",
    "argostranslate.package",
    "argostranslate.translate",
    "argostranslate.settings",
    "argostranslate.tokenizer",
    
    # 标准库
    "dataclasses",
    "pydantic",
    "pydantic_settings",
    "pydantic_core",
    "annotated_types",
    "loguru",
    "numpy",
    "sounddevice",
    "soundfile",
    "_sounddevice_data",
    "_soundfile_data",
    "pycaw",
    "comtypes",
    "ctypes",
    "queue",
    "threading",
    "json",
    "pathlib",
    "typing",
    "collections",
    "enum",
    "time",
    "abc",
    "concurrent",
    "concurrent.futures",
]

if importlib.util.find_spec("funasr") is not None:
    hiddenimports += [
        "funasr.register",
        "funasr.train_utils.load_pretrained_model",
        "funasr.train_utils.set_all_random_seed",
        "funasr.utils.postprocess_utils",
        "funasr.models.paraformer.search",
        "funasr.models.specaug.specaug",
        "funasr.tokenizer.char_tokenizer",
        "funasr.tokenizer.hf_tokenizer",
        "funasr.tokenizer.sentencepiece_tokenizer",
        "funasr.tokenizer.whisper_tokenizer",
        "funasr.frontends.default",
        "funasr.frontends.wav_frontend",
        "funasr.frontends.whisper_frontend",
        "funasr.models.sense_voice.model",
        "funasr.models.llm_asr.model",
        "funasr.models.paraformer.model",
        "funasr.models.paraformer_streaming.model",
    ]

if importlib.util.find_spec("sherpa_onnx") is not None:
    hiddenimports.append("sherpa_onnx")

# 排除不需要的模块
excludes = [
    "tkinter",
    "test",
    "tests",
    "IPython",
    "jupyter",
    "notebook",
    "matplotlib",
    "PIL",
    "cv2",
    "transformers",
    "whisper",
    "TTS",
    "PyQt6",  # 使用 PySide6
]

a = Analysis(
    ["src/localtrans/ui/main.py"],
    pathex=[str(project_root)],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=excludes,
    noarchive=False,
)

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="localtrans-qml",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,  # GUI 模式
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
