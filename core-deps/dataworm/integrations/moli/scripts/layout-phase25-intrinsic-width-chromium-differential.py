#!/usr/bin/env python3
"""Pin Phase 25 intrinsic width sizing geometry to Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase25-intrinsic-width-chromium-differential.py
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
FIXTURE = ROOT / "scripts" / "fixtures" / "layout-phase25-intrinsic-width.html"
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()

# Recorded from the pinned local Chromium 147 build. Keep this explicit: the
# fixture is the semantic oracle, not a screenshot or a Moli expectation.
EXPECTED_GEOMETRY: dict[str, list[float]] = {
    "min": [20, 20, 160, 20],
    "max": [380, 20, 248, 20],
    "fit": [740, 20, 248, 20],
    "fit-narrow": [20, 120, 200, 20],
    "fit-min": [380, 120, 160, 20],
    "min-clamp": [740, 120, 248, 20],
    "max-clamp": [20, 220, 160, 20],
    "conflict": [380, 220, 248, 20],
    "content-box": [740, 220, 184, 24],
    "border-box": [20, 320, 184, 20],
    "flex-min": [380, 320, 160, 20],
    "flex-max": [740, 320, 248, 20],
    "flex-cross": [20, 420, 160, 20],
    "grid-min": [380, 420, 160, 20],
    "grid-max": [740, 420, 248, 20],
    "absolute-min": [20, 520, 160, 20],
    "absolute-fit": [380, 520, 200, 20],
    "float-min": [740, 520, 160, 20],
    "float-max": [20, 620, 248, 20],
    "replaced": [380, 620, 180, 40],
    "stretch": [750, 620, 275, 20],
    "webkit-fill": [30, 720, 275, 20],
    "aspect-block": [380, 720, 400, 20],
    "aspect-flex": [740, 720, 400, 20],
    "aspect-grid": [20, 820, 400, 20],
    "flex-grow": [380, 820, 300, 20],
    "flex-shrink": [740, 820, 120, 20],
    "flex-basis-content": [20, 920, 248, 20],
    "auto-grid-min": [380, 920, 160, 20],
    "auto-grid-min-item": [380, 920, 160, 20],
    "auto-grid-max": [740, 920, 248, 20],
    "auto-grid-max-item": [740, 920, 248, 20],
    "absolute-inset-fit": [60, 1020, 160, 20],
    "float-fit-margin": [390, 1020, 175, 20],
    "float-stretch-margin": [750, 1020, 175, 20],
    "inline-min": [20, 1120, 160, 20],
    "inline-max": [380, 1120, 248, 20],
    "inline-fit": [740, 1120, 200, 20],
    "min-fit": [20, 1220, 200, 20],
    "max-fit": [380, 1220, 200, 20],
    "min-stretch": [750, 1220, 275, 20],
    "max-stretch": [30, 1320, 275, 20],
    "min-webkit-fill": [390, 1320, 275, 20],
    "max-webkit-fill": [750, 1320, 275, 20],
}
EXPECTED_COMPUTED: dict[str, dict[str, str]] = {
    "min-clamp": {"width": "248px", "minWidth": "max-content", "maxWidth": "none"},
    "max-clamp": {"width": "160px", "minWidth": "0px", "maxWidth": "min-content"},
    "conflict": {"width": "248px", "minWidth": "max-content", "maxWidth": "min-content"},
    "min-fit": {"width": "200px", "minWidth": "fit-content", "maxWidth": "none"},
    "max-fit": {"width": "200px", "minWidth": "0px", "maxWidth": "fit-content"},
    "min-stretch": {"width": "275px", "minWidth": "stretch", "maxWidth": "none"},
    "max-stretch": {"width": "275px", "minWidth": "0px", "maxWidth": "stretch"},
    "min-webkit-fill": {"width": "275px", "minWidth": "stretch", "maxWidth": "none"},
    "max-webkit-fill": {"width": "275px", "minWidth": "0px", "maxWidth": "stretch"},
}
EXPECTED_SUPPORTS: dict[str, bool] = {
    "min-content": True,
    "max-content": True,
    "fit-content": True,
    "fit-content(120px)": False,
    "fit-content(50%)": False,
    "stretch": True,
    "-webkit-fill-available": True,
    "grid-fit-content(120px)": True,
    "min-width:fit-content": True,
    "max-width:fit-content": True,
    "min-width:stretch": True,
    "max-width:stretch": True,
    "min-width:-webkit-fill-available": True,
    "max-width:-webkit-fill-available": True,
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
            "--window-size=1200,820",
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
        assert_close("supports", measured["supports"], EXPECTED_SUPPORTS)
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
