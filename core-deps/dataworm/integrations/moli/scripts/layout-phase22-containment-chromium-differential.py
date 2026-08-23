#!/usr/bin/env python3
"""Pin Phase 22 containment geometry and paint semantics to Chromium.

Run with:

    uv run --project moli-benchmark --with pillow \
      python scripts/layout-phase22-containment-chromium-differential.py

The paired Rust regression is
`containment_matches_chromium_containing_block_eligibility_and_paint_clip`.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from PIL import Image


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "scripts" / "fixtures" / "layout-phase22-containment.html"
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()

EXPECTED_GEOMETRY: dict[str, list[float]] = {
    "layout": [20, 20, 160, 100],
    "layout-abs": [146, 32, 20, 16],
    "layout-fixed": [36, 34, 18, 14],
    "paint-abs": [346, 32, 20, 16],
    "paint-fixed": [236, 34, 18, 14],
    "content-abs": [546, 32, 20, 16],
    "content-fixed": [436, 34, 18, 14],
    "strict-abs": [746, 32, 20, 16],
    "strict-fixed": [636, 34, 18, 14],
    "inline-abs": [725, 186, 20, 15],
    "shadow-root": [140, 180, 200, 100],
    "shadow-action": [302, 189, 30, 20],
    "will-contain-abs": [133, 328, 20, 15],
    "will-contain-fixed": [29, 330, 18, 14],
    "will-position-abs": [293, 328, 20, 15],
    "will-position-fixed": [9, 10, 18, 14],
    "will-transform-abs": [453, 328, 20, 15],
    "will-transform-fixed": [349, 330, 18, 14],
    "content-auto": [380, 500, 140, 80],
    "content-auto-abs": [493, 508, 20, 15],
    "content-auto-fixed": [389, 510, 18, 14],
    "plain-bfc": [20, 430, 100, 0],
    "layout-bfc": [160, 430, 100, 30],
    "paint-bfc": [240, 460, 100, 30],
}

EXPECTED_OFFSET_PARENTS: dict[str, str | None] = {
    "layout-abs": "layout",
    "layout-fixed": "layout",
    "paint-abs": "paint",
    "paint-fixed": "paint",
    "content-abs": "content",
    "content-fixed": "content",
    "strict-abs": "strict",
    "strict-fixed": "strict",
    "inline-abs": "inline-outer",
    "will-contain-abs": "will-contain",
    "will-contain-fixed": "will-contain",
    "will-position-abs": "will-position",
    "will-position-fixed": None,
    "will-transform-abs": "will-transform",
    "will-transform-fixed": "will-transform",
    "content-auto-abs": "content-auto",
    "content-auto-fixed": "content-auto",
    "row-abs": "table",
    "cell-abs": "table-cell-contained",
    "shadow-action": "root",
}

SAMPLE_POINTS: dict[str, tuple[int, int]] = {
    "layout-overflow": (10, 105),
    "paint-clipped": (210, 105),
    "paint-inside": (230, 105),
    "content-clipped": (410, 105),
    "content-inside": (430, 105),
    "strict-clipped": (610, 105),
    "strict-inside": (630, 105),
}

EXPECTED_PIXELS: dict[str, list[int]] = {
    "layout-overflow": [0, 255, 0, 255],
    "paint-clipped": [255, 255, 255, 255],
    "paint-inside": [0, 255, 0, 255],
    "content-clipped": [255, 255, 255, 255],
    "content-inside": [0, 255, 0, 255],
    "strict-clipped": [255, 255, 255, 255],
    "strict-inside": [0, 255, 0, 255],
}


def run_chromium(binary: Path, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            str(binary),
            "--headless=new",
            "--disable-background-networking",
            "--disable-default-apps",
            "--disable-gpu",
            "--hide-scrollbars",
            "--no-first-run",
            "--no-sandbox",
            "--window-size=800,600",
            *arguments,
        ],
        check=True,
        capture_output=True,
        text=True,
    )


def assert_close(label: str, actual: Any, expected: Any) -> None:
    if isinstance(expected, (int, float)) and not isinstance(expected, bool):
        if not isinstance(actual, (int, float)) or abs(actual - expected) > 0.05:
            raise AssertionError(f"{label}: expected {expected}, got {actual}")
        return
    if isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise AssertionError(f"{label}: expected {expected}, got {actual}")
        for index, (actual_item, expected_item) in enumerate(zip(actual, expected)):
            assert_close(f"{label}[{index}]", actual_item, expected_item)
        return
    if actual != expected:
        raise AssertionError(f"{label}: expected {expected!r}, got {actual!r}")


def measure(binary: Path) -> dict[str, Any]:
    uri = FIXTURE.resolve().as_uri()
    dumped = run_chromium(binary, "--dump-dom", uri).stdout
    match = re.search(r'<pre id="output">(.*?)</pre>', dumped, re.DOTALL)
    if match is None:
        raise RuntimeError("Chromium dump did not contain the fixture output")
    observable = json.loads(html.unescape(match.group(1)))

    with tempfile.TemporaryDirectory(prefix="moli-phase22-containment-") as directory:
        screenshot = Path(directory) / "capture.png"
        run_chromium(binary, f"--screenshot={screenshot}", uri)
        image = Image.open(screenshot).convert("RGBA")
        pixels = {
            name: list(image.getpixel(point)) for name, point in SAMPLE_POINTS.items()
        }
    return {**observable, "samplePoints": SAMPLE_POINTS, "pixels": pixels}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    measured = measure(args.chromium)
    if not args.record:
        for name, expected in EXPECTED_GEOMETRY.items():
            assert_close(f"geometry.{name}", measured["geometry"].get(name), expected)
        for name, expected in EXPECTED_OFFSET_PARENTS.items():
            assert_close(f"offsets.{name}", measured["offsets"].get(name), expected)
        assert_close("pixels", measured["pixels"], EXPECTED_PIXELS)
    print(
        json.dumps(
            {
                "status": "recorded" if args.record else "passed",
                "chromium": str(args.chromium),
                **measured,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
