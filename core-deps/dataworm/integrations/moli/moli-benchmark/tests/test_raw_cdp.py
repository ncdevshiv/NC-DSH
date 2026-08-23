from __future__ import annotations

import asyncio
import json
import unittest

from moli_benchmark.raw_cdp import (
    RawCdpCommandError,
    RawCdpTimeoutError,
    RoutedRawCdpClient,
)


class _FakeWebSocket:
    def __init__(self) -> None:
        self.sent: list[dict[str, object]] = []
        self.incoming: asyncio.Queue[object] = asyncio.Queue()
        self.sent_event = asyncio.Event()

    async def send(self, raw: str) -> None:
        self.sent.append(json.loads(raw))
        self.sent_event.set()

    async def recv(self) -> str:
        value = await self.incoming.get()
        if isinstance(value, BaseException):
            raise value
        return json.dumps(value)

    async def close(self) -> None:
        await self.incoming.put(RuntimeError("closed"))


class RoutedRawCdpClientTests(unittest.IsolatedAsyncioTestCase):
    async def test_routes_out_of_order_responses_and_preserves_event(self) -> None:
        websocket = _FakeWebSocket()
        client = RoutedRawCdpClient(websocket)  # type: ignore[arg-type]
        client.start()
        try:
            first = asyncio.create_task(client.command("Runtime.evaluate"))
            second = asyncio.create_task(client.command("Page.getFrameTree"))
            while len(websocket.sent) < 2:
                websocket.sent_event.clear()
                await websocket.sent_event.wait()

            first_id = int(websocket.sent[0]["id"])
            second_id = int(websocket.sent[1]["id"])
            await websocket.incoming.put(
                {
                    "method": "Page.frameNavigated",
                    "sessionId": "session-1",
                    "params": {"frame": {"url": "http://fixture/next"}},
                }
            )
            await websocket.incoming.put({"id": second_id, "result": {"second": True}})
            await websocket.incoming.put({"id": first_id, "result": {"first": True}})

            first_result, second_result = await asyncio.gather(first, second)
            event = await client.wait_for_event(
                "Page.frameNavigated",
                session_id="session-1",
                timeout=0.2,
            )

            self.assertEqual(first_result.response["result"], {"first": True})
            self.assertEqual(second_result.response["result"], {"second": True})
            self.assertEqual(event.payload["params"]["frame"]["url"], "http://fixture/next")
            self.assertEqual(
                [record["payload"].get("id") for record in client.recorded_messages()],
                [None, second_id, first_id],
            )
            self.assertEqual(
                [record.sequence for record in client.messages_since(1)],
                [2, 3],
            )
        finally:
            await client.close()

    async def test_event_waiter_checks_each_candidate_only_once(self) -> None:
        websocket = _FakeWebSocket()
        client = RoutedRawCdpClient(websocket)  # type: ignore[arg-type]
        client.start()
        try:
            seed = asyncio.create_task(client.command("Runtime.enable"))
            await websocket.sent_event.wait()
            seed_id = int(websocket.sent[0]["id"])
            for label in ("old-1", "old-2"):
                await websocket.incoming.put(
                    {"method": "Page.loadEventFired", "params": {"label": label}}
                )
            await websocket.incoming.put({"id": seed_id, "result": {}})
            await seed

            checked: list[str] = []
            initial_checked = asyncio.Event()
            new_checked = asyncio.Event()

            def matches_target(payload: dict[str, object]) -> bool:
                params = payload.get("params")
                label = str(params.get("label")) if isinstance(params, dict) else ""
                checked.append(label)
                if len(checked) == 2:
                    initial_checked.set()
                if label == "new-ignore":
                    new_checked.set()
                return label == "target"

            waiter = asyncio.create_task(
                client.wait_for_event(
                    "Page.loadEventFired",
                    predicate=matches_target,
                    timeout=0.2,
                )
            )
            await initial_checked.wait()
            await websocket.incoming.put(
                {
                    "method": "Page.loadEventFired",
                    "params": {"label": "new-ignore"},
                }
            )
            await new_checked.wait()
            await websocket.incoming.put(
                {"method": "Page.loadEventFired", "params": {"label": "target"}}
            )

            event = await waiter
            self.assertEqual(event.payload["params"]["label"], "target")
            self.assertEqual(checked, ["old-1", "old-2", "new-ignore", "target"])
        finally:
            await client.close()

    async def test_command_error_retains_exact_cdp_error_and_frames(self) -> None:
        websocket = _FakeWebSocket()
        client = RoutedRawCdpClient(websocket)  # type: ignore[arg-type]
        client.start()
        try:
            task = asyncio.create_task(client.command("Runtime.evaluate"))
            await websocket.sent_event.wait()
            message_id = int(websocket.sent[0]["id"])
            await websocket.incoming.put(
                {
                    "id": message_id,
                    "error": {"code": -32000, "message": "Promise was collected"},
                }
            )
            with self.assertRaises(RawCdpCommandError) as raised:
                await task
            self.assertEqual(raised.exception.error["code"], -32000)
            self.assertEqual(raised.exception.error["message"], "Promise was collected")
            self.assertEqual(raised.exception.messages[-1]["payload"]["id"], message_id)
        finally:
            await client.close()

    async def test_timeout_keeps_intervening_events(self) -> None:
        websocket = _FakeWebSocket()
        client = RoutedRawCdpClient(websocket)  # type: ignore[arg-type]
        client.start()
        try:
            task = asyncio.create_task(client.command("Runtime.evaluate", timeout=0.03))
            await websocket.sent_event.wait()
            await websocket.incoming.put(
                {"method": "Runtime.consoleAPICalled", "params": {"type": "log"}}
            )
            with self.assertRaises(RawCdpTimeoutError) as raised:
                await task
            self.assertEqual(raised.exception.method, "Runtime.evaluate")
            self.assertEqual(
                raised.exception.messages[-1]["payload"]["method"],
                "Runtime.consoleAPICalled",
            )
        finally:
            await client.close()


if __name__ == "__main__":
    unittest.main()
