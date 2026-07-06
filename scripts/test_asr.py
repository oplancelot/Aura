"""
Aura ASR test utilities.
Compute WER, parse references, batch test offline & online logs.
"""

from __future__ import annotations
import csv, json, re, sys, time
from pathlib import Path
from typing import Callable

try:
    import Levenshtein
    def wer(r: str, h: str) -> float:
        if not r:
            return 100.0 if h else 0.0
        return Levenshtein.distance(r.split(), h.split()) / len(r.split()) * 100
except ImportError:
    def wer(r: str, h: str) -> float:
        r"""Fallback WER without python-Levenshtein."""
        if not r:
            return 100.0 if h else 0.0
        rw, hw = r.split(), h.split()
        n = len(rw)
        d: list[list[int]] = [[0] * (len(hw) + 1) for _ in range(n + 1)]
        for i in range(n + 1): d[i][0] = i
        for j in range(len(hw) + 1): d[0][j] = j
        for i in range(1, n + 1):
            for j in range(1, len(hw) + 1):
                cost = 0 if rw[i - 1] == hw[j - 1] else 1
                d[i][j] = min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost)
        return d[n][len(hw)] / n * 100


def load_ljspeech_refs(metadata_csv: str) -> dict[str, str]:
    """Load LJSpeech metadata.csv -> {filename_prefix: reference_text}."""
    refs: dict[str, str] = {}
    with open(metadata_csv, encoding="utf-8") as f:
        for line in f:
            parts = line.strip().split("|", 2)
            if len(parts) >= 2:
                refs[parts[0]] = parts[1]
    return refs


def load_transcribe_wav_output(text: str) -> tuple[str, str, float | None]:
    """Parse stdout of transcribe_wav example -> (hyp, ref, wer_pct)."""
    hyp = ref = ""
    wer_val: float | None = None
    in_hyp = in_ref = False
    for line in text.splitlines():
        if "=== Full transcription ===" in line:
            in_hyp, in_ref = True, False
            continue
        if "=== Reference ===" in line:
            in_ref, in_hyp = True, False
            continue
        m = re.match(r"WER:\s*([\d.]+)%", line)
        if m:
            wer_val = float(m.group(1))
            continue
        stripped = line.strip()
        if in_hyp and stripped:
            hyp = stripped
        elif in_ref and stripped:
            ref = stripped
    return hyp, ref, wer_val


def batch_test_wavs(wav_dir: str, refs: dict[str, str] | None = None,
                    transcribe_cmd: str = "cargo run --release --example transcribe_wav --",
                    max_files: int = 0, progress_cb: Callable = lambda i, n: None) -> list[dict]:
    """Batch test WAV files and return results. Shells out to transcribe_wav."""
    import subprocess

    wavs = sorted(Path(wav_dir).glob("*.wav"))
    if max_files > 0:
        wavs = wavs[:max_files]

    results: list[dict] = []
    total = len(wavs)
    found_refs = 0

    for i, wav in enumerate(wavs):
        progress_cb(i, total)
        name = wav.stem
        try:
            out = subprocess.check_output(
                transcribe_cmd.split() + [str(wav)],
                stderr=subprocess.DEVNULL, timeout=120, text=True)
        except subprocess.TimeoutExpired:
            results.append({"file": name, "status": "timeout"})
            continue
        except subprocess.CalledProcessError:
            results.append({"file": name, "status": "error"})
            continue

        hyp, ref, w = load_transcribe_wav_output(out)

        if not ref and refs:
            for prefix, rtext in refs.items():
                if name.startswith(prefix):
                    ref = rtext
                    break

        entry: dict = {"file": name, "hyp": hyp, "ref": ref, "status": "ok"}
        if ref:
            entry["wer"] = round(wer(ref, hyp), 2)
            found_refs += 1
        else:
            entry["wer"] = None

        results.append(entry)

    return results


def batch_summary(results: list[dict]) -> dict:
    """Compute aggregate stats."""
    wers = [r["wer"] for r in results if r.get("wer") is not None]
    return {
        "total": len(results),
        "with_ref": len(wers),
        "avg_wer": round(sum(wers) / len(wers), 2) if wers else None,
        "min_wer": min(wers) if wers else None,
        "max_wer": max(wers) if wers else None,
        "zero_wer": sum(1 for w in wers if w == 0),
        "wer_under_5": sum(1 for w in wers if w < 5),
        "wer_distribution": {
            "0%": sum(1 for w in wers if w == 0),
            "0-5%": sum(1 for w in wers if 0 < w < 5),
            "5-10%": sum(1 for w in wers if 5 <= w < 10),
            "10-20%": sum(1 for w in wers if 10 <= w < 20),
            "20-50%": sum(1 for w in wers if 20 <= w < 50),
            "50%+": sum(1 for w in wers if w >= 50),
        },
    }


def save_csv(results: list[dict], path: str = "asr_results.csv"):
    """Save results to CSV."""
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=["file", "wer", "hyp", "ref", "status"])
        w.writeheader()
        for r in results:
            w.writerow({k: r.get(k, "") for k in w.fieldnames})
    print(f"Saved {len(results)} rows -> {path}")


def parse_asr_log(log_path: str) -> list[dict]:
    """Parse Aura online ASR log (asr_*.txt) -> list of entries."""
    entries: list[dict] = []
    with open(log_path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            m = re.match(r"([\d.]+)\s+(\w)\s+(.*)", line)
            if m:
                entries.append({
                    "elapsed": float(m.group(1)),
                    "type": m.group(2),
                    "text": m.group(3),
                })
    return entries


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser(description="Aura ASR test utilities")
    ap.add_argument("mode", choices=["batch", "log", "wer", "csv-summary"],
                    help="batch: test WAVs, log: parse ASR log, wer: compute WER, csv-summary: summarize CSV")
    ap.add_argument("--wav-dir", default="OpenSLR/LJSpeech/wavs",
                    help="WAV directory for batch mode")
    ap.add_argument("--metadata", default="OpenSLR/LJSpeech/metadata.csv",
                    help="LJSpeech metadata.csv path")
    ap.add_argument("--log", help="Path to asr_*.txt log file")
    ap.add_argument("--ref", help="Reference text (for 'wer' mode)")
    ap.add_argument("--hyp", help="Hypothesis text (for 'wer' mode)")
    ap.add_argument("--csv", help="CSV results file to summarize")
    ap.add_argument("--max", type=int, default=0,
                    help="Max files (batch mode)")
    ap.add_argument("--output", default="asr_results.csv",
                    help="Output CSV path")
    args = ap.parse_args()

    if args.mode == "wer":
        if not args.ref or not args.hyp:
            print("Usage: test_asr.py wer --ref 'reference text' --hyp 'hypothesis text'")
            sys.exit(1)
        w = wer(args.ref, args.hyp)
        print(f"WER: {w:.2f}%")

    elif args.mode == "batch":
        refs = load_ljspeech_refs(args.metadata)
        print(f"Loaded {len(refs)} references from {args.metadata}")
        results = batch_test_wavs(args.wav_dir, refs=refs,
                                   max_files=args.max,
                                   progress_cb=lambda i, n: print(
                                       f"\r[{i}/{n}]", end="", flush=True))
        print()
        save_csv(results, args.output)
        s = batch_summary(results)
        print(json.dumps(s, indent=2))

    elif args.mode == "log":
        if not args.log:
            print("Usage: test_asr.py log --log path/to/asr_*.txt")
            sys.exit(1)
        entries = parse_asr_log(args.log)
        print(f"Parsed {len(entries)} entries from {args.log}")
        for e in entries:
            print(f"  {e['elapsed']:8.3f}s  {e['type']}  {e['text'][:80]}")

    elif args.mode == "csv-summary":
        if not args.csv:
            print("Usage: test_asr.py csv-summary --csv asr_results.csv")
            sys.exit(1)
        with open(args.csv, encoding="utf-8") as f:
            results = list(csv.DictReader(f))
        for r in results:
            if r.get("wer"):
                try:
                    r["wer"] = float(r["wer"])
                except ValueError:
                    r["wer"] = None
        s = batch_summary(results)
        print(json.dumps(s, indent=2))

    else:
        ap.print_help()
