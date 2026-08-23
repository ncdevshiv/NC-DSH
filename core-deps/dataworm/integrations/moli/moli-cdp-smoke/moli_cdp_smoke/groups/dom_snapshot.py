from __future__ import annotations

import asyncio
import json
from typing import Any

from playwright.async_api import Error as PlaywrightError

from . import SmokeState
from ..assertions import SmokeError, assert_equal


async def run_dom_snapshot_group(state: SmokeState) -> None:
    await _verify_document_open_replacement_uses_current_dom(state)


async def _verify_document_open_replacement_uses_current_dom(state: SmokeState) -> None:
    page = state.page
    cdp = state.cdp
    await page.goto(f"{state.fixture}/plain?dom-snapshot-replacement", wait_until="load", timeout=10_000)
    await cdp.send("Runtime.enable")
    await cdp.send("DOM.enable")

    await _document_open_write(
        cdp,
        "<!doctype html>"
        "<input id='snapshot-target' data-phase='old' value='old-value'>"
        "<p id='snapshot-marker'>old marker</p>",
    )
    first_document = await cdp.send("DOM.getDocument", {"depth": -1, "pierce": True})
    first_node_id = await _query_selector_node_id(cdp, first_document, "#snapshot-target")
    first_node = (await cdp.send("DOM.describeNode", {"nodeId": first_node_id}))["node"]
    first_backend_node_id = first_node["backendNodeId"]
    assert_equal(first_node["nodeName"], "INPUT", "old replacement CDP node name")
    assert_equal(
        _attribute_value(first_node, "data-phase"),
        "old",
        "old replacement CDP data-phase",
    )
    first_object_id = (await cdp.send("DOM.resolveNode", {"nodeId": first_node_id}))["object"]["objectId"]
    first_object_before = await _call_node_summary(cdp, first_object_id)
    assert_equal(
        first_object_before,
        {"tag": "INPUT", "phase": "old", "value": "old-value", "connected": True},
        "old replacement object before document.open",
    )
    first_snapshot = await cdp.send("DOMSnapshot.captureSnapshot", {"computedStyles": []})
    _assert_true(
        "old marker" in _snapshot_strings(first_snapshot),
        "initial DOMSnapshot should include the old document text",
    )

    await _document_open_write(
        cdp,
        "<!doctype html>"
        "<textarea id='snapshot-target' data-phase='new'>new-value</textarea>"
        "<p id='snapshot-marker'>new marker</p>",
    )
    stale_error = await _expect_cdp_error(
        cdp,
        "DOM.describeNode",
        {"nodeId": first_node_id},
    )
    if "Could not find node with given id" not in stale_error:
        raise SmokeError(
            "stale frontend nodeId should be rejected like Chromium after document.open; "
            f"got {stale_error!r}"
        )
    first_object_after = await _call_node_summary(cdp, first_object_id)
    assert_equal(
        first_object_after,
        {"tag": "INPUT", "phase": "old", "value": "old-value", "connected": False},
        "old replacement object after document.open",
    )

    second_document = await cdp.send("DOM.getDocument", {"depth": -1, "pierce": True})
    second_node_id = await _query_selector_node_id(cdp, second_document, "#snapshot-target")
    second_node = (await cdp.send("DOM.describeNode", {"nodeId": second_node_id}))["node"]
    second_backend_node_id = second_node["backendNodeId"]
    _assert_true(
        second_node_id != first_node_id,
        "new replacement frontend nodeId should not reuse the stale frontend nodeId",
    )
    _assert_true(
        second_backend_node_id != first_backend_node_id,
        "new replacement backendNodeId should not reuse the old live node identity",
    )
    assert_equal(second_node["nodeName"], "TEXTAREA", "new replacement CDP node name")
    assert_equal(
        _attribute_value(second_node, "data-phase"),
        "new",
        "new replacement CDP data-phase",
    )

    new_object = await cdp.send(
        "Runtime.evaluate",
        {
            "expression": "document.querySelector('#snapshot-target')",
            "objectGroup": "dom-snapshot-smoke",
        },
    )
    if new_object.get("exceptionDetails"):
        raise SmokeError(f"new replacement object lookup failed: {new_object!r}")
    described_new_object = (
        await cdp.send("DOM.describeNode", {"objectId": new_object["result"]["objectId"]})
    )["node"]
    assert_equal(
        described_new_object["backendNodeId"],
        second_backend_node_id,
        "DOM.describeNode(objectId) should describe the current live node",
    )
    assert_equal(
        described_new_object["nodeName"],
        "TEXTAREA",
        "DOM.describeNode(objectId) should not resolve through an old snapshot",
    )

    second_snapshot = await cdp.send("DOMSnapshot.captureSnapshot", {"computedStyles": []})
    snapshot_strings = _snapshot_strings(second_snapshot)
    _assert_true("new marker" in snapshot_strings, "DOMSnapshot should include current document text")
    _assert_true("new-value" in snapshot_strings, "DOMSnapshot should include current textarea value")
    _assert_true("TEXTAREA" in snapshot_strings, "DOMSnapshot should include current textarea node")
    _assert_true("old marker" not in snapshot_strings, "DOMSnapshot should not retain old document text")
    _assert_true("old-value" not in snapshot_strings, "DOMSnapshot should not retain old input value")

    state.record(
        "cdp_dom_snapshot_document_open_replacement_identity",
        {
            "oldNodeId": first_node_id,
            "newNodeId": second_node_id,
            "oldBackendNodeId": first_backend_node_id,
            "newBackendNodeId": second_backend_node_id,
        },
    )


async def _document_open_write(cdp: Any, html: str) -> None:
    document_updated = asyncio.Event()

    def on_document_updated(_: dict[str, Any]) -> None:
        document_updated.set()

    # Runtime.evaluate may reply before the replacement reaches its Inspector
    # DOM binding barrier. Register first so an event emitted before the
    # command response is retained, then wait before requesting frontend node
    # ids that DOM.documentUpdated intentionally invalidates.
    cdp.on("DOM.documentUpdated", on_document_updated)
    try:
        result = await cdp.send(
            "Runtime.evaluate",
            {
                "expression": (
                    "document.open();"
                    f"document.write({json.dumps(html)});"
                    "document.close();"
                    "true"
                ),
                "returnByValue": True,
                "awaitPromise": True,
            },
        )
        if result.get("exceptionDetails"):
            raise SmokeError(f"document.open/write failed: {result!r}")
        assert_equal(
            result.get("result", {}).get("value"),
            True,
            "document.open/write Runtime.evaluate result",
        )
        try:
            await asyncio.wait_for(document_updated.wait(), timeout=10)
        except TimeoutError as error:
            raise SmokeError(
                "document.open/write did not publish DOM.documentUpdated"
            ) from error
    finally:
        cdp.remove_listener("DOM.documentUpdated", on_document_updated)


async def _query_selector_node_id(cdp: Any, document: dict[str, Any], selector: str) -> int:
    root_node_id = document.get("root", {}).get("nodeId")
    _assert_true(isinstance(root_node_id, int) and root_node_id > 0, "DOM.getDocument root nodeId")
    result = await cdp.send("DOM.querySelector", {"nodeId": root_node_id, "selector": selector})
    node_id = result.get("nodeId")
    _assert_true(isinstance(node_id, int) and node_id > 0, f"DOM.querySelector({selector}) nodeId")
    return node_id


async def _call_node_summary(cdp: Any, object_id: str) -> dict[str, Any]:
    result = await cdp.send(
        "Runtime.callFunctionOn",
        {
            "objectId": object_id,
            "functionDeclaration": (
                "function() {"
                "return {"
                "tag: this.tagName,"
                "phase: this.getAttribute('data-phase'),"
                "value: this.value,"
                "connected: this.isConnected"
                "};"
                "}"
            ),
            "returnByValue": True,
        },
    )
    if result.get("exceptionDetails"):
        raise SmokeError(f"Runtime.callFunctionOn node summary failed: {result!r}")
    value = result.get("result", {}).get("value")
    _assert_true(isinstance(value, dict), "Runtime.callFunctionOn node summary value")
    return value


async def _expect_cdp_error(cdp: Any, method: str, params: dict[str, Any]) -> str:
    try:
        await cdp.send(method, params)
    except PlaywrightError as error:
        return str(error)
    raise SmokeError(f"{method} unexpectedly succeeded for {params!r}")


def _attribute_value(node: dict[str, Any], name: str) -> str | None:
    attributes = node.get("attributes") or []
    for index in range(0, len(attributes) - 1, 2):
        if attributes[index] == name:
            return attributes[index + 1]
    return None


def _assert_true(condition: bool, label: str) -> None:
    if not condition:
        raise SmokeError(label)


def _snapshot_strings(snapshot: dict[str, Any]) -> set[str]:
    strings = snapshot.get("strings") or []
    values: set[str] = set()
    for document in snapshot.get("documents") or []:
        nodes = document.get("nodes") or {}
        for field in ("nodeName", "nodeValue", "textValue", "inputValue"):
            indexes = nodes.get(field) or []
            if not isinstance(indexes, list):
                continue
            for string_index in indexes:
                if isinstance(string_index, int) and 0 <= string_index < len(strings):
                    values.add(strings[string_index])
    return values
