from __future__ import annotations

import json
from typing import Any

from ..assertions import SmokeError, assert_equal, record_contract
from ..state import SmokeState


FILE_URL = "file:///moli-policy-must-not-open"
BAD_PORT_URL = "http://example.test:1/"
MALFORMED_DATA_URL = "data:text/html;charset=utf-8;base64,PT0NUWVBFIGh0bWw%2BDQo8"
SOURCE = (
    "Chromium WPT xhr/send-sync-response-event-order.htm, "
    "xhr/send-sync-no-response-event-order.htm, xhr/sync-no-progress.any.js, "
    "xhr/send-network-error-sync-events.sub.htm, xhr/send-redirect-bogus-sync.sub.htm, "
    "xhr/send-redirect-infinite-sync.htm, xhr/open-method-responsetype-set-sync.htm, "
    "xhr/open-open-sync-send.htm, xhr/open-sync-open-send.htm, "
    "xhr/send-entity-body-get-head.htm, xhr/send-send.any.js, xhr/timeout-sync.htm, "
    "xhr/send-sync-timeout.htm, xhr/send-sync-blocks-async.htm, and executable Debian "
    "Chromium 145.0.7632.116"
)


def assert_true(value: Any, label: str) -> None:
    if not value:
        raise SmokeError(f"{label}: expected a truthy value, got {value!r}")


async def run_xhr_sync_semantics_group(state: SmokeState) -> None:
    await state.page.goto(f"{state.fixture}/plain", wait_until="load")
    state.subresource_events.clear()
    observed = await state.page.evaluate(
        _PROBE_EXPRESSION,
        {
            "base": state.fixture,
            "fileUrl": FILE_URL,
            "badPortUrl": BAD_PORT_URL,
            "malformedDataUrl": MALFORMED_DATA_URL,
        },
    )
    assert_true(isinstance(observed, dict), "synchronous XHR probe result")

    successes = observed["successes"]
    expected_successes = {
        "contentLength": {
            "status": 200,
            "text": "fixture api body",
            "lengthComputable": True,
        },
        "post": {
            "status": 200,
            "json": {
                "method": "POST",
                "body": "request body",
                "contentType": "text/plain;charset=UTF-8",
                "customHeader": None,
            },
            "lengthComputable": True,
        },
        "postNoBody": {
            "status": 200,
            "json": {
                "method": "POST",
                "body": "",
                "contentType": None,
                "customHeader": None,
            },
            "lengthComputable": True,
            "requestMethod": "POST",
            "requestBodyLength": "0",
        },
        "noContent": {
            "status": 204,
            "text": "",
            "lengthComputable": False,
        },
        "noLength": {
            "status": 200,
            "text": "OK",
            "lengthComputable": False,
        },
        "data": {
            "status": 200,
            "text": "ok",
            "lengthComputable": True,
        },
        "redirect": {
            "status": 200,
            "json": {"redirected": True, "method": "GET"},
            "lengthComputable": True,
            "responseURL": f"{state.fixture}/api-redirect-final",
        },
        "getBodyIgnored": {
            "status": 200,
            "json": {
                "method": "GET",
                "body": "",
                "contentType": None,
                "customHeader": None,
            },
            "lengthComputable": True,
            "requestMethod": "GET",
            "requestBodyLength": "0",
        },
        "headBodyIgnored": {
            "status": 200,
            "text": "",
            "lengthComputable": True,
            "totalFromContentLength": True,
            "requestMethod": "HEAD",
            "requestBodyLength": "0",
        },
    }
    assert_equal(
        set(successes),
        set(expected_successes),
        "synchronous XHR success matrix names",
    )
    for name, expected in expected_successes.items():
        _assert_success(successes[name], expected, name)

    expected_failure_urls = {
        "connectionReset": f"{state.fixture}/chromium-network-reset-before-response?xhr-sync=1",
        "badPort": BAD_PORT_URL,
        "file": FILE_URL,
        "malformedData": MALFORMED_DATA_URL,
        "redirectFoobar": f"{state.fixture}/xhr-sync-redirect-foobar",
        "redirectMailto": f"{state.fixture}/xhr-sync-redirect-mailto",
        "redirectTel": f"{state.fixture}/xhr-sync-redirect-tel",
        "redirectNonexistent302": f"{state.fixture}/xhr-sync-redirect-nonexistent-302",
        "redirectNonexistent303": f"{state.fixture}/xhr-sync-redirect-nonexistent-303",
        "redirectLoop": f"{state.fixture}/xhr-sync-redirect-loop",
    }
    assert_equal(
        set(observed["failures"]),
        set(expected_failure_urls),
        "synchronous XHR failure matrix names",
    )
    for name, url in expected_failure_urls.items():
        _assert_failure(observed["failures"][name], url, name)

    _assert_restrictions(observed["restrictions"])
    assert_equal(
        observed["reopen"],
        {
            "replaceBeforeSend": {
                "states": [1, 4],
                "responseText": "second",
            },
            "reopenAfterSend": {
                "states": [1, 4, 1],
                "completed": {
                    "readyState": 4,
                    "status": 200,
                    "responseText": "complete",
                    "responseURL": "data:,complete",
                },
                "reset": {
                    "readyState": 1,
                    "status": 0,
                    "statusText": "",
                    "responseText": "",
                    "responseURL": "",
                    "responseXML": None,
                    "allHeaders": "",
                },
            },
        },
        "synchronous XHR reopen/reset matrix",
    )
    assert_equal(
        observed["secondSend"],
        {"name": "InvalidStateError", "isDomException": True},
        "second XHR send while send flag is set",
    )
    assert_equal(
        observed["pendingAsyncReopen"],
        {
            "immediate": {
                "states": [1],
                "readyState": 1,
                "status": 0,
                "statusText": "",
                "responseText": "",
                "responseURL": "",
                "responseXML": None,
                "allHeaders": "",
            },
            "afterNetworkBarrier": {
                "states": [1],
                "readyState": 1,
                "status": 0,
                "statusText": "",
                "responseText": "",
                "responseURL": "",
                "responseXML": None,
                "allHeaders": "",
            },
        },
        "pending async send canceled by synchronous open",
    )
    sync_blocks_async = observed["syncBlocksAsync"]
    assert_equal(
        sync_blocks_async["afterSync"],
        {"order": ["sync:4"], "timerRan": False},
        "synchronous XHR blocks pending async events and timers",
    )
    final_order = sync_blocks_async["finalOrder"]
    assert_true(bool(final_order), "synchronous XHR blocking final event order")
    assert_equal(final_order[0], "sync:4", "synchronous XHR completes first")
    assert_equal(final_order[-1], "async:4", "pending asynchronous XHR completes last")
    assert_true(
        all(event.startswith("async:") for event in final_order[1:]),
        "no pending asynchronous XHR event runs before synchronous completion",
    )
    assert_true("async:2" in final_order, "pending asynchronous XHR HEADERS_RECEIVED event")
    assert_true("async:3" in final_order, "pending asynchronous XHR LOADING event")
    assert_equal(sync_blocks_async["timerRan"], True, "timer runs after synchronous XHR")

    # Commands and events share the same CDP session. Once this command's
    # response arrives, the synchronous collector has observed every preceding
    # Network event without a timer-based drain.
    await state.cdp.send("Runtime.evaluate", {"expression": "0", "returnByValue": True})
    network = _assert_network_projection(state.subresource_events, state.fixture)

    record_contract(
        state.results,
        "xhr_sync_chromium_wpt_matrix",
        contract=(
            "Window synchronous XHR skips intermediate/progress/upload events, preserves "
            "Chromium progress totals, throws reset NetworkError surfaces, enforces document "
            "timeout/responseType restrictions, blocks pending event delivery, and resets "
            "correctly across open()."
        ),
        source=SOURCE,
        commands=[
            "Runtime.evaluate",
            "Network.requestWillBeSent",
            "Network.responseReceived",
            "Network.loadingFinished",
            "Network.loadingFailed",
        ],
        observed={
            "successCases": sorted(successes),
            "failureCases": sorted(expected_failure_urls),
            "responseTypeCases": len(observed["restrictions"]["responseType"]),
            "network": network,
        },
    )


def _assert_success(actual: dict[str, Any], expected: dict[str, Any], name: str) -> None:
    assert_equal(actual["error"], None, f"{name} synchronous XHR exception")
    assert_equal(actual["readyState"], 4, f"{name} synchronous XHR readyState")
    assert_equal(actual["status"], expected["status"], f"{name} synchronous XHR status")
    if "text" in expected:
        assert_equal(actual["responseText"], expected["text"], f"{name} response text")
    if "json" in expected:
        assert_equal(
            json.loads(actual["responseText"]),
            expected["json"],
            f"{name} response JSON",
        )
    if "responseURL" in expected:
        assert_equal(actual["responseURL"], expected["responseURL"], f"{name} response URL")
    loaded = len(actual["responseText"].encode("utf-8"))
    computable = expected["lengthComputable"]
    total = (
        int(actual["contentLength"])
        if expected.get("totalFromContentLength")
        else loaded if computable else 0
    )
    assert_equal(
        actual["events"],
        [
            "readystatechange:1",
            "readystatechange:4",
            f"load:{loaded}:{total}:{str(computable).lower()}",
            f"loadend:{loaded}:{total}:{str(computable).lower()}",
        ],
        f"{name} synchronous XHR event order",
    )
    assert_equal(actual["uploadEvents"], [], f"{name} synchronous XHR upload events")
    if "requestMethod" in expected:
        assert_equal(actual["requestMethod"], expected["requestMethod"], f"{name} wire method")
    if "requestBodyLength" in expected:
        assert_equal(
            actual["requestBodyLength"],
            expected["requestBodyLength"],
            f"{name} wire request body length",
        )


def _assert_failure(actual: dict[str, Any], url: str, name: str) -> None:
    assert_equal(
        actual,
        {
            "error": {
                "name": "NetworkError",
                "message": f"Failed to execute 'send' on 'XMLHttpRequest': Failed to load '{url}'.",
                "isDomException": True,
            },
            "events": ["readystatechange:1"],
            "uploadEvents": [],
            "readyState": 4,
            "status": 0,
            "statusText": "",
            "responseText": "",
            "responseURL": "",
            "contentType": None,
            "contentLength": None,
            "requestMethod": None,
            "requestBodyLength": None,
            "allHeaders": "",
        },
        f"{name} synchronous XHR network-error surface",
    )


def _assert_restrictions(actual: dict[str, Any]) -> None:
    response_type = actual["responseType"]
    assert_equal(len(response_type), 10, "synchronous document responseType case count")
    for entry in response_type:
        before_open = entry["when"] == "beforeOpen"
        assert_equal(entry["error"]["name"], "InvalidAccessError", "responseType error name")
        assert_equal(entry["error"]["isDomException"], True, "responseType DOMException")
        assert_equal(entry["readyState"], 0 if before_open else 1, "responseType readyState")
        assert_equal(entry["events"], [] if before_open else [1], "responseType events")
        assert_equal(
            entry["responseType"],
            entry["type"] if before_open else "",
            "responseType retained value",
        )
        expected_message = (
            "Failed to execute 'open' on 'XMLHttpRequest': Synchronous requests from a "
            "document must not set a response type."
            if before_open
            else "Failed to set the 'responseType' property on 'XMLHttpRequest': The response "
            "type cannot be changed for synchronous requests made from a document."
        )
        assert_equal(entry["error"]["message"], expected_message, "responseType error message")

    assert_equal(
        actual["timeout"],
        {
            "beforeOpen": {
                "error": {
                    "name": "InvalidAccessError",
                    "message": "Failed to execute 'open' on 'XMLHttpRequest': Synchronous requests must not set a timeout.",
                    "isDomException": True,
                },
                "readyState": 0,
                "timeout": 25,
            },
            "afterOpen": {
                "error": {
                    "name": "InvalidAccessError",
                    "message": "Failed to set the 'timeout' property on 'XMLHttpRequest': Timeouts cannot be set for synchronous requests made from a document.",
                    "isDomException": True,
                },
                "readyState": 1,
                "timeout": 0,
            },
        },
        "synchronous document timeout restrictions",
    )


def _assert_network_projection(events: list[dict[str, Any]], fixture: str) -> dict[str, Any]:
    success_url = f"{fixture}/api?xhr-sync=content-length"
    reset_url = f"{fixture}/chromium-network-reset-before-response?xhr-sync=1"
    success = _events_for_request_url(events, success_url)
    reset = _events_for_request_url(events, reset_url)
    assert_true(success, "CDP Network events for synchronous XHR success")
    assert_equal(success[0]["method"], "Network.requestWillBeSent", "success first event")
    assert_equal(success[0]["params"]["type"], "XHR", "success resource type")
    assert_true(
        any(event["method"] == "Network.responseReceived" for event in success),
        "success response event",
    )
    assert_equal(success[-1]["method"], "Network.loadingFinished", "success terminal event")
    assert_true(reset, "CDP Network events for synchronous XHR reset")
    assert_equal(reset[0]["method"], "Network.requestWillBeSent", "reset first event")
    assert_equal(reset[0]["params"]["type"], "XHR", "reset resource type")
    assert_true(
        not any(event["method"] == "Network.responseReceived" for event in reset),
        "reset has no response event",
    )
    assert_equal(reset[-1]["method"], "Network.loadingFailed", "reset terminal event")
    return {
        "success": [event["method"] for event in success],
        "reset": [event["method"] for event in reset],
    }


def _events_for_request_url(
    events: list[dict[str, Any]],
    url: str,
) -> list[dict[str, Any]]:
    request_ids = {
        event["params"]["requestId"]
        for event in events
        if event["method"] == "Network.requestWillBeSent"
        and event["params"]["request"]["url"] == url
    }
    if not request_ids:
        return []
    return [
        event
        for event in events
        if event["params"].get("requestId") in request_ids
        and event["method"]
        in {
            "Network.requestWillBeSent",
            "Network.responseReceived",
            "Network.dataReceived",
            "Network.loadingFinished",
            "Network.loadingFailed",
        }
    ]


_PROBE_EXPRESSION = r"""
async ({base, fileUrl, badPortUrl, malformedDataUrl}) => {
  const exceptionShape = callback => {
    try {
      callback();
      return null;
    } catch (error) {
      return {
        name: error && error.name,
        message: error && error.message,
        isDomException: error instanceof DOMException,
      };
    }
  };
  const syncRequest = (method, url, body = null) => {
    const xhr = new XMLHttpRequest();
    const events = [];
    const uploadEvents = [];
    xhr.onreadystatechange = () => events.push(`readystatechange:${xhr.readyState}`);
    for (const type of ["loadstart", "progress", "error", "timeout", "load", "loadend"]) {
      xhr.addEventListener(type, event => events.push(
        `${type}:${event.loaded}:${event.total}:${event.lengthComputable}`
      ));
      xhr.upload.addEventListener(type, event => uploadEvents.push(
        `${type}:${event.loaded}:${event.total}:${event.lengthComputable}`
      ));
    }
    xhr.open(method, url, false);
    const error = exceptionShape(() => xhr.send(body));
    return {
      error,
      events,
      uploadEvents,
      readyState: xhr.readyState,
      status: xhr.status,
      statusText: xhr.statusText,
      responseText: xhr.responseText,
      responseURL: xhr.responseURL,
      contentType: xhr.getResponseHeader("Content-Type"),
      contentLength: xhr.getResponseHeader("Content-Length"),
      requestMethod: xhr.getResponseHeader("X-Smoke-Request-Method"),
      requestBodyLength: xhr.getResponseHeader("X-Smoke-Request-Body-Length"),
      allHeaders: xhr.getAllResponseHeaders(),
    };
  };

  const successes = {
    contentLength: syncRequest("GET", `${base}/api?xhr-sync=content-length`),
    post: syncRequest("POST", `${base}/api-echo?xhr-sync=post`, "request body"),
    postNoBody: syncRequest("POST", `${base}/api-echo?xhr-sync=post-no-body`),
    noContent: syncRequest("GET", `${base}/favicon.ico?xhr-sync=no-content`),
    noLength: syncRequest("GET", `${base}/conversation-stream?xhr-sync=no-length`),
    data: syncRequest("GET", "data:text/plain,ok", "ignored GET body"),
    redirect: syncRequest("GET", `${base}/api-redirect-start?xhr-sync=redirect`),
    getBodyIgnored: syncRequest("GET", `${base}/api-echo?xhr-sync=get-body`, "ignored GET body"),
    headBodyIgnored: syncRequest("HEAD", `${base}/api-echo?xhr-sync=head-body`, "ignored HEAD body"),
  };
  const failures = {
    connectionReset: syncRequest("GET", `${base}/chromium-network-reset-before-response?xhr-sync=1`),
    badPort: syncRequest("POST", badPortUrl, "request body"),
    file: syncRequest("GET", fileUrl),
    malformedData: syncRequest("GET", malformedDataUrl),
    redirectFoobar: syncRequest("GET", `${base}/xhr-sync-redirect-foobar`),
    redirectMailto: syncRequest("GET", `${base}/xhr-sync-redirect-mailto`),
    redirectTel: syncRequest("GET", `${base}/xhr-sync-redirect-tel`),
    redirectNonexistent302: syncRequest("GET", `${base}/xhr-sync-redirect-nonexistent-302`),
    redirectNonexistent303: syncRequest("GET", `${base}/xhr-sync-redirect-nonexistent-303`),
    redirectLoop: syncRequest("GET", `${base}/xhr-sync-redirect-loop`),
  };

  const responseType = [];
  for (const type of ["arraybuffer", "blob", "json", "text", "document"]) {
    const before = new XMLHttpRequest();
    const beforeEvents = [];
    before.onreadystatechange = () => beforeEvents.push(before.readyState);
    before.responseType = type;
    responseType.push({
      type,
      when: "beforeOpen",
      error: exceptionShape(() => before.open("GET", `${base}/api`, false)),
      events: beforeEvents,
      readyState: before.readyState,
      responseType: before.responseType,
    });

    const after = new XMLHttpRequest();
    const afterEvents = [];
    after.onreadystatechange = () => afterEvents.push(after.readyState);
    after.open("GET", `${base}/api`, false);
    responseType.push({
      type,
      when: "afterOpen",
      error: exceptionShape(() => { after.responseType = type; }),
      events: afterEvents,
      readyState: after.readyState,
      responseType: after.responseType,
    });
  }

  const timeoutBefore = new XMLHttpRequest();
  timeoutBefore.timeout = 25;
  const timeoutBeforeError = exceptionShape(() => {
    timeoutBefore.open("GET", `${base}/api`, false);
  });
  const timeoutAfter = new XMLHttpRequest();
  timeoutAfter.open("GET", `${base}/api`, false);
  const timeoutAfterError = exceptionShape(() => { timeoutAfter.timeout = 25; });

  const replaceBeforeSend = new XMLHttpRequest();
  const replaceBeforeSendStates = [];
  replaceBeforeSend.onreadystatechange = () => replaceBeforeSendStates.push(replaceBeforeSend.readyState);
  replaceBeforeSend.open("GET", "data:,first");
  replaceBeforeSend.open("GET", "data:,second", false);
  replaceBeforeSend.send();

  const reopenAfterSend = new XMLHttpRequest();
  const reopenAfterSendStates = [];
  reopenAfterSend.onreadystatechange = () => reopenAfterSendStates.push(reopenAfterSend.readyState);
  reopenAfterSend.open("GET", "data:,complete", false);
  reopenAfterSend.send();
  const completed = {
    readyState: reopenAfterSend.readyState,
    status: reopenAfterSend.status,
    responseText: reopenAfterSend.responseText,
    responseURL: reopenAfterSend.responseURL,
  };
  reopenAfterSend.open("GET", `${base}/api`);
  const reset = {
    readyState: reopenAfterSend.readyState,
    status: reopenAfterSend.status,
    statusText: reopenAfterSend.statusText,
    responseText: reopenAfterSend.responseText,
    responseURL: reopenAfterSend.responseURL,
    responseXML: reopenAfterSend.responseXML,
    allHeaders: reopenAfterSend.getAllResponseHeaders(),
  };

  const secondSendXhr = new XMLHttpRequest();
  secondSendXhr.open("GET", "data:,pending");
  secondSendXhr.send();
  const secondSendError = exceptionShape(() => secondSendXhr.send());
  secondSendXhr.abort();

  const pendingThenReopened = new XMLHttpRequest();
  const pendingThenReopenedStates = [];
  pendingThenReopened.onreadystatechange = () => {
    pendingThenReopenedStates.push(pendingThenReopened.readyState);
  };
  pendingThenReopened.open("GET", `${base}/conversation-stream?xhr=pending-before-reopen`);
  pendingThenReopened.send();
  pendingThenReopened.open("GET", "data:,replacement", false);
  const pendingThenReopenedSnapshot = () => ({
    states: [...pendingThenReopenedStates],
    readyState: pendingThenReopened.readyState,
    status: pendingThenReopened.status,
    statusText: pendingThenReopened.statusText,
    responseText: pendingThenReopened.responseText,
    responseURL: pendingThenReopened.responseURL,
    responseXML: pendingThenReopened.responseXML,
    allHeaders: pendingThenReopened.getAllResponseHeaders(),
  });
  const pendingAsyncReopenImmediate = pendingThenReopenedSnapshot();

  const syncBlocksAsyncOrder = [];
  const asyncBeforeSync = new XMLHttpRequest();
  asyncBeforeSync.open("GET", `${base}/conversation-stream?xhr=async-before-sync`);
  asyncBeforeSync.onreadystatechange = () => {
    if (asyncBeforeSync.readyState >= 2) {
      syncBlocksAsyncOrder.push(`async:${asyncBeforeSync.readyState}`);
    }
  };
  const asyncBeforeSyncComplete = new Promise((resolve, reject) => {
    asyncBeforeSync.onload = resolve;
    asyncBeforeSync.onerror = () => reject(new Error("async XHR before sync XHR failed"));
  });
  asyncBeforeSync.send();
  let timerRan = false;
  setTimeout(() => { timerRan = true; }, 0);
  const blockingSync = new XMLHttpRequest();
  blockingSync.open("GET", `${base}/api?xhr=blocking-sync`, false);
  blockingSync.onreadystatechange = () => {
    syncBlocksAsyncOrder.push(`sync:${blockingSync.readyState}`);
  };
  blockingSync.send();
  const syncBlocksAsyncAfterSync = {
    order: [...syncBlocksAsyncOrder],
    timerRan,
  };
  await asyncBeforeSyncComplete;
  const syncBlocksAsync = {
    afterSync: syncBlocksAsyncAfterSync,
    finalOrder: [...syncBlocksAsyncOrder],
    timerRan,
  };

  return {
    successes,
    failures,
    restrictions: {
      responseType,
      timeout: {
        beforeOpen: {
          error: timeoutBeforeError,
          readyState: timeoutBefore.readyState,
          timeout: timeoutBefore.timeout,
        },
        afterOpen: {
          error: timeoutAfterError,
          readyState: timeoutAfter.readyState,
          timeout: timeoutAfter.timeout,
        },
      },
    },
    reopen: {
      replaceBeforeSend: {
        states: replaceBeforeSendStates,
        responseText: replaceBeforeSend.responseText,
      },
      reopenAfterSend: {states: reopenAfterSendStates, completed, reset},
    },
    secondSend: {
      name: secondSendError && secondSendError.name,
      isDomException: secondSendError && secondSendError.isDomException,
    },
    pendingAsyncReopen: {
      immediate: pendingAsyncReopenImmediate,
      afterNetworkBarrier: pendingThenReopenedSnapshot(),
    },
    syncBlocksAsync,
  };
}
"""
