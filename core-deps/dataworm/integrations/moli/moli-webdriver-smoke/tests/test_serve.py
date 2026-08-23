from __future__ import annotations

import asyncio
import unittest
from typing import Any, cast

from moli_webdriver_smoke.serve import (
    MoliServe,
    render_moli_serve_diagnostics,
    stop_moli_serve,
)


class _ExitedProcess:
    returncode = 0


class MoliServeDiagnosticsTests(unittest.IsolatedAsyncioTestCase):
    async def test_stop_drains_late_child_output(self) -> None:
        logs: list[str] = []

        async def collect_late_output() -> None:
            await asyncio.sleep(0)
            logs.append("stderr: request handler panicked")

        collector = asyncio.create_task(collect_late_output())
        serve = MoliServe(
            process=cast(Any, _ExitedProcess()),
            logs=logs,
            tasks=[collector],
        )

        await stop_moli_serve(serve)

        self.assertFalse(collector.cancelled())
        self.assertEqual(logs, ["stderr: request handler panicked"])

    def test_diagnostics_are_bounded_to_the_requested_tail(self) -> None:
        serve = MoliServe(
            process=cast(Any, _ExitedProcess()),
            logs=["stdout: first", "stderr: second", "stderr: third"],
            tasks=[],
        )

        diagnostics = render_moli_serve_diagnostics(serve, max_lines=2)

        self.assertEqual(
            diagnostics,
            "moli serve diagnostics (last 2 of 3 captured lines):\n"
            "stderr: second\n"
            "stderr: third",
        )

    def test_empty_diagnostics_state_is_explicit(self) -> None:
        serve = MoliServe(
            process=cast(Any, _ExitedProcess()),
            logs=[],
            tasks=[],
        )

        self.assertIn("<no child output captured>", render_moli_serve_diagnostics(serve))


if __name__ == "__main__":
    unittest.main()
