from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from .external_process import run_external_json_process


CRI_SCRIPT = Path(__file__).resolve().parents[1] / "chrome_remote_interface_smoke.cjs"


async def run_chrome_remote_interface_group(
    endpoint: str,
    fixture: str,
    results: list[dict[str, Any]],
) -> None:
    await run_external_json_process(
        "chrome-remote-interface",
        [os.environ.get("NODE", "node"), str(CRI_SCRIPT), endpoint, fixture],
        results,
    )
