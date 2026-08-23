from __future__ import annotations

import json
from typing import Any

import websockets

from ..assertions import SmokeError, assert_equal, assert_true, record_contract
from ..client import ClassicClient, classic_value
from ..config import WebDriverTarget


SOURCE = (
    "Chromium WPT webdriver/tests/classic/navigate_to/navigate.py, "
    "webdriver/tests/bidi/browsing_context/navigate/invalid.py, "
    "webdriver/tests/bidi/browsing_context/navigate/error.py, and executable "
    "Debian Chromium/ChromeDriver 145.0.7632.116/145.0.7632.117"
)


async def run_navigation_errors_group(
    target: WebDriverTarget,
    _fixture: str,
    results: list[dict[str, Any]],
    _continue_on_failure: bool = False,
) -> None:
    client = ClassicClient(target.endpoint)
    created = client.post("/session", _new_session_payload(target))
    value = classic_value(created)
    session_id = value["sessionId"]
    capabilities = value["capabilities"]
    web_socket_url = capabilities.get("webSocketUrl")
    assert_true(isinstance(session_id, str) and session_id, "navigation-errors session id")
    assert_true(
        isinstance(web_socket_url, str) and web_socket_url,
        "navigation-errors BiDi webSocketUrl",
    )

    try:
        classic = _run_classic_invalid_argument_matrix(client, session_id)
        bidi = await _run_bidi_navigation_matrix(web_socket_url)
        record_contract(
            results,
            "webdriver_navigation_error_matrix",
            contract=(
                "Classic navigation rejects malformed command bodies with HTTP 400 W3C invalid "
                "argument envelopes; BiDi validates context/url/wait before dispatch, reports "
                "missing contexts as no such frame, and maps real address failures to unknown error."
            ),
            source=SOURCE,
            commands=[
                "POST /session/{sessionId}/url",
                "browsingContext.getTree",
                "browsingContext.navigate",
            ],
            observed={"classic": classic, "bidi": bidi},
        )
    finally:
        client.delete(f"/session/{session_id}")


def _new_session_payload(target: WebDriverTarget) -> dict[str, Any]:
    always_match: dict[str, Any] = {
        "browserName": target.browser_name,
        "webSocketUrl": True,
    }
    if target.browser_name == "chrome":
        options: dict[str, Any] = {
            "args": [
                "--headless=new",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--no-proxy-server",
            ]
        }
        if target.browser_binary is not None:
            options["binary"] = str(target.browser_binary)
        always_match["goog:chromeOptions"] = options
    return {"capabilities": {"alwaysMatch": always_match}}


def _run_classic_invalid_argument_matrix(
    client: ClassicClient,
    session_id: str,
) -> dict[str, Any]:
    cases: list[tuple[str, Any]] = [
        ("null-body", None),
        ("missing-url", {}),
        ("null-url", {"url": None}),
        ("boolean-url", {"url": False}),
        ("number-url", {"url": 42}),
        ("object-url", {"url": {}}),
        ("array-url", {"url": []}),
        ("relative-url", {"url": "relative/path"}),
        ("invalid-http-host", {"url": "http://:invalid"}),
        ("invalid-https-host", {"url": "https://#invalid"}),
    ]
    observed: list[dict[str, Any]] = []
    for name, body in cases:
        response = client.request(
            "POST",
            f"/session/{session_id}/url",
            body,
            expected_status=400,
        )
        value = response.body.get("value")
        assert_true(isinstance(value, dict), f"Classic {name} W3C value envelope")
        assert_equal(value.get("error"), "invalid argument", f"Classic {name} error code")
        assert_true(isinstance(value.get("message"), str), f"Classic {name} error message")
        assert_true(isinstance(value.get("stacktrace"), str), f"Classic {name} stacktrace")
        observed.append({"name": name, "httpStatus": response.status, "error": value["error"]})

    current_url = classic_value(client.get(f"/session/{session_id}/url"))
    assert_equal(current_url, "about:blank", "Classic URL after invalid navigation matrix")
    return {"cases": observed, "urlAfterFailures": current_url}


async def _run_bidi_navigation_matrix(web_socket_url: str) -> dict[str, Any]:
    async with websockets.connect(web_socket_url, max_size=2**24) as websocket:
        command_id = 1

        async def call(method: str, params: dict[str, Any]) -> dict[str, Any]:
            nonlocal command_id
            current_id = command_id
            command_id += 1
            await websocket.send(
                json.dumps(
                    {"id": current_id, "method": method, "params": params},
                    separators=(",", ":"),
                )
            )
            while True:
                raw = await websocket.recv()
                message = json.loads(raw)
                if not isinstance(message, dict):
                    raise SmokeError(f"unexpected BiDi payload: {message!r}")
                if message.get("id") == current_id:
                    return message

        tree = await call("browsingContext.getTree", {})
        assert_equal(tree.get("type"), "success", "BiDi navigation-errors getTree")
        contexts = tree["result"]["contexts"]
        assert_true(bool(contexts), "BiDi navigation-errors top-level context")
        context = contexts[0]["context"]

        invalid_argument_cases: list[tuple[str, dict[str, Any]]] = [
            ("context-missing", {"url": "about:blank"}),
            ("context-null", {"context": None, "url": "about:blank"}),
            ("context-boolean", {"context": False, "url": "about:blank"}),
            ("context-number", {"context": 42, "url": "about:blank"}),
            ("context-object", {"context": {}, "url": "about:blank"}),
            ("context-array", {"context": [], "url": "about:blank"}),
            ("url-missing", {"context": context}),
            ("url-null", {"context": context, "url": None}),
            ("url-boolean", {"context": context, "url": False}),
            ("url-number", {"context": context, "url": 42}),
            ("url-object", {"context": context, "url": {}}),
            ("url-array", {"context": context, "url": []}),
            ("url-http-invalid", {"context": context, "url": "http://:invalid"}),
            ("url-http-fragment", {"context": context, "url": "http://#invalid"}),
            ("url-https-invalid", {"context": context, "url": "https://:invalid"}),
            ("url-https-fragment", {"context": context, "url": "https://#invalid"}),
            ("wait-boolean", {"context": context, "url": "about:blank", "wait": False}),
            ("wait-number", {"context": context, "url": "about:blank", "wait": 42}),
            ("wait-object", {"context": context, "url": "about:blank", "wait": {}}),
            ("wait-array", {"context": context, "url": "about:blank", "wait": []}),
            ("wait-empty", {"context": context, "url": "about:blank", "wait": ""}),
            (
                "wait-unknown",
                {"context": context, "url": "about:blank", "wait": "somestring"},
            ),
        ]
        no_such_frame_cases: list[tuple[str, dict[str, Any]]] = [
            ("context-empty", {"context": "", "url": "about:blank"}),
            ("context-unknown", {"context": "somestring", "url": "about:blank"}),
        ]
        address_cases: list[tuple[str, dict[str, Any]]] = [
            (
                "unknown-protocol",
                {"context": context, "url": "thisprotocoldoesnotexist://", "wait": "complete"},
            ),
            (
                "nonexistent-localhost",
                {"context": context, "url": "https://doesnotexist.localhost/", "wait": "complete"},
            ),
            (
                "unsafe-port",
                {"context": context, "url": "https://localhost:0", "wait": "complete"},
            ),
        ]

        observed: list[dict[str, Any]] = []
        for expected_error, cases in [
            ("invalid argument", invalid_argument_cases),
            ("no such frame", no_such_frame_cases),
            ("unknown error", address_cases),
        ]:
            for name, params in cases:
                response = await call("browsingContext.navigate", params)
                _assert_bidi_error(response, expected_error, name)
                observed.append({"name": name, "error": expected_error})

        final_tree = await call("browsingContext.getTree", {"root": context})
        assert_equal(final_tree.get("type"), "success", "BiDi session after error matrix")
        return {
            "context": context,
            "cases": observed,
            "sessionUsableAfterFailures": True,
        }


def _assert_bidi_error(response: dict[str, Any], expected: str, name: str) -> None:
    assert_equal(response.get("type"), "error", f"BiDi {name} response type")
    assert_equal(response.get("error"), expected, f"BiDi {name} error code")
    assert_true(isinstance(response.get("id"), int), f"BiDi {name} response id")
    assert_true(isinstance(response.get("message"), str), f"BiDi {name} message")
    assert_true("result" not in response, f"BiDi {name} must not contain result")
    if "stacktrace" in response:
        assert_true(isinstance(response["stacktrace"], str), f"BiDi {name} stacktrace")
