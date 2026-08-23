from __future__ import annotations

import asyncio
import inspect
import time
from typing import Any, Awaitable, Callable


class SmokeError(AssertionError):
    pass


def assert_equal(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        raise SmokeError(f"{label}: expected {expected!r}, got {actual!r}")


def assert_true(condition: bool, label: str) -> None:
    if not condition:
        raise SmokeError(label)


def record(results: list[dict[str, Any]], name: str, data: dict[str, Any] | None = None) -> None:
    entry: dict[str, Any] = {"name": name, "ok": True}
    if data:
        entry.update(data)
    results.append(entry)


def record_contract(
    results: list[dict[str, Any]],
    name: str,
    *,
    contract: str,
    source: str,
    commands: list[str],
    observed: Any,
) -> None:
    record(
        results,
        name,
        {
            "contract": contract,
            "source": source,
            "commands": commands,
            "observed": observed,
        },
    )


async def wait_until(
    predicate: Callable[[], bool | Awaitable[bool]],
    label: str,
    *,
    timeout_ms: int = 10_000,
    interval_ms: int = 50,
) -> None:
    deadline = time.monotonic() + timeout_ms / 1000
    while time.monotonic() < deadline:
        result = predicate()
        if inspect.isawaitable(result):
            result = await result
        if result:
            return
        await asyncio.sleep(interval_ms / 1000)
    raise SmokeError(f"timed out waiting for {label} after {timeout_ms}ms")
