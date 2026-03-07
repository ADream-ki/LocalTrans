# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller 打包配置文件 - GUI版"""

from pathlib import Path
from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

project_root = Path(SPECPATH)

datas = []
binaries = []

# onefile下显式收集动态库和数据，保证本地模型与TTS在GUI版可用
datas += collect_data_files("vosk")
datas += collect_data_files("argostranslate")
datas += collect_data_files("piper")
datas += collect_data_files("faster_whisper")
binaries += collect_dynamic_libs("vosk")
binaries += collect_dynamic_libs("ctranslate2")
binaries += collect_dynamic_libs("sentencepiece")
binaries += collect_dynamic_libs("piper")

config_files = [
    ("src/localtrans/py.typed", "localtrans"),
]
for src, dst in config_files:
    if (project_root / src).exists():
        datas.append((src, dst))

hiddenimports = [
    "localtrans",
    "localtrans.config",
    "localtrans.config.settings",
    "localtrans.config.models",
    "localtrans.audio",
    "localtrans.audio.capturer",
    "localtrans.audio.virtual_device",
    "localtrans.audio.router",
    "localtrans.audio.utils",
    "localtrans.audio.utils.vad",
    "localtrans.audio.utils.resampler",
    "localtrans.asr",
    "localtrans.asr.engine",
    "localtrans.asr.stream",
    "localtrans.mt",
    "localtrans.mt.engine",
    "localtrans.mt.term_bank",
    "localtrans.tts",
    "localtrans.tts.engine",
    "localtrans.tts.stream",
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
    "argostranslate",
    "argostranslate.package",
    "argostranslate.translate",
    "argostranslate.settings",
    "argostranslate.tokenizer",
    "localtrans.pipeline",
    "localtrans.pipeline.translator",
    "localtrans.pipeline.realtime",
    "localtrans.utils",
    "localtrans.utils.monitor",
    "localtrans.utils.model_downloader",
    "localtrans.gui",
    "localtrans.gui.main",
    "localtrans.gui.app",
    "PyQt6",
    "PyQt6.QtCore",
    "PyQt6.QtGui",
    "PyQt6.QtWidgets",
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
    "queue",
    "threading",
    "json",
    "pathlib",
    "typing",
    "collections",
    "enum",
    "time",
    "abc",
]

excludes = [
    "tkinter",
    "unittest",
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
    "scipy",
]

a = Analysis(
    ["src/localtrans/gui/main.py"],
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

# 避免 Qt 自带旧版 VC 运行库与环境/系统运行库冲突导致 APPCRASH(MSVCP140.dll)
def _is_conflicting_qt_runtime(entry):
    name = entry[0].replace("\\", "/").lower()
    return (
        name.startswith("pyqt6/qt6/bin/msvcp140")
        or name.startswith("pyqt6/qt6/bin/vcruntime140")
    )


a.binaries = [entry for entry in a.binaries if not _is_conflicting_qt_runtime(entry)]

pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name="localtrans-gui",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
