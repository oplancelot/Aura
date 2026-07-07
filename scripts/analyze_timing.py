import csv
import glob
import os

log_dir = r"D:\repo\aura\ui\Aura\publish\logs"
csv_files = glob.glob(os.path.join(log_dir, "timing_*.csv"))
if not csv_files:
    print("No timing CSV files found.")
    exit(1)
latest_csv = max(csv_files, key=os.path.getctime)
print(f"Analyzing {latest_csv}...\n")

rows = []
with open(latest_csv) as f:
    reader = csv.DictReader(f)
    for r in reader:
        r['asr_ms'] = int(r['asr_ms'])
        r['rust_ms'] = int(r['rust_ms'])
        r['display_delay_us'] = int(r['display_delay_us'])
        r['e2e_render_ms'] = int(r['e2e_render_ms'])
        rows.append(r)

finals = [r for r in rows if r['type'] == 'F' and r['asr_ms'] > 0]
provisionals = [r for r in rows if r['type'] == 'P']

print("=== ASR Final chunks (real inference) ===")
if finals:
    asr_times = [r['asr_ms'] for r in finals]
    e2e_times = [r['e2e_render_ms'] for r in finals]
    display_delays = [r['display_delay_us'] for r in finals]
    print(f"  Count: {len(finals)}")
    print(f"  ASR(T5):     min={min(asr_times)}ms  max={max(asr_times)}ms  avg={sum(asr_times)//len(asr_times)}ms")
    print(f"  E2E render:  min={min(e2e_times)}ms  max={max(e2e_times)}ms  avg={sum(e2e_times)//len(e2e_times)}ms")
    print(f"  C# display:  min={min(display_delays)}us  max={max(display_delays)}us  avg={sum(display_delays)//len(display_delays)}us")

print()
print("=== Provisional chunks (typing preview) ===")
if provisionals:
    delays = [r['display_delay_us'] for r in provisionals]
    print(f"  Count: {len(provisionals)}")
    print(f"  C# display:  min={min(delays)}us  max={max(delays)}us  avg={sum(delays)//len(delays)}us")
