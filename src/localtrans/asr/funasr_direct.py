"""
FunASR direct inference runtime.

This module builds FunASR-compatible models directly from local model folders
without importing ``funasr.AutoModel``. It bootstraps the minimum registry and
injects a tiny ``torchaudio`` compatibility shim when the environment does not
provide the real package.
"""

from __future__ import annotations

import importlib
import importlib.util
import json
import logging
import copy
import importlib.machinery
import sys
import types
from hashlib import sha1
from pathlib import Path
from typing import Any, Optional

import numpy as np
import soundfile as sf
import torch
from omegaconf import DictConfig, OmegaConf

from localtrans.config import settings


_LOGGER = logging.getLogger(__name__)


def _deep_update(base: dict, extra: dict) -> dict:
    for key, value in extra.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            _deep_update(base[key], value)
        else:
            base[key] = value
    return base


def _to_plain_dict(value: Any) -> Any:
    if isinstance(value, DictConfig):
        return OmegaConf.to_container(value, resolve=True)
    return value


def _resolve_file_metas(model_dir: Path, metas: dict, target: Optional[dict] = None) -> dict:
    result = target if target is not None else {}
    for key, value in metas.items():
        if isinstance(value, str):
            candidate = model_dir / value
            result[key] = str(candidate) if candidate.exists() else value
        elif isinstance(value, dict):
            child = result.get(key, {})
            result[key] = _resolve_file_metas(model_dir, value, child)
        elif isinstance(value, list):
            items = []
            for item in value:
                if isinstance(item, str):
                    candidate = model_dir / item
                    items.append(str(candidate) if candidate.exists() else item)
                elif isinstance(item, dict):
                    items.append(_resolve_file_metas(model_dir, item, {}))
                else:
                    items.append(item)
            result[key] = items
        else:
            result[key] = value
    return result


class FunASRDirectRuntime:
    """Build and run FunASR models without AutoModel."""

    _CORE_BOOTSTRAPPED = False

    def __init__(
        self,
        model_ref: str,
        *,
        device: str,
        language: str = "auto",
        trust_remote_code: bool = True,
        use_itn: bool = True,
        word_timestamps: bool = False,
    ):
        self.model_ref = str(model_ref)
        self.device = device
        self.language = language or "auto"
        self.trust_remote_code = bool(trust_remote_code)
        self.use_itn = bool(use_itn)
        self.word_timestamps = bool(word_timestamps)

        self.model_dir = self._resolve_model_dir(self.model_ref)
        self.raw_config = self._load_model_config(self.model_dir)

        self._ensure_funasr_bootstrap()
        self._import_remote_code(self.raw_config.get("remote_code"))
        self._bootstrap_registry(self.raw_config)

        self.model, self.runtime_config = self._build_model(self.raw_config)
        self.model.eval()
        self.frontend = self.runtime_config.get("frontend")
        self.tokenizer = self.runtime_config.get("tokenizer")
        self._rich_postprocess = self._resolve_rich_postprocess()

    @staticmethod
    def _normalize_aliases(model_ref: str) -> list[str]:
        ref = (model_ref or "").strip()
        lowered = ref.lower()
        aliases = [ref]
        alias_map = {
            "funaudiollm/sensevoicesmall": "funasr-sensevoice-small",
            "iic/sensevoicesmall": "funasr-sensevoice-small",
            "sensevoicesmall": "funasr-sensevoice-small",
            "qwen/qwen3-asr-0.6b": "qwen3-asr-0.6b",
            "qwen/qwen3-asr-1.7b": "qwen3-asr-1.7b",
        }
        mapped = alias_map.get(lowered)
        if mapped:
            aliases.append(mapped)

        if "/" in ref:
            tail = ref.split("/")[-1]
            aliases.append(tail)
            aliases.append(tail.lower())
            aliases.append(tail.lower().replace("_", "-"))

        cleaned = lowered.replace("_", "-")
        if cleaned not in aliases:
            aliases.append(cleaned)
        return [alias for alias in aliases if alias]

    def _resolve_model_dir(self, model_ref: str) -> Path:
        path_candidate = Path(model_ref)
        if path_candidate.exists():
            return path_candidate.resolve()

        for alias in self._normalize_aliases(model_ref):
            local_dir = settings.models_dir / "asr" / alias
            if local_dir.exists():
                return local_dir.resolve()

        if "/" in model_ref:
            try:
                from huggingface_hub import snapshot_download

                downloaded = snapshot_download(repo_id=model_ref)
                return Path(downloaded).resolve()
            except Exception as exc:
                raise FileNotFoundError(f"未找到FunASR模型目录，也无法下载: {model_ref}") from exc

        raise FileNotFoundError(f"未找到FunASR模型目录: {model_ref}")

    def _load_model_config(self, model_dir: Path) -> dict:
        merged: dict[str, Any] = {"model_path": str(model_dir)}
        remote_code: Optional[str] = None

        configuration_json = model_dir / "configuration.json"
        if configuration_json.exists():
            with configuration_json.open("r", encoding="utf-8") as fh:
                conf_json = json.load(fh)

            file_metas = conf_json.get("file_path_metas", {})
            if isinstance(file_metas, dict):
                _deep_update(merged, _resolve_file_metas(model_dir, file_metas, {}))
            remote_code = merged.get("remote_code") or conf_json.get("remote_code")

        config_path: Optional[Path] = None
        config_from_meta = merged.get("config")
        if isinstance(config_from_meta, str):
            config_path = Path(config_from_meta)
            if not config_path.is_absolute():
                config_path = model_dir / config_path
        elif (model_dir / "config.yaml").exists():
            config_path = model_dir / "config.yaml"

        if config_path is not None and config_path.exists():
            config_data = OmegaConf.load(config_path)
            config_plain = _to_plain_dict(config_data) or {}
            if isinstance(config_plain, dict):
                merged = _deep_update(config_plain, merged)

        tokenizer_conf = merged.setdefault("tokenizer_conf", {})
        frontend_conf = merged.setdefault("frontend_conf", {})

        tokens_txt = model_dir / "tokens.txt"
        tokens_json = model_dir / "tokens.json"
        seg_dict = model_dir / "seg_dict"
        bpe_model = model_dir / "bpe.model"
        am_mvn = model_dir / "am.mvn"
        init_param = model_dir / "model.pt"

        if tokens_txt.exists() and "token_list" not in tokenizer_conf:
            tokenizer_conf["token_list"] = str(tokens_txt)
        if tokens_json.exists():
            tokenizer_conf.setdefault("token_list", str(tokens_json))
            tokenizer_conf.setdefault("vocab_path", str(tokens_json))
        if seg_dict.exists() and "seg_dict" not in tokenizer_conf:
            tokenizer_conf["seg_dict"] = str(seg_dict)
        if bpe_model.exists() and "bpemodel" not in tokenizer_conf:
            tokenizer_conf["bpemodel"] = str(bpe_model)
        if am_mvn.exists() and "cmvn_file" not in frontend_conf:
            frontend_conf["cmvn_file"] = str(am_mvn)
        if "init_param" not in merged and init_param.exists():
            merged["init_param"] = str(init_param)

        if remote_code is None:
            model_py = model_dir / "model.py"
            if model_py.exists():
                remote_code = str(model_py)

        if remote_code is not None:
            remote_path = Path(remote_code)
            if not remote_path.is_absolute():
                remote_path = model_dir / remote_path
            merged["remote_code"] = str(remote_path.resolve())

        return merged

    @classmethod
    def _detect_funasr_package_dir(cls) -> Path:
        cached_pkg = sys.modules.get("funasr")
        cached_path = getattr(cached_pkg, "__path__", None)
        if cached_path:
            return Path(list(cached_path)[0]).resolve()

        spec = importlib.util.find_spec("funasr")
        if spec is None or not spec.submodule_search_locations:
            raise ImportError("未找到funasr包，请先安装funasr")
        return Path(list(spec.submodule_search_locations)[0]).resolve()

    @staticmethod
    def _install_torchaudio_shim() -> None:
        if importlib.util.find_spec("torchaudio") is not None or "torchaudio" in sys.modules:
            return

        def _load_audio(src):
            data, sample_rate = sf.read(src, dtype="float32", always_2d=True)
            tensor = torch.from_numpy(data.T.copy())
            return tensor, int(sample_rate)

        class _Resample:
            def __init__(self, orig_freq: int, new_freq: int):
                self.orig_freq = int(orig_freq)
                self.new_freq = int(new_freq)

            def __call__(self, waveform: torch.Tensor) -> torch.Tensor:
                if self.orig_freq == self.new_freq:
                    return waveform

                import librosa

                arr = waveform.detach().cpu().numpy()
                resampled = [
                    librosa.resample(channel.astype(np.float32), orig_sr=self.orig_freq, target_sr=self.new_freq)
                    for channel in arr
                ]
                return torch.from_numpy(np.stack(resampled)).to(waveform.dtype)

        def _kaldi_fbank(
            waveform: torch.Tensor,
            *,
            num_mel_bins: int = 80,
            frame_length: float = 25.0,
            frame_shift: float = 10.0,
            dither: float = 0.0,
            energy_floor: float = 0.0,
            window_type: str = "hamming",
            sample_frequency: int = 16000,
            snip_edges: bool = True,
            **kwargs,
        ) -> torch.Tensor:
            import librosa

            if isinstance(waveform, torch.Tensor):
                arr = waveform.detach().cpu().numpy()
            else:
                arr = np.asarray(waveform)

            if arr.ndim == 2:
                arr = arr[0]
            arr = np.asarray(arr, dtype=np.float32).reshape(-1)
            if arr.size == 0:
                return torch.zeros((0, int(num_mel_bins)), dtype=torch.float32)

            if dither and float(dither) > 0.0:
                rng = np.random.default_rng(0)
                arr = arr + rng.normal(0.0, float(dither), size=arr.shape).astype(np.float32)

            sr = int(sample_frequency)
            win_length = max(1, int(round(sr * float(frame_length) / 1000.0)))
            hop_length = max(1, int(round(sr * float(frame_shift) / 1000.0)))
            n_fft = 1
            while n_fft < win_length:
                n_fft <<= 1

            min_required = max(win_length, n_fft)
            if arr.size < min_required:
                arr = np.pad(arr, (0, min_required - arr.size))

            mel = librosa.feature.melspectrogram(
                y=arr,
                sr=sr,
                n_fft=n_fft,
                hop_length=hop_length,
                win_length=win_length,
                window="hann" if str(window_type).lower() == "hann" else "hamming",
                center=not bool(snip_edges),
                power=2.0,
                n_mels=int(num_mel_bins),
                htk=True,
                norm=None,
            )
            mel = np.maximum(mel, 1e-10)
            if energy_floor and float(energy_floor) > 0.0:
                mel = np.maximum(mel, float(energy_floor))
            feats = np.log(mel).T
            return torch.from_numpy(np.ascontiguousarray(feats.astype(np.float32)))

        torchaudio_mod = types.ModuleType("torchaudio")
        transforms_mod = types.ModuleType("torchaudio.transforms")
        compliance_mod = types.ModuleType("torchaudio.compliance")
        kaldi_mod = types.ModuleType("torchaudio.compliance.kaldi")

        torchaudio_mod.__spec__ = importlib.machinery.ModuleSpec("torchaudio", loader=None)
        transforms_mod.__spec__ = importlib.machinery.ModuleSpec("torchaudio.transforms", loader=None)
        compliance_mod.__spec__ = importlib.machinery.ModuleSpec("torchaudio.compliance", loader=None)
        kaldi_mod.__spec__ = importlib.machinery.ModuleSpec("torchaudio.compliance.kaldi", loader=None)

        transforms_mod.Resample = _Resample
        kaldi_mod.fbank = _kaldi_fbank
        compliance_mod.kaldi = kaldi_mod

        torchaudio_mod.load = _load_audio
        torchaudio_mod.transforms = transforms_mod
        torchaudio_mod.compliance = compliance_mod

        sys.modules["torchaudio"] = torchaudio_mod
        sys.modules["torchaudio.transforms"] = transforms_mod
        sys.modules["torchaudio.compliance"] = compliance_mod
        sys.modules["torchaudio.compliance.kaldi"] = kaldi_mod

    @classmethod
    def _ensure_funasr_bootstrap(cls) -> None:
        package_dir = cls._detect_funasr_package_dir()
        cls._install_torchaudio_shim()

        pkg = sys.modules.get("funasr")
        if pkg is None or not hasattr(pkg, "__path__"):
            pkg = types.ModuleType("funasr")
            pkg.__path__ = [str(package_dir)]
            pkg.__package__ = "funasr"
            pkg.__spec__ = importlib.machinery.ModuleSpec("funasr", loader=None, is_package=True)
            if pkg.__spec__.submodule_search_locations is not None:
                pkg.__spec__.submodule_search_locations.append(str(package_dir))
            sys.modules["funasr"] = pkg

        if cls._CORE_BOOTSTRAPPED:
            return

        importlib.import_module("funasr.register")
        cls._CORE_BOOTSTRAPPED = True

    @staticmethod
    def _load_local_python_module(file_path: str) -> None:
        path = Path(file_path).resolve()
        package_hash = sha1(str(path.parent).encode("utf-8")).hexdigest()[:12]
        package_name = f"_localtrans_funasr_{package_hash}"
        module_name = f"{package_name}.{path.stem}"

        if package_name not in sys.modules:
            package_mod = types.ModuleType(package_name)
            package_mod.__path__ = [str(path.parent)]
            package_mod.__package__ = package_name
            sys.modules[package_name] = package_mod

        if module_name in sys.modules:
            return

        spec = importlib.util.spec_from_file_location(module_name, str(path))
        if spec is None or spec.loader is None:
            raise ImportError(f"无法导入本地FunASR模型代码: {file_path}")

        module = importlib.util.module_from_spec(spec)
        sys.modules[module_name] = module
        spec.loader.exec_module(module)

    def _import_remote_code(self, remote_code: Optional[str]) -> None:
        if not remote_code or not self.trust_remote_code:
            return
        candidate = Path(remote_code)
        if candidate.exists():
            self._load_local_python_module(str(candidate))

    @staticmethod
    def _safe_import(module_name: str) -> None:
        try:
            importlib.import_module(module_name)
        except Exception as exc:
            _LOGGER.debug("Skip FunASR module bootstrap %s: %s", module_name, exc)

    def _bootstrap_registry(self, config: dict) -> None:
        tokenizer_name = config.get("tokenizer")
        frontend_name = config.get("frontend")
        model_name = config.get("model")
        specaug_name = config.get("specaug")
        normalize_name = config.get("normalize")

        tokenizer_modules = {
            "CharTokenizer": "funasr.tokenizer.char_tokenizer",
            "HuggingfaceTokenizer": "funasr.tokenizer.hf_tokenizer",
            "SentencepiecesTokenizer": "funasr.tokenizer.sentencepiece_tokenizer",
            "WhisperTokenizer": "funasr.tokenizer.whisper_tokenizer",
            "SenseVoiceTokenizer": "funasr.tokenizer.whisper_tokenizer",
        }
        frontend_modules = {
            "DefaultFrontend": "funasr.frontends.default",
            "EspnetFrontend": "funasr.frontends.default",
            "WavFrontend": "funasr.frontends.wav_frontend",
            "wav_frontend": "funasr.frontends.wav_frontend",
            "WavFrontendOnline": "funasr.frontends.wav_frontend",
            "WhisperFrontend": "funasr.frontends.whisper_frontend",
        }
        model_modules = {
            "SenseVoiceSmall": "funasr.models.sense_voice.model",
            "LLMASR": "funasr.models.llm_asr.model",
            "LLMASR2": "funasr.models.llm_asr.model",
            "LLMASR3": "funasr.models.llm_asr.model",
            "LLMASR4": "funasr.models.llm_asr.model",
            "Paraformer": "funasr.models.paraformer.model",
            "ParaformerStreaming": "funasr.models.paraformer_streaming.model",
        }
        specaug_modules = {
            "SpecAug": "funasr.models.specaug.specaug",
            "SpecAugLFR": "funasr.models.specaug.specaug",
        }
        normalize_modules = {
            "GlobalMVN": "funasr.layers.global_mvn",
            "UtteranceMVN": "funasr.layers.utterance_mvn",
        }

        self._safe_import("funasr.train_utils.load_pretrained_model")
        self._safe_import("funasr.train_utils.set_all_random_seed")
        self._safe_import("funasr.models.paraformer.search")
        self._safe_import("funasr.utils.postprocess_utils")

        module_name = tokenizer_modules.get(str(tokenizer_name))
        if module_name:
            self._safe_import(module_name)

        module_name = frontend_modules.get(str(frontend_name))
        if module_name:
            self._safe_import(module_name)

        module_name = model_modules.get(str(model_name))
        if module_name:
            self._safe_import(module_name)

        module_name = specaug_modules.get(str(specaug_name))
        if module_name:
            self._safe_import(module_name)

        module_name = normalize_modules.get(str(normalize_name))
        if module_name:
            self._safe_import(module_name)

    @staticmethod
    def _build_tokenizer(config: dict, tables) -> Any:
        tokenizer_name = config.get("tokenizer")
        if not tokenizer_name:
            return None

        tokenizer_factory = tables.tokenizer_classes.get(tokenizer_name)
        if tokenizer_factory is None:
            raise ValueError(f"未注册的FunASR tokenizer: {tokenizer_name}")

        tokenizer_conf = _to_plain_dict(config.get("tokenizer_conf", {})) or {}
        return tokenizer_factory(**tokenizer_conf)

    @staticmethod
    def _build_frontend(config: dict, tables) -> Any:
        frontend_name = config.get("frontend")
        if not frontend_name:
            return None

        frontend_factory = tables.frontend_classes.get(frontend_name)
        if frontend_factory is None:
            raise ValueError(f"未注册的FunASR frontend: {frontend_name}")

        frontend_conf = _to_plain_dict(config.get("frontend_conf", {})) or {}
        frontend = frontend_factory(**frontend_conf)
        config["input_size"] = frontend.output_size() if hasattr(frontend, "output_size") else None
        return frontend

    def _build_model(self, raw_config: dict):
        tables = importlib.import_module("funasr.register").tables
        set_seed = importlib.import_module("funasr.train_utils.set_all_random_seed").set_all_random_seed
        load_pretrained_model = importlib.import_module(
            "funasr.train_utils.load_pretrained_model"
        ).load_pretrained_model

        config = copy.deepcopy(raw_config)
        config["device"] = self.device
        set_seed(int(config.get("seed", 0)))

        tokenizer = self._build_tokenizer(config, tables)
        config["tokenizer"] = tokenizer
        if tokenizer is not None:
            token_list = getattr(tokenizer, "token_list", None)
            if token_list is None and hasattr(tokenizer, "get_vocab"):
                token_list = tokenizer.get_vocab()
            vocab_size = -1
            if token_list is not None:
                vocab_size = len(token_list)
            elif hasattr(tokenizer, "get_vocab_size"):
                try:
                    vocab_size = int(tokenizer.get_vocab_size())
                except Exception:
                    vocab_size = -1
            config["token_list"] = token_list
            config["vocab_size"] = vocab_size
        else:
            config["token_list"] = None
            config["vocab_size"] = -1

        frontend = self._build_frontend(config, tables)
        config["frontend"] = frontend

        model_name = config.get("model")
        model_class = tables.model_classes.get(model_name)
        if model_class is None:
            raise ValueError(f"未注册的FunASR模型类: {model_name}")

        model_conf = _to_plain_dict(config.get("model_conf", {})) or {}
        build_kwargs = copy.deepcopy(model_conf)
        _deep_update(build_kwargs, config.copy())
        model = model_class(**build_kwargs)

        init_param = config.get("init_param")
        if init_param and Path(str(init_param)).exists():
            load_pretrained_model(
                model=model,
                path=str(init_param),
                ignore_init_mismatch=bool(config.get("ignore_init_mismatch", True)),
                oss_bucket=config.get("oss_bucket"),
                scope_map=config.get("scope_map", []),
                excludes=config.get("excludes"),
            )

        if bool(config.get("fp16", False)):
            model.to(torch.float16)
        elif bool(config.get("bf16", False)):
            model.to(torch.bfloat16)

        model.to(self.device)
        return model, config

    @staticmethod
    def _resolve_rich_postprocess():
        try:
            module = importlib.import_module("funasr.utils.postprocess_utils")
            return getattr(module, "rich_transcription_postprocess", None)
        except Exception:
            return None

    def infer(self, audio: np.ndarray, **kwargs) -> dict:
        inference_kwargs = {
            "device": self.device,
            "language": kwargs.pop("language", None) or self.language or "auto",
            "use_itn": kwargs.pop("use_itn", self.use_itn),
            "output_timestamp": kwargs.pop("output_timestamp", self.word_timestamps),
        }
        inference_kwargs.update(kwargs)

        inference_result = self.model.inference(
            audio,
            key=["localtrans"],
            tokenizer=self.tokenizer,
            frontend=self.frontend,
            **inference_kwargs,
        )

        if isinstance(inference_result, tuple) and len(inference_result) == 2:
            results, meta_data = inference_result
        else:
            results, meta_data = inference_result, {}

        if isinstance(results, tuple):
            results = results[0]
        if not isinstance(results, list):
            results = [results]

        text_parts = []
        timestamps = []
        words = []
        for item in results:
            if not isinstance(item, dict):
                continue
            text = str(item.get("text", "")).strip()
            if text and self._rich_postprocess is not None:
                try:
                    text = str(self._rich_postprocess(text)).strip()
                except Exception:
                    pass
            if text:
                text_parts.append(text)

            item_timestamps = item.get("timestamp")
            if isinstance(item_timestamps, list):
                timestamps.extend(item_timestamps)
            item_words = item.get("words")
            if isinstance(item_words, list):
                words.extend(item_words)

        return {
            "text": " ".join(text_parts).strip(),
            "timestamps": timestamps if timestamps else None,
            "words": words if words else None,
            "meta": meta_data,
        }
