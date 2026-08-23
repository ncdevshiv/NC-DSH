#!/usr/bin/env python3
"""Validate the resource-free Phase 4 geometry corpus in local Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase4-chromium-differential.py

The paired Rust corpus uses the same HTML/CSS inputs where the public paint
snapshot can represent the geometry. Images deliberately have no source: this
phase validates replaced sizing and fallback boxes, not decoded pixels.
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
DEFAULT_CHROMIUM = Path(
    os.environ.get(
        "CHROMIUM_BINARY",
        str(Path.home() / "chromium" / "src" / "out" / "Default" / "chrome"),
    )
).expanduser()
TOLERANCE = 0.05


@dataclass(frozen=True)
class Case:
    name: str
    width: int
    height: int
    html: str
    element_ids: tuple[str, ...]
    range_ids: tuple[str, ...] = ()


def fixture_cases() -> tuple[Case, ...]:
    font = base64.b64encode(FONT_PATH.read_bytes()).decode("ascii")
    fixed_font = f"""
@font-face{{font-family:MoliAhem;src:url(data:font/ttf;base64,{font}) format('truetype')}}
.fixed{{font-family:MoliAhem;font-size:20px;line-height:20px}}
"""
    return (
        Case(
            "table-wrapper-spans-and-spacing",
            400,
            240,
            """<!doctype html><style>
html,body{margin:0;padding:0}
#table{border-spacing:5px 7px;width:300px;table-layout:fixed;background:#ddd}
#caption{height:20px;background:#ff0}#first{height:30px}#second{height:40px}
td{padding:0;border:0}#a{background:red}#b{background:green}#c{background:blue}#d{background:cyan}
</style><table id=table><caption id=caption>cap</caption>
<colgroup id=columns><col id=column-a style="width:80px"><col id=column-bc span=2></colgroup>
<tbody id=body><tr id=first><td id=a rowspan=2>A</td><td id=b colspan=2>B</td></tr>
<tr id=second><td id=c>C</td><td id=d>D</td></tr></tbody></table>""",
            (
                "table",
                "caption",
                "columns",
                "column-a",
                "column-bc",
                "body",
                "first",
                "second",
                "a",
                "b",
                "c",
                "d",
            ),
        ),
        Case(
            "float-exclusion-and-clear",
            240,
            180,
            f"""<!doctype html><style>
html,body{{margin:0;padding:0}}{fixed_font}
#flow{{display:flow-root;width:200px;word-break:break-all}}
#left{{float:left;width:60px;height:40px;background:red}}
#right{{float:right;width:50px;height:30px;background:green}}
#text{{color:blue}}#clear-root{{width:200px}}
#clear-float{{float:left;width:70px;height:35px}}#clear{{clear:both;height:10px;background:cyan}}
</style><div id=flow class=fixed><div id=left></div><div id=right></div><span id=text>AAAAAAAAAAAA</span></div>
<div id=clear-root><div id=clear-float></div><div id=clear></div></div>""",
            ("flow", "left", "right", "text", "clear-root", "clear-float", "clear"),
            ("text",),
        ),
        Case(
            "html-and-css-list-markers",
            280,
            180,
            f"""<!doctype html><style>
html,body{{margin:0;padding:0}}{fixed_font}
#list{{margin:0;padding-left:40px;width:200px}}
#inside{{list-style-position:inside}}#custom{{list-style:none}}
#custom::marker{{content:'X '}}
</style><ol id=list class=fixed reversed start=5><li id=first>AA</li><li id=valued value=9>BB</li>
<li id=inside>CC</li><li id=custom>DD</li></ol>""",
            ("list", "first", "valued", "inside", "custom"),
            ("first", "valued", "inside", "custom"),
        ),
        Case(
            "replaced-attributes-and-ratio",
            700,
            700,
            """<!doctype html><style>
html,body{margin:0;padding:0}img,canvas,iframe,svg{display:block;margin:0;border:0;padding:0}
#image{width:120px;height:auto}
</style><img id=image width=80 height=40 alt=""><canvas id=canvas width=600></canvas>
<iframe id=frame width=90 height=45></iframe><svg id=svg width=70 height=35></svg>""",
            ("image", "canvas", "frame", "svg"),
        ),
        Case(
            "form-control-intrinsic-sizes",
            500,
            400,
            f"""<!doctype html><style>
html,body{{margin:0;padding:0}}{fixed_font}
input,textarea,select,button{{display:block;box-sizing:content-box;margin:0;border:0;padding:0;font:20px/20px MoliAhem}}
</style><input id=input size=4 value=AAAA><textarea id=textarea cols=4 rows=2>AAAA</textarea>
<select id=select size=2><option>A</option><option selected>AAAA</option></select>
<input id=checkbox type=checkbox checked><input id=radio type=radio><button id=button>AAAA</button>""",
            ("input", "textarea", "select", "checkbox", "radio", "button"),
            ("input", "textarea", "select", "button"),
        ),
        Case(
            "sticky-scrollport-and-fixed-containing-block",
            320,
            360,
            """<!doctype html><style>
html,body{margin:0;padding:0}
#clip{overflow:clip;width:100px;height:50px;margin-top:100px}#clip-sticky{position:sticky;top:10px;width:30px;height:20px}
#scroll{overflow:hidden;width:100px;height:50px}#scroll-sticky{position:sticky;top:10px;width:30px;height:20px}
#transform{transform:translate(0);margin-left:50px;width:100px;height:100px}
#fixed{position:fixed;right:0;top:0;width:20px;height:20px}
</style><div id=clip><div id=clip-sticky></div></div><div id=scroll><div id=scroll-sticky></div></div>
<div id=transform><div id=fixed></div></div>""",
            ("clip", "clip-sticky", "scroll", "scroll-sticky", "transform", "fixed"),
        ),
    )


# Filled from the recorded Chromium 147 corpus below. Keeping the oracle in
# source makes changes to either the fixture or expected geometry reviewable.
EXPECTED: dict[str, dict[str, dict[str, list[list[float]]]]] = {
    "table-wrapper-spans-and-spacing": {
        "elementRects": {
            "table": [[0, 0, 300, 111]],
            "caption": [[0, 0, 300, 20]],
            "columns": [[5, 27, 290, 77]],
            "column-a": [[5, 27, 80, 77]],
            "column-bc": [[90, 27, 205, 77]],
            "body": [[5, 27, 290, 77]],
            "first": [[5, 27, 290, 30]],
            "second": [[5, 64, 290, 40]],
            "a": [[5, 27, 80, 77]],
            "b": [[90, 27, 205, 30]],
            "c": [[90, 64, 100, 40]],
            "d": [[195, 64, 100, 40]],
        }
    },
    "float-exclusion-and-clear": {
        "elementRects": {
            "flow": [[0, 0, 200, 40]],
            "left": [[0, 0, 60, 40]],
            "right": [[150, 0, 50, 30]],
            "text": [[60, 0, 84, 20], [60, 20, 60, 20]],
            "clear-root": [[0, 40, 200, 45]],
            "clear-float": [[0, 40, 70, 35]],
            "clear": [[0, 75, 200, 10]],
        },
        "rangeRects": {"text": [[60, 0, 84, 20], [60, 20, 60, 20]]},
    },
    "html-and-css-list-markers": {
        "elementRects": {
            "list": [[0, 0, 240, 80]],
            "first": [[40, 0, 200, 20]],
            "valued": [[40, 20, 200, 20]],
            "inside": [[40, 40, 200, 20]],
            "custom": [[40, 60, 200, 20]],
        },
        "rangeRects": {
            "first": [[40, 0, 24, 20]],
            "valued": [[40, 20, 24, 20]],
            "inside": [[76, 40, 24, 20]],
            "custom": [[40, 60, 24, 20]],
        },
    },
    "replaced-attributes-and-ratio": {
        "elementRects": {
            "image": [[0, 0, 120, 60]],
            "canvas": [[0, 60, 600, 150]],
            "frame": [[0, 210, 90, 45]],
            "svg": [[0, 255, 70, 35]],
        }
    },
    "form-control-intrinsic-sizes": {
        "elementRects": {
            "input": [[0, 0, 48, 20]],
            "textarea": [[0, 20, 63, 40]],
            "select": [[0, 60, 52, 50]],
            "checkbox": [[0, 110, 13, 13]],
            "radio": [[0, 123, 13, 13]],
            "button": [[0, 136, 48, 20]],
        },
        "rangeRects": {
            "input": [],
            "textarea": [],
            "select": [[0, 60, 52, 25], [0, 85, 52, 25]],
            "button": [[0, 136, 48, 20]],
        },
    },
    "sticky-scrollport-and-fixed-containing-block": {
        "elementRects": {
            "clip": [[0, 100, 100, 50]],
            "clip-sticky": [[0, 100, 30, 20]],
            "scroll": [[0, 150, 100, 50]],
            "scroll-sticky": [[0, 160, 30, 20]],
            "transform": [[50, 200, 100, 100]],
            "fixed": [[130, 200, 20, 20]],
        }
    },
}


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
    client: RawCdpClient, session_id: str, messages: list[dict[str, Any]]
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


def assert_rects(label: str, actual: list[list[float]], expected: list[list[float]]) -> None:
    if len(actual) != len(expected):
        raise AssertionError(f"{label}: expected {expected}, got {actual}")
    for rect_index, (actual_rect, expected_rect) in enumerate(zip(actual, expected)):
        for value_index, (actual_value, expected_value) in enumerate(
            zip(actual_rect, expected_rect)
        ):
            if abs(actual_value - expected_value) > TOLERANCE:
                raise AssertionError(
                    f"{label}[{rect_index}][{value_index}]: "
                    f"expected {expected_value}, got {actual_value}"
                )


async def measure_case(
    client: RawCdpClient, session_id: str, case: Case, *, record: bool
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
        {"url": "data:text/html;charset=utf-8," + urllib.parse.quote(case.html)},
        session_id=session_id,
    )
    await wait_for_load(client, session_id, messages)
    expression = """(async () => {
      await document.fonts.ready;
      const elementIds = %s;
      const rangeIds = %s;
      const rect = value => [value.x, value.y, value.width, value.height];
      const elementRects = Object.fromEntries(elementIds.map(id => [
        id, Array.from(document.getElementById(id).getClientRects(), rect)
      ]));
      const rangeRects = Object.fromEntries(rangeIds.map(id => {
        const range = document.createRange();
        range.selectNodeContents(document.getElementById(id));
        return [id, Array.from(range.getClientRects(), rect)];
      }));
      return {elementRects, rangeRects};
    })()""" % (json.dumps(case.element_ids), json.dumps(case.range_ids))
    result, _ = await command(
        client,
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True, "awaitPromise": True},
        session_id=session_id,
    )
    if result.get("exceptionDetails") is not None:
        raise RuntimeError(f"{case.name}: Runtime.evaluate failed: {result['exceptionDetails']}")
    value = dict(result["result"]["value"])
    if not record:
        expected = EXPECTED.get(case.name)
        if expected is None:
            raise AssertionError(f"{case.name}: expected geometry is not recorded")
        for kind in ("elementRects", "rangeRects"):
            for key, expected_rects in expected.get(kind, {}).items():
                assert_rects(
                    f"{case.name}.{kind}.{key}", value[kind][key], expected_rects
                )
    return value


async def run(binary: Path, *, record: bool) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(
        prefix="moli-layout-phase4-chromium-", ignore_cleanup_errors=True
    ) as profile:
        process = subprocess.Popen(
            [
                str(binary),
                "--headless=new",
                "--disable-background-networking",
                "--disable-default-apps",
                "--disable-gpu",
                "--hide-scrollbars",
                "--no-first-run",
                "--no-sandbox",
                "--remote-debugging-address=127.0.0.1",
                f"--remote-debugging-port={port}",
                f"--user-data-dir={profile}",
                "about:blank",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
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
            measured = {
                case.name: await measure_case(client, session_id, case, record=record)
                for case in fixture_cases()
            }
            return {
                "status": "recorded" if record else "passed",
                "product": version.get("product"),
                "revision": version.get("revision"),
                "tolerance_css_px": TOLERANCE,
                "cases": measured,
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


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", type=Path, default=DEFAULT_CHROMIUM)
    parser.add_argument("--record", action="store_true")
    args = parser.parse_args()
    binary = args.chromium.expanduser().resolve()
    if not binary.is_file():
        raise SystemExit(f"Chromium binary does not exist: {binary}")
    print(json.dumps(asyncio.run(run(binary, record=args.record)), indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
