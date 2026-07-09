"""LJSpeech E2E pipeline batch test.

Compiles e2e_transcribe_wav, walks WAVs, collects metrics, saves CSV + JSON.

Usage:
    python scripts/run_e2e_batch.py [--max-files N] [--realtime] [--display-eval] [--silence-close N] [--hard-cut N] [--threads N]
    python scripts/run_e2e_batch.py --max-files 10
    python scripts/run_e2e_batch.py --max-files 5 --suite Latency --display-eval
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


def parse_summary(lines, mode_name):
    """Parse e2e_transcribe_wav stdout summary lines into a dict."""
    result = {
        "mode": None, "wer": None, "audio_s": None, "proc_s": None,
        "asr_ms": 0, "chunks": 0, "final": 0, "hardcut": 0, "provisional": 0,
        "avg_chunk": 0, "min_chunk": 0, "max_chunk": 0,
        "flush": False, "asr_errors": 0,
        "ep_p50": None, "ep_p90": None, "ep_p95": None,
        "ttfp": None,
        "pm_avg": None, "pm_p50": None, "pm_p90": None,
        "stability": None,
    }

    for line in lines:
        m = re.match(r"^Mode: (\w+)", line)
        if m:
            result["mode"] = m.group(1)

        m = re.match(r"^WER: ([\d.]+)%", line)
        if m:
            result["wer"] = float(m.group(1))

        m = re.match(
            r"^Audio: ([\d.]+)s\s*\|\s*Processing: ([\d.]+)s\s*\|\s*ASR: (\d+)ms",
            line,
        )
        if m:
            result["audio_s"] = float(m.group(1))
            result["proc_s"] = float(m.group(2))
            result["asr_ms"] = float(m.group(3))

        m = re.match(
            r"^Total chunks: (\d+).*Final: (\d+), HardCut: (\d+), Provisional: (\d+)",
            line,
        )
        if m:
            result["chunks"] = int(m.group(1))
            result["final"] = int(m.group(2))
            result["hardcut"] = int(m.group(3))
            result["provisional"] = int(m.group(4))

        m = re.match(r"^Avg chunk: ([\d.]+)s.*Min: ([\d.]+)s.*Max: ([\d.]+)s", line)
        if m:
            result["avg_chunk"] = float(m.group(1))
            result["min_chunk"] = float(m.group(2))
            result["max_chunk"] = float(m.group(3))

        m = re.match(r"^Flush: (\w+).*ASR errors: (\d+)", line)
        if m:
            result["flush"] = m.group(1) == "yes"
            result["asr_errors"] = int(m.group(2))

        m = re.match(
            r"^Endpoint latency \(Final\): p50=([\d.]+)ms.*p90=([\d.]+)ms.*p95=([\d.]+)ms",
            line,
        )
        if m:
            result["ep_p50"] = float(m.group(1))
            result["ep_p90"] = float(m.group(2))
            result["ep_p95"] = float(m.group(3))

        m = re.match(r"^TTFP: ([\d.]+)ms", line)
        if m:
            result["ttfp"] = float(m.group(1))

        m = re.match(
            r"^Provisional ASR chunks: \d+  \|  Prefix match: "
            r"p50=([\d.]+)%  p90=([\d.]+)%  avg=([\d.]+)%",
            line,
        )
        if m:
            result["pm_p50"] = float(m.group(1))
            result["pm_p90"] = float(m.group(2))
            result["pm_avg"] = float(m.group(3))

        m = re.match(r"^Text stability: ([\d.]+)%", line)
        if m:
            result["stability"] = float(m.group(1))

    if result["mode"] and result["mode"] != mode_name:
        print(f"  WARN: binary Mode={result['mode']} expected={mode_name}")

    return result


def main():
    parser = argparse.ArgumentParser(description="LJSpeech E2E pipeline batch test")
    parser.add_argument("--max-files", type=int, default=0, help="Max files (0=all)")
    parser.add_argument("--realtime", action="store_true", help="Latency/realtime mode")
    parser.add_argument("--display-eval", action="store_true", help="Display quality eval")
    parser.add_argument("--suite", choices=["Accuracy", "Latency"], default="Accuracy")
    parser.add_argument("--silence-close", type=int, default=0, help="Override silence_close_ms")
    parser.add_argument("--hard-cut", type=int, default=0, help="Override hard_cut_ms")
    parser.add_argument("--threads", type=int, default=0, help="Override ASR threads")
    parser.add_argument("--skip-build", action="store_true", help="Skip cargo build (use existing binary)")
    parser.add_argument("--wav-dir", default="OpenSLR/LJSpeech/wavs",
                        help="WAV directory")
    args = parser.parse_args()

    if args.suite == "Latency":
        args.realtime = True
    mode_name = "realtime" if args.realtime else "accuracy"

    log_dir = Path("scripts/logs")
    log_dir.mkdir(parents=True, exist_ok=True)

    wav_dir = Path(args.wav_dir)
    example = Path("core/target/release/examples/e2e_transcribe_wav.exe")

    now = datetime.datetime.now(datetime.timezone.utc)
    timestamp = now.strftime("%Y%m%d_%H%M%S")
    csv_out = log_dir / f"e2e_batch_results_{mode_name}_{timestamp}.csv"
    json_out = log_dir / f"e2e_batch_summary_{mode_name}_{timestamp}.json"
    started_at = now.isoformat()

    git_commit, git_dirty = git_info()
    machine = os.environ.get("COMPUTERNAME", "unknown")

    chunking_config = {
        "silence_close_ms": 200,
        "provisional_start_ms": 1000,
        "provisional_interval_ms": 200,
        "hard_cut_ms": 5000,
        "hard_cut_overlap_ms": 2000,
    }

    # Build
    if not args.skip_build:
        print(f"Building e2e_transcribe_wav...")
        print(f"Suite: {args.suite}  |  Mode: {mode_name}  |  Commit: {git_commit[:12]}")
        result = subprocess.run(
            ["cargo", "build", "--release", "--example", "e2e_transcribe_wav"],
            cwd="core", capture_output=True, text=True
        )
        if result.returncode != 0:
            print("Build failed:", result.stderr, file=sys.stderr)
            sys.exit(1)
    else:
        print(f"Suite: {args.suite}  |  Mode: {mode_name}  |  Commit: {git_commit[:12]}")

    if not example.exists():
        print(f"ERROR: {example} not found", file=sys.stderr)
        sys.exit(1)

    wavs = sorted(wav_dir.glob("*.wav"))
    if args.max_files > 0:
        wavs = wavs[:args.max_files]
    total = len(wavs)

    results = []
    wer_list = []
    asr_list = []
    total_wer = 0.0
    total_asr_ms = 0.0
    total_proc_time = 0.0
    total_chunks = 0
    total_final = 0
    total_hardcut = 0
    total_provisional = 0
    sum_min_chunk = 0.0
    sum_avg_chunk = 0.0
    sum_max_chunk = 0.0
    global_min_chunk = float("inf")
    global_max_chunk = float("-inf")
    multi_chunk_files = 0
    flush_files = 0
    asr_error_files = 0
    no_ref_count = 0
    wer_zero = 0
    wer_under5 = 0
    wer_over20 = 0
    endpoint_list = []
    ttfp_list = []
    prefix_match_list = []
    stability_list = []
    tested = 0

    print(f"\nTesting {total} files...\n")

    for i, wav in enumerate(wavs):
        name = wav.stem
        cmd = [str(example), str(wav)]
        if args.realtime:
            cmd.append("--realtime")
        if args.display_eval:
            cmd.append("--display-eval")
        if args.silence_close > 0:
            cmd.extend(["--silence-close", str(args.silence_close)])
        if args.hard_cut > 0:
            cmd.extend(["--hard-cut", str(args.hard_cut)])
        if args.threads > 0:
            cmd.extend(["--threads", str(args.threads)])

        try:
            out = subprocess.check_output(
                cmd, stderr=subprocess.DEVNULL, text=True, timeout=300
            )
        except subprocess.CalledProcessError:
            print(f"[{i + 1}/{total}] {name}  (error)")
            continue
        except subprocess.TimeoutExpired:
            print(f"[{i + 1}/{total}] {name}  (timeout)")
            continue

        p = parse_summary(out.splitlines(), mode_name)
        wer_val = p["wer"]

        if wer_val is not None:
            results.append({
                "File": name, "WER": wer_val,
                "ASR_Time_ms": p["asr_ms"],
                "Process_Time_s": round(p["proc_s"], 2) if p["proc_s"] else 0,
                "Audio_Time_s": round(p["audio_s"], 1) if p["audio_s"] else 0,
                "Chunks": p["chunks"],
                "Final": p["final"], "HardCut": p["hardcut"],
                "Provisional": p["provisional"],
                "Min_Chunk_s": p["min_chunk"],
                "Avg_Chunk_s": p["avg_chunk"],
                "Max_Chunk_s": p["max_chunk"],
                "Flush": p["flush"],
                "ASR_Errors": p["asr_errors"],
                "Endpoint_p50_ms": p["ep_p50"],
                "Endpoint_p90_ms": p["ep_p90"],
                "Endpoint_p95_ms": p["ep_p95"],
                "TTFP_ms": p["ttfp"],
                "PrefixMatch_avg": p["pm_avg"],
                "PrefixMatch_p50": p["pm_p50"],
                "PrefixMatch_p90": p["pm_p90"],
                "Stability_pct": p["stability"],
            })
            total_wer += wer_val
            total_asr_ms += p["asr_ms"]
            if p["proc_s"]:
                total_proc_time += p["proc_s"]
            total_chunks += p["chunks"]
            total_final += p["final"]
            total_hardcut += p["hardcut"]
            total_provisional += p["provisional"]
            wer_list.append(wer_val)
            asr_list.append(p["asr_ms"])

            if wer_val == 0:
                wer_zero += 1
            if wer_val < 5:
                wer_under5 += 1
            if wer_val >= 20:
                wer_over20 += 1

            sum_min_chunk += p["min_chunk"]
            sum_avg_chunk += p["avg_chunk"]
            sum_max_chunk += p["max_chunk"]
            if p["min_chunk"] > 0 and p["min_chunk"] < global_min_chunk:
                global_min_chunk = p["min_chunk"]
            if p["max_chunk"] > global_max_chunk:
                global_max_chunk = p["max_chunk"]
            if p["chunks"] > 1:
                multi_chunk_files += 1
            if p["flush"]:
                flush_files += 1
            if p["asr_errors"] > 0:
                asr_error_files += 1
            if p["ep_p50"] is not None:
                endpoint_list.append(p["ep_p50"])
            if p["ttfp"] is not None:
                ttfp_list.append(p["ttfp"])
            if p["pm_avg"] is not None:
                prefix_match_list.append(p["pm_avg"])
            if p["stability"] is not None:
                stability_list.append(p["stability"])
            tested += 1
            print(f"[{i + 1}/{total}] {name}  WER: {wer_val}%  ASR: {p['asr_ms']}ms")
        else:
            no_ref_count += 1
            print(f"[{i + 1}/{total}] {name}  (no reference)")

    finished_at = datetime.datetime.now(datetime.timezone.utc).isoformat()

    # Aggregates
    print(f"\n=== E2E Batch Summary ===")
    print(f"Files tested: {tested} / {total}")
    print(f"Suite: {args.suite}  |  Mode: {mode_name}")

    avg_wer = avg_asr = avg_proc = None
    wer_p50 = wer_p90 = wer_p95 = None
    asr_p50 = asr_p90 = asr_p95 = None

    if tested > 0:
        avg_wer = round(total_wer / tested, 1)
        avg_asr = round(total_asr_ms / tested)
        avg_proc = round(total_proc_time / tested, 2)
        wer_p50 = round(percentile(wer_list, 50), 1)
        wer_p90 = round(percentile(wer_list, 90), 1)
        wer_p95 = round(percentile(wer_list, 95), 1)
        asr_p50 = round(percentile(asr_list, 50))
        asr_p90 = round(percentile(asr_list, 90))
        asr_p95 = round(percentile(asr_list, 95))

        print(f"Avg WER: {avg_wer}%  |  p50/p90/p95: {wer_p50}% / {wer_p90}% / {wer_p95}%")
        print(f"Avg ASR: {avg_asr}ms  |  p50/p90/p95: {asr_p50}ms / {asr_p90}ms / {asr_p95}ms")
        print(f"Avg Processing: {avg_proc}s")
        print(f"Total ASR time: {round(total_asr_ms / 1000, 1)}s")
        print(f"WER distribution: 0%={wer_zero} ({round(100 * wer_zero / tested, 0)}%)  |  "
              f"<5%={wer_under5} ({round(100 * wer_under5 / tested, 0)}%)  |  "
              f">=20%={wer_over20} ({round(100 * wer_over20 / tested, 0)}%)")

    mean_avg_chunk = round(sum_avg_chunk / tested, 2) if tested > 0 else None
    mean_min_chunk = round(sum_min_chunk / tested, 2) if tested > 0 else None
    mean_max_chunk = round(sum_max_chunk / tested, 2) if tested > 0 else None
    g_min = round(global_min_chunk, 2) if global_min_chunk != float("inf") else None
    g_max = round(global_max_chunk, 2) if global_max_chunk != float("-inf") else None
    multi_chunk_pct = round(100.0 * multi_chunk_files / tested, 1) if tested > 0 else None

    # Segmentation quality output
    if tested > 0:
        flush_pct = round(100.0 * flush_files / tested, 0)
        print(f"\n=== Segmentation Quality ({tested} files) ===")
        print(f"Total chunks: {total_chunks}  (Final: {total_final} | "
              f"HardCut: {total_hardcut} | Provisional: {total_provisional})")
        print(f"Files with >1 chunk: {multi_chunk_files} ({multi_chunk_pct}%)")
        print(f"Flush used: {flush_files} ({flush_pct}%)  |  "
              f"ASR errors: {asr_error_files} files")
        print(f"No reference found: {no_ref_count}")

        if prefix_match_list:
            pm_avg = round(sum(prefix_match_list) / len(prefix_match_list))
            pm_p50 = round(percentile(prefix_match_list, 50))
            pm_p90 = round(percentile(prefix_match_list, 90))
            print(f"Prefix match: avg={pm_avg}%  p50/p90: {pm_p50}% / {pm_p90}%")
        else:
            pm_avg = pm_p50 = pm_p90 = None

        if stability_list:
            st_avg = round(sum(stability_list) / len(stability_list))
            print(f"Text stability: avg={st_avg}%")
        else:
            st_avg = None

        if endpoint_list:
            ep_avg = round(sum(endpoint_list) / len(endpoint_list))
            ep_p50 = round(percentile(endpoint_list, 50))
            ep_p90 = round(percentile(endpoint_list, 90))
            ep_p95 = round(percentile(endpoint_list, 95))
            print(f"Endpoint latency (Final p50, per-file): avg={ep_avg}ms  "
                  f"p50/p90/p95: {ep_p50}ms / {ep_p90}ms / {ep_p95}ms")
        else:
            ep_avg = ep_p50 = ep_p90 = ep_p95 = None

        if ttfp_list:
            ttfp_avg = round(sum(ttfp_list) / len(ttfp_list))
            ttfp_p50 = round(percentile(ttfp_list, 50))
            ttfp_p90 = round(percentile(ttfp_list, 90))
            ttfp_p95 = round(percentile(ttfp_list, 95))
            print(f"TTFP: avg={ttfp_avg}ms  p50/p90/p95: {ttfp_p50}ms / {ttfp_p90}ms / {ttfp_p95}ms")
        else:
            ttfp_avg = ttfp_p50 = ttfp_p90 = ttfp_p95 = None

        print(f"Mean of per-file avg/min/max chunk: {mean_avg_chunk}s / "
              f"{mean_min_chunk}s / {mean_max_chunk}s")
        print(f"Global min/max chunk: {g_min}s / {g_max}s")

    # Save CSV
    with open(csv_out, "w", newline="", encoding="utf-8") as f:
        fieldnames = [
            "File", "WER", "ASR_Time_ms", "Process_Time_s", "Audio_Time_s",
            "Chunks", "Final", "HardCut", "Provisional",
            "Min_Chunk_s", "Avg_Chunk_s", "Max_Chunk_s",
            "Flush", "ASR_Errors",
            "Endpoint_p50_ms", "Endpoint_p90_ms", "Endpoint_p95_ms",
            "TTFP_ms",
            "PrefixMatch_avg", "PrefixMatch_p50", "PrefixMatch_p90",
            "Stability_pct",
        ]
        w = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
        w.writeheader()
        w.writerows(results)
    print(f"Results saved to {csv_out}")
    print(f"Summary saved to {json_out}")

    # Save JSON summary
    summary = {
        "protocol_version": "1.0",
        "suite": args.suite,
        "mode": mode_name,
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
        "models": {
            "vad": "assets/silero_vad.onnx",
            "asr": "assets/sense-voice-small-q4_k.gguf",
        },
        "chunking_config": chunking_config,
        "metrics": {
            "avg_wer_pct": avg_wer,
            "wer_p50_pct": wer_p50,
            "wer_p90_pct": wer_p90,
            "wer_p95_pct": wer_p95,
            "wer_zero_count": wer_zero,
            "wer_under_5_count": wer_under5,
            "wer_over_20_count": wer_over20,
            "avg_asr_ms": avg_asr,
            "asr_p50_ms": asr_p50,
            "asr_p90_ms": asr_p90,
            "asr_p95_ms": asr_p95,
            "avg_processing_s": avg_proc,
            "total_asr_s": round(total_asr_ms / 1000, 1) if tested > 0 else None,
            "total_chunks": total_chunks,
            "final_chunks": total_final,
            "hardcut_chunks": total_hardcut,
            "provisional_chunks": total_provisional,
            "multi_chunk_files": multi_chunk_files,
            "multi_chunk_pct": multi_chunk_pct,
            "flush_files": flush_files,
            "flush_pct": round(100.0 * flush_files / tested, 1) if tested > 0 else None,
            "asr_error_files": asr_error_files,
            "no_ref_count": no_ref_count,
            "prefix_match_avg_pct": pm_avg,
            "prefix_match_p50_pct": pm_p50,
            "prefix_match_p90_pct": pm_p90,
            "text_stability_avg_pct": st_avg,
            "mean_avg_chunk_s": mean_avg_chunk,
            "mean_min_chunk_s": mean_min_chunk,
            "mean_max_chunk_s": mean_max_chunk,
            "endpoint_avg_ms": ep_avg,
            "endpoint_p50_ms": ep_p50,
            "endpoint_p90_ms": ep_p90,
            "endpoint_p95_ms": ep_p95,
            "ttfp_avg_ms": ttfp_avg,
            "ttfp_p50_ms": ttfp_p50,
            "ttfp_p90_ms": ttfp_p90,
            "ttfp_p95_ms": ttfp_p95,
            "global_min_chunk_s": g_min,
            "global_max_chunk_s": g_max,
        },
        "artifacts": {
            "results_csv": csv_out.name,
            "summary_json": json_out.name,
        },
    }
    with open(json_out, "w", encoding="utf-8") as f:
        json.dump(summary, f, indent=2, ensure_ascii=False)

if __name__ == "__main__":
    main()
