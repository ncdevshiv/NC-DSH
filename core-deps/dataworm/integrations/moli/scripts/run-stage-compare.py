#!/usr/bin/env python3
"""Run CDP stage compare for ifeng and huxiu."""
from __future__ import annotations

import asyncio
import json
import time
from pathlib import Path
from typing import Any

from moli_benchmark.chrome_dcl import run_served_cdp_dcl_dump
from moli_benchmark.config import REPO_ROOT


def run_single_test(target: str, binary: Path, url: str, runs: int = 3) -> list[dict[str, Any]]:
    """Run CDP stage test for a single target and URL."""
    results = []
    for i in range(runs):
        print(f"  Run {i+1}/{runs}: {target} - {url}")
        result = run_served_cdp_dcl_dump(
            target=target,
            binary=binary,
            url=url,
            timeout_seconds=30.0,
        )
        results.append({
            "target": target,
            "url": url,
            "run": i + 1,
            "returncode": result.returncode,
            "elapsed_ms": result.elapsed_ms,
            "stdout": result.stdout.decode("utf-8", errors="replace") if result.stdout else "",
            "stderr": result.stderr.decode("utf-8", errors="replace") if result.stderr else "",
            "timed_out": result.timed_out,
            "resources": result.resources,
        })
    return results


def main() -> None:
    moli_bin = REPO_ROOT / "target" / "release" / "moli"
    if not moli_bin.exists():
        print(f"Error: moli binary not found at {moli_bin}")
        print("Please run: cargo build --release")
        return

    urls = [
        ("ifeng", "https://www.ifeng.com/"),
        ("huxiu", "https://www.huxiu.com/"),
    ]

    all_results = []
    for name, url in urls:
        print(f"\nTesting {name}: {url}")
        results = run_single_test(
            target="moli-cdp",
            binary=moli_bin,
            url=url,
            runs=3,
        )
        all_results.extend(results)

    # Save results
    output_file = REPO_ROOT / "tmp" / f"stage-compare-{int(time.time())}.json"
    output_file.parent.mkdir(parents=True, exist_ok=True)
    with open(output_file, "w") as f:
        json.dump(all_results, f, indent=2)

    print(f"\nResults saved to: {output_file}")

    # Print summary
    print("\n=== Summary ===")
    for name, url in urls:
        print(f"\n{name}:")
        url_results = [r for r in all_results if r["url"] == url]
        for r in url_results:
            status = "OK" if r["returncode"] == 0 else f"FAIL (rc={r['returncode']})"
            print(f"  Run {r['run']}: {status}, elapsed={r['elapsed_ms']:.1f}ms")


if __name__ == "__main__":
    main()
