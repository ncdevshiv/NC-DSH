from __future__ import annotations

import asyncio
import collections
import contextlib
import copy
import hashlib
import json
import math
import time
import urllib.parse
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from .agent_episode_fixture import AgentEpisodeFixtureServer, FIXTURE_VERSION
from .artifacts import write_csv, write_json, write_text
from .config import REPO_ROOT
from .raw_cdp import (
    RawCdpCommandError,
    RawCdpConnectionClosed,
    RawCdpError,
    RawCdpTimeoutError,
    RoutedCommandResult,
    RoutedRawCdpClient,
    connect_routed_raw_cdp,
)
from .stats import summarize
from .synthetic_compare import target_metadata
from .target_serve import (
    TargetServeError,
    TargetServeHandle,
    TargetServeProcessExit,
    start_target_serve,
    stop_target_serve,
)


AGENT_EPISODE_SCHEMA = "moli.agent-episode.manifest.v1"
AGENT_EPISODE_REPORT_SCHEMA = "moli.agent-episode.report.v1"
AGENT_EPISODE_TARGETS = ("moli-cdp", "chrome-cdp")
DEFAULT_MANIFEST_PATH = (
    REPO_ROOT / "moli-benchmark" / "fixtures" / "agent-episode" / "manifest.json"
)
ALLOWED_OPERATIONS = {"navigate", "observe", "fill", "click"}
MAX_FAILURE_MESSAGES = 240
KNOWN_CDP_ERROR_MESSAGES = (
    "Promise was collected",
    "NoDocumentLoaded",
    "Execution context was destroyed",
)


OBSERVE_EXPRESSION = r"""
(() => JSON.stringify({
  url: location.href,
  visible_text: (document.body?.innerText || '').replace(/\s+/g, ' ').trim().slice(0, 1600),
  elements: Array.from(document.querySelectorAll(
    'input,textarea,select,button,a,[role=button],[contenteditable=true]'
  )).filter(el => {
    const r = el.getBoundingClientRect();
    return r.width > 1 && r.height > 1;
  }).slice(0, 32).map((el, i) => {
    const id = `lex-${i}`;
    el.setAttribute('data-lex-id', id);
    const label = (el.getAttribute('aria-label') || el.getAttribute('placeholder') ||
      el.innerText || el.value || el.textContent || '').replace(/\s+/g, ' ').trim().slice(0, 80);
    return {
      selector: `[data-lex-id=${id}]`,
      tag: el.tagName.toLowerCase(),
      description: label || el.tagName.toLowerCase(),
      actions: /^(input|textarea|select)$/i.test(el.tagName) || el.isContentEditable ? 'fill' : 'click'
    };
  }),
  realm: {
    old_marker_type: typeof globalThis.__agentOldRealmMarker,
    new_marker: globalThis.__agentNewRealmMarker ?? null,
    idle_boot_type: typeof globalThis.__agentIdleBoot,
    isolation_token: globalThis.__agentIsolationToken ?? null
  }
}))()
"""


class AgentEpisodeError(RuntimeError):
    pass


class JavaScriptEvaluationError(AgentEpisodeError):
    def __init__(self, description: str, details: dict[str, Any]) -> None:
        super().__init__(description)
        self.description = description
        self.details = details


class AgentExpectationError(AgentEpisodeError):
    def __init__(self, failures: list[str], observed: Any) -> None:
        super().__init__("; ".join(failures))
        self.failures = failures
        self.observed = observed


@dataclass(frozen=True)
class AgentEpisodeManifest:
    path: Path
    sha256: str
    fixture_version: str
    episodes: tuple[dict[str, Any], ...]


@dataclass
class AgentWorker:
    index: int
    handle: TargetServeHandle
    client: RoutedRawCdpClient

    @property
    def label(self) -> str:
        return f"worker-{self.index}"


@dataclass(frozen=True)
class AgentParticipant:
    name: str
    start_url: str
    browser_context_id: str
    target_id: str
    session_id: str
    setup_phases_ms: dict[str, float]


def _validate_observe_expect(
    episode_id: str,
    step_index: int,
    expected: object,
) -> None:
    if not isinstance(expected, dict):
        raise AgentEpisodeError(
            f"episode {episode_id} step {step_index} observe requires expect object"
        )
    for key in ("text_contains", "text_not_contains"):
        values = expected.get(key, [])
        if not isinstance(values, list) or not all(
            isinstance(value, str) for value in values
        ):
            raise AgentEpisodeError(
                f"episode {episode_id} step {step_index} {key} must be a string array"
            )
    min_controls = expected.get("min_controls")
    if min_controls is not None and (
        not isinstance(min_controls, int)
        or isinstance(min_controls, bool)
        or min_controls < 0
    ):
        raise AgentEpisodeError(
            f"episode {episode_id} step {step_index} min_controls must be non-negative"
        )
    for key in ("url", "url_path", "unreachable_path"):
        if key in expected and not isinstance(expected[key], str):
            raise AgentEpisodeError(
                f"episode {episode_id} step {step_index} {key} must be a string"
            )
    if "allow_empty_text" in expected and not isinstance(
        expected["allow_empty_text"], bool
    ):
        raise AgentEpisodeError(
            f"episode {episode_id} step {step_index} allow_empty_text must be boolean"
        )


def load_agent_episode_manifest(path: Path = DEFAULT_MANIFEST_PATH) -> AgentEpisodeManifest:
    raw = path.read_bytes()
    try:
        payload = json.loads(raw)
    except json.JSONDecodeError as error:
        raise AgentEpisodeError(f"invalid agent episode manifest JSON: {path}: {error}") from error
    if not isinstance(payload, dict):
        raise AgentEpisodeError("agent episode manifest root must be an object")
    if payload.get("schema") != AGENT_EPISODE_SCHEMA:
        raise AgentEpisodeError(
            f"unsupported agent episode manifest schema: {payload.get('schema')!r}"
        )
    fixture_version = payload.get("fixture_version")
    if fixture_version != FIXTURE_VERSION:
        raise AgentEpisodeError(
            f"manifest fixture_version must be {FIXTURE_VERSION!r}, got {fixture_version!r}"
        )
    raw_episodes = payload.get("episodes")
    if not isinstance(raw_episodes, list) or not raw_episodes:
        raise AgentEpisodeError("agent episode manifest must contain a non-empty episodes array")

    episodes: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for index, episode in enumerate(raw_episodes):
        if not isinstance(episode, dict):
            raise AgentEpisodeError(f"episode {index} must be an object")
        episode_id = episode.get("id")
        if not isinstance(episode_id, str) or not episode_id:
            raise AgentEpisodeError(f"episode {index} is missing a non-empty id")
        if episode_id in seen_ids:
            raise AgentEpisodeError(f"duplicate episode id: {episode_id}")
        seen_ids.add(episode_id)
        start_path = episode.get("start_path")
        if not isinstance(start_path, str) or not start_path.startswith("/agent/"):
            raise AgentEpisodeError(
                f"episode {episode_id} start_path must be a local /agent/ path"
            )
        peer_start_path = episode.get("peer_start_path")
        if peer_start_path is not None and (
            not isinstance(peer_start_path, str)
            or not peer_start_path.startswith("/agent/")
        ):
            raise AgentEpisodeError(
                f"episode {episode_id} peer_start_path must be a local /agent/ path"
            )
        steps = episode.get("steps")
        if not isinstance(steps, list) or not steps:
            raise AgentEpisodeError(f"episode {episode_id} must contain steps")
        saw_peer_step = False
        for step_index, step in enumerate(steps, start=1):
            if not isinstance(step, dict):
                raise AgentEpisodeError(
                    f"episode {episode_id} step {step_index} must be an object"
                )
            operation = step.get("operation")
            if operation not in ALLOWED_OPERATIONS:
                raise AgentEpisodeError(
                    f"episode {episode_id} step {step_index} has unsupported operation {operation!r}"
                )
            participant = step.get("participant", "primary")
            allowed_participants = {"primary", "peer"} if peer_start_path else {"primary"}
            if participant not in allowed_participants:
                raise AgentEpisodeError(
                    f"episode {episode_id} step {step_index} has unsupported "
                    f"participant {participant!r}"
                )
            saw_peer_step = saw_peer_step or participant == "peer"
            if operation in {"fill", "click"}:
                selector = step.get("selector")
                if not isinstance(selector, str) or not selector:
                    raise AgentEpisodeError(
                        f"episode {episode_id} step {step_index} requires selector"
                    )
            if operation == "fill" and not isinstance(step.get("value"), str):
                raise AgentEpisodeError(
                    f"episode {episode_id} step {step_index} fill requires string value"
                )
            navigation_path = step.get("expect_navigation_path")
            if navigation_path is not None and (
                operation != "click"
                or not isinstance(navigation_path, str)
                or not navigation_path.startswith("/agent/")
            ):
                raise AgentEpisodeError(
                    f"episode {episode_id} step {step_index} has invalid "
                    "expect_navigation_path"
                )
            if operation == "navigate":
                navigation_path = step.get("path")
                if not isinstance(navigation_path, str) or not navigation_path.startswith("/agent/"):
                    raise AgentEpisodeError(
                        f"episode {episode_id} step {step_index} navigate requires local /agent/ path"
                    )
                error_text = step.get("expect_error_text_contains")
                if error_text is not None and (
                    not isinstance(error_text, str) or not error_text
                ):
                    raise AgentEpisodeError(
                        f"episode {episode_id} step {step_index} "
                        "expect_error_text_contains must be a non-empty string"
                    )
                expect_error_document = step.get("expect_error_document", False)
                if not isinstance(expect_error_document, bool) or (
                    expect_error_document and error_text is None
                ):
                    raise AgentEpisodeError(
                        f"episode {episode_id} step {step_index} error Document "
                        "expectation requires an expected navigation error"
                    )
            if operation == "observe":
                _validate_observe_expect(
                    episode_id,
                    step_index,
                    step.get("expect"),
                )
        if peer_start_path is not None and not saw_peer_step:
            raise AgentEpisodeError(
                f"episode {episode_id} declares peer_start_path without a peer step"
            )
        episodes.append(copy.deepcopy(episode))

    return AgentEpisodeManifest(
        path=path,
        sha256=hashlib.sha256(raw).hexdigest(),
        fixture_version=fixture_version,
        episodes=tuple(episodes),
    )


def _fill_expression(selector: str, value: str) -> str:
    selector_json = json.dumps(selector)
    value_json = json.dumps(value)
    return (
        "(() => {"
        f" const e=document.querySelector({selector_json});"
        " if(!e) throw new Error('selector not found'); e.focus();"
        " const p=Object.getPrototypeOf(e);"
        " const d=Object.getOwnPropertyDescriptor(p,'value');"
        f" if(d&&d.set) d.set.call(e,{value_json}); else e.value={value_json};"
        " e.dispatchEvent(new InputEvent('input',{bubbles:true}));"
        " e.dispatchEvent(new Event('change',{bubbles:true}));"
        " return 'filled'; })()"
    )


def _click_expression(selector: str) -> str:
    selector_json = json.dumps(selector)
    return (
        "(() => {"
        f" const e=document.querySelector({selector_json});"
        " if(!e) throw new Error('selector not found'); e.focus();"
        " for(const t of ['pointerdown','mousedown','pointerup','mouseup'])"
        " e.dispatchEvent(new MouseEvent(t,{bubbles:true,cancelable:true,view:window}));"
        " e.click(); return 'clicked'; })()"
    )


def _message_payloads(result: RoutedCommandResult) -> list[dict[str, Any]]:
    return [message.json_value() for message in result.messages]


def _exception_description(details: dict[str, Any]) -> str:
    exception = details.get("exception")
    if isinstance(exception, dict):
        description = exception.get("description")
        if isinstance(description, str) and description:
            return description
    text = details.get("text")
    return str(text) if text is not None else "Runtime.evaluate failed"


async def _evaluate(
    client: RoutedRawCdpClient,
    session_id: str,
    expression: str,
    timeout_seconds: float,
) -> tuple[Any, RoutedCommandResult]:
    result = await client.command(
        "Runtime.evaluate",
        {
            "expression": expression,
            "returnByValue": True,
            "awaitPromise": True,
        },
        session_id=session_id,
        timeout=timeout_seconds,
    )
    command_result = result.response.get("result", {})
    if not isinstance(command_result, dict):
        raise JavaScriptEvaluationError(
            "Runtime.evaluate returned a non-object result",
            {"response": result.response},
        )
    exception_details = command_result.get("exceptionDetails")
    if isinstance(exception_details, dict):
        raise JavaScriptEvaluationError(
            _exception_description(exception_details),
            exception_details,
        )
    remote = command_result.get("result", {})
    if not isinstance(remote, dict):
        raise JavaScriptEvaluationError(
            "Runtime.evaluate returned no RemoteObject",
            {"response": result.response},
        )
    if remote.get("subtype") == "error":
        description = remote.get("description") or remote.get("value") or "evaluation error"
        raise JavaScriptEvaluationError(str(description), {"remote": remote})
    return remote.get("value"), result


async def _observe(
    client: RoutedRawCdpClient,
    session_id: str,
    timeout_seconds: float,
) -> tuple[dict[str, Any], RoutedCommandResult]:
    raw, result = await _evaluate(
        client,
        session_id,
        OBSERVE_EXPRESSION,
        timeout_seconds,
    )
    if not isinstance(raw, str):
        raise AgentExpectationError(["observe result was not a JSON string"], raw)
    try:
        observation = json.loads(raw)
    except json.JSONDecodeError as error:
        raise AgentExpectationError([f"observe returned invalid JSON: {error}"], raw) from error
    if not isinstance(observation, dict):
        raise AgentExpectationError(["observe JSON root was not an object"], observation)
    return observation, result


def _main_frame(response: dict[str, Any]) -> dict[str, Any]:
    frame = response.get("result", {}).get("frameTree", {}).get("frame")
    if not isinstance(frame, dict):
        raise AgentExpectationError(["Page.getFrameTree returned no main frame"], response)
    return frame


def _path(value: str) -> str:
    return urllib.parse.urlparse(value).path


def _observation_failures(
    observation: dict[str, Any],
    expected: dict[str, Any],
) -> list[str]:
    failures: list[str] = []
    observed_url = observation.get("url")
    if not isinstance(observed_url, str):
        failures.append("observation url was missing")
        observed_url = ""
    expected_url = expected.get("url")
    if isinstance(expected_url, str) and observed_url != expected_url:
        failures.append(f"expected url {expected_url!r}, got {observed_url!r}")
    expected_path = expected.get("url_path")
    if isinstance(expected_path, str) and _path(observed_url) != expected_path:
        failures.append(
            f"expected url path {expected_path!r}, got {_path(observed_url)!r}"
        )

    visible_text = observation.get("visible_text")
    if not isinstance(visible_text, str):
        failures.append("observation visible_text was missing")
        visible_text = ""
    if not visible_text and not bool(expected.get("allow_empty_text")):
        failures.append("observation visible_text was empty")
    for value in expected.get("text_contains", []):
        if str(value) not in visible_text:
            failures.append(f"visible_text did not contain {value!r}")
    for value in expected.get("text_not_contains", []):
        if str(value) in visible_text:
            failures.append(f"visible_text unexpectedly contained {value!r}")

    elements = observation.get("elements")
    if not isinstance(elements, list):
        failures.append("observation elements was not an array")
        elements = []
    min_controls = expected.get("min_controls")
    if isinstance(min_controls, int) and len(elements) < min_controls:
        failures.append(
            f"expected at least {min_controls} controls, got {len(elements)}"
        )

    realm = observation.get("realm")
    if not isinstance(realm, dict):
        realm = {}
    for expected_key in (
        "old_marker_type",
        "new_marker",
        "idle_boot_type",
        "isolation_token",
    ):
        if expected_key in expected and realm.get(expected_key) != expected[expected_key]:
            failures.append(
                f"expected realm {expected_key}={expected[expected_key]!r}, "
                f"got {realm.get(expected_key)!r}"
            )

    unreachable_path = expected.get("unreachable_path")
    if isinstance(unreachable_path, str):
        frame = observation.get("frame")
        if not isinstance(frame, dict):
            failures.append("observation did not include frame metadata")
        else:
            unreachable_url = frame.get("unreachableUrl")
            if not isinstance(unreachable_url, str) or _path(unreachable_url) != unreachable_path:
                failures.append(
                    f"expected unreachable path {unreachable_path!r}, got {unreachable_url!r}"
                )
    return failures


def _step_assertion_count(step: dict[str, Any]) -> int:
    """Return the number of explicit correctness checks owned by a step.

    This is intentionally derived from the manifest contract rather than from
    exception text. A successful step passes every check; a failed step passes
    none, because partial browser state is diagnostic data rather than a
    benchmark success.
    """
    operation = str(step["operation"])
    if operation == "navigate":
        return 1 + int(bool(step.get("expect_error_document")))
    if operation in {"fill", "click"}:
        return 1 + int(isinstance(step.get("expect_navigation_path"), str))

    expected = step.get("expect", {})
    if not isinstance(expected, dict):
        return 0
    count = sum(
        key in expected
        for key in (
            "url",
            "url_path",
            "min_controls",
            "old_marker_type",
            "new_marker",
            "idle_boot_type",
            "isolation_token",
            "unreachable_path",
        )
    )
    count += len(expected.get("text_contains", []))
    count += len(expected.get("text_not_contains", []))
    return count


def _main_frame_event_for_path(path: str):
    def matches(message: dict[str, Any]) -> bool:
        frame = message.get("params", {}).get("frame")
        return (
            isinstance(frame, dict)
            and not frame.get("parentId")
            and _path(str(frame.get("url", ""))) == path
        )

    return matches


def _error_frame_event_for_url(url: str):
    def matches(message: dict[str, Any]) -> bool:
        frame = message.get("params", {}).get("frame")
        return (
            isinstance(frame, dict)
            and not frame.get("parentId")
            and frame.get("url") == "chrome-error://chromewebdata/"
            and frame.get("unreachableUrl") == url
        )

    return matches


async def _wait_for_document(
    client: RoutedRawCdpClient,
    session_id: str,
    *,
    after_sequence: int,
    expected_path: str,
    timeout_seconds: float,
) -> None:
    await _wait_for_frame_and_load(
        client,
        session_id,
        after_sequence=after_sequence,
        frame_predicate=_main_frame_event_for_path(expected_path),
        timeout_seconds=timeout_seconds,
    )


async def _wait_for_frame_and_load(
    client: RoutedRawCdpClient,
    session_id: str,
    *,
    after_sequence: int,
    frame_predicate: Callable[[dict[str, Any]], bool],
    timeout_seconds: float,
) -> None:
    loop = asyncio.get_running_loop()
    deadline = loop.time() + timeout_seconds

    def remaining_timeout() -> float:
        return max(0.0, deadline - loop.time())

    await client.wait_for_event(
        "Page.frameNavigated",
        after_sequence=after_sequence,
        session_id=session_id,
        predicate=frame_predicate,
        timeout=remaining_timeout(),
    )
    await client.wait_for_event(
        "Page.loadEventFired",
        after_sequence=after_sequence,
        session_id=session_id,
        timeout=remaining_timeout(),
    )


async def _navigate(
    client: RoutedRawCdpClient,
    session_id: str,
    url: str,
    timeout_seconds: float,
    *,
    expect_error_text_contains: str | None = None,
    expect_error_document: bool = False,
) -> dict[str, Any]:
    sequence_before = client.current_sequence
    result = await client.command(
        "Page.navigate",
        {"url": url},
        session_id=session_id,
        timeout=timeout_seconds,
    )
    navigate_result = result.response.get("result", {})
    if not isinstance(navigate_result, dict):
        raise AgentExpectationError(["Page.navigate returned no result object"], result.response)
    error_text = navigate_result.get("errorText")
    if expect_error_text_contains is None:
        if error_text:
            raise AgentExpectationError(
                [f"unexpected Page.navigate errorText: {error_text}"],
                navigate_result,
            )
        await _wait_for_document(
            client,
            session_id,
            after_sequence=sequence_before,
            expected_path=_path(url),
            timeout_seconds=timeout_seconds,
        )
    else:
        if expect_error_text_contains not in str(error_text or ""):
            raise AgentExpectationError(
                [
                    f"expected Page.navigate errorText containing {expect_error_text_contains!r}, "
                    f"got {error_text!r}"
                ],
                navigate_result,
            )
        if expect_error_document:
            await _wait_for_frame_and_load(
                client,
                session_id,
                after_sequence=sequence_before,
                frame_predicate=_error_frame_event_for_url(url),
                timeout_seconds=timeout_seconds,
            )
    return {
        "value": navigate_result,
        "command": result,
        "error_text": error_text,
    }


async def _create_participant(
    *,
    client: RoutedRawCdpClient,
    fixture: AgentEpisodeFixtureServer,
    name: str,
    start_path: str,
    timeout_seconds: float,
    created_context_ids: list[str],
    created_target_ids: list[str],
) -> AgentParticipant:
    context_result = await client.command(
        "Target.createBrowserContext",
        timeout=timeout_seconds,
    )
    browser_context_id = context_result.response.get("result", {}).get(
        "browserContextId"
    )
    if not isinstance(browser_context_id, str) or not browser_context_id:
        raise AgentExpectationError(
            ["Target.createBrowserContext returned no browserContextId"],
            context_result.response,
        )
    created_context_ids.append(browser_context_id)

    target_result = await client.command(
        "Target.createTarget",
        {"url": "about:blank", "browserContextId": browser_context_id},
        timeout=timeout_seconds,
    )
    target_id = target_result.response.get("result", {}).get("targetId")
    if not isinstance(target_id, str) or not target_id:
        raise AgentExpectationError(
            ["Target.createTarget returned no targetId"],
            target_result.response,
        )
    created_target_ids.append(target_id)

    attach_result = await client.command(
        "Target.attachToTarget",
        {"targetId": target_id, "flatten": True},
        timeout=timeout_seconds,
    )
    session_id = attach_result.response.get("result", {}).get("sessionId")
    if not isinstance(session_id, str) or not session_id:
        raise AgentExpectationError(
            ["Target.attachToTarget returned no sessionId"],
            attach_result.response,
        )
    enable_results = []
    for method in ("Page.enable", "Runtime.enable"):
        enable_results.append(
            await client.command(
                method,
                session_id=session_id,
                timeout=timeout_seconds,
            )
        )
    start_url = fixture.url(start_path)
    setup_navigation = await _navigate(
        client,
        session_id,
        start_url,
        timeout_seconds,
    )
    return AgentParticipant(
        name=name,
        start_url=start_url,
        browser_context_id=browser_context_id,
        target_id=target_id,
        session_id=session_id,
        setup_phases_ms={
            "create_browser_context": context_result.elapsed_ms,
            "create_target": target_result.elapsed_ms,
            "attach_target": attach_result.elapsed_ms,
            "page_enable": enable_results[0].elapsed_ms,
            "runtime_enable": enable_results[1].elapsed_ms,
            "initial_navigate_response": setup_navigation["command"].elapsed_ms,
        },
    )


async def _execute_step(
    client: RoutedRawCdpClient,
    session_id: str,
    fixture: AgentEpisodeFixtureServer,
    step: dict[str, Any],
    timeout_seconds: float,
) -> dict[str, Any]:
    operation = str(step["operation"])
    if operation == "navigate":
        target_url = fixture.url(str(step["path"]))
        return await _navigate(
            client,
            session_id,
            target_url,
            timeout_seconds,
            expect_error_text_contains=step.get("expect_error_text_contains"),
            expect_error_document=bool(step.get("expect_error_document")),
        )
    if operation == "observe":
        observation, result = await _observe(client, session_id, timeout_seconds)
        expected = step.get("expect", {})
        if not isinstance(expected, dict):
            raise AgentExpectationError(["observe expect must be an object"], expected)
        if "unreachable_path" in expected:
            frame_result = await client.command(
                "Page.getFrameTree",
                session_id=session_id,
                timeout=timeout_seconds,
            )
            observation["frame"] = _main_frame(frame_result.response)
        failures = _observation_failures(observation, expected)
        if failures:
            raise AgentExpectationError(failures, observation)
        return {"value": observation, "command": result}
    if operation in {"fill", "click"}:
        pre_observation, pre_result = await _observe(client, session_id, timeout_seconds)
        selector = str(step["selector"])
        expression = (
            _fill_expression(selector, str(step["value"]))
            if operation == "fill"
            else _click_expression(selector)
        )
        sequence_before_action = client.current_sequence
        value, result = await _evaluate(
            client,
            session_id,
            expression,
            timeout_seconds,
        )
        expected_value = "filled" if operation == "fill" else "clicked"
        if value != expected_value:
            raise AgentExpectationError(
                [f"expected action result {expected_value!r}, got {value!r}"],
                value,
            )
        navigation_path = step.get("expect_navigation_path")
        if isinstance(navigation_path, str):
            await _wait_for_document(
                client,
                session_id,
                after_sequence=sequence_before_action,
                expected_path=navigation_path,
                timeout_seconds=timeout_seconds,
            )
        return {
            "value": value,
            "command": result,
            "pre_observation": pre_observation,
            "pre_observation_elapsed_ms": pre_result.elapsed_ms,
        }
    raise AgentEpisodeError(f"unsupported operation: {operation}")


def _phase(
    markers: list[dict[str, Any]],
    *,
    target: str,
    event: str,
    worker: str | None = None,
    run: int | None = None,
    episode: str | None = None,
    step: int | None = None,
    operation: str | None = None,
    participant: str | None = None,
) -> None:
    markers.append(
        {
            "timestamp": time.time(),
            "target": target,
            "event": event,
            "worker": worker,
            "run": run,
            "episode": episode,
            "step": step,
            "operation": operation,
            "participant": participant,
        }
    )


async def _diagnose_timeout(
    client: RoutedRawCdpClient,
    target_id: str | None,
    timeout_seconds: float,
) -> dict[str, Any]:
    diagnostic: dict[str, Any] = {
        "browser_alive": False,
        "reattach": None,
        "runtime_recovered": None,
    }
    probe_timeout = max(0.25, min(3.0, timeout_seconds))
    try:
        version = await client.command("Browser.getVersion", timeout=probe_timeout)
    except Exception as error:
        diagnostic["browser_error"] = f"{type(error).__name__}: {error}"
        return diagnostic
    diagnostic["browser_alive"] = True
    diagnostic["browser_version"] = version.response.get("result")
    if not target_id:
        return diagnostic
    try:
        attached = await client.command(
            "Target.attachToTarget",
            {"targetId": target_id, "flatten": True},
            timeout=probe_timeout,
        )
        session_id = attached.response.get("result", {}).get("sessionId")
        diagnostic["reattach"] = bool(session_id)
        if not isinstance(session_id, str) or not session_id:
            return diagnostic
        await client.command("Runtime.enable", session_id=session_id, timeout=probe_timeout)
        value, _ = await _evaluate(
            client,
            session_id,
            "1 + 1",
            probe_timeout,
        )
        diagnostic["runtime_recovered"] = value == 2
        with contextlib.suppress(Exception):
            await client.command(
                "Target.detachFromTarget",
                {"sessionId": session_id},
                timeout=probe_timeout,
            )
    except Exception as error:
        diagnostic["reattach_error"] = f"{type(error).__name__}: {error}"
    return diagnostic


def _failure_status(error: BaseException) -> tuple[str, dict[str, Any]]:
    if isinstance(error, RawCdpCommandError):
        return "protocol_error", {
            "cdp_method": error.method,
            "cdp_error_code": error.error.get("code"),
            "cdp_error_message": error.error.get("message"),
            "cdp_error_data": error.error.get("data"),
        }
    if isinstance(error, RawCdpTimeoutError):
        return "timeout", {"timeout_method": error.method}
    if isinstance(error, RawCdpConnectionClosed):
        return "websocket_dropped", {}
    if isinstance(error, JavaScriptEvaluationError):
        status = (
            "selector_not_found"
            if "selector not found" in error.description.lower()
            else "javascript_exception"
        )
        return status, {"javascript_exception": error.description}
    if isinstance(error, AgentExpectationError):
        return "content_mismatch", {"expectation_failures": error.failures}
    if isinstance(error, RawCdpError):
        return "websocket_dropped", {}
    if isinstance(error, (ConnectionError, OSError)):
        return "websocket_dropped", {}
    if isinstance(error, TimeoutError):
        return "timeout_browser_dead", {}
    return "harness_error", {}


def _step_row_base(
    *,
    target: str,
    worker: AgentWorker,
    run_id: int,
    episode_id: str,
    step_index: int,
    operation: str,
    participant: str,
    dwell_ms: int,
) -> dict[str, Any]:
    return {
        "target": target,
        **target_metadata(target),
        "worker": worker.label,
        "run": run_id,
        "episode": episode_id,
        "step": step_index,
        "operation": operation,
        "participant": participant,
        "dwell_ms": dwell_ms,
    }


async def _best_effort_html_snapshot(
    client: RoutedRawCdpClient,
    session_id: str | None,
    timeout_seconds: float,
) -> str | None:
    if not session_id:
        return None
    try:
        value, _ = await _evaluate(
            client,
            session_id,
            "document.documentElement ? document.documentElement.outerHTML : ''",
            max(0.25, min(2.0, timeout_seconds)),
        )
    except Exception:
        return None
    return value if isinstance(value, str) else None


async def _run_episode(
    *,
    suite_dir: Path,
    target: str,
    worker: AgentWorker,
    run_id: int,
    episode: dict[str, Any],
    fixture: AgentEpisodeFixtureServer,
    step_dwell_ms: int,
    timeout_seconds: float,
    markers: list[dict[str, Any]],
) -> tuple[dict[str, Any], list[dict[str, Any]], dict[str, Any]]:
    episode_id = str(episode["id"])
    started = time.perf_counter()
    client = worker.client
    message_sequence_start = client.current_sequence
    participants: dict[str, AgentParticipant] = {}
    created_context_ids: list[str] = []
    created_target_ids: list[str] = []
    failure_participant = "primary"
    steps: list[dict[str, Any]] = []
    detail: dict[str, Any] = {
        "target": target,
        "worker": worker.label,
        "run": run_id,
        "episode": episode_id,
        "start_url": fixture.url(str(episode["start_path"])),
        "ok": False,
    }
    episode_status = "ok"
    episode_error: str | None = None
    episode_extra: dict[str, Any] = {}
    failure_step: int | None = None
    last_observations: dict[str, dict[str, Any]] = {}
    setup_elapsed_ms = 0.0
    setup_phases_ms: dict[str, float] = {}

    _phase(
        markers,
        target=target,
        worker=worker.label,
        run=run_id,
        episode=episode_id,
        event="episode-start",
    )
    try:
        setup_started = time.perf_counter()
        participant_paths = [("primary", str(episode["start_path"]))]
        if episode.get("peer_start_path") is not None:
            participant_paths.append(("peer", str(episode["peer_start_path"])))
        for participant_name, start_path in participant_paths:
            participants[participant_name] = await _create_participant(
                client=client,
                fixture=fixture,
                name=participant_name,
                start_path=start_path,
                timeout_seconds=timeout_seconds,
                created_context_ids=created_context_ids,
                created_target_ids=created_target_ids,
            )
        setup_elapsed_ms = (time.perf_counter() - setup_started) * 1000.0
        setup_phases_ms = {
            phase: sum(
                participant.setup_phases_ms[phase]
                for participant in participants.values()
            )
            for phase in (
                "create_browser_context",
                "create_target",
                "attach_target",
                "page_enable",
                "runtime_enable",
                "initial_navigate_response",
            )
        }
        detail["setup"] = {
            "elapsed_ms": setup_elapsed_ms,
            "phases_ms": setup_phases_ms,
            "participants": {
                name: {
                    "start_url": participant.start_url,
                    "browser_context_id": participant.browser_context_id,
                    "target_id": participant.target_id,
                    "session_id": participant.session_id,
                    "phases_ms": participant.setup_phases_ms,
                }
                for name, participant in participants.items()
            },
        }

        for step_index, step in enumerate(episode["steps"], start=1):
            operation = str(step["operation"])
            participant_name = str(step.get("participant", "primary"))
            participant = participants[participant_name]
            failure_participant = participant_name
            if step_index > 1 and step_dwell_ms:
                _phase(
                    markers,
                    target=target,
                    worker=worker.label,
                    run=run_id,
                    episode=episode_id,
                    step=step_index,
                    operation=operation,
                    participant=participant_name,
                    event="dwell-start",
                )
                await asyncio.sleep(step_dwell_ms / 1000.0)
                _phase(
                    markers,
                    target=target,
                    worker=worker.label,
                    run=run_id,
                    episode=episode_id,
                    step=step_index,
                    operation=operation,
                    participant=participant_name,
                    event="dwell-end",
                )
            _phase(
                markers,
                target=target,
                worker=worker.label,
                run=run_id,
                episode=episode_id,
                step=step_index,
                operation=operation,
                participant=participant_name,
                event="step-start",
            )
            row = _step_row_base(
                target=target,
                worker=worker,
                run_id=run_id,
                episode_id=episode_id,
                step_index=step_index,
                operation=operation,
                participant=participant_name,
                dwell_ms=step_dwell_ms if step_index > 1 else 0,
            )
            assertion_count = _step_assertion_count(step)
            step_started = time.perf_counter()
            try:
                result = await _execute_step(
                    client,
                    participant.session_id,
                    fixture,
                    step,
                    timeout_seconds,
                )
                elapsed_ms = (time.perf_counter() - step_started) * 1000.0
                value = result.get("value")
                command = result.get("command")
                if operation == "observe" and isinstance(value, dict):
                    last_observations[participant_name] = value
                row.update(
                    {
                        "ok": True,
                        "status": "ok",
                        "elapsed_ms": elapsed_ms,
                        "response_id": command.message_id
                        if isinstance(command, RoutedCommandResult)
                        else None,
                        "url": value.get("url")
                        if isinstance(value, dict)
                        else None,
                        "visible_text_chars": len(value.get("visible_text", ""))
                        if isinstance(value, dict)
                        else None,
                        "control_count": len(value.get("elements", []))
                        if isinstance(value, dict)
                        else None,
                        "pre_observation_elapsed_ms": result.get(
                            "pre_observation_elapsed_ms"
                        ),
                        "assertions_total": assertion_count,
                        "assertions_passed": assertion_count,
                        "cdp_error_code": None,
                        "cdp_error_message": None,
                        "error": None,
                    }
                )
                detail.setdefault("step_details", []).append(
                    {
                        **row,
                        "value": value,
                        "messages": _message_payloads(command)
                        if isinstance(command, RoutedCommandResult)
                        else [],
                        "pre_observation": result.get("pre_observation"),
                    }
                )
            except Exception as error:
                elapsed_ms = (time.perf_counter() - step_started) * 1000.0
                status, extra = _failure_status(error)
                diagnostic = None
                if isinstance(error, RawCdpTimeoutError):
                    diagnostic = await _diagnose_timeout(
                        client,
                        participant.target_id,
                        timeout_seconds,
                    )
                    status = (
                        "timeout_page_alive"
                        if diagnostic.get("browser_alive")
                        else "timeout_browser_dead"
                    )
                if worker.handle.process.poll() is not None:
                    status = "process_exit"
                row.update(
                    {
                        "ok": False,
                        "status": status,
                        "elapsed_ms": elapsed_ms,
                        "response_id": getattr(error, "message_id", None),
                        "url": None,
                        "visible_text_chars": None,
                        "control_count": None,
                        "pre_observation_elapsed_ms": None,
                        "assertions_total": assertion_count,
                        "assertions_passed": 0,
                        "cdp_error_code": extra.get("cdp_error_code"),
                        "cdp_error_message": extra.get("cdp_error_message"),
                        "error": str(error),
                    }
                )
                detail.setdefault("step_details", []).append(
                    {
                        **row,
                        **extra,
                        "diagnostic": diagnostic,
                        "exception_type": type(error).__name__,
                        "exception_messages": getattr(error, "messages", []),
                    }
                )
                episode_status = status
                episode_error = str(error)
                episode_extra = extra
                failure_step = step_index
                steps.append(row)
                _phase(
                    markers,
                    target=target,
                    worker=worker.label,
                    run=run_id,
                    episode=episode_id,
                    step=step_index,
                    operation=operation,
                    participant=participant_name,
                    event="step-failed",
                )
                break
            steps.append(row)
            _phase(
                markers,
                target=target,
                worker=worker.label,
                run=run_id,
                episode=episode_id,
                step=step_index,
                operation=operation,
                participant=participant_name,
                event="step-done",
            )
    except Exception as error:
        episode_status, extra = _failure_status(error)
        if worker.handle.process.poll() is not None:
            episode_status = "process_exit"
        episode_error = str(error)
        episode_extra = extra
        failure_step = 0
        detail["setup_error"] = {
            "status": episode_status,
            "error": episode_error,
            "exception_type": type(error).__name__,
            "created_browser_context_ids": created_context_ids,
            "created_target_ids": created_target_ids,
            **extra,
        }
    finally:
        if episode_status == "ok" and worker.handle.process.poll() is not None:
            episode_status = "process_exit"
            episode_error = (
                f"target process exited before episode cleanup with "
                f"return code {worker.handle.process.returncode}"
            )
        failure_runtime = participants.get(failure_participant) or participants.get(
            "primary"
        )
        if episode_status != "ok":
            detail["html_snapshot"] = await _best_effort_html_snapshot(
                client,
                failure_runtime.session_id if failure_runtime is not None else None,
                timeout_seconds,
            )
        for context_id in reversed(created_context_ids):
            with contextlib.suppress(Exception):
                await client.command(
                    "Target.disposeBrowserContext",
                    {"browserContextId": context_id},
                    timeout=max(0.25, min(2.0, timeout_seconds)),
                )
        recorded = [
            message.json_value()
            for message in client.messages_since(message_sequence_start)
        ]
        if episode_status != "ok":
            detail["cdp_messages"] = recorded[-MAX_FAILURE_MESSAGES:]
        detail["cdp_message_count"] = len(recorded)

    elapsed_ms = (time.perf_counter() - started) * 1000.0
    active_elapsed_ms = setup_elapsed_ms + sum(
        float(step.get("elapsed_ms") or 0.0) for step in steps
    )
    ok = episode_status == "ok" and len(steps) == len(episode["steps"])
    primary_runtime = participants.get("primary")
    primary_observation = last_observations.get("primary")
    if primary_observation is None and last_observations:
        primary_observation = next(iter(last_observations.values()))
    detail["final_observations"] = last_observations
    row = {
        "target": target,
        **target_metadata(target),
        "worker": worker.label,
        "run": run_id,
        "episode": episode_id,
        "ok": ok,
        "status": "ok" if ok else episode_status,
        "elapsed_ms": elapsed_ms,
        "active_elapsed_ms": active_elapsed_ms,
        "setup_elapsed_ms": setup_elapsed_ms,
        "context_create_ms": setup_phases_ms.get("create_browser_context"),
        "target_create_ms": setup_phases_ms.get("create_target"),
        "attach_ms": setup_phases_ms.get("attach_target"),
        "page_enable_ms": setup_phases_ms.get("page_enable"),
        "runtime_enable_ms": setup_phases_ms.get("runtime_enable"),
        "initial_navigate_response_ms": setup_phases_ms.get(
            "initial_navigate_response"
        ),
        "dwell_ms": step_dwell_ms * max(0, len(steps) - 1),
        "step_count": len(steps),
        "expected_step_count": len(episode["steps"]),
        "failure_step": failure_step,
        "error": episode_error,
        "cdp_error_code": episode_extra.get("cdp_error_code"),
        "cdp_error_message": episode_extra.get("cdp_error_message"),
        "target_id": primary_runtime.target_id if primary_runtime else None,
        "browser_context_id": primary_runtime.browser_context_id
        if primary_runtime
        else None,
        "participant_count": len(participants),
        "final_url": primary_observation.get("url")
        if isinstance(primary_observation, dict)
        else None,
        "visible_text_chars": len(primary_observation.get("visible_text", ""))
        if isinstance(primary_observation, dict)
        else None,
        "control_count": len(primary_observation.get("elements", []))
        if isinstance(primary_observation, dict)
        else None,
        "failure_artifact": None,
    }
    detail.update(row)
    if not ok:
        artifact_base = (
            f"{target}-run-{run_id}-{episode_id}-{worker.label}"
        )
        html_snapshot = detail.pop("html_snapshot", None)
        if isinstance(html_snapshot, str):
            html_path = suite_dir / "failures" / f"{artifact_base}.html"
            write_text(html_path, html_snapshot)
            detail["html_snapshot_artifact"] = str(html_path.relative_to(suite_dir))
        detail["target_log_tail"] = worker.handle.logs[-80:]
        artifact_path = suite_dir / "failures" / f"{artifact_base}.json"
        write_json(artifact_path, detail)
        row["failure_artifact"] = str(artifact_path.relative_to(suite_dir))
        detail["failure_artifact"] = row["failure_artifact"]

    _phase(
        markers,
        target=target,
        worker=worker.label,
        run=run_id,
        episode=episode_id,
        event="episode-done" if ok else "episode-failed",
    )
    return row, steps, detail


def _normalize_worker_samples(
    worker: AgentWorker,
    target_started_epoch: float,
) -> list[dict[str, Any]]:
    normalized = []
    for sample in worker.handle.sampler.samples:
        item = dict(sample)
        item["target_elapsed_ms"] = max(
            0.0,
            (float(sample.get("timestamp", target_started_epoch)) - target_started_epoch)
            * 1000.0,
        )
        item["worker"] = worker.label
        item["root_pid"] = worker.handle.process.pid
        normalized.append(item)
    return normalized


def _sum_complete(samples: list[dict[str, Any]], key: str) -> float | int | None:
    values = [sample.get(key) for sample in samples]
    if not values or any(value is None for value in values):
        return None
    return sum(values)  # type: ignore[arg-type]


def _aggregate_resource_samples(
    worker_samples: dict[str, list[dict[str, Any]]],
) -> list[dict[str, Any]]:
    timestamps = sorted(
        {
            float(sample["timestamp"])
            for samples in worker_samples.values()
            for sample in samples
            if sample.get("timestamp") is not None
        }
    )
    cursors = {worker: 0 for worker in worker_samples}
    latest: dict[str, dict[str, Any]] = {}
    rows: list[dict[str, Any]] = []
    first_timestamp = timestamps[0] if timestamps else 0.0
    for timestamp in timestamps:
        for worker, samples in worker_samples.items():
            cursor = cursors[worker]
            while cursor < len(samples) and float(samples[cursor]["timestamp"]) <= timestamp:
                latest[worker] = samples[cursor]
                cursor += 1
            cursors[worker] = cursor
        current = [latest[worker] for worker in sorted(latest)]
        if not current:
            continue
        rows.append(
            {
                "timestamp": timestamp,
                "elapsed_ms": (timestamp - first_timestamp) * 1000.0,
                "worker_count": len(worker_samples),
                "observed_worker_count": len(current),
                "cpu_percent": _sum_complete(current, "cpu_percent"),
                "rss_bytes": _sum_complete(current, "rss_bytes"),
                "pss_bytes": _sum_complete(current, "pss_bytes"),
                "process_count": _sum_complete(current, "process_count"),
                "thread_count": _sum_complete(current, "thread_count"),
                "fd_count": _sum_complete(current, "fd_count"),
                "capture_duration_ms": _sum_complete(
                    current, "capture_duration_ms"
                ),
            }
        )
    return rows


def _resource_summary(samples: list[dict[str, Any]]) -> dict[str, Any]:
    complete = [
        sample
        for sample in samples
        if sample.get("observed_worker_count") == sample.get("worker_count")
    ]

    def values(key: str) -> list[float]:
        return [
            float(sample[key])
            for sample in complete
            if sample.get(key) is not None and math.isfinite(float(sample[key]))
        ]

    cpu = values("cpu_percent")
    rss = values("rss_bytes")
    pss = values("pss_bytes")
    process_count = values("process_count")
    thread_count = values("thread_count")
    captures = values("capture_duration_ms")
    intervals = [
        float(current["elapsed_ms"]) - float(previous["elapsed_ms"])
        for previous, current in zip(complete, complete[1:])
    ]
    return {
        "sample_count": len(samples),
        "complete_sample_count": len(complete),
        "peak_cpu_percent": max(cpu) if cpu else None,
        "average_cpu_percent": sum(cpu) / len(cpu) if cpu else None,
        "peak_rss_bytes": max(rss) if rss else None,
        "peak_pss_bytes": max(pss) if pss else None,
        "peak_process_count": max(process_count) if process_count else None,
        "peak_thread_count": max(thread_count) if thread_count else None,
        "capture_duration_ms": {
            "average": sum(captures) / len(captures) if captures else None,
            "max": max(captures) if captures else None,
        },
        "aggregate_update_interval_ms": {
            "average": sum(intervals) / len(intervals) if intervals else None,
            "max": max(intervals) if intervals else None,
        },
    }


def _sampler_health(
    workers: list[AgentWorker],
    stop_details: dict[str, Any],
) -> dict[str, Any]:
    summaries = [
        stop_details.get(worker.label, {}).get("resources", {})
        for worker in workers
    ]
    cpu_averages = [summary.get("average_cpu_percent") for summary in summaries]
    observed_average = [
        summary.get("observed_interval_ms", {}).get("average")
        for summary in summaries
    ]
    observed_max = [
        summary.get("observed_interval_ms", {}).get("max") for summary in summaries
    ]
    observer_errors = [
        {"worker": worker.label, "error": summary.get("observer_error")}
        for worker, summary in zip(workers, summaries)
        if summary.get("observer_error")
    ]
    alive_workers = [
        worker.label
        for worker, summary in zip(workers, summaries)
        if summary.get("thread_alive_after_stop")
    ]
    return {
        "worker_count": len(workers),
        "sample_count": sum(int(summary.get("sample_count", 0)) for summary in summaries),
        "late_sample_count": sum(
            int(summary.get("late_sample_count", 0)) for summary in summaries
        ),
        "pss_complete": bool(summaries)
        and all(bool(summary.get("pss_complete")) for summary in summaries),
        "observed_interval_ms": {
            "average": sum(float(value) for value in observed_average)
            / len(observed_average)
            if observed_average and all(value is not None for value in observed_average)
            else None,
            "max": max(float(value) for value in observed_max)
            if observed_max and all(value is not None for value in observed_max)
            else None,
        },
        "combined_average_cpu_percent": sum(float(value) for value in cpu_averages)
        if cpu_averages and all(value is not None for value in cpu_averages)
        else None,
        "healthy": bool(summaries) and not observer_errors and not alive_workers,
        "observer_errors": observer_errors,
        "alive_workers_after_stop": alive_workers,
        "sampling_method": "procfs_process_tree_cpu_ticks_smaps_rollup",
    }


async def _run_target(
    *,
    suite_dir: Path,
    target: str,
    binary: Path,
    fixture: AgentEpisodeFixtureServer,
    manifest: AgentEpisodeManifest,
    runs: int,
    workers: int,
    parallelism: int,
    step_dwell_ms: int,
    sample_interval_ms: int,
    timeout_seconds: float,
) -> dict[str, Any]:
    markers: list[dict[str, Any]] = []
    episode_rows: list[dict[str, Any]] = []
    step_rows: list[dict[str, Any]] = []
    details: list[dict[str, Any]] = []
    agent_workers: list[AgentWorker] = []
    stop_details: dict[str, Any] = {}
    target_started_epoch = time.time()
    _phase(markers, target=target, event="target-start")
    try:
        for worker_index in range(1, workers + 1):
            _phase(
                markers,
                target=target,
                worker=f"worker-{worker_index}",
                event="worker-start",
            )
            handle = start_target_serve(
                target,
                binary,
                timeout_seconds,
                sample_interval_seconds=sample_interval_ms / 1000.0,
            )
            try:
                client = await connect_routed_raw_cdp(handle.endpoint)
            except Exception as error:
                stop_target_serve(handle)
                raise TargetServeError(
                    f"failed to connect {target} {worker_index} browser frontend: {error}"
                ) from error
            worker = AgentWorker(
                index=worker_index,
                handle=handle,
                client=client,
            )
            agent_workers.append(worker)
            _phase(
                markers,
                target=target,
                worker=worker.label,
                event="worker-ready",
            )

        jobs: asyncio.Queue[tuple[int, int, dict[str, Any]]] = asyncio.Queue()
        job_index = 0
        for run_id in range(1, runs + 1):
            for episode in manifest.episodes:
                jobs.put_nowait((job_index, run_id, episode))
                job_index += 1

        async def run_slot() -> None:
            while True:
                try:
                    index, run_id, episode = jobs.get_nowait()
                except asyncio.QueueEmpty:
                    return
                worker = agent_workers[index % len(agent_workers)]
                try:
                    row, steps, detail = await _run_episode(
                        suite_dir=suite_dir,
                        target=target,
                        worker=worker,
                        run_id=run_id,
                        episode=episode,
                        fixture=fixture,
                        step_dwell_ms=step_dwell_ms,
                        timeout_seconds=timeout_seconds,
                        markers=markers,
                    )
                    episode_rows.append(row)
                    step_rows.extend(steps)
                    details.append(detail)
                finally:
                    jobs.task_done()

        await asyncio.gather(
            *(run_slot() for _ in range(min(parallelism, max(1, jobs.qsize()))))
        )
    finally:
        _phase(markers, target=target, event="target-shutdown-start")
        for worker in agent_workers:
            with contextlib.suppress(Exception):
                await worker.client.close()
            stop_details[worker.label] = stop_target_serve(worker.handle)
            _phase(
                markers,
                target=target,
                worker=worker.label,
                event="worker-stopped",
            )
        _phase(markers, target=target, event="target-done")

    worker_samples = {
        worker.label: _normalize_worker_samples(worker, target_started_epoch)
        for worker in agent_workers
    }
    aggregate_samples = _aggregate_resource_samples(worker_samples)
    resource_summary = _resource_summary(aggregate_samples)
    sampler_health = _sampler_health(agent_workers, stop_details)
    resource_summary["average_cpu_percent"] = sampler_health[
        "combined_average_cpu_percent"
    ]
    resource_summary["sampler_health"] = sampler_health
    return {
        "episode_rows": episode_rows,
        "step_rows": step_rows,
        "details": details,
        "markers": markers,
        "resources": {
            "summary": resource_summary,
            "samples": aggregate_samples,
            "workers": {
                worker.label: {
                    "root_pid": worker.handle.process.pid,
                    "ready_ms": worker.handle.ready_ms,
                    "summary": stop_details.get(worker.label, {}).get("resources", {}),
                    "samples": worker_samples[worker.label],
                }
                for worker in agent_workers
            },
        },
        "serve": {
            worker.label: {
                "command": worker.handle.command,
                "ready_ms": worker.handle.ready_ms,
                **stop_details.get(worker.label, {}),
            }
            for worker in agent_workers
        },
    }


def _unavailable_rows(
    *,
    target: str,
    manifest: AgentEpisodeManifest,
    runs: int,
    error: str,
    status: str = "target_unavailable",
) -> list[dict[str, Any]]:
    return [
        {
            "target": target,
            **target_metadata(target),
            "worker": None,
            "run": run_id,
            "episode": str(episode["id"]),
            "ok": False,
            "status": status,
            "elapsed_ms": None,
            "active_elapsed_ms": None,
            "setup_elapsed_ms": None,
            "context_create_ms": None,
            "target_create_ms": None,
            "attach_ms": None,
            "page_enable_ms": None,
            "runtime_enable_ms": None,
            "initial_navigate_response_ms": None,
            "dwell_ms": 0,
            "step_count": 0,
            "expected_step_count": len(episode["steps"]),
            "failure_step": 0,
            "error": error,
            "cdp_error_code": None,
            "cdp_error_message": None,
            "target_id": None,
            "browser_context_id": None,
            "participant_count": 0,
            "final_url": None,
            "visible_text_chars": None,
            "control_count": None,
            "failure_artifact": None,
        }
        for run_id in range(1, runs + 1)
        for episode in manifest.episodes
    ]


def _exact_error_counts(
    step_rows: list[dict[str, Any]],
    episode_rows: list[dict[str, Any]] = (),
) -> dict[str, int]:
    counts: collections.Counter[str] = collections.Counter()
    rows = list(step_rows) + [
        row for row in episode_rows if row.get("failure_step") == 0
    ]
    for row in rows:
        message = row.get("cdp_error_message")
        if isinstance(message, str) and message:
            counts[message] += 1
        elif not row.get("ok"):
            counts[str(row.get("status") or "unknown")] += 1
    return dict(sorted(counts.items()))


def _known_error_counts(
    step_rows: list[dict[str, Any]],
    episode_rows: list[dict[str, Any]],
) -> dict[str, int]:
    exact = _exact_error_counts(step_rows, episode_rows)
    rows = list(step_rows) + [
        row for row in episode_rows if row.get("failure_step") == 0
    ]
    messages = [
        str(row.get("cdp_error_message") or "")
        for row in rows
    ]
    statuses = [str(row.get("status") or "") for row in rows]
    result = {message: exact.get(message, 0) for message in KNOWN_CDP_ERROR_MESSAGES}
    result["target/session closed/detached"] = sum(
        1
        for message in messages
        if any(token in message.lower() for token in ("target closed", "session closed", "detached"))
    )
    result["command timeout"] = sum(
        1 for status in statuses if status.startswith("timeout")
    )
    return result


def _build_summary(
    *,
    manifest: AgentEpisodeManifest,
    targets: tuple[str, ...],
    episode_rows: list[dict[str, Any]],
    step_rows: list[dict[str, Any]],
    resources: dict[str, Any],
    runs: int,
    workers: int,
    parallelism: int,
    step_dwell_ms: int,
    sample_interval_ms: int,
    timeout_seconds: float,
) -> dict[str, Any]:
    summary: dict[str, Any] = {
        "suite": "agent-episode",
        "schema": AGENT_EPISODE_REPORT_SCHEMA,
        "manifest_sha256": manifest.sha256,
        "fixture_version": manifest.fixture_version,
        "runs": runs,
        "workers": workers,
        "parallelism": parallelism,
        "step_dwell_ms": step_dwell_ms,
        "sample_interval_ms": sample_interval_ms,
        "timeout_seconds": timeout_seconds,
        "cases": [str(episode["id"]) for episode in manifest.episodes],
        "targets": {},
        "episodes_total": len(episode_rows),
        "episodes_passed": sum(1 for row in episode_rows if row.get("ok")),
        "total_failures": sum(1 for row in episode_rows if not row.get("ok")),
        "steps_total": len(step_rows),
        "assertions_total": sum(
            int(row.get("assertions_total") or 0) for row in step_rows
        ),
        "assertions_passed": sum(
            int(row.get("assertions_passed") or 0) for row in step_rows
        ),
        "exact_errors": _exact_error_counts(step_rows, episode_rows),
        "known_error_counts": _known_error_counts(step_rows, episode_rows),
    }
    summary["gate_failures"] = summary["total_failures"]
    for target in targets:
        target_episodes = [row for row in episode_rows if row["target"] == target]
        target_steps = [row for row in step_rows if row["target"] == target]
        operation_summaries = {}
        for operation in sorted(ALLOWED_OPERATIONS):
            values = [
                float(row["elapsed_ms"])
                for row in target_steps
                if row.get("operation") == operation
                and row.get("ok")
                and row.get("elapsed_ms") is not None
            ]
            operation_summaries[operation] = summarize(values)
        episode_summaries = {}
        for episode in manifest.episodes:
            episode_id = str(episode["id"])
            rows = [row for row in target_episodes if row["episode"] == episode_id]
            episode_summaries[episode_id] = {
                "failures": sum(1 for row in rows if not row.get("ok")),
                "elapsed_ms": summarize(
                    float(row["elapsed_ms"])
                    for row in rows
                    if row.get("ok") and row.get("elapsed_ms") is not None
                ),
                "active_elapsed_ms": summarize(
                    float(row["active_elapsed_ms"])
                    for row in rows
                    if row.get("ok") and row.get("active_elapsed_ms") is not None
                ),
            }
        summary["targets"][target] = {
            **target_metadata(target),
            "episodes": len(target_episodes),
            "passed": sum(1 for row in target_episodes if row.get("ok")),
            "failures": sum(1 for row in target_episodes if not row.get("ok")),
            "status_counts": dict(
                sorted(collections.Counter(str(row["status"]) for row in target_episodes).items())
            ),
            "step_status_counts": dict(
                sorted(collections.Counter(str(row["status"]) for row in target_steps).items())
            ),
            "assertions_total": sum(
                int(row.get("assertions_total") or 0) for row in target_steps
            ),
            "assertions_passed": sum(
                int(row.get("assertions_passed") or 0) for row in target_steps
            ),
            "exact_errors": _exact_error_counts(target_steps, target_episodes),
            "known_error_counts": _known_error_counts(
                target_steps,
                target_episodes,
            ),
            "elapsed_ms": summarize(
                float(row["elapsed_ms"])
                for row in target_episodes
                if row.get("ok") and row.get("elapsed_ms") is not None
            ),
            "active_elapsed_ms": summarize(
                float(row["active_elapsed_ms"])
                for row in target_episodes
                if row.get("ok") and row.get("active_elapsed_ms") is not None
            ),
            "operations": operation_summaries,
            "cases": episode_summaries,
            "resources": resources.get(target, {}).get("summary", {}),
            "ready_ms": summarize(
                float(worker["ready_ms"])
                for worker in resources.get(target, {}).get("workers", {}).values()
                if worker.get("ready_ms") is not None
            ),
        }
    return summary


def _resource_csv_rows(resources: dict[str, Any]) -> list[dict[str, Any]]:
    rows = []
    for target, payload in resources.items():
        for sample in payload.get("samples", []):
            rows.append({"target": target, "scope": "service", "worker": None, **sample})
        for worker, worker_payload in payload.get("workers", {}).items():
            for sample in worker_payload.get("samples", []):
                rows.append(
                    {
                        "target": target,
                        "scope": "worker",
                        "worker": worker,
                        **sample,
                    }
                )
    return rows


def _event_log(markers: list[dict[str, Any]]) -> str:
    return "".join(
        json.dumps(marker, sort_keys=True, ensure_ascii=False) + "\n"
        for marker in sorted(markers, key=lambda marker: float(marker["timestamp"]))
    )


def _normalized_launch_command(command: list[str]) -> list[str]:
    normalized: list[str] = []
    skip_dynamic_value = False
    for index, argument in enumerate(command):
        if index == 0:
            normalized.append("<binary>")
            continue
        if skip_dynamic_value:
            normalized.append("<dynamic>")
            skip_dynamic_value = False
            continue
        if argument in {"--port"}:
            normalized.append(argument)
            skip_dynamic_value = True
            continue
        if argument.startswith("--remote-debugging-port="):
            normalized.append("--remote-debugging-port=<dynamic>")
            continue
        if argument.startswith("--user-data-dir="):
            normalized.append("--user-data-dir=<dynamic>")
            continue
        normalized.append(argument)
    return normalized


def run_agent_episode_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...] = AGENT_EPISODE_TARGETS,
    runs: int = 1,
    workers: int = 1,
    parallelism: int = 1,
    step_dwell_ms: int = 14_000,
    sample_interval_ms: int = 500,
    timeout_seconds: float = 30.0,
    manifest_path: Path = DEFAULT_MANIFEST_PATH,
) -> dict[str, Any]:
    from .agent_episode_report import write_agent_episode_report

    targets = tuple(dict.fromkeys(targets))
    unknown_targets = [target for target in targets if target not in AGENT_EPISODE_TARGETS]
    if unknown_targets:
        raise AgentEpisodeError(
            f"unsupported agent episode target(s): {', '.join(unknown_targets)}"
        )
    if not targets:
        raise AgentEpisodeError("at least one agent episode target is required")
    if runs <= 0 or workers <= 0 or parallelism <= 0:
        raise AgentEpisodeError("runs, workers, and parallelism must be positive")
    if step_dwell_ms < 0:
        raise AgentEpisodeError("step dwell must not be negative")
    if sample_interval_ms < 100:
        raise AgentEpisodeError("sample interval must be at least 100ms")
    if timeout_seconds <= 0:
        raise AgentEpisodeError("timeout must be positive")

    manifest = load_agent_episode_manifest(manifest_path)
    suite_dir = output_dir / "agent-episode"
    episode_rows: list[dict[str, Any]] = []
    step_rows: list[dict[str, Any]] = []
    details: list[dict[str, Any]] = []
    markers: list[dict[str, Any]] = []
    resources: dict[str, Any] = {}
    serve: dict[str, Any] = {}
    target_order: list[str] = []

    with AgentEpisodeFixtureServer() as fixture:
        for target in targets:
            target_order.append(target)
            metadata = target_metadata(target)
            binary_info = target_matrix.get(metadata["binary_key"], {})
            binary_path = binary_info.get("path")
            if not binary_info.get("available") or not binary_path:
                unavailable = _unavailable_rows(
                    target=target,
                    manifest=manifest,
                    runs=runs,
                    error="target binary unavailable",
                )
                episode_rows.extend(unavailable)
                details.extend(unavailable)
                resources[target] = {"summary": {}, "samples": [], "workers": {}}
                continue
            try:
                result = asyncio.run(
                    _run_target(
                        suite_dir=suite_dir,
                        target=target,
                        binary=Path(binary_path),
                        fixture=fixture,
                        manifest=manifest,
                        runs=runs,
                        workers=workers,
                        parallelism=parallelism,
                        step_dwell_ms=step_dwell_ms,
                        sample_interval_ms=sample_interval_ms,
                        timeout_seconds=timeout_seconds,
                    )
                )
            except TargetServeError as error:
                status = (
                    "process_exit"
                    if isinstance(error, TargetServeProcessExit)
                    else "target_unavailable"
                )
                artifact_path = suite_dir / "failures" / f"{target}-startup.json"
                startup_detail = {
                    "target": target,
                    "status": status,
                    "error": str(error),
                    "exception_type": type(error).__name__,
                }
                write_json(artifact_path, startup_detail)
                failed_rows = _unavailable_rows(
                    target=target,
                    manifest=manifest,
                    runs=runs,
                    error=str(error),
                    status=status,
                )
                for row in failed_rows:
                    row["failure_artifact"] = str(
                        artifact_path.relative_to(suite_dir)
                    )
                episode_rows.extend(failed_rows)
                details.extend({**startup_detail, **row} for row in failed_rows)
                resources[target] = {"summary": {}, "samples": [], "workers": {}}
                serve[target] = {"status": status, "error": str(error)}
                markers.append(
                    {
                        "timestamp": time.time(),
                        "target": target,
                        "event": "target-start-failed",
                        "worker": None,
                        "run": None,
                        "episode": None,
                        "step": None,
                        "operation": None,
                    }
                )
                continue
            episode_rows.extend(result["episode_rows"])
            step_rows.extend(result["step_rows"])
            details.extend(result["details"])
            markers.extend(result["markers"])
            resources[target] = result["resources"]
            serve[target] = result["serve"]

        fixture_requests = fixture.requests

    episode_rows.sort(key=lambda row: (targets.index(str(row["target"])), int(row["run"]), str(row["episode"])))
    step_rows.sort(
        key=lambda row: (
            targets.index(str(row["target"])),
            int(row["run"]),
            str(row["episode"]),
            int(row["step"]),
        )
    )
    summary = _build_summary(
        manifest=manifest,
        targets=targets,
        episode_rows=episode_rows,
        step_rows=step_rows,
        resources=resources,
        runs=runs,
        workers=workers,
        parallelism=parallelism,
        step_dwell_ms=step_dwell_ms,
        sample_interval_ms=sample_interval_ms,
        timeout_seconds=timeout_seconds,
    )
    summary["target_order"] = target_order
    launch_contracts = {
        target: [
            _normalized_launch_command(list(worker.get("command", [])))
            for worker in target_serve.values()
            if isinstance(worker, dict) and worker.get("command")
        ]
        for target, target_serve in serve.items()
        if isinstance(target_serve, dict)
    }
    config = {
        "targets": list(targets),
        "target_order": target_order,
        "runs": runs,
        "workers": workers,
        "parallelism": parallelism,
        "step_dwell_ms": step_dwell_ms,
        "sample_interval_ms": sample_interval_ms,
        "timeout_seconds": timeout_seconds,
        "manifest_path": str(manifest.path),
        "manifest_sha256": manifest.sha256,
        "fixture_version": manifest.fixture_version,
        "launch_contracts": launch_contracts,
    }
    write_csv(suite_dir / "runs.csv", episode_rows)
    write_json(suite_dir / "runs.json", details)
    write_csv(suite_dir / "steps.csv", step_rows)
    write_json(suite_dir / "steps.json", step_rows)
    write_json(suite_dir / "summary.json", summary)
    write_json(suite_dir / "resource-samples.json", resources)
    write_csv(suite_dir / "resource-samples.csv", _resource_csv_rows(resources))
    write_json(suite_dir / "phase-markers.json", markers)
    write_text(suite_dir / "events.log", _event_log(markers))
    write_json(suite_dir / "fixture-requests.json", fixture_requests)
    write_json(
        suite_dir / "versions.json",
        {
            "targets": {target: target_matrix.get(target_metadata(target)["binary_key"], {}) for target in targets},
            "manifest_sha256": manifest.sha256,
            "fixture_version": manifest.fixture_version,
        },
    )
    write_json(suite_dir / "serve.json", serve)
    write_agent_episode_report(
        suite_dir=suite_dir,
        summary=summary,
        episode_rows=episode_rows,
        step_rows=step_rows,
        resources=resources,
        markers=markers,
        config=config,
    )
    return summary
