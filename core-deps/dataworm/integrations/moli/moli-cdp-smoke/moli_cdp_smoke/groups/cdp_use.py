from __future__ import annotations

import os
import sys
from pathlib import Path
from typing import Any

from .external_process import run_external_json_process


CDP_USE_SCRIPT = Path(__file__).resolve().parents[1] / "cdp_use_smoke.py"


async def run_cdp_use_group(
    endpoint: str,
    fixture: str,
    results: list[dict[str, Any]],
) -> None:
    await run_external_json_process(
        "cdp-use",
        [
            os.environ.get("CDP_USE_PYTHON", sys.executable),
            str(CDP_USE_SCRIPT),
            endpoint,
            fixture,
        ],
        results,
    )
