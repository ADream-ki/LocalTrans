"""
端到端延迟基准脚本

示例:
  python scripts/benchmark_latency.py --iterations 20 --warmup 3
  python scripts/benchmark_latency.py --no-tts
  python scripts/benchmark_latency.py --audio-file .\sample.wav
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def _bootstrap_path() -> None:
    project_root = Path(__file__).resolve().parents[1]
    src_dir = project_root / "src"
    if str(src_dir) not in sys.path:
        sys.path.insert(0, str(src_dir))


def main() -> int:
    _bootstrap_path()

    from localtrans.main import benchmark_latency

    parser = argparse.ArgumentParser(description="LocalTrans 延迟基准脚本")
    parser.add_argument("-s", "--source", default="zh", help="源语言 (默认: zh)")
    parser.add_argument("-t", "--target", default="en", help="目标语言 (默认: en)")
    parser.add_argument("--text", default="这是一次实时翻译延迟测试。", help="文本模式输入")
    parser.add_argument("--iterations", type=int, default=30, help="统计轮数")
    parser.add_argument("--warmup", type=int, default=5, help="预热轮数")
    parser.add_argument("--no-tts", action="store_true", help="基准中禁用TTS")
    parser.add_argument("--audio-file", help="可选音频文件，提供后将包含ASR阶段")
    args = parser.parse_args()

    return benchmark_latency(
        source_lang=args.source,
        target_lang=args.target,
        text=args.text,
        iterations=args.iterations,
        warmup=args.warmup,
        include_tts=not args.no_tts,
        audio_file=args.audio_file,
    )


if __name__ == "__main__":
    raise SystemExit(main())
