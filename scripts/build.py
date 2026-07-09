"""Build Aura: Rust core + dotnet publish + asset copy.

Usage:
    python scripts/build.py
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description="Build Aura (Rust core + dotnet publish)")
    parser.add_argument("--repo-root", default=None,
                        help="Repository root (default: parent of scripts/)")
    args = parser.parse_args()

    repo_root = Path(args.repo_root) if args.repo_root else Path(__file__).resolve().parent.parent
    assets_dir = repo_root / "assets"
    core_release = repo_root / "core" / "target" / "release"
    ui_project = repo_root / "ui" / "Aura"
    publish_dir = ui_project / "publish"

    print("[1/3] Building Rust core (release)...")
    result = subprocess.run(
        ["cargo", "build", "--release"],
        cwd=repo_root / "core",
        capture_output=True, text=True
    )
    if result.returncode != 0:
        print("cargo build failed:", result.stderr, file=sys.stderr)
        sys.exit(1)

    # Remove debug aura_core.dll to avoid NETSDK1152 conflict with release build
    debug_dll = core_release.parent / "debug" / "aura_core.dll"
    if debug_dll.exists():
        debug_dll.unlink()

    print("[2/3] Publishing Aura...")
    if publish_dir.exists():
        shutil.rmtree(publish_dir)
    result = subprocess.run(
        ["dotnet", "publish", str(ui_project),
         "-c", "Release", "--no-self-contained",
         "-r", "win-x64", "-o", str(publish_dir)],
        capture_output=True, text=True
    )
    if result.returncode != 0:
        print("dotnet publish failed:", file=sys.stderr)
        print(result.stdout, file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        sys.exit(1)

    print("[3/3] Copying runtime assets...")
    shutil.copy2(core_release / "aura_core.dll", publish_dir)
    shutil.copy2(assets_dir / "silero_vad.onnx", publish_dir)
    gguf = assets_dir / "sense-voice-small-q4_k.gguf"
    if gguf.exists():
        shutil.copy2(gguf, publish_dir)

    print(f"\nDone! Run Aura.exe from:")
    print(f"  {publish_dir}")


if __name__ == "__main__":
    main()
