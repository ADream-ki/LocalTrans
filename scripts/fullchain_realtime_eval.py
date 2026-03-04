"""
全链路实时评估脚本（长语音）

目标:
1) 用较长中文语音输入（默认约40-60秒）
2) 实时走 ASR -> MT -> TTS
3) 输出是否可实时翻译、延迟统计、近似准确度

说明:
- 该脚本默认用本地 Piper 先合成中文测试音频，再实时喂给 StreamingASR。
- 准确度为近似指标（SequenceMatcher 文本相似度），用于工程回归对比。
"""

from __future__ import annotations

import argparse
import re
import time
import gc
from dataclasses import dataclass
from pathlib import Path
from typing import Optional
from difflib import SequenceMatcher

import numpy as np
from loguru import logger

from localtrans.asr import ASREngine, StreamingASR
from localtrans.config.models import ASRConfig, MTConfig, TTSConfig
from localtrans.mt import MTEngine
from localtrans.tts import TTSEngine


DEFAULT_ZH_TEXT = (
    "各位同事大家好，今天我想先汇报一下本周项目进展。"
    "目前我们已经完成了离线语音识别、机器翻译和语音合成三大模块的基础联调，"
    "并且在本地端实现了稳定运行。接下来我们会重点优化实时性，目标是把端到端延迟控制在一秒以内。"
    "在准确率方面，我们针对会议场景加入了术语库和上下文策略，初步测试效果明显提升。"
    "本周还完成了图形界面改造，支持一键下载模型、设备选择和参数调优。"
    "下周计划是扩大测试语料，覆盖技术讨论、商务沟通和培训讲解等不同说话风格。"
    "如果大家对术语翻译有特定需求，请在今天会后提交词表，我们会统一纳入版本。谢谢大家。"
)

DEFAULT_EN_REF = (
    "Hello everyone. I would like to report this week's project progress. "
    "We have completed the initial integration of offline speech recognition, machine translation, and text-to-speech, "
    "and achieved stable local execution. Next, we will focus on real-time performance and keep end-to-end latency under one second. "
    "For accuracy, we added terminology and context strategies for meeting scenarios, and initial results improved clearly. "
    "We also finished a GUI upgrade with one-click model download, device selection, and parameter tuning. "
    "Next week we will expand test corpora to cover technical discussion, business communication, and training lectures. "
    "If you have terminology requirements, please submit glossaries after this meeting. Thank you."
)


@dataclass
class Event:
    emit_s: float
    asr_text: str
    delta_text: str
    translation: str
    mt_ms: float
    tts_ms: float


def normalize_text(text: str) -> str:
    text = text or ""
    text = text.replace("\r", " ").replace("\n", " ")
    text = re.sub(r"\s+", " ", text).strip()
    return text


def suffix_prefix_overlap(a: str, b: str) -> int:
    n = min(len(a), len(b))
    for size in range(n, 0, -1):
        if a.endswith(b[:size]):
            return size
    return 0


def incremental_delta(prev_text: str, curr_text: str) -> str:
    if not curr_text:
        return ""
    if not prev_text:
        return curr_text
    if curr_text == prev_text:
        return ""
    if curr_text.startswith(prev_text):
        return curr_text[len(prev_text):]
    overlap = suffix_prefix_overlap(prev_text, curr_text)
    if overlap > 0:
        delta = curr_text[overlap:]
        if delta:
            return delta
    if len(curr_text) + 4 < len(prev_text):
        return curr_text
    return curr_text


def resample_mono(audio: np.ndarray, src_sr: int, dst_sr: int) -> np.ndarray:
    audio = np.asarray(audio)
    if audio.ndim > 1:
        audio = np.mean(audio, axis=1)
    if src_sr == dst_sr:
        return audio.astype(np.float32, copy=False)
    if len(audio) == 0:
        return np.array([], dtype=np.float32)
    duration = len(audio) / float(src_sr)
    dst_len = max(1, int(round(duration * dst_sr)))
    src_x = np.linspace(0.0, 1.0, num=len(audio), endpoint=True)
    dst_x = np.linspace(0.0, 1.0, num=dst_len, endpoint=True)
    out = np.interp(dst_x, src_x, audio).astype(np.float32)
    return out


def find_model_file(pattern: str) -> Path:
    base = Path.home() / ".localtrans" / "models" / "tts"
    matches = sorted(base.rglob(pattern))
    if not matches:
        raise FileNotFoundError(f"未找到模型文件: {pattern} (搜索目录: {base})")
    return matches[0]


def similarity_ratio(a: str, b: str) -> float:
    return SequenceMatcher(None, normalize_text(a), normalize_text(b)).ratio()


def main() -> int:
    logger.remove()
    logger.add(lambda msg: None, level="ERROR")

    parser = argparse.ArgumentParser(description="LocalTrans 全链路实时评估")
    parser.add_argument("--text", default=DEFAULT_ZH_TEXT, help="中文输入文本")
    parser.add_argument("--reference", default=DEFAULT_EN_REF, help="英文参考译文")
    parser.add_argument("--chunk-size", type=int, default=1024, help="实时流输入chunk大小")
    parser.add_argument("--sleep-scale", type=float, default=1.0, help="实时缩放系数(1.0=实时)")
    parser.add_argument("--asr-model-type", default="faster-whisper", help="ASR后端类型")
    parser.add_argument("--asr-model-size", default="base", help="ASR模型大小/名称")
    parser.add_argument("--asr-language", default="zh", help="ASR语言")
    parser.add_argument(
        "--asr-task",
        default="transcribe",
        choices=["transcribe", "translate"],
        help="ASR任务类型",
    )
    parser.add_argument(
        "--direct-asr-translate",
        action="store_true",
        help="ASR直接英译输出（跳过MT）",
    )
    args = parser.parse_args()

    zh_voice = find_model_file("zh_CN-huayan-medium.onnx")
    en_voice = find_model_file("en_US-lessac-medium.onnx")

    # 1) 生成较长中文测试语音
    tts_zh = TTSEngine(
        TTSConfig(
            engine="piper",
            model_path=zh_voice,
            language="zh",
            sample_rate=22050,
            device="cpu",
        )
    )
    source_text = normalize_text(args.text)
    synth_start = time.perf_counter()
    zh_synth = tts_zh.synthesize(source_text)
    synth_ms = (time.perf_counter() - synth_start) * 1000.0
    src_audio = zh_synth.audio.astype(np.float32) / 32768.0
    src_audio_16k = resample_mono(src_audio, zh_synth.sample_rate, 16000)
    audio_duration_s = len(src_audio_16k) / 16000.0
    # 仅用于生成测试语音，后续不再需要，尽早释放可降低原生库冲突概率
    del tts_zh
    gc.collect()

    asr_model_path: Optional[Path] = None
    if args.asr_model_type == "faster-whisper":
        candidate = Path.home() / ".localtrans" / "models" / "asr" / f"faster-whisper-{args.asr_model_size}"
        if candidate.exists():
            asr_model_path = candidate

    # 2) 初始化 ASR/MT/TTS
    asr_engine = ASREngine(
        ASRConfig(
            model_type=args.asr_model_type,
            model_size=args.asr_model_size,
            model_path=asr_model_path,
            language=args.asr_language,
            task="translate" if args.direct_asr_translate else args.asr_task,
            device="auto",
            compute_type="int8",
            beam_size=1,
            vad_filter=False,
            word_timestamps=False,
        )
    )
    mt_engine = (
        None
        if args.direct_asr_translate
        else MTEngine(
            MTConfig(
                model_type="argos-ct2",
                model_name="argos-zh-en",
                source_lang="zh",
                target_lang="en",
                device="auto",
                compute_type="int8",
            )
        )
    )
    tts_en = TTSEngine(
        TTSConfig(
            engine="piper",
            model_path=en_voice,
            language="en",
            sample_rate=22050,
            device="cpu",
        )
    )

    events: list[Event] = []
    last_asr_text = ""
    final_asr_text = ""
    last_delta_text = ""
    pending_delta_text = ""
    stream_start = time.perf_counter()

    def flush_pending(asr_text: str, emit_s: float) -> None:
        nonlocal pending_delta_text
        delta = normalize_text(pending_delta_text)
        if not delta:
            return
        pending_delta_text = ""

        if args.direct_asr_translate:
            mt_out = delta
            mt_ms = 0.0
        else:
            mt_t0 = time.perf_counter()
            mt_out = mt_engine.translate(delta, source_lang="zh", target_lang="en").translated_text
            mt_ms = (time.perf_counter() - mt_t0) * 1000.0

        tts_t0 = time.perf_counter()
        _ = tts_en.synthesize(mt_out)
        tts_ms = (time.perf_counter() - tts_t0) * 1000.0

        events.append(
            Event(
                emit_s=emit_s,
                asr_text=asr_text,
                delta_text=delta,
                translation=normalize_text(mt_out),
                mt_ms=mt_ms,
                tts_ms=tts_ms,
            )
        )

    def on_asr(result) -> None:
        nonlocal last_asr_text, final_asr_text, last_delta_text, pending_delta_text
        asr_text = normalize_text(result.text)
        if not asr_text:
            return
        final_asr_text = asr_text
        delta = normalize_text(incremental_delta(last_asr_text, asr_text))
        last_asr_text = asr_text
        if not delta:
            return
        if delta == last_delta_text:
            return
        last_delta_text = delta
        pending_delta_text = normalize_text(f"{pending_delta_text} {delta}")
        if len(pending_delta_text) >= 28 or pending_delta_text[-1:] in "。！？!?;":
            flush_pending(asr_text=asr_text, emit_s=time.perf_counter() - stream_start)

    streaming = StreamingASR(
        asr_engine=asr_engine,
        callback=on_asr,
        buffer_duration=0.6,
        overlap_duration=0.05,
    )

    # 3) 实时喂流
    streaming.start()
    chunk = max(256, int(args.chunk_size))
    sleep_s = (chunk / 16000.0) * max(0.01, float(args.sleep_scale))

    for idx in range(0, len(src_audio_16k), chunk):
        streaming.put_audio(src_audio_16k[idx:idx + chunk])
        time.sleep(sleep_s)

    # 给最后窗口处理时间
    time.sleep(1.0)
    streaming.stop()
    flush_pending(asr_text=final_asr_text, emit_s=time.perf_counter() - stream_start)
    total_elapsed_s = time.perf_counter() - stream_start

    # 4) 统计
    full_translation = normalize_text(" ".join(e.translation for e in events))
    asr_joined_text = normalize_text(" ".join(e.delta_text for e in events)) or final_asr_text
    first_emit_s = events[0].emit_s if events else float("inf")
    realtime_ok = first_emit_s < audio_duration_s

    mt_list = [e.mt_ms for e in events]
    tts_list = [e.tts_ms for e in events]
    e2e_list = [e.mt_ms + e.tts_ms for e in events]

    def pctl(values: list[float], p: float) -> float:
        if not values:
            return 0.0
        return float(np.percentile(np.asarray(values, dtype=np.float32), p))

    asr_acc = (
        similarity_ratio(args.reference, asr_joined_text)
        if args.direct_asr_translate
        else similarity_ratio(source_text, asr_joined_text)
    )
    mt_acc = similarity_ratio(args.reference, full_translation)

    print("\n=== Full-Chain Realtime Evaluation ===")
    print(f"ASR后端/模型: {args.asr_model_type} / {args.asr_model_size}")
    if asr_model_path:
        print(f"ASR本地模型路径: {asr_model_path}")
    print(f"输入文本长度(字): {len(source_text)}")
    print(f"合成音频时长(s): {audio_duration_s:.2f}")
    print(f"测试音频合成耗时(ms): {synth_ms:.1f}")
    print(f"实时流总耗时(s): {total_elapsed_s:.2f}")
    print(f"实时输出段数: {len(events)}")
    print(f"首次输出时间(s): {first_emit_s:.2f}" if events else "首次输出时间(s): N/A")
    print(f"是否在音频结束前实时输出: {'是' if realtime_ok else '否'}")

    print("\n--- Latency (per output segment) ---")
    print(f"MT  P50/P95 (ms): {pctl(mt_list, 50):.1f} / {pctl(mt_list, 95):.1f}")
    print(f"TTS P50/P95 (ms): {pctl(tts_list, 50):.1f} / {pctl(tts_list, 95):.1f}")
    print(f"E2E P50/P95 (ms): {pctl(e2e_list, 50):.1f} / {pctl(e2e_list, 95):.1f}")

    print("\n--- Approx Accuracy ---")
    print(f"ASR文本相似度(0-1): {asr_acc:.3f}")
    print(f"翻译文本相似度(0-1): {mt_acc:.3f}")

    print("\n--- Preview ---")
    print(f"[ASR拼接文本] {asr_joined_text[:220]}")
    print(f"[翻译拼接结果] {full_translation[:260]}")

    return 0


if __name__ == "__main__":
    import os
    try:
        code = int(main())
        # Windows 下部分原生推理库在进程析构阶段可能触发异常退出，
        # 评测结果已输出时直接硬退出可避免误判为链路失败。
        if code == 0 and os.name == "nt":
            os._exit(0)
        raise SystemExit(code)
    except Exception as exc:
        import traceback

        print("\n[ERROR] 评估脚本异常退出")
        print(str(exc))
        traceback.print_exc()
        raise
