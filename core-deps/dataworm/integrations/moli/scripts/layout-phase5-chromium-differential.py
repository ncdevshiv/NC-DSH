#!/usr/bin/env python3
"""Validate Phase 5 fragment/geometry semantics in local Chromium.

Run with:

    uv run --project moli-benchmark \
      python scripts/layout-phase5-chromium-differential.py

The paired Rust corpus is `phase5_output_contract.rs`. This script is the
browser-semantics oracle; Phase 6 will separately expose the same Moli
answers through JS and CDP.
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
    expression: str
    box_model_ids: tuple[str, ...] = ()


def fixture_cases() -> tuple[Case, ...]:
    font = base64.b64encode(FONT_PATH.read_bytes()).decode("ascii")
    fixed_font = (
        "@font-face{font-family:MoliAhem;"
        f"src:url(data:font/ttf;base64,{font}) format('truetype')}}"
    )
    rect_helper = "const rect=v=>[v.x,v.y,v.width,v.height];"
    return (
        Case(
            "physical-box-model",
            320,
            240,
            """<!doctype html><style>
html,body{margin:0;padding:0}#target{box-sizing:content-box;width:100px;height:60px;
margin:3px;padding:5px;border:2px solid}
</style><div id=target></div>""",
            f"""(() => {{{rect_helper}
const target=document.getElementById('target');
return {{clientRects:Array.from(target.getClientRects(),rect)}};
}})()""",
            ("target",),
        ),
        Case(
            "utf16-inline-range",
            320,
            240,
            f"""<!doctype html><style>
{fixed_font}html,body{{margin:0;padding:0}}#target{{font:20px/20px MoliAhem;width:200px}}
</style><div id=target>ab😀cd</div>""",
            f"""(async () => {{await document.fonts.ready;{rect_helper}
const target=document.getElementById('target');const text=target.firstChild;
const all=document.createRange();all.selectNodeContents(target);
const emoji=document.createRange();emoji.setStart(text,2);emoji.setEnd(text,4);
return {{element:Array.from(target.getClientRects(),rect),
all:Array.from(all.getClientRects(),rect),emoji:Array.from(emoji.getClientRects(),rect),
utf16Length:text.length}};
}})()""",
        ),
        Case(
            "nested-scroll-clip-hit-test",
            320,
            240,
            """<!doctype html><style>
html,body{margin:0;padding:0}#root{width:320px;height:240px}#scroller{width:100px;height:80px;
overflow:hidden}#wide{width:300px;height:200px}
</style><div id=root><div id=scroller><div id=wide></div></div></div>""",
            f"""(() => {{{rect_helper}
const scroller=document.getElementById('scroller');scroller.scrollLeft=40;scroller.scrollTop=30;
return {{scroller:rect(scroller.getBoundingClientRect()),
wide:rect(document.getElementById('wide').getBoundingClientRect()),
scroll:[scroller.scrollLeft,scroller.scrollTop,scroller.scrollWidth,scroller.scrollHeight,
scroller.clientWidth,scroller.clientHeight],hits:[document.elementFromPoint(10,10).id,
document.elementFromPoint(150,10).id]}};
}})()""",
        ),
        Case(
            "transform-and-paint-order-hit-test",
            320,
            240,
            """<!doctype html><style>
html,body{margin:0;padding:0}#root{position:relative;width:240px;height:180px}
#under,#over{position:absolute;left:20px;top:20px;width:80px;height:80px}
#over{transform:translate(10px,5px)}
</style><div id=root><div id=under></div><div id=over></div></div>""",
            f"""(() => {{{rect_helper}
return {{under:rect(document.getElementById('under').getBoundingClientRect()),
over:rect(document.getElementById('over').getBoundingClientRect()),
hit:document.elementFromPoint(40,40).id}};
}})()""",
            ("over",),
        ),
        Case(
            "viewport-fixed-root-scroll",
            320,
            240,
            """<!doctype html><style>
html,body{margin:0;padding:0}#flow{width:200px;height:400px}#fixed{position:fixed;
left:10px;top:15px;width:40px;height:30px}
</style><div id=flow></div><div id=fixed></div>""",
            f"""(() => {{{rect_helper}
const fixed=document.getElementById('fixed');const before=rect(fixed.getBoundingClientRect());
scrollTo(0,50);return {{before,fixed:rect(fixed.getBoundingClientRect()),
flow:rect(document.getElementById('flow').getBoundingClientRect()),scroll:[scrollX,scrollY]}};
}})()""",
        ),
    )


EXPECTED: dict[str, dict[str, Any]] = {
    "physical-box-model": {
        "clientRects": [[3, 3, 114, 74]],
        "boxModels": {
            "target": {
                "content": [10, 10, 110, 10, 110, 70, 10, 70],
                "padding": [5, 5, 115, 5, 115, 75, 5, 75],
                "border": [3, 3, 117, 3, 117, 77, 3, 77],
                "margin": [0, 0, 120, 0, 120, 80, 0, 80],
            }
        },
    },
    "utf16-inline-range": {
        "element": [[0, 0, 200, 20]],
        "all": [[0, 0, 68.859375, 20]],
        "emoji": [[23.984375, 0, 20.875, 20]],
        "utf16Length": 6,
        "boxModels": {},
    },
    "nested-scroll-clip-hit-test": {
        "scroller": [0, 0, 100, 80],
        "wide": [-40, -30, 300, 200],
        "scroll": [40, 30, 300, 200, 100, 80],
        "hits": ["wide", "root"],
        "boxModels": {},
    },
    "transform-and-paint-order-hit-test": {
        "under": [20, 20, 80, 80],
        "over": [30, 25, 80, 80],
        "hit": "over",
        "boxModels": {
            "over": {
                "content": [30, 25, 110, 25, 110, 105, 30, 105],
                "padding": [30, 25, 110, 25, 110, 105, 30, 105],
                "border": [30, 25, 110, 25, 110, 105, 30, 105],
                "margin": [30, 25, 110, 25, 110, 105, 30, 105],
            }
        },
    },
    "viewport-fixed-root-scroll": {
        "before": [10, 15, 40, 30],
        "fixed": [10, 15, 40, 30],
        "flow": [0, -50, 200, 400],
        "scroll": [0, 50],
        "boxModels": {},
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


def assert_value(label: str, actual: Any, expected: Any) -> None:
    if isinstance(expected, (int, float)) and not isinstance(expected, bool):
        if not isinstance(actual, (int, float)) or abs(actual - expected) > TOLERANCE:
            raise AssertionError(f"{label}: expected {expected}, got {actual}")
        return
    if isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            raise AssertionError(f"{label}: expected {expected}, got {actual}")
        for index, (actual_item, expected_item) in enumerate(zip(actual, expected)):
            assert_value(f"{label}[{index}]", actual_item, expected_item)
        return
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or actual.keys() != expected.keys():
            raise AssertionError(f"{label}: expected keys {expected.keys()}, got {actual}")
        for key, expected_item in expected.items():
            assert_value(f"{label}.{key}", actual[key], expected_item)
        return
    if actual != expected:
        raise AssertionError(f"{label}: expected {expected!r}, got {actual!r}")


async def cdp_box_models(
    client: RawCdpClient, session_id: str, element_ids: tuple[str, ...]
) -> dict[str, Any]:
    if not element_ids:
        return {}
    document, _ = await command(client, "DOM.getDocument", session_id=session_id)
    root_id = document["root"]["nodeId"]
    models: dict[str, Any] = {}
    for element_id in element_ids:
        node, _ = await command(
            client,
            "DOM.querySelector",
            {"nodeId": root_id, "selector": f"#{element_id}"},
            session_id=session_id,
        )
        result, _ = await command(
            client,
            "DOM.getBoxModel",
            {"nodeId": node["nodeId"]},
            session_id=session_id,
        )
        model = result["model"]
        models[element_id] = {
            key: model[key] for key in ("content", "padding", "border", "margin")
        }
    return models


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
    result, _ = await command(
        client,
        "Runtime.evaluate",
        {"expression": case.expression, "returnByValue": True, "awaitPromise": True},
        session_id=session_id,
    )
    if result.get("exceptionDetails") is not None:
        raise RuntimeError(f"{case.name}: Runtime.evaluate failed: {result['exceptionDetails']}")
    value = dict(result["result"]["value"])
    value["boxModels"] = await cdp_box_models(
        client, session_id, case.box_model_ids
    )
    if not record:
        expected = EXPECTED.get(case.name)
        if expected is None:
            raise AssertionError(f"{case.name}: expected geometry is not recorded")
        assert_value(case.name, value, expected)
    return value


async def run(binary: Path, *, record: bool) -> dict[str, Any]:
    port = reserve_port()
    endpoint = f"http://127.0.0.1:{port}"
    with tempfile.TemporaryDirectory(
        prefix="moli-layout-phase5-chromium-", ignore_cleanup_errors=True
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
                    raise RuntimeError(
                        f"Chromium exited during startup: {process.returncode}"
                    )
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
