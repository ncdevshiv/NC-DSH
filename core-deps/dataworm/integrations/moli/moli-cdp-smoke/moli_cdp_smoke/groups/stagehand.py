from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from .external_process import run_external_json_process


STAGEHAND_SCRIPT = Path(__file__).resolve().parents[1] / "stagehand_smoke.cjs"


async def run_stagehand_group(
    endpoint: str,
    fixture: str,
    results: list[dict[str, Any]],
) -> None:
    await run_external_json_process(
        "Stagehand",
        [os.environ.get("NODE", "node"), str(STAGEHAND_SCRIPT), endpoint, fixture],
        results,
        timeout_seconds=60,
    )
