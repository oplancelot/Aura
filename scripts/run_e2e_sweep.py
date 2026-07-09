"""ChunkingConfig parameter sweep.

Enumerates silence_close_ms x hard_cut_ms combinations and compares results.

Usage:
    python scripts/run_e2e_sweep.py [--max-files 10] [--suite Accuracy|Latency]
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
    parser = argparse.ArgumentParser(description="ChunkingConfig parameter sweep")
    parser.add_argument("--max-files", type=int, default=10)
    parser.add_argument("--suite", choices=["Accuracy", "Latency"], default="Accuracy")
    args = parser.parse_args()

    silence_close_values = [100, 200, 400]
    hard_cut_values = [3000, 5000, 7000]

    LOG_DIR.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d_%H%M%S")
    results = []

    try:
        git_commit = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], stderr=subprocess.DEVNULL, text=True
        ).strip()
    except Exception:
        git_commit = "unknown"

    print("--- ChunkingConfig Sweep ---")
    print(f"Suite: {args.suite}  |  MaxFiles: {args.max_files}")
    print(f"silence_close: {silence_close_values}")
    print(f"hard_cut:      {hard_cut_values}")

    # Record JSONs that exist before the sweep
    before = set(glob.glob(str(LOG_DIR / "e2e_batch_summary_*")))

    for sc in silence_close_values:
        for hc in hard_cut_values:
            label = f"sc{sc}_hc{hc}"
            print(f"\n--- Sweeping: silence_close={sc}ms  hard_cut={hc}ms ---")

            cmd = [
                sys.executable, "scripts/run_e2e_batch.py",
                "--max-files", str(args.max_files),
                "--suite", args.suite,
                "--silence-close", str(sc),
                "--hard-cut", str(hc),
                "--skip-build",
            ]
            result = subprocess.run(cmd, capture_output=True, text=True)
            if result.returncode != 0:
                print(f"  WARN: run_e2e_batch.py failed for {label}", file=sys.stderr)
                print(result.stderr, file=sys.stderr)
                continue

            # Find the new summary JSON
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
                "SilenceClose_ms": sc,
                "HardCut_ms": hc,
                "FilesTested": meta["dataset"]["tested"],
                "AvgWER": m["avg_wer_pct"],
                "WerP50": m["wer_p50_pct"],
                "WerP90": m["wer_p90_pct"],
                "WerP95": m["wer_p95_pct"],
                "WerZero": m["wer_zero_count"],
                "AvgASR_ms": m["avg_asr_ms"],
                "AsrP50_ms": m["asr_p50_ms"],
                "AsrP90_ms": m["asr_p90_ms"],
                "AvgProc_s": m["avg_processing_s"],
                "TotalChunks": m["total_chunks"],
                "MultiChunkPct": m["multi_chunk_pct"],
                "FlushPct": m["flush_pct"],
                "NoRef": m["no_ref_count"],
                "EndpointAvg_ms": m["endpoint_avg_ms"],
                "EndpointP50_ms": m["endpoint_p50_ms"],
                "EndpointP90_ms": m["endpoint_p90_ms"],
                "TtfpAvg_ms": m["ttfp_avg_ms"],
            })

    # Print results table
    print(f"\n\n========================================")
    print(f"   Sweep Results")
    print(f"========================================\n")
    header = f"{'Config':<14} {'WER_avg':>8} {'WER_p50':>8} {'ASR_ms':>8} {'Ep_avg':>8} {'Mchunk%':>8} {'Flush%':>8} {'Chunks':>10} {'n':>8}"
    print(header)
    print("-" * 88)
    for r in results:
        ep = str(r["EndpointAvg_ms"]) if r["EndpointAvg_ms"] is not None else "-"
        print(
            f"{r['Config']:<14} "
            f"{r['AvgWER']:>7.1f}% "
            f"{r['WerP50']:>7.1f}% "
            f"{r['AvgASR_ms']:>7.0f} "
            f"{ep:>7} "
            f"{r['MultiChunkPct']:>7.0f}% "
            f"{r['FlushPct']:>7.0f}% "
            f"{r['TotalChunks']:>10} "
            f"{r['FilesTested']:>8}"
        )

    # Save CSV
    csv_out = LOG_DIR / f"sweep_comparison_{timestamp}.csv"
    with open(csv_out, "w", newline="", encoding="utf-8") as f:
        fieldnames = [
            "Config", "SilenceClose_ms", "HardCut_ms", "FilesTested",
            "AvgWER", "WerP50", "WerP90", "WerP95", "WerZero",
            "AvgASR_ms", "AsrP50_ms", "AsrP90_ms",
            "AvgProc_s", "TotalChunks", "MultiChunkPct", "FlushPct", "NoRef",
            "EndpointAvg_ms", "EndpointP50_ms", "EndpointP90_ms", "TtfpAvg_ms",
        ]
        w = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
        w.writeheader()
        w.writerows(results)

    # Save JSON
    json_out = LOG_DIR / f"sweep_comparison_{timestamp}.json"
    summary = {
        "protocol_version": "1.0",
        "suite": args.suite,
        "max_files_per_config": args.max_files,
        "git_commit": git_commit,
        "silence_close_values": silence_close_values,
        "hard_cut_values": hard_cut_values,
        "configs": [
            {
                "config": r["Config"],
                "silence_close_ms": r["SilenceClose_ms"],
                "hard_cut_ms": r["HardCut_ms"],
                "files_tested": r["FilesTested"],
                "avg_wer_pct": r["AvgWER"],
                "wer_p50_pct": r["WerP50"],
                "wer_p90_pct": r["WerP90"],
                "avg_asr_ms": r["AvgASR_ms"],
                "asr_p50_ms": r["AsrP50_ms"],
                "asr_p90_ms": r["AsrP90_ms"],
                "avg_proc_s": r["AvgProc_s"],
                "total_chunks": r["TotalChunks"],
                "multi_chunk_pct": r["MultiChunkPct"],
                "flush_pct": r["FlushPct"],
                "no_ref": r["NoRef"],
                "endpoint_avg_ms": r["EndpointAvg_ms"],
                "endpoint_p50_ms": r["EndpointP50_ms"],
                "endpoint_p90_ms": r["EndpointP90_ms"],
                "ttfp_avg_ms": r["TtfpAvg_ms"],
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
