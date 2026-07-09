"""ASR thread count sweep.

Enumerates different n_threads values and compares WER / ASR latency.

Usage:
    python scripts/run_thread_sweep.py [--max-files 10] [--suite Accuracy|Latency]
"""

import argparse
import csv
import datetime
import json
import os
import subprocess
import sys
import glob
from pathlib import Path

LOG_DIR = Path("scripts/logs")


def main():
    parser = argparse.ArgumentParser(description="ASR thread count sweep")
    parser.add_argument("--max-files", type=int, default=10)
    parser.add_argument("--suite", choices=["Accuracy", "Latency"], default="Accuracy")
    args = parser.parse_args()

    thread_values = [1, 2, 4, 8]

    LOG_DIR.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d_%H%M%S")
    results = []

    try:
        git_commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], stderr=subprocess.DEVNULL, text=True
        ).strip()
    except Exception:
        git_commit = "unknown"

    print("--- ASR Thread Sweep ---")
    print(f"Suite: {args.suite}  |  MaxFiles: {args.max_files}")
    print(f"threads: {thread_values}")

    before = set(glob.glob(str(LOG_DIR / "e2e_batch_summary_*")))

    for t in thread_values:
        label = f"t{t}"
        print(f"\n--- Sweeping: threads={t} ---")

        cmd = [
            sys.executable, "scripts/run_e2e_batch.py",
            "--max-files", str(args.max_files),
            "--suite", args.suite,
            "--threads", str(t),
            "--skip-build",
        ]
        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            print(f"  WARN: run_e2e_batch.py failed for {label}", file=sys.stderr)
            print(result.stderr, file=sys.stderr)
            continue

        after = set(glob.glob(str(LOG_DIR / "e2e_batch_summary_*")))
        new_files = list(after - before)
        if new_files:
            latest_json = max(new_files, key=os.path.getmtime)
        else:
            all_files = list(after)
            latest_json = max(all_files, key=os.path.getmtime) if all_files else None
        before = after

        if not latest_json or not os.path.exists(latest_json):
            print(f"  WARN: no summary JSON found for {label}")
            continue

        with open(latest_json, encoding="utf-8") as f:
            meta = json.load(f)
        m = meta["metrics"]

        results.append({
            "Config": label,
            "Threads": t,
            "FilesTested": meta["dataset"]["tested"],
            "AvgWER": m["avg_wer_pct"],
            "WerP50": m["wer_p50_pct"],
            "WerP90": m["wer_p90_pct"],
            "AvgASR_ms": m["avg_asr_ms"],
            "AsrP50_ms": m["asr_p50_ms"],
            "AsrP90_ms": m["asr_p90_ms"],
            "AvgProc_s": m["avg_processing_s"],
            "EndpointAvg_ms": m["endpoint_avg_ms"],
            "MultiChunkPct": m["multi_chunk_pct"],
        })

    # Print results table
    print(f"\n\n========================================")
    print(f"   Thread Sweep Results")
    print(f"========================================\n")
    header = f"{'Threads':<14} {'WER_avg':>8} {'WER_p50':>8} {'ASR_avg':>8} {'ASR_p50':>8} {'Ep_avg':>8} {'Proc_s':>10}"
    print(header)
    print("-" * 72)
    for r in results:
        ep = str(r["EndpointAvg_ms"]) if r["EndpointAvg_ms"] is not None else "-"
        print(
            f"{r['Config']:<14} "
            f"{r['AvgWER']:>7.1f}% "
            f"{r['WerP50']:>7.1f}% "
            f"{r['AvgASR_ms']:>7.0f} "
            f"{r['AsrP50_ms']:>7.0f} "
            f"{ep:>7} "
            f"{r['AvgProc_s']:>9.2f}"
        )

    # Save CSV
    csv_out = LOG_DIR / f"thread_sweep_{timestamp}.csv"
    with open(csv_out, "w", newline="", encoding="utf-8") as f:
        fieldnames = [
            "Config", "Threads", "FilesTested",
            "AvgWER", "WerP50", "WerP90",
            "AvgASR_ms", "AsrP50_ms", "AsrP90_ms",
            "AvgProc_s", "EndpointAvg_ms", "MultiChunkPct",
        ]
        w = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
        w.writeheader()
        w.writerows(results)

    # Save JSON
    json_out = LOG_DIR / f"thread_sweep_{timestamp}.json"
    summary = {
        "protocol_version": "1.0",
        "suite": args.suite,
        "max_files_per_config": args.max_files,
        "git_commit": git_commit,
        "thread_values": thread_values,
        "configs": [
            {
                "config": r["Config"],
                "threads": r["Threads"],
                "files_tested": r["FilesTested"],
                "avg_wer_pct": r["AvgWER"],
                "wer_p50_pct": r["WerP50"],
                "wer_p90_pct": r["WerP90"],
                "avg_asr_ms": r["AvgASR_ms"],
                "asr_p50_ms": r["AsrP50_ms"],
                "asr_p90_ms": r["AsrP90_ms"],
                "avg_proc_s": r["AvgProc_s"],
                "endpoint_avg_ms": r["EndpointAvg_ms"],
                "multi_chunk_pct": r["MultiChunkPct"],
            }
            for r in results
        ],
    }
    with open(json_out, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, ensure_ascii=False)

    print(f"\nResults saved to {csv_out}")
    print(f"Summary saved to {json_out}")


if __name__ == "__main__":
    main()
