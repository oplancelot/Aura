"""Generate E2E test report markdown from JSON summaries.

Usage:
    python scripts/generate_report.py                          # auto-find latest JSONs
    python scripts/generate_report.py -a summary.json           # accuracy only
    python scripts/generate_report.py -a a.json -l l.json       # both
    python scripts/generate_report.py -o scripts/logs/report.md # custom output
"""

import argparse
import json
import glob
import os
import sys
from datetime import date
from pathlib import Path

LOG_DIR = "scripts/logs"


def bar(val, mx, width=12):
    if mx <= 0 or val <= 0:
        return "░" * width
    filled = max(0, min(width, round(val / mx * width)))
    return "█" * filled + "░" * (width - filled)


def load_json(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def pick_latest(pattern):
    files = glob.glob(os.path.join(LOG_DIR, pattern))
    if not files:
        return None
    return max(files, key=os.path.getmtime)


def fmt_num(v, ndigits=1):
    if v is None:
        return "N/A"
    return f"{v:.{ndigits}f}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("-a", "--accuracy", help="Accuracy mode summary JSON")
    parser.add_argument("-l", "--latency", help="Latency mode summary JSON")
    parser.add_argument("-o", "--out", help="Output markdown path")
    args = parser.parse_args()

    acc_path = args.accuracy or pick_latest("e2e_batch_summary_accuracy_*.json")
    lat_path = args.latency or pick_latest("e2e_batch_summary_realtime_*.json")

    if not acc_path and not lat_path:
        print("ERROR: no summary JSONs found", file=sys.stderr)
        sys.exit(1)

    acc = load_json(acc_path) if acc_path and os.path.exists(acc_path) else None
    lat = load_json(lat_path) if lat_path and os.path.exists(lat_path) else None

    commit = (acc or lat)["git_commit"][:8]
    today = date.today().isoformat()
    out_path = args.out or os.path.join(LOG_DIR, f"e2e_test_report_{today}.md")

    L = []
    L.append("# E2E Test Report")
    L.append("")
    L.append(f"> Date: {today}")
    L.append(f"> Git commit: `{commit}`")

    if acc:
        m = acc["metrics"]
        d = acc["dataset"]
        L.append("")
        L.append("## 1. Accuracy")
        L.append("")
        L.append("| Metric | Value |")
        L.append("|--------|-------|")
        L.append(f"| **Avg WER** | **{fmt_num(m['avg_wer_pct'])}%** |")
        L.append(f"| Files tested | {d['tested']} / {d['total_wavs']} |")
        L.append(f"| p50 / p90 / p95 | {fmt_num(m['wer_p50_pct'])}% / {fmt_num(m['wer_p90_pct'])}% / {fmt_num(m['wer_p95_pct'])}% |")
        L.append(f"| WER=0 | {m['wer_zero_count']} files |")
        L.append(f"| WER < 5% | {m['wer_under_5_count']} files |")
        L.append(f"| WER >= 20% | {m['wer_over_20_count']} files |")
        L.append(f"| ASR errors | {m['asr_error_files']} files |")
        L.append(f"| No reference | {m['no_ref_count']} files |")

        L.append("")
        L.append("## 2. Latency (Accuracy mode)")
        L.append("")
        L.append("| Metric | Value |")
        L.append("|--------|-------|")
        L.append(f"| **Avg ASR** | **{fmt_num(m['avg_asr_ms'], 0)}ms** |")
        L.append(f"| ASR p50 / p90 / p95 | {fmt_num(m['asr_p50_ms'], 0)}ms / {fmt_num(m['asr_p90_ms'], 0)}ms / {fmt_num(m['asr_p95_ms'], 0)}ms |")
        L.append(f"| **Avg processing** | **{fmt_num(m['avg_processing_s'], 2)}s** |")
        if d["tested"] > 0 and m.get("avg_processing_s"):
            rtf = round(m["avg_processing_s"] * d["tested"] / 100.0, 2)
            L.append(f"| **RTF** | **{rtf}x** |")
        if m.get("endpoint_avg_ms") is not None:
            L.append(f"| **Endpoint** | avg={fmt_num(m['endpoint_avg_ms'], 0)}ms, "
                     f"p50={fmt_num(m['endpoint_p50_ms'], 0)}ms, "
                     f"p90={fmt_num(m['endpoint_p90_ms'], 0)}ms |")
        if m.get("ttfp_avg_ms") is not None:
            L.append(f"| **TTFP** | avg={fmt_num(m['ttfp_avg_ms'], 0)}ms |")

        L.append("")
        L.append("## 3. Segmentation")
        L.append("")
        L.append("| Metric | Value |")
        L.append("|--------|-------|")
        L.append(f"| Total chunks | {m['total_chunks']} "
                 f"(Final={m['final_chunks']}, HardCut={m['hardcut_chunks']}, "
                 f"Provisional={m['provisional_chunks']}) |")
        L.append(f"| Multi-chunk pct | {fmt_num(m['multi_chunk_pct'])}% |")
        L.append(f"| Flush pct | {fmt_num(m['flush_pct'])}% |")
        L.append(f"| ASR error files | {m['asr_error_files']} |")
        if m.get("mean_avg_chunk_s") is not None:
            L.append(f"| Avg chunk | {fmt_num(m['mean_avg_chunk_s'], 1)}s |")
        if m.get("global_min_chunk_s") is not None:
            L.append(f"| Global min/max chunk | {fmt_num(m['global_min_chunk_s'], 1)}s / "
                     f"{fmt_num(m['global_max_chunk_s'], 1)}s |")

    if lat:
        m = lat["metrics"]
        L.append("")
        L.append("## 4. Display Quality (Latency + DisplayEval)")
        L.append("")
        L.append("| Metric | Value |")
        L.append("|--------|-------|")
        if m.get("prefix_match_avg_pct") is not None:
            L.append(f"| **Prefix match avg** | **{fmt_num(m['prefix_match_avg_pct'])}%** |")
            L.append(f"| Prefix match p50/p90 | {fmt_num(m['prefix_match_p50_pct'])}% / "
                     f"{fmt_num(m['prefix_match_p90_pct'])}% |")
        if m.get("text_stability_avg_pct") is not None:
            L.append(f"| **Text stability avg** | **{fmt_num(m['text_stability_avg_pct'])}%** |")
        if m.get("ttfp_avg_ms") is not None:
            L.append(f"| **TTFP avg** | **{fmt_num(m['ttfp_avg_ms'], 0)}ms** |")
        if m.get("endpoint_avg_ms") is not None:
            L.append(f"| **Endpoint** | avg={fmt_num(m['endpoint_avg_ms'], 0)}ms, "
                     f"p50={fmt_num(m['endpoint_p50_ms'], 0)}ms, "
                     f"p90={fmt_num(m['endpoint_p90_ms'], 0)}ms |")
        L.append(f"| Total chunks | {m['total_chunks']} "
                 f"(Final={m['final_chunks']}, HardCut={m['hardcut_chunks']}, "
                 f"Provisional={m['provisional_chunks']}) |")

    if acc and lat:
        am = acc["metrics"]
        lm = lat["metrics"]
        max_wer = max(am["avg_wer_pct"], lm["avg_wer_pct"])
        max_asr = max(am["avg_asr_ms"], lm["avg_asr_ms"])
        max_ep = max(am.get("endpoint_avg_ms", 0) or 0, lm.get("endpoint_avg_ms", 0) or 0)

        L.append("")
        L.append("## 5. Comparison")
        L.append("")
        L.append("```")
        L.append("                        Accuracy              Latency")
        L.append("                   -----------------  -----------------")

        aw = round(am["avg_wer_pct"], 1)
        lw = round(lm["avg_wer_pct"], 1)
        L.append(f"WER  (avg)         {bar(aw, max_wer)} {aw:>5.1f}%      "
                 f"{bar(lw, max_wer)} {lw:>5.1f}%")

        aa = int(round(am["avg_asr_ms"]))
        la = int(round(lm["avg_asr_ms"]))
        L.append(f"ASR  (avg)         {bar(aa, max_asr)} {aa:>5}ms     "
                 f"{bar(la, max_asr)} {la:>5}ms")

        if am.get("endpoint_avg_ms") and lm.get("endpoint_avg_ms"):
            ae = int(round(am["endpoint_avg_ms"]))
            le = int(round(lm["endpoint_avg_ms"]))
            L.append(f"Endpoint (p50)     {bar(ae, max_ep)} {ae:>5}ms     "
                     f"{bar(le, max_ep)} {le:>5}ms")

        at = int(round(am["ttfp_avg_ms"])) if am.get("ttfp_avg_ms") else 0
        lt = int(round(lm["ttfp_avg_ms"])) if lm.get("ttfp_avg_ms") else 0
        if at or lt:
            mt = max(at, lt)
            at_bar = bar(at, mt) if at > 0 else "░" * 12
            lt_bar = bar(lt, mt) if lt > 0 else "░" * 12
            at_str = f"{at}ms" if at > 0 else " N/A"
            lt_str = f"{lt}ms" if lt > 0 else " N/A"
            L.append(f"TTFP (avg)         {at_bar} {at_str:>7}     "
                     f"{lt_bar} {lt_str:>7}")

        L.append("```")
        L.append("")
        L.append("<sub>* DisplayEval ASR/WER is intrusive; only display metrics are meaningful</sub>")

    with open(out_path, "w", encoding="utf-8") as f:
        f.write("\n".join(L) + "\n")
    print(f"Report saved to {out_path}")


if __name__ == "__main__":
    main()
