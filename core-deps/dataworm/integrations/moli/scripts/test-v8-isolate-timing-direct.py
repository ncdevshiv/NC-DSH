#!/usr/bin/env python3
"""Test V8 isolate initialization timing by running moli serve directly."""
from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path

from moli_benchmark.config import REPO_ROOT


def main() -> None:
    moli_bin = REPO_ROOT / "target" / "release" / "moli"
    if not moli_bin.exists():
        print(f"Error: moli binary not found at {moli_bin}")
        print("Please run: cargo build --release")
        return

    print("=" * 80)
    print("Testing V8 isolate initialization timing")
    print("=" * 80)

    # Run moli serve with timing enabled
    env = {
        "MOLI_CDP_NAV_TIMING": "1",
        "RUST_LOG": "moli_cdp_nav_timing=info",
    }

    print("\nRunning: moli serve --port 19223")
    print("Look for timing logs with 'v8_isolate', 'isolate_bootstrap', 'context_bootstrap', 'constructor_specs'")
    print("=" * 80)

    try:
        proc = subprocess.Popen(
            [str(moli_bin), "serve", "--port", "19223"],
            env={**subprocess.os.environ, **env},
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

        # Wait a bit for startup
        time.sleep(2)

        # Terminate
        proc.terminate()
        try:
            stdout, stderr = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            stdout, stderr = proc.communicate()

        print("\n=== Stdout ===")
        if stdout:
            print(stdout[:2000])

        print("\n=== Stderr (timing logs) ===")
        if stderr:
            # Filter for timing-related lines
            lines = stderr.split("\n")
            timing_lines = [l for l in lines if any(kw in l for kw in [
                "v8_isolate", "isolate_bootstrap", "context_bootstrap",
                "constructor_specs", "elapsed_ms", "build_constructor",
                "define_global", "native_bridge", "inspector_backend"
            ])]
            if timing_lines:
                for line in timing_lines:
                    print(line)
            else:
                print("No timing logs found. Showing first 100 lines:")
                for line in lines[:100]:
                    print(line)
        else:
            print("No stderr output")

    except Exception as e:
        print(f"Error: {e}")


if __name__ == "__main__":
    main()
