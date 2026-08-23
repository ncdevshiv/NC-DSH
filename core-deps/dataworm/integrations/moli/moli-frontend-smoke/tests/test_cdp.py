from __future__ import annotations

import asyncio

from moli_frontend_smoke.cdp import (
    _DOM_ENABLE_PARAMS,
    _OBSERVABLE_TREE_BARRIER_EXPRESSION,
    _capture_document,
    _diagnostics,
    _diagnostics_have_errors,
    _reconcile_expected_diagnostics,
    _resume_expression,
    _state_expression,
)


def test_diagnostics_preserve_runtime_console_and_network_failures() -> None:
    events = [
        {
            "sessionId": "wanted",
            "method": "Network.requestWillBeSent",
            "params": {
                "requestId": "request-1",
                "request": {"url": "http://fixture.test/failed.js"},
            },
        },
        {
            "sessionId": "wanted",
            "method": "Runtime.exceptionThrown",
            "params": {
                "exceptionDetails": {
                    "text": "Uncaught",
                    "lineNumber": 4,
                    "columnNumber": 2,
                    "url": "http://fixture.test/app.js",
                    "exception": {"description": "TypeError: broken"},
                }
            },
        },
        {
            "sessionId": "wanted",
            "method": "Runtime.consoleAPICalled",
            "params": {
                "type": "error",
                "args": [{"value": "failed"}, {"description": "Error object"}],
            },
        },
        {
            "sessionId": "wanted",
            "method": "Network.loadingFailed",
            "params": {
                "requestId": "request-1",
                "errorText": "net::ERR_FAILED",
                "type": "Script",
                "canceled": False,
            },
        },
        {
            "sessionId": "wanted",
            "method": "Network.responseReceived",
            "params": {
                "requestId": "request-2",
                "type": "Script",
                "response": {
                    "url": "http://fixture.test/missing.js",
                    "status": 404,
                    "statusText": "Not Found",
                },
            },
        },
        {
            "sessionId": "wanted",
            "method": "Network.responseReceived",
            "params": {
                "requestId": "request-3",
                "type": "Script",
                "response": {"url": "http://fixture.test/app.js", "status": 200},
            },
        },
        {
            "sessionId": "other",
            "method": "Network.loadingFailed",
            "params": {"requestId": "unrelated", "errorText": "ignore me"},
        },
    ]

    diagnostics = _diagnostics(events, "wanted")

    assert diagnostics["exceptions"] == [
        {
            "text": "Uncaught",
            "lineNumber": 4,
            "columnNumber": 2,
            "url": "http://fixture.test/app.js",
            "exception": "TypeError: broken",
        }
    ]
    assert diagnostics["consoleErrors"] == [
        {"type": "error", "args": ["failed", "Error object"]}
    ]
    assert diagnostics["networkFailures"] == [
        {
            "requestId": "request-1",
            "url": "http://fixture.test/failed.js",
            "errorText": "net::ERR_FAILED",
            "type": "Script",
            "canceled": False,
            "blockedReason": None,
        }
    ]
    assert diagnostics["httpErrors"] == [
        {
            "requestId": "request-2",
            "url": "http://fixture.test/missing.js",
            "status": 404,
            "statusText": "Not Found",
            "type": "Script",
        }
    ]
    assert _diagnostics_have_errors(diagnostics)


def test_empty_diagnostics_are_clean() -> None:
    assert not _diagnostics_have_errors(
        {
            "exceptions": [],
            "consoleErrors": [],
            "networkFailures": [],
            "httpErrors": [],
        }
    )


def test_expected_network_failure_is_claimed_but_preserves_cdp_projection() -> None:
    diagnostics = _diagnostics(
        [
            {
                "sessionId": "wanted",
                "method": "Network.requestWillBeSent",
                "params": {
                    "requestId": "request-1",
                    "request": {"url": "http://fixture.test/abort"},
                },
            },
            {
                "sessionId": "wanted",
                "method": "Network.loadingFailed",
                "params": {
                    "requestId": "request-1",
                    "errorText": "net::ERR_ABORTED",
                    "type": "Fetch",
                    "canceled": True,
                },
            },
        ],
        "wanted",
    )

    _reconcile_expected_diagnostics(
        diagnostics,
        {
            "expectedDiagnostics": {
                "networkFailures": [
                    {
                        "label": "expected-abort",
                        "url": "http://fixture.test/abort",
                        "type": "Fetch",
                        "canceled": True,
                    }
                ]
            }
        },
    )

    assert diagnostics["networkFailures"] == []
    assert diagnostics["expectedNetworkFailures"] == [
        {
            "label": "expected-abort",
            "errorText": "net::ERR_ABORTED",
            "type": "Fetch",
            "canceled": True,
            "blockedReason": None,
        }
    ]
    assert diagnostics["missingExpectedNetworkFailures"] == []
    assert not _diagnostics_have_errors(diagnostics)


def test_missing_expected_network_failure_remains_a_diagnostic_error() -> None:
    diagnostics = {
        "exceptions": [],
        "consoleErrors": [],
        "networkFailures": [],
        "httpErrors": [],
    }

    _reconcile_expected_diagnostics(
        diagnostics,
        {
            "expectedDiagnostics": {
                "networkFailures": [
                    {
                        "label": "missing-abort",
                        "url": "http://fixture.test/abort",
                        "type": "Fetch",
                        "canceled": True,
                    }
                ]
            }
        },
    )

    assert diagnostics["expectedNetworkFailures"] == []
    assert diagnostics["missingExpectedNetworkFailures"] == [
        {
            "label": "missing-abort",
            "url": "http://fixture.test/abort",
            "type": "Fetch",
            "canceled": True,
        }
    ]
    assert _diagnostics_have_errors(diagnostics)


def test_state_expression_waits_for_a_new_checkpoint_or_terminal_state() -> None:
    expression = _state_expression("react/family/case", 12_345, "old-token")

    assert '"react/family/case"' in expression
    assert "12345" in expression
    assert '"old-token"' in expression
    assert 'value.phase === "checkpoint"' in expression
    assert 'value.phase === "ready"' in expression
    assert 'value.phase === "error"' in expression
    assert "value.pendingFrame.token !== afterToken" in expression


def test_resume_expression_requires_the_exact_frame_token() -> None:
    expression = _resume_expression('case:"quoted"')

    assert "__MOLI_FRONTEND_SMOKE_RESUME__" in expression
    assert '"case:\\"quoted\\""' in expression


def test_document_capture_materializes_computed_and_pseudo_styles_first() -> None:
    class FakeClient:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object] | None]] = []

        async def command(
            self,
            method: str,
            params: dict[str, object] | None = None,
            **_kwargs: object,
        ) -> dict[str, object]:
            self.calls.append((method, params))
            if method == "Runtime.evaluate":
                return {"result": {"value": 7}}
            return {"root": {"nodeType": 9, "nodeName": "#document"}}

    client = FakeClient()
    root = asyncio.run(
        _capture_document(client, session_id="session", timeout=1.0)  # type: ignore[arg-type]
    )

    assert root == {"nodeType": 9, "nodeName": "#document"}
    assert [method for method, _params in client.calls] == [
        "Runtime.evaluate",
        "DOM.getDocument",
    ]
    assert client.calls[1][1] == {"depth": -1, "pierce": True}
    expression = client.calls[0][1]["expression"]  # type: ignore[index]
    assert expression == _OBSERVABLE_TREE_BARRIER_EXPRESSION
    assert "::before" in expression
    assert "::after" in expression
    assert "::marker" in expression


def test_whitespace_mode_belongs_to_dom_enable_not_get_document() -> None:
    assert _DOM_ENABLE_PARAMS == {"includeWhitespace": "all"}
