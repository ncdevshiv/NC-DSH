from __future__ import annotations

import json
from typing import Any, Awaitable, Callable

import websockets

from ..assertions import SmokeError, assert_equal, assert_true, record
from ..client import ClassicClient, classic_value
from ..config import WebDriverTarget
from ..scenarios import record_failure


FILE_URL = "file:///moli-policy-must-not-open"
NAVIGATION_POLICY_MESSAGE = (
    "Navigation to a local file URL requires an explicitly granted browser capability."
)
BIDI_NAVIGATION_EVENTS = {
    "browsingContext.navigationStarted",
    "browsingContext.fragmentNavigated",
    "browsingContext.domContentLoaded",
    "browsingContext.load",
}
UrlPolicyScenario = Callable[
    [WebDriverTarget, list[dict[str, Any]]],
    Awaitable[None],
]


async def run_url_policy_group(
    target: WebDriverTarget,
    _fixture: str,
    results: list[dict[str, Any]],
    continue_on_failure: bool = False,
) -> None:
    if target.browser_name != "moli":
        record(
            results,
            "webdriver_url_policy_not_applicable",
            {
                "skipped": True,
                "reason": "hosted file-navigation policy is Moli-specific; desktop Chromium grants a different capability",
            },
        )
        return

    for name, scenario in (
        ("classic_file_navigation_policy", _run_classic_file_navigation_policy),
        ("bidi_file_navigation_policy", _run_bidi_file_navigation_policy),
    ):
        try:
            await scenario(target, results)
        except Exception as error:
            if not continue_on_failure:
                raise
            record_failure(results, "url-policy", name, error)


async def _run_classic_file_navigation_policy(
    target: WebDriverTarget,
    results: list[dict[str, Any]],
) -> None:
    client = ClassicClient(target.endpoint)
    session = client.post("/session", {"capabilities": {"alwaysMatch": {}}})
    session_id = classic_value(session)["sessionId"]
    assert_true(isinstance(session_id, str) and session_id, "Classic policy session id")
    try:
        rejected = client.request(
            "POST",
            f"/session/{session_id}/url",
            {"url": FILE_URL},
            expected_status=500,
        )
        assert_equal(
            rejected.body,
            {
                "value": {
                    "error": "unknown error",
                    "message": NAVIGATION_POLICY_MESSAGE,
                    "stacktrace": "",
                }
            },
            "Classic file navigation W3C error shape",
        )
        current_url = classic_value(client.get(f"/session/{session_id}/url"))
        assert_equal(current_url, "about:blank", "Classic URL after rejected file navigation")
        record(
            results,
            "classic_file_navigation_policy",
            {
                "httpStatus": rejected.status,
                "error": "unknown error",
                "urlAfterFailure": current_url,
            },
        )
    finally:
        client.delete(f"/session/{session_id}")


async def _run_bidi_file_navigation_policy(
    target: WebDriverTarget,
    results: list[dict[str, Any]],
) -> None:
    ws_endpoint = target.endpoint.replace("http://", "ws://", 1).rstrip("/") + "/session"
    async with websockets.connect(ws_endpoint, max_size=2**24) as websocket:
        session_started = False
        try:
            session, _ = await _call(websocket, 1, "session.new", {"capabilities": {}})
            assert_equal(session["type"], "success", "BiDi policy session.new type")
            session_started = True

            create, _ = await _call(websocket, 2, "browsingContext.create", {"type": "tab"})
            assert_equal(create["type"], "success", "BiDi policy create context type")
            context = create["result"]["context"]
            assert_true(isinstance(context, str) and context, "BiDi policy context id")

            subscribe, _ = await _call(
                websocket,
                3,
                "session.subscribe",
                {
                    "events": sorted(BIDI_NAVIGATION_EVENTS),
                    "contexts": [context],
                },
            )
            assert_equal(subscribe["type"], "success", "BiDi policy lifecycle subscription")

            rejected, navigation_messages = await _call(
                websocket,
                4,
                "browsingContext.navigate",
                {"context": context, "url": FILE_URL, "wait": "complete"},
            )
            assert_equal(
                rejected,
                {
                    "type": "error",
                    "id": 4,
                    "error": "unknown error",
                    "message": NAVIGATION_POLICY_MESSAGE,
                    "stacktrace": "",
                },
                "BiDi file navigation error shape",
            )

            tree, barrier_messages = await _call(
                websocket,
                5,
                "browsingContext.getTree",
                {"root": context},
            )
            assert_equal(tree["type"], "success", "BiDi getTree after rejected navigation")
            contexts = tree["result"]["contexts"]
            assert_equal(len(contexts), 1, "BiDi rejected navigation context count")
            assert_equal(
                contexts[0]["url"],
                "about:blank",
                "BiDi URL after rejected file navigation",
            )
            _assert_no_navigation_events(
                navigation_messages + barrier_messages,
                "BiDi rejected file navigation",
            )
            record(
                results,
                "bidi_file_navigation_policy",
                {
                    "error": "unknown error",
                    "context": context,
                    "urlAfterFailure": contexts[0]["url"],
                },
            )
        finally:
            if session_started:
                end, _ = await _call(websocket, 6, "session.end", {})
                assert_equal(end["type"], "success", "BiDi policy session.end type")


async def _call(
    websocket: Any,
    id_: int,
    method: str,
    params: dict[str, Any],
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    await websocket.send(
        json.dumps(
            {"id": id_, "method": method, "params": params},
            separators=(",", ":"),
        )
    )
    messages: list[dict[str, Any]] = []
    while True:
        raw = await websocket.recv()
        message = json.loads(raw)
        if not isinstance(message, dict):
            raise SmokeError(f"unexpected BiDi payload: {message!r}")
        messages.append(message)
        if message.get("id") == id_:
            return message, messages


def _assert_no_navigation_events(messages: list[dict[str, Any]], label: str) -> None:
    unexpected = [
        message for message in messages if message.get("method") in BIDI_NAVIGATION_EVENTS
    ]
    if unexpected:
        raise SmokeError(f"{label} emitted lifecycle events: {unexpected!r}")
