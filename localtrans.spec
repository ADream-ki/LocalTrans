# -*- mode: python ; coding: utf-8 -*-
"""PyInstaller 打包配置文件 - 轻量版"""

from pathlib import Path
import importlib.util
from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs

project_root = Path(SPECPATH)

datas = []
binaries = []

# onefile下需要显式收集动态库和运行时数据，否则部分本地模型后端会在_MEI目录加载失败
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
    "localtrans.asr.funasr_direct",
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
    "localtrans.pipeline",
    "localtrans.pipeline.translator",
    "localtrans.pipeline.realtime",
    "localtrans.utils",
    "localtrans.utils.monitor",
    "localtrans.utils.model_downloader",
    "localtrans.gui",
    "localtrans.gui.app",
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
    "PyQt6",
    "transformers",
    "whisper",
    "TTS",
    "rich",
]

a = Analysis(
    ["src/localtrans/main.py"],
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
    name="localtrans",
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
