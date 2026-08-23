#!/usr/bin/env python3
"""Assert and report the fixed-font Phase 3 inline corpus in local Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase3-chromium-differential.py

The matching Rust contracts use the same bundled Moli Ahem font, viewport,
CSS, and DOM. This script measures CSS geometry instead of raster pixels so a
font fallback or antialiasing difference cannot hide an IFC regression.
"""

from __future__ import annotations

import argparse
import asyncio
import base64
import json
import os
import socket
import subprocess
import tempfile
import time
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from moli_benchmark.raw_cdp import RawCdpClient, connect_raw_cdp


ROOT = Path(__file__).resolve().parents[1]
FONT_PATH = ROOT / "moli-layout" / "tests" / "fixtures" / "moli-ahem.ttf"
HEBREW_EMOJI_FONT_PATH = Path(
    os.environ.get(
        "MOLI_PHASE3_HEBREW_EMOJI_FONT",
        ROOT / "moli-layout" / "tests" / "fixtures" / "moli-hebrew-emoji.ttf",
    )
)
CJK_FONT_PATH = Path(
    os.environ.get(
        "MOLI_PHASE3_CJK_FONT",
        ROOT / "moli-layout" / "tests" / "fixtures" / "moli-cjk.ttf",
    )
)
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()


@dataclass(frozen=True)
class Case:
    name: str
    width: int
    height: int
    body: str
    element_ids: tuple[str, ...]
    range_ids: tuple[str, ...]


RECT_TOLERANCE = 0.05
EXPECTED_RECTS: dict[str, dict[str, dict[str, list[list[float]]]]] = {
    "shared-inline-stream": {
        "elementRects": {
            "stream": [[0, 0, 80, 40]],
            "a": [[12, 0, 12, 20]],
            "nested": [[26, 0, 20, 20]],
            "c": [[48, 0, 12, 20]],
            "d": [[0, 20, 12, 20]],
            "e": [[12, 20, 12, 20]],
            "trailing": [[0, 40, 120, 20]],
            "after-trailing": [[0, 60, 10, 5]],
        },
        "rangeRects": {
            "nested": [[30, 0, 12, 20]],
            "trailing": [[0, 40, 12, 20], [12, 40, 0, 20]],
        },
    },
    "whitespace-transform-cjk": {
        "elementRects": {
            "collapse": [[0, 0, 60, 40]],
            "ca": [[0, 0, 12, 20]],
            "upper": [[24, 0, 24, 20]],
            "cb": [[0, 20, 12, 20]],
            "cjk": [[0, 40, 41, 40]],
            "preserve": [[0, 80, 60, 40]],
            "breakspaces": [[0, 120, 36, 40]],
            "nowrap": [[0, 160, 24, 20]],
        },
        "rangeRects": {
            "cjk": [[0, 37, 20, 26], [0, 57, 40.859375, 26]],
            "preserve": [[0, 80, 36, 20], [36, 80, 0, 20], [0, 100, 12, 20]],
            "breakspaces": [[0, 120, 36, 20], [0, 140, 24, 20]],
            "nowrap": [[0, 160, 36, 20]],
        },
    },
    "bidi-and-font-spacing": {
        "elementRects": {
            "bidi": [[0, 0, 120, 21]],
            "emoji": [[50.203125, -2, 20.859375, 24]],
            "latin": [[71.0625, 1, 24, 20]],
            "hebrew": [[95.0625, -2, 24.9375, 24]],
            "spacing": [[0, 21, 120, 20]],
            "spacing-text": [[12, 21, 46, 20]],
        },
    },
    "vertical-align": {
        "elementRects": {
            "align": [[0, 0, 140, 30]],
            "strut": [[0, 10, 12, 20]],
            "top": [[12, 0, 10, 30]],
            "bottom": [[22, 20, 10, 10]],
            "middle": [[32, 14, 10, 8]],
            "raised": [[42, 0, 12, 20]],
            "after": [[0, 30, 10, 5]],
        },
    },
    "inline-continuation": {
        "elementRects": {
            "wrap-root": [[0, 0, 40, 40]],
            "wrap": [[0, 0, 38, 20], [0, 20, 26, 20]],
        },
        "rangeRects": {
            "wrap": [[2, 0, 36, 20], [0, 20, 24, 20]],
        },
    },
}

EXPECTED_PLATFORM_FONTS: dict[str, dict[str, list[str]]] = {
    "shared-inline-stream": {
        "nested": ["MoliAhem-Regular"],
        "trailing": ["MoliAhem-Regular"],
    },
    "whitespace-transform-cjk": {
        "collapse": ["MoliAhem-Regular"],
        "cjk": ["DroidSansFallback", "DejaVuSans"],
    },
    "bidi-and-font-spacing": {
        "latin": ["MoliAhem-Regular"],
        "hebrew": ["DejaVuSans"],
        "emoji": ["DejaVuSans"],
        "spacing-text": ["MoliAhem-Regular"],
    },
    "vertical-align": {"align": ["MoliAhem-Regular"]},
    "inline-continuation": {"wrap": ["MoliAhem-Regular"]},
}


def fixture_cases() -> tuple[Case, ...]:
    encoded_font = base64.b64encode(FONT_PATH.read_bytes()).decode("ascii")
    encoded_hebrew_emoji = base64.b64encode(HEBREW_EMOJI_FONT_PATH.read_bytes()).decode(
        "ascii"
    )
    encoded_cjk = base64.b64encode(CJK_FONT_PATH.read_bytes()).decode("ascii")
    common = f"""<!doctype html><html lang=en><head><meta charset=utf-8><style>
@font-face{{font-family:MoliAhem;src:url(data:font/ttf;base64,{encoded_font}) format('truetype')}}
@font-face{{font-family:MoliHebrewEmoji;src:url(data:font/ttf;base64,{encoded_hebrew_emoji}) format('truetype')}}
@font-face{{font-family:MoliCJK;src:url(data:font/ttf;base64,{encoded_cjk}) format('truetype')}}
html,body{{margin:0;padding:0}}
.fixed{{font-family:MoliAhem,MoliHebrewEmoji,MoliCJK;font-size:20px;line-height:20px}}
</style></head><body>"""
    return (
        Case(
            name="shared-inline-stream",
            width=120,
            height=100,
            body=common
            + """<style>
#stream{width:80px}
#stream::before{content:'X';color:rgb(1,2,3)}
#nested{margin:0 2px;padding:0 3px;border-left:1px solid;border-right:1px solid;background:rgb(4,5,6)}
#trailing{color:rgb(61,62,63)}
#after-trailing{width:10px;height:5px;background:rgb(71,72,73)}
</style><div id=stream class=fixed><span id=a>A</span><span id=nested>B</span><span id=c>C</span><br><span id=d>D</span><span id=e>E</span></div><div id=trailing class=fixed>A<br></div><div id=after-trailing></div>""",
            element_ids=("stream", "a", "nested", "c", "d", "e", "trailing", "after-trailing"),
            range_ids=("stream", "a", "nested", "c", "d", "e", "trailing"),
        ),
        Case(
            name="whitespace-transform-cjk",
            width=120,
            height=180,
            body=common
            + """<style>
#collapse{width:60px}
#upper{text-transform:uppercase}
#cjk{width:41px;font-family:MoliCJK,MoliHebrewEmoji}
#preserve{white-space-collapse:preserve-breaks;width:60px}
#breakspaces{white-space-collapse:break-spaces;width:36px}
#nowrap{white-space:nowrap;width:24px}
</style><div id=collapse class=fixed><span id=ca>A</span>  <span id=upper>ab</span>   <span id=cb>B</span></div><div id=cjk class=fixed>中
文😀</div><div id=preserve class=fixed>A   B
C</div><div id=breakspaces class=fixed>A   B</div><div id=nowrap class=fixed>ABC</div>""",
            element_ids=("collapse", "ca", "upper", "cb", "cjk", "preserve", "breakspaces", "nowrap"),
            range_ids=("collapse", "ca", "upper", "cb", "cjk", "preserve", "breakspaces", "nowrap"),
        ),
        Case(
            name="bidi-and-font-spacing",
            width=160,
            height=100,
            body=common
            + """<style>
#bidi{direction:rtl;width:120px}
#hebrew{font-family:MoliHebrewEmoji;color:rgb(11,12,13)}
#latin{direction:ltr;unicode-bidi:isolate;color:rgb(21,22,23)}
#emoji{font-family:MoliHebrewEmoji;color:rgb(31,32,33)}
#spacing{width:120px;letter-spacing:2px;word-spacing:4px;text-indent:12px;font-weight:625;font-stretch:87.5%;font-style:italic}
</style><div id=bidi class=fixed><span id=hebrew>אב</span><span id=latin>AB</span><span id=emoji>😀</span></div><div id=spacing class=fixed><span id=spacing-text>A A</span></div>""",
            element_ids=("bidi", "hebrew", "latin", "emoji", "spacing", "spacing-text"),
            range_ids=("bidi", "hebrew", "latin", "emoji", "spacing", "spacing-text"),
        ),
        Case(
            name="vertical-align",
            width=160,
            height=100,
            body=common
            + """<style>
#align{width:140px}
.atomic{display:inline-block}
#top{width:10px;height:30px;vertical-align:top;background:rgb(41,42,43)}
#bottom{width:10px;height:10px;vertical-align:bottom;background:rgb(51,52,53)}
#middle{width:10px;height:8px;vertical-align:middle;background:rgb(61,62,63)}
#raised{vertical-align:10px;background:rgb(71,72,73)}
#after{width:10px;height:5px;background:rgb(81,82,83)}
</style><div id=align class=fixed><span id=strut>A</span><span id=top class=atomic></span><span id=bottom class=atomic></span><span id=middle class=atomic></span><span id=raised>R</span></div><div id=after></div>""",
            element_ids=("align", "strut", "top", "bottom", "middle", "raised", "after"),
            range_ids=("align", "strut", "raised"),
        ),
        Case(
            name="inline-continuation",
            width=100,
            height=100,
            body=common
            + """<style>
#wrap-root{width:40px;word-break:break-all}
#wrap{padding:0 1px;border-left:1px solid;border-right:1px solid;background:rgb(91,92,93)}
</style><div id=wrap-root class=fixed><span id=wrap>ABCDE</span></div>""",
            element_ids=("wrap-root", "wrap"),
            range_ids=("wrap-root", "wrap"),
        ),
    )


def reserve_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


async def command(
    client: RawCdpClient,
    method: str,
    params: dict[str, Any] | None = None,
    *,
    session_id: str | None = None,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    message_id = await client.send(method, params, session_id=session_id)
    response, messages = await client.recv_until_id(message_id, timeout=10.0)
    return dict(response.get("result") or {}), messages


async def wait_for_load(
    client: RawCdpClient,
    session_id: str,
    messages: list[dict[str, Any]],
) -> None:
    def loaded(message: dict[str, Any]) -> bool:
        return (
            message.get("sessionId") == session_id
            and message.get("method") == "Page.loadEventFired"
        )

    if any(loaded(message) for message in messages):
        return
    deadline = asyncio.get_running_loop().time() + 10.0
    while True:
        remaining = deadline - asyncio.get_running_loop().time()
        if remaining <= 0:
            raise TimeoutError("timed out waiting for Page.loadEventFired")
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        if loaded(message):
            return


async def measure_case(
    client: RawCdpClient,
    session_id: str,
    case: Case,
) -> dict[str, Any]:
    await command(
        client,
        "Emulation.setDeviceMetricsOverride",
        {
            "width": case.width,
            "height": case.height,
            "deviceScaleFactor": 1,
            "mobile": False,
        },
        session_id=session_id,
    )
    _, messages = await command(
        client,
        "Page.navigate",
        {"url": "data:text/html;charset=utf-8," + urllib.parse.quote(case.body)},
        session_id=session_id,
    )
    await wait_for_load(client, session_id, messages)
    expression = """(async () => {
      await document.fonts.ready;
      const elementIds = %s;
      const rangeIds = %s;
      const rect = value => [value.x, value.y, value.width, value.height];
      const elementRects = Object.fromEntries(elementIds.map(id => {
        const element = document.getElementById(id);
        return [id, Array.from(element.getClientRects(), rect)];
      }));
      const rangeRects = Object.fromEntries(rangeIds.map(id => {
        const range = document.createRange();
        range.selectNodeContents(document.getElementById(id));
        return [id, Array.from(range.getClientRects(), rect)];
      }));
      const computedFamilies = Object.fromEntries(rangeIds.map(id => [
        id,
        getComputedStyle(document.getElementById(id)).fontFamily,
      ]));
      const computed = getComputedStyle(document.querySelector('.fixed'));
      return {
        elementRects,
        rangeRects,
        computedFamilies,
        loadedFaces: Array.from(document.fonts, face => ({
          family: face.family,
          status: face.status,
          weight: face.weight,
          stretch: face.stretch,
          style: face.style,
        })),
        coverage: {
          latin: document.fonts.check('20px MoliAhem', 'A'),
          hebrew: document.fonts.check('20px MoliHebrewEmoji', 'אב'),
          cjk: document.fonts.check('20px MoliCJK', '中文'),
          emoji: document.fonts.check('20px MoliHebrewEmoji', '😀'),
        },
        font: {
          family: computed.fontFamily,
          size: computed.fontSize,
          lineHeight: computed.lineHeight,
          weight: computed.fontWeight,
          stretch: computed.fontStretch,
          style: computed.fontStyle,
        },
      };
    })()""" % (json.dumps(case.element_ids), json.dumps(case.range_ids))
    result, _ = await command(
        client,
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True, "awaitPromise": True},
        session_id=session_id,
    )
    if result.get("exceptionDetails") is not None:
        raise RuntimeError(f"{case.name}: {result['exceptionDetails']}")
    value = dict(result["result"]["value"])
    document, _ = await command(
        client,
        "DOM.getDocument",
        {"depth": 0},
        session_id=session_id,
    )
    root_id = int(document["root"]["nodeId"])
    platform_fonts: dict[str, list[dict[str, Any]]] = {}
    for element_id in case.range_ids:
        node, _ = await command(
            client,
            "DOM.querySelector",
            {"nodeId": root_id, "selector": f"#{element_id}"},
            session_id=session_id,
        )
        fonts, _ = await command(
            client,
            "CSS.getPlatformFontsForNode",
            {"nodeId": int(node["nodeId"])},
            session_id=session_id,
        )
        platform_fonts[element_id] = list(fonts.get("fonts") or [])
    value["platformFonts"] = platform_fonts
    return value


async def run(binary: Path, *, verbose_browser: bool = False) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(prefix="moli-layout-phase3-chromium-") as profile:
        process = subprocess.Popen(
            [
                str(binary),
                "--headless=new",
                "--disable-background-networking",
                "--disable-default-apps",
                "--disable-gpu",
                "--hide-scrollbars",
                "--lang=en-US",
                "--no-first-run",
                "--no-sandbox",
                "--remote-debugging-address=127.0.0.1",
                f"--remote-debugging-port={port}",
                f"--user-data-dir={profile}",
                "about:blank",
            ],
            stdout=subprocess.DEVNULL,
            stderr=None if verbose_browser else subprocess.DEVNULL,
        )
        client: RawCdpClient | None = None
        target_id: str | None = None
        try:
            deadline = time.monotonic() + 10.0
            last_error: BaseException | None = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"Chromium exited during startup: {process.returncode}")
                try:
                    client = await connect_raw_cdp(endpoint)
                    break
                except BaseException as error:
                    last_error = error
                    await asyncio.sleep(0.05)
            if client is None:
                raise TimeoutError(f"Chromium CDP startup timed out: {last_error!r}")

            version, _ = await command(client, "Browser.getVersion")
            target, _ = await command(client, "Target.createTarget", {"url": "about:blank"})
            target_id = str(target["targetId"])
            attached, _ = await command(
                client,
                "Target.attachToTarget",
                {"targetId": target_id, "flatten": True},
            )
            session_id = str(attached["sessionId"])
            await command(client, "Page.enable", session_id=session_id)
            await command(client, "DOM.enable", session_id=session_id)
            await command(client, "CSS.enable", session_id=session_id)
            return {
                "product": version.get("product"),
                "revision": version.get("revision"),
                "font_inputs": [
                    FONT_PATH.name,
                    HEBREW_EMOJI_FONT_PATH.name,
                    CJK_FONT_PATH.name,
                ],
                "cases": {
                    case.name: await measure_case(client, session_id, case)
                    for case in fixture_cases()
                },
            }
        finally:
            if client is not None:
                if target_id is not None:
                    try:
                        await command(client, "Target.closeTarget", {"targetId": target_id})
                    except BaseException:
                        pass
                await client.websocket.close()
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=3.0)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=3.0)


def assert_rects(
    label: str,
    actual: list[list[float]],
    expected: list[list[float]],
) -> None:
    if len(actual) != len(expected):
        raise AssertionError(f"{label}: expected {expected}, got {actual}")
    for rect_index, (actual_rect, expected_rect) in enumerate(zip(actual, expected)):
        if len(actual_rect) != 4 or any(
            abs(float(actual_value) - float(expected_value)) > RECT_TOLERANCE
            for actual_value, expected_value in zip(actual_rect, expected_rect)
        ):
            raise AssertionError(
                f"{label}[{rect_index}]: expected {expected_rect}, got {actual_rect}"
            )


def validate_contract(report: dict[str, Any]) -> None:
    cases = dict(report["cases"])
    for case_name, surfaces in EXPECTED_RECTS.items():
        actual_case = dict(cases[case_name])
        for surface_name, expected_by_id in surfaces.items():
            actual_by_id = dict(actual_case[surface_name])
            for element_id, expected in expected_by_id.items():
                assert_rects(
                    f"{case_name}.{surface_name}.{element_id}",
                    list(actual_by_id[element_id]),
                    expected,
                )

    for case_name, expected_by_id in EXPECTED_PLATFORM_FONTS.items():
        actual_by_id = dict(cases[case_name]["platformFonts"])
        for element_id, expected_names in expected_by_id.items():
            fonts = list(actual_by_id[element_id])
            actual_names = [str(font.get("postScriptName")) for font in fonts]
            if actual_names != expected_names or not all(
                font.get("isCustomFont") is True for font in fonts
            ):
                raise AssertionError(
                    f"{case_name}.platformFonts.{element_id}: "
                    f"expected custom {expected_names}, got {fonts}"
                )

    report["contract"] = {
        "status": "passed",
        "rectToleranceCssPx": RECT_TOLERANCE,
        "locale": "en-US / html[lang=en]",
        "deviceScaleFactor": 1,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    parser.add_argument("--verbose-browser", action="store_true")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    binary = args.chromium.expanduser().resolve()
    if not binary.is_file():
        raise SystemExit(f"Chromium binary does not exist: {binary}")
    for font_path in (FONT_PATH, HEBREW_EMOJI_FONT_PATH, CJK_FONT_PATH):
        if not font_path.is_file():
            raise SystemExit(f"fixed font fixture does not exist: {font_path}")
    report = asyncio.run(run(binary, verbose_browser=args.verbose_browser))
    validate_contract(report)
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
