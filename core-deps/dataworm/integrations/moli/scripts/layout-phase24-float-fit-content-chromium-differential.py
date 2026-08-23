#!/usr/bin/env python3
"""Pin Phase 24 float fit-content geometry to Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase24-float-fit-content-chromium-differential.py

The paired Rust regression is
`floated_auto_width_inline_formatting_contexts_shrink_to_fit`.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "scripts" / "fixtures" / "layout-phase24-float-fit-content.html"
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()

EXPECTED_GEOMETRY: dict[str, list[float]] = {
    "baidu-case": [20, 20, 1076, 70],
    "baidu-logo": [20, 37, 101, 33],
    "baidu-logo-image": [20, 37, 101, 33],
    "baidu-main": [139, 35, 748, 45],
    "max-case": [20, 120, 300, 70],
    "max": [20, 120, 248, 20],
    "limit-case": [380, 120, 200, 70],
    "limit": [380, 120, 200, 40],
    "min-case": [640, 120, 120, 70],
    "min": [640, 120, 160, 40],
    "right-case": [20, 220, 300, 70],
    "right": [72, 220, 248, 20],
    "margin-case": [380, 220, 200, 70],
    "margin-float": [390, 220, 175, 40],
    "max-clamp-case": [640, 220, 300, 70],
    "max-clamp": [640, 220, 200, 40],
    "min-clamp-case": [20, 320, 300, 70],
    "min-clamp": [20, 320, 260, 20],
    "specified-case": [380, 320, 300, 70],
    "specified": [380, 320, 120, 40],
    "edge-case": [740, 320, 300, 70],
    "edge": [750, 320, 272, 24],
    "block-control-case": [20, 420, 300, 70],
    "block-control": [20, 420, 180, 20],
    "replaced-control-case": [380, 420, 300, 70],
    "replaced-control": [380, 420, 101, 33],
    "stretch-control-case": [740, 420, 300, 70],
    "stretch-control": [740, 420, 300, 20],
    "inline-margin-case": [20, 520, 200, 70],
    "inline-margin-float": [30, 520, 175, 40],
    "negative-margin-case": [380, 520, 200, 70],
    "negative-margin-float": [370, 520, 225, 40],
    "inline-negative-margin-case": [740, 520, 200, 70],
    "inline-negative-margin-float": [730, 520, 225, 40],
}

EXPECTED_COMPUTED: dict[str, dict[str, str]] = {
    "baidu-logo": {"display": "block", "float": "left", "width": "101px", "boxSizing": "content-box"},
    "max": {"display": "block", "float": "left", "width": "248px", "boxSizing": "content-box"},
    "limit": {"display": "block", "float": "left", "width": "200px", "boxSizing": "content-box"},
    "min": {"display": "block", "float": "left", "width": "160px", "boxSizing": "content-box"},
    "right": {"display": "block", "float": "right", "width": "248px", "boxSizing": "content-box"},
    "margin-float": {"display": "block", "float": "left", "width": "175px", "boxSizing": "content-box"},
    "max-clamp": {"display": "block", "float": "left", "width": "200px", "boxSizing": "content-box"},
    "min-clamp": {"display": "block", "float": "left", "width": "260px", "boxSizing": "content-box"},
    "specified": {"display": "block", "float": "left", "width": "120px", "boxSizing": "content-box"},
    "edge": {"display": "block", "float": "left", "width": "248px", "boxSizing": "content-box"},
    "block-control": {"display": "block", "float": "left", "width": "180px", "boxSizing": "content-box"},
    "replaced-control": {"display": "block", "float": "left", "width": "101px", "boxSizing": "content-box"},
    "stretch-control": {"display": "block", "float": "none", "width": "300px", "boxSizing": "content-box"},
    "inline-margin-float": {"display": "block", "float": "left", "width": "175px", "boxSizing": "content-box"},
    "negative-margin-float": {"display": "block", "float": "left", "width": "225px", "boxSizing": "content-box"},
    "inline-negative-margin-float": {"display": "block", "float": "left", "width": "225px", "boxSizing": "content-box"},
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
            "--window-size=1200,620",
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
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys() != expected.keys():
            raise AssertionError(f"{label}: expected keys {expected.keys()}, got {actual}")
        for key, expected_item in expected.items():
            assert_close(f"{label}.{key}", actual[key], expected_item)
        return
    if actual != expected:
        raise AssertionError(f"{label}: expected {expected!r}, got {actual!r}")


def measure(binary: Path) -> dict[str, Any]:
    dumped = run_chromium(binary, "--dump-dom", FIXTURE.resolve().as_uri()).stdout
    match = re.search(r'<pre id="output">(.*?)</pre>', dumped, re.DOTALL)
    if match is None:
        raise RuntimeError("Chromium dump did not contain the fixture output")
    return json.loads(html.unescape(match.group(1)))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    measured = measure(args.chromium)
    if not args.record:
        assert_close("geometry", measured["geometry"], EXPECTED_GEOMETRY)
        assert_close("computed", measured["computed"], EXPECTED_COMPUTED)
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
