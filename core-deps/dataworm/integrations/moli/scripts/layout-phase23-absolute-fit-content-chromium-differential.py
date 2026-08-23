#!/usr/bin/env python3
"""Pin Phase 23 absolute fit-content geometry to Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase23-absolute-fit-content-chromium-differential.py

The paired Rust regression is
`absolute_auto_width_from_an_inline_formatting_context_shrinks_to_fit`.
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
FIXTURE = ROOT / "scripts" / "fixtures" / "layout-phase23-absolute-fit-content.html"
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()

EXPECTED_GEOMETRY: dict[str, list[float]] = {
    "left-max-case": [20, 20, 300, 70],
    "left-max": [40, 25, 248, 20],
    "left-limit-case": [380, 20, 200, 70],
    "left-limit": [400, 25, 180, 40],
    "left-min-case": [640, 20, 120, 70],
    "left-min": [650, 25, 160, 40],
    "right-max-case": [20, 110, 300, 70],
    "right-max": [52, 115, 248, 20],
    "stretch-case": [380, 110, 300, 70],
    "stretch": [400, 115, 250, 20],
    "margin-min-case": [20, 200, 200, 70],
    "margin-min": [50, 205, 160, 40],
    "max-clamp-case": [260, 200, 300, 70],
    "max-clamp": [280, 205, 200, 40],
    "min-clamp-case": [20, 290, 300, 70],
    "min-clamp": [40, 295, 260, 20],
    "specified-case": [380, 290, 300, 70],
    "specified": [400, 295, 120, 40],
    "static-ltr-case": [20, 380, 300, 70],
    "static-ltr-prefix": [20, 380, 30, 10],
    "static-ltr": [50, 385, 248, 20],
    "static-rtl-case": [380, 380, 300, 70],
    "static-rtl-prefix": [380, 380, 30, 10],
    "static-rtl": [220, 385, 160, 40],
    "flex-case": [20, 470, 300, 70],
    "flex-abs": [40, 475, 248, 20],
}

EXPECTED_OFFSET_PARENTS: dict[str, str | None] = {
    "left-max": "left-max-case",
    "left-limit": "left-limit-case",
    "left-min": "left-min-case",
    "right-max": "right-max-case",
    "stretch": "stretch-case",
    "margin-min": "margin-min-case",
    "max-clamp": "max-clamp-case",
    "min-clamp": "min-clamp-case",
    "specified": "specified-case",
    "static-ltr-prefix": "static-ltr-case",
    "static-ltr": "static-ltr-case",
    "static-rtl-prefix": "static-rtl-case",
    "static-rtl": "static-rtl-case",
    "flex-abs": "flex-case",
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
        assert_close("offsets", measured["offsets"], EXPECTED_OFFSET_PARENTS)
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
