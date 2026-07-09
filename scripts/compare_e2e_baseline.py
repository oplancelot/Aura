"""Compare E2E vs offline ASR baseline (delta WER).

Usage:
    python scripts/compare_e2e_baseline.py -a asr_summary.json -e e2e_summary.json
    python scripts/compare_e2e_baseline.py                          # auto-find latest pair
"""

import argparse
import csv
import datetime
import json
import glob
import os
import sys


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


def load_summary(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


LOG_DIR = "scripts/logs"

def pick_latest(pattern):
    files = glob.glob(os.path.join(LOG_DIR, pattern))
    if not files:
        return None
    return max(files, key=os.path.getmtime)


def load_csv(path):
    rows = {}
    with open(path, encoding="utf-8") as f:
        reader = csv.DictReader(f)
        for r in reader:
            rows[r["File"]] = r
    return rows


def main():
    parser = argparse.ArgumentParser(description="Compare E2E vs offline ASR baseline")
    parser.add_argument("-a", "--asr-summary", help="ASR batch summary JSON")
    parser.add_argument("-e", "--e2e-summary", help="E2E batch summary JSON")
    parser.add_argument("--show-detail", action="store_true", help="Show per-file detail")
    args = parser.parse_args()

    asr_path = args.asr_summary or pick_latest("asr_batch_summary_*.json")
    e2e_path = args.e2e_summary or pick_latest("e2e_batch_summary_accuracy_*.json")

    if not asr_path or not e2e_path:
        print("ERROR: Cannot find summary JSONs. Specify -a and -e.", file=sys.stderr)
        sys.exit(1)

    print(f"Comparing:  ASR: {asr_path}  vs  E2E: {e2e_path}")

    asr_meta = load_summary(asr_path)
    e2e_meta = load_summary(e2e_path)

    asr_csv = os.path.join(LOG_DIR, asr_meta["artifacts"]["results_csv"])
    e2e_csv = os.path.join(LOG_DIR, e2e_meta["artifacts"]["results_csv"])

    if not os.path.exists(asr_csv):
        print(f"ERROR: {asr_csv} not found", file=sys.stderr)
        sys.exit(1)
    if not os.path.exists(e2e_csv):
        print(f"ERROR: {e2e_csv} not found", file=sys.stderr)
        sys.exit(1)

    asr_rows = load_csv(asr_csv)
    e2e_rows = load_csv(e2e_csv)

    delta_list = []
    results = []

    for file in sorted(e2e_rows.keys()):
        asr = asr_rows.get(file)
        if not asr:
            continue
        e2e_wer = float(e2e_rows[file]["WER"])
        asr_wer = float(asr["WER"])
        delta = round(e2e_wer - asr_wer, 1)
        delta_list.append(delta)
        results.append({"File": file, "Asr_WER": asr_wer, "E2e_WER": e2e_wer, "Delta_WER": delta})
        if args.show_detail:
            sign = "+" if delta > 0 else ""
            print(f"  {file}  ASR={asr_wer}%  E2E={e2e_wer}%  Δ={sign}{delta}")

    timestamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%d_%H%M%S")
    csv_out = os.path.join(LOG_DIR, f"baseline_comparison_{timestamp}.csv")
    with open(csv_out, "w", newline="", encoding="utf-8") as f:
        w = csv.DictWriter(f, fieldnames=["File", "Asr_WER", "E2e_WER", "Delta_WER"])
        w.writeheader()
        w.writerows(results)

    n = len(delta_list)
    mean_delta = round(sum(delta_list) / n, 2) if n > 0 else None
    p50 = round(percentile(delta_list, 50), 1) if n > 0 else None
    p90 = round(percentile(delta_list, 90), 1) if n > 0 else None
    p95 = round(percentile(delta_list, 95), 1) if n > 0 else None
    delta_gt_0 = sum(1 for d in delta_list if d > 0)
    delta_gt_5 = sum(1 for d in delta_list if d > 5)
    delta_gt_10 = sum(1 for d in delta_list if d > 10)
    delta_lt_minus5 = sum(1 for d in delta_list if d < -5)

    l0_threshold = 10.0
    l1_threshold = 5.0
    l0_fail = [d for d in delta_list if d > l0_threshold]
    l1_fail_count = sum(1 for d in delta_list if d > l1_threshold)
    l1_fail_pct = round(100.0 * l1_fail_count / n, 1) if n > 0 else None
    l0_pass = len(l0_fail) == 0
    l1_pass = l1_fail_pct < 20.0 if l1_fail_pct is not None else None

    print(f"\n========================================")
    print(f"   ΔWER Comparison Report")
    print(f"========================================")
    print(f"Files matched: {n}")
    print(f"")
    print(f"ΔWER = E2E_WER - ASR_WER")
    print(f"")
    print(f"Mean ΔWER: {mean_delta}pp")
    print(f"p50/p90/p95: {p50}pp / {p90}pp / {p95}pp")
    print(f"")
    print(f"Distribution:")
    print(f"  Δ >  +0pp: {delta_gt_0} files ({round(100 * delta_gt_0 / n, 0)}%)")
    print(f"  Δ >  +5pp: {delta_gt_5} files ({round(100 * delta_gt_5 / n, 0)}%)")
    print(f"  Δ > +10pp: {delta_gt_10} files ({round(100 * delta_gt_10 / n, 0)}%)")
    print(f"  Δ <  -5pp: {delta_lt_minus5} files ({round(100 * delta_lt_minus5 / n, 0)}%)")
    print(f"")
    print(f"--- Gates ---")
    print(f"L0 (no ΔWER > +10pp): {'PASS' if l0_pass else 'FAIL'}  "
          f"(max Δ = {round(max(l0_fail), 1) if l0_fail else 'N/A'})")
    print(f"L1 (<20% files with Δ > +5pp): {'PASS' if l1_pass else 'FAIL'}  "
          f"({l1_fail_count}/{n} = {l1_fail_pct}%)")
    print(f"")
    if not l0_pass:
        print("Worst offenders (Δ > +10pp):")
        for r in sorted(results, key=lambda x: x["Delta_WER"], reverse=True):
            if r["Delta_WER"] > l0_threshold:
                print(f"  {r['File']}: ASR={r['Asr_WER']}%  E2E={r['E2e_WER']}%  Δ=+{r['Delta_WER']}")

    # Save JSON
    json_out = os.path.join(LOG_DIR, f"baseline_comparison_{timestamp}.json")
    summary = {
        "protocol_version": "1.0",
        "asr_summary": asr_path,
        "e2e_summary": e2e_path,
        "files_matched": n,
        "delta_metrics": {
            "mean_delta_pp": mean_delta,
            "p50_delta_pp": p50,
            "p90_delta_pp": p90,
            "p95_delta_pp": p95,
            "delta_gt_0": delta_gt_0,
            "delta_gt_5": delta_gt_5,
            "delta_gt_10": delta_gt_10,
            "delta_lt_minus5": delta_lt_minus5,
        },
        "gates": {
            "L0": {
                "description": "No file exceeds ΔWER > +10pp",
                "threshold": l0_threshold,
                "pass": l0_pass,
                "max_delta": round(max(l0_fail), 1) if l0_fail else None,
            },
            "L1": {
                "description": "Fewer than 20% of files exceed ΔWER > +5pp",
                "threshold": l1_threshold,
                "pass": l1_pass,
                "fail_pct": l1_fail_pct,
            },
        },
    }
    with open(json_out, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, ensure_ascii=False)

    print(f"\nResults saved to {csv_out}")
    print(f"Summary saved to {json_out}")


if __name__ == "__main__":
    main()
