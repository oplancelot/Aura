"""LJSpeech offline ASR batch test.

Usage:
    python scripts/run_asr_batch.py [--max-files N]
    python scripts/run_asr_batch.py --max-files 10
"""

import argparse
import csv
import datetime
import json
import os
import re
import subprocess
import sys
from pathlib import Path


def percentile(values, p):
    vals = sorted(values)
    n = len(vals)
    if n == 0:
        return None
    if n == 1:
        return vals[0]
    rank = (p / 100.0) * (n - 1)
    lo = int(rank)
    hi = lo + 1 if lo < n - 1 else lo
    w = rank - lo
    return vals[lo] * (1.0 - w) + vals[hi] * w


def git_info():
    try:
        commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], stderr=subprocess.DEVNULL, text=True
        ).strip()
    except Exception:
        commit = "unknown"
    try:
        dirty = bool(subprocess.check_output(
            ["git", "status", "--porcelain"], stderr=subprocess.DEVNULL, text=True
        ).strip())
    except Exception:
        dirty = False
    return commit, dirty


def main():
    parser = argparse.ArgumentParser(description="LJSpeech offline ASR batch test")
    parser.add_argument("--max-files", type=int, default=0, help="Max files to test (0=all)")
    parser.add_argument("--skip-build", action="store_true", help="Skip cargo build")
    parser.add_argument("--wav-dir", default="OpenSLR/LJSpeech/wavs",
                        help="WAV directory (default: OpenSLR/LJSpeech/wavs)")
    args = parser.parse_args()

    log_dir = Path("scripts/logs")
    log_dir.mkdir(parents=True, exist_ok=True)

    wav_dir = Path(args.wav_dir)
    example = Path("core/target/release/examples/transcribe_wav.exe")

    now = datetime.datetime.now(datetime.timezone.utc)
    timestamp = now.strftime("%Y%m%d_%H%M%S")
    csv_out = log_dir / f"asr_batch_results_{timestamp}.csv"
    json_out = log_dir / f"asr_batch_summary_{timestamp}.json"
    started_at = now.isoformat()

    git_commit, git_dirty = git_info()
    machine = os.environ.get("COMPUTERNAME", "unknown")

    # Build
    if not args.skip_build:
        print(f"Building transcribe_wav...")
        print(f"Commit: {git_commit[:12]}")
        result = subprocess.run(
            ["cargo", "build", "--release", "--example", "transcribe_wav"],
            cwd="core", capture_output=True, text=True
        )
        if result.returncode != 0:
            print("Build failed:", result.stderr, file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Commit: {git_commit[:12]}")

    if not example.exists():
        print(f"ERROR: {example} not found", file=sys.stderr)
        sys.exit(1)

    wavs = sorted(wav_dir.glob("*.wav"))
    if args.max_files > 0:
        wavs = wavs[:args.max_files]
    total = len(wavs)

    results = []
    wer_list = []
    time_list = []
    total_wer = 0.0
    total_time = 0.0
    wer_zero = 0
    wer_under5 = 0
    wer_over20 = 0
    no_ref_count = 0
    tested = 0

    print(f"\nTesting {total} files...\n")

    for i, wav in enumerate(wavs):
        name = wav.stem
        try:
            out = subprocess.check_output(
                [str(example), str(wav)], stderr=subprocess.DEVNULL, text=True, timeout=120
            )
        except subprocess.CalledProcessError:
            print(f"[{i + 1}/{total}] {name}  (error)")
            continue
        except subprocess.TimeoutExpired:
            print(f"[{i + 1}/{total}] {name}  (timeout)")
            continue

        # Parse output
        lines = out.splitlines()
        wer_val = None
        time_sec = 0.0
        hyp = ""
        ref = ""
        in_hyp = False
        in_ref = False

        for line in lines:
            if "=== Full transcription ===" in line:
                in_hyp, in_ref = True, False
                continue
            if "=== Reference ===" in line:
                in_ref, in_hyp = True, False
                continue
            stripped = line.strip()
            if in_hyp and stripped:
                hyp = stripped
                in_hyp = False
            elif in_ref and stripped:
                ref = stripped
                in_ref = False
            m = re.match(r"WER:\s*([\d.]+)%", line)
            if m:
                wer_val = float(m.group(1))
            m = re.match(r"^Audio: .+ Processing: ([\d.]+)s", line)
            if m:
                time_sec = float(m.group(1))

        print(f"[{i + 1}/{total}] {name}  WER: {wer_val}%  time: {time_sec}s" if wer_val is not None
              else f"[{i + 1}/{total}] {name}  (no reference)")

        if wer_val is not None:
            results.append({
                "File": name, "WER": wer_val,
                "Time_s": round(time_sec, 2)
            })
            total_wer += wer_val
            total_time += time_sec
            wer_list.append(wer_val)
            time_list.append(time_sec)
            if wer_val == 0:
                wer_zero += 1
            if wer_val < 5:
                wer_under5 += 1
            if wer_val >= 20:
                wer_over20 += 1
            tested += 1
        else:
            no_ref_count += 1

    finished_at = datetime.datetime.now(datetime.timezone.utc).isoformat()

    # Compute aggregates
    print(f"\n=== ASR Batch Summary ===")
    print(f"Files tested: {tested} / {total}")
    if tested > 0:
        avg_wer = round(total_wer / tested, 1)
        avg_time = round(total_time / tested, 2)
        wer_p50 = round(percentile(wer_list, 50), 1)
        wer_p90 = round(percentile(wer_list, 90), 1)
        wer_p95 = round(percentile(wer_list, 95), 1)
        time_p50 = round(percentile(time_list, 50), 2)
        time_p90 = round(percentile(time_list, 90), 2)
        time_p95 = round(percentile(time_list, 95), 2)

        print(f"Avg WER: {avg_wer}%  |  p50/p90/p95: {wer_p50}% / {wer_p90}% / {wer_p95}%")
        print(f"Avg time: {avg_time}s  |  p50/p90/p95: {time_p50}s / {time_p90}s / {time_p95}s")
        print(f"Total ASR time: {round(total_time, 1)}s")
        print(f"WER distribution: 0%={wer_zero} ({round(100 * wer_zero / tested, 0)}%)  |  "
              f"<5%={wer_under5} ({round(100 * wer_under5 / tested, 0)}%)  |  "
              f">=20%={wer_over20} ({round(100 * wer_over20 / tested, 0)}%)")
        print(f"No reference found: {no_ref_count}")
    else:
        avg_wer = wer_p50 = wer_p90 = wer_p95 = None
        avg_time = time_p50 = time_p90 = time_p95 = None

    # Save CSV
    with open(csv_out, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=["File", "WER", "Time_s"])
        w.writeheader()
        w.writerows(results)
    print(f"Results saved to {csv_out}")

    # Save JSON summary
    summary = {
        "protocol_version": "1.0",
        "suite": "offline-asr",
        "mode": "30s-chunk-2s-overlap",
        "started_at_utc": started_at,
        "finished_at_utc": finished_at,
        "git_commit": git_commit,
        "git_dirty": git_dirty,
        "machine": machine,
        "dataset": {
            "name": "LJSpeech",
            "wav_dir": str(wav_dir),
            "max_files": args.max_files,
            "total_wavs": total,
            "tested": tested,
            "selection": "first_n_sorted_by_name" if args.max_files > 0 else "all_sorted_by_name",
        },
        "models": {"asr": "assets/sense-voice-small-q4_k.gguf"},
        "metrics": {
            "avg_wer_pct": avg_wer,
            "wer_p50_pct": wer_p50,
            "wer_p90_pct": wer_p90,
            "wer_p95_pct": wer_p95,
            "wer_zero_count": wer_zero,
            "wer_under_5_count": wer_under5,
            "wer_over_20_count": wer_over20,
            "avg_time_s": avg_time,
            "time_p50_s": time_p50,
            "time_p90_s": time_p90,
            "time_p95_s": time_p95,
            "total_time_s": round(total_time, 1) if tested > 0 else None,
            "no_ref_count": no_ref_count,
        },
        "artifacts": {
            "results_csv": csv_out.name,
            "summary_json": json_out.name,
        },
    }
    with open(json_out, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, ensure_ascii=False)
    print(f"Summary saved to {json_out}")


if __name__ == "__main__":
    main()
