from __future__ import annotations

import os
import sys
import traceback
from typing import Any


def record_failure(
    results: list[dict[str, Any]],
    group: str,
    scenario: str,
    error: BaseException,
) -> None:
    results.append(
        {
            "name": scenario,
            "group": group,
            "ok": False,
            "error": "".join(traceback.format_exception_only(type(error), error)).strip(),
            "traceback": "".join(traceback.format_exception(error)),
        }
    )


def has_failures(results: list[dict[str, Any]]) -> bool:
    return any(entry.get("ok") is False for entry in results)


def record_progress(group: str, scenario: str, phase: str) -> None:
    if os.environ.get("MOLI_WEBDRIVER_SMOKE_PROGRESS") not in {"1", "true", "yes"}:
        return
    print(f"[smoke] {group}:{scenario}:{phase}", file=sys.stderr, flush=True)
