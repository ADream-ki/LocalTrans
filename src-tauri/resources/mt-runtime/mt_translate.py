import os
import sys
import json
import pathlib
import ctranslate2
import sentencepiece as spm

def main():
    if len(sys.argv) < 4:
        raise RuntimeError("usage: mt_translate.py <text> <source_lang> <target_lang>")

    text, src, dst = sys.argv[1], sys.argv[2], sys.argv[3]
    root = os.environ.get("ARGOS_PACKAGES_DIR") or str(pathlib.Path.home() / ".local/share/argos-translate/packages")
    rp = pathlib.Path(root)
    pkg = None
    if rp.exists():
        for d in rp.iterdir():
            m = d / "metadata.json"
            if not m.exists():
                continue
            md = json.loads(m.read_text(encoding="utf-8"))
            if md.get("from_code") == src and md.get("to_code") == dst:
                pkg = d
                break

    if pkg is None:
        raise RuntimeError(f"No Argos package for {src}->{dst} under {root}")

    sp = spm.SentencePieceProcessor(model_file=str(pkg / "sentencepiece.model"))
    tr = ctranslate2.Translator(str(pkg / "model"), device="cpu")
    pieces = sp.encode(text, out_type=str)
    res = tr.translate_batch([pieces], beam_size=1, max_decoding_length=256, no_repeat_ngram_size=3)
    out = "".join(res[0].hypotheses[0]).replace("▁", " ").strip()
    print(out, end="")

if __name__ == "__main__":
    main()
