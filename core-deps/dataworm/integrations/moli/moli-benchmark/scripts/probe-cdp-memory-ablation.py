#!/usr/bin/env python3
from __future__ import annotations

import argparse
import asyncio
import contextlib
import http.server
import json
import re
import threading
import time
import urllib.parse
from collections import Counter, defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from moli_benchmark.chrome_dcl import (
    CDP_LATE_ERROR_GRACE_SECONDS,
    _recv_command_response,
    _wait_for_cdp,
)
from moli_benchmark.config import REPO_ROOT
from moli_benchmark.sampling import _process_tree, snapshot_resources
from moli_benchmark.target_serve import start_target_serve, stop_target_serve


DEFAULT_URL = "https://www.ifeng.com/"
HEADER_RE = re.compile(r"^[0-9a-fA-F]+-[0-9a-fA-F]+\s+")
SMAPS_KEYS = (
    "Size",
    "Rss",
    "Pss",
    "Anonymous",
    "Private_Dirty",
    "Shared_Clean",
)


@dataclass(frozen=True)
class ProbeCase:
    name: str
    page: bool
    runtime: bool
    network: bool
    outer_html: bool
    idle: bool
    gc_after_dcl: bool = False
    gc_after_outer: bool = False
    gc_after_idle: bool = False
    reset_engine_after_close: bool = False


@dataclass(frozen=True)
class FixedPageTarget:
    index: int
    target_id: str
    session_id: str
    frame_id: str | None
    url: str
    browser_context_id: str | None = None


@dataclass
class FixedProbeFixtureServer:
    server: http.server.ThreadingHTTPServer
    thread: threading.Thread
    base_url: str

    def stop(self) -> None:
        self.server.shutdown()
        self.thread.join(timeout=5.0)
        self.server.server_close()


CASES = {
    "page": ProbeCase("page", page=True, runtime=False, network=False, outer_html=False, idle=False),
    "page-runtime": ProbeCase(
        "page-runtime",
        page=True,
        runtime=True,
        network=False,
        outer_html=False,
        idle=False,
    ),
    "page-network": ProbeCase(
        "page-network",
        page=True,
        runtime=False,
        network=True,
        outer_html=False,
        idle=False,
    ),
    "full": ProbeCase("full", page=True, runtime=True, network=True, outer_html=False, idle=False),
    "full-gc": ProbeCase(
        "full-gc",
        page=True,
        runtime=True,
        network=True,
        outer_html=False,
        idle=False,
        gc_after_dcl=True,
    ),
    "full-outer": ProbeCase("full-outer", page=True, runtime=True, network=True, outer_html=True, idle=False),
    "full-outer-gc": ProbeCase(
        "full-outer-gc",
        page=True,
        runtime=True,
        network=True,
        outer_html=True,
        idle=False,
        gc_after_outer=True,
    ),
    "full-idle": ProbeCase("full-idle", page=True, runtime=True, network=True, outer_html=True, idle=True),
    "full-idle-gc": ProbeCase(
        "full-idle-gc",
        page=True,
        runtime=True,
        network=True,
        outer_html=True,
        idle=True,
        gc_after_idle=True,
    ),
}

FIXED_PROBE_NONE = "none"
FIXED_PROBE_DATA_PAGES = "data-pages"
FIXED_PROBE_BACKGROUND_TARGETS = "background-targets"
FIXED_PROBE_POPUP_TARGETS = "popup-targets"
FIXED_PROBE_DIFFERENT_BROWSER_CONTEXTS = "different-browser-contexts"
FIXED_PROBE_DEDICATED_WORKER = "dedicated-worker"
FIXED_PROBE_SHARED_WORKER_SAME_KEY = "shared-worker-same-key"
FIXED_PROBE_SHARED_WORKER_DISTINCT_KEY = "shared-worker-distinct-key"
FIXED_PROBE_CHOICES = (
    FIXED_PROBE_NONE,
    FIXED_PROBE_DATA_PAGES,
    FIXED_PROBE_BACKGROUND_TARGETS,
    FIXED_PROBE_POPUP_TARGETS,
    FIXED_PROBE_DIFFERENT_BROWSER_CONTEXTS,
    FIXED_PROBE_DEDICATED_WORKER,
    FIXED_PROBE_SHARED_WORKER_SAME_KEY,
    FIXED_PROBE_SHARED_WORKER_DISTINCT_KEY,
)
FIXED_PROBE_WORKER_MODES = {
    FIXED_PROBE_DEDICATED_WORKER,
    FIXED_PROBE_SHARED_WORKER_SAME_KEY,
    FIXED_PROBE_SHARED_WORKER_DISTINCT_KEY,
}


def mib(value: int | float | None) -> float | None:
    if value is None:
        return None
    return value / 1024 / 1024


def mapping_category(path: str) -> str:
    if not path:
        return "anonymous_unnamed"
    if path == "[heap]":
        return "heap"
    if path.startswith("[stack"):
        return "thread_stacks"
    if path.startswith("[anon:"):
        lower = path.lower()
        if "v8" in lower:
            return "anon_v8_named"
        return "anonymous_named"
    if path.startswith("[vdso") or path.startswith("[vvar") or path.startswith("[vsyscall"):
        return "kernel_virtual"
    if path.startswith("/dev/zero") or path.startswith("memfd:") or path.startswith("/memfd:"):
        return "anonymous_memfd"
    base = Path(path).name
    lower = path.lower()
    if path.endswith("/target/release/moli") or base == "moli":
        return "moli_binary"
    if "libv8" in lower or "rusty_v8" in lower:
        return "v8_shared_library"
    if path.startswith("/usr/lib") or path.startswith("/lib") or ".so" in base:
        return "shared_libraries"
    if path.startswith("/"):
        return "file_backed_other"
    return "special_other"


def bucket_name(size: int) -> str:
    if size < 64 * 1024:
        return "<64K"
    if size < 256 * 1024:
        return "64K-256K"
    if size < 1024 * 1024:
        return "256K-1M"
    if size < 4 * 1024 * 1024:
        return "1M-4M"
    if size < 16 * 1024 * 1024:
        return "4M-16M"
    if size < 64 * 1024 * 1024:
        return "16M-64M"
    if size < 1024 * 1024 * 1024:
        return "64M-1G"
    return ">=1G"


def smaps_summary(root_pid: int) -> dict[str, Any]:
    categories: dict[str, dict[str, int]] = defaultdict(lambda: {key: 0 for key in SMAPS_KEYS})
    top_mappings: list[dict[str, Any]] = []
    anonymous_histogram: dict[str, dict[str, int]] = defaultdict(
        lambda: {"count": 0, "size": 0, "rss": 0, "pss": 0}
    )
    mapping_count = 0
    anonymous_mapping_count = 0
    for pid in _process_tree(root_pid):
        try:
            lines = Path(f"/proc/{pid}/smaps").read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        current: dict[str, Any] | None = None
        for line in lines:
            if HEADER_RE.match(line):
                if current is not None:
                    record_mapping(current, categories, top_mappings, anonymous_histogram)
                    mapping_count += 1
                    if current["category"] == "anonymous_unnamed":
                        anonymous_mapping_count += 1
                parts = line.split(None, 5)
                start_s, end_s = parts[0].split("-", 1)
                path = parts[5] if len(parts) >= 6 else ""
                current = {
                    "pid": pid,
                    "range": parts[0],
                    "start": int(start_s, 16),
                    "end": int(end_s, 16),
                    "perms": parts[1] if len(parts) > 1 else "",
                    "offset": parts[2] if len(parts) > 2 else "",
                    "dev": parts[3] if len(parts) > 3 else "",
                    "inode": parts[4] if len(parts) > 4 else "",
                    "path": path,
                    "category": mapping_category(path),
                    **{key: 0 for key in SMAPS_KEYS},
                }
                continue
            if current is None or ":" not in line:
                continue
            key, raw = line.split(":", 1)
            if key not in SMAPS_KEYS:
                continue
            pieces = raw.split()
            if not pieces:
                continue
            try:
                current[key] = int(pieces[0]) * 1024
            except ValueError:
                pass
        if current is not None:
            record_mapping(current, categories, top_mappings, anonymous_histogram)
            mapping_count += 1
            if current["category"] == "anonymous_unnamed":
                anonymous_mapping_count += 1
    return {
        "categories": {name: dict(values) for name, values in sorted(categories.items())},
        "anonymous_histogram_by_vma_size": dict(sorted(anonymous_histogram.items())),
        "top_mappings": sorted(top_mappings, key=lambda row: row.get("Pss", 0), reverse=True)[:40],
        "mapping_count": mapping_count,
        "anonymous_mapping_count": anonymous_mapping_count,
    }


def record_mapping(
    row: dict[str, Any],
    categories: dict[str, dict[str, int]],
    top_mappings: list[dict[str, Any]],
    anonymous_histogram: dict[str, dict[str, int]],
) -> None:
    bucket = categories[row["category"]]
    for key in SMAPS_KEYS:
        bucket[key] += int(row.get(key) or 0)
    top_mappings.append(
        {
            key: row[key]
            for key in (
                "pid",
                "range",
                "start",
                "end",
                "perms",
                "offset",
                "inode",
                "path",
                "category",
                *SMAPS_KEYS,
            )
        }
    )
    if row["category"] == "anonymous_unnamed":
        size_bucket = bucket_name(int(row.get("Size") or 0))
        histogram = anonymous_histogram[size_bucket]
        histogram["count"] += 1
        histogram["size"] += int(row.get("Size") or 0)
        histogram["rss"] += int(row.get("Rss") or 0)
        histogram["pss"] += int(row.get("Pss") or 0)


def snapshot(label: str, root_pid: int, extra: dict[str, Any] | None = None) -> dict[str, Any]:
    resources = snapshot_resources(root_pid)
    result = {
        "label": label,
        "timestamp": time.time(),
        "resources": resources,
        "smaps": smaps_summary(root_pid),
    }
    if extra is not None:
        result["extra"] = extra
    print(
        f"SNAP {label}: rss={mib(resources.get('rss_bytes')):.1f}MiB "
        f"pss={mib(resources.get('pss_bytes')):.1f}MiB "
        f"threads={resources.get('thread_count')}",
        flush=True,
    )
    return result


def timeline_sample(
    label: str,
    root_pid: int,
    elapsed_ms: float,
    counter: dict[str, Any],
    include_smaps: bool,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    resources = snapshot_resources(root_pid)
    result: dict[str, Any] = {
        "label": label,
        "elapsed_ms": elapsed_ms,
        "timestamp": time.time(),
        "rss_mib": round(mib(resources.get("rss_bytes")) or 0.0, 1),
        "pss_mib": round(mib(resources.get("pss_bytes")) or 0.0, 1),
        "thread_count": resources.get("thread_count"),
        "event_delta": serialize_counter(counter),
    }
    if include_smaps:
        summary = smaps_summary(root_pid)
        result["smaps_top_categories"] = compact_smaps_categories(summary)
        result["anonymous_histogram_by_vma_size"] = {
            name: {
                "count": values["count"],
                "pss_mib": round(values["pss"] / 1024 / 1024, 1),
                "rss_mib": round(values["rss"] / 1024 / 1024, 1),
                "size_mib": round(values["size"] / 1024 / 1024, 1),
            }
            for name, values in summary["anonymous_histogram_by_vma_size"].items()
        }
    if extra:
        result.update(extra)
    return result


def compact_heap_diagnostic(heap_result: dict[str, Any]) -> dict[str, Any]:
    heap = heap_result.get("response", {}) if isinstance(heap_result, dict) else {}
    if not isinstance(heap, dict):
        heap = {}
    result: dict[str, Any] = {
        "elapsed_ms": heap_result.get("elapsed_ms") if isinstance(heap_result, dict) else None,
        "seen_count": heap_result.get("seen_count") if isinstance(heap_result, dict) else None,
        "used_mib": round(mib(heap.get("usedSize")) or 0.0, 1),
        "total_mib": round(mib(heap.get("totalSize")) or 0.0, 1),
        "physical_mib": round(mib(heap.get("totalPhysicalSize")) or 0.0, 1),
        "malloced_mib": round(mib(heap.get("mallocedMemory")) or 0.0, 1),
        "external_mib": round(mib(heap.get("externalMemory")) or 0.0, 1),
        "native_contexts": heap.get("numberOfNativeContexts"),
        "detached_contexts": heap.get("numberOfDetachedContexts"),
    }
    if isinstance(heap_result, dict) and heap_result.get("error"):
        result["error"] = heap_result.get("error")
    spaces = heap.get("heapSpaces")
    if isinstance(spaces, list):
        result["top_spaces"] = sorted(
            [
                {
                    "name": str(space.get("name")),
                    "used_mib": round(mib(space.get("usedSize")) or 0.0, 1),
                    "physical_mib": round(mib(space.get("physicalSize")) or 0.0, 1),
                    "size_mib": round(mib(space.get("size")) or 0.0, 1),
                }
                for space in spaces
                if isinstance(space, dict)
            ],
            key=lambda item: (item["physical_mib"], item["used_mib"]),
            reverse=True,
        )[:8]
    moli = heap.get("moli")
    if isinstance(moli, dict):
        result["moli_counters"] = moli
    return result


def compact_moli_diagnostic(diagnostics_result: dict[str, Any]) -> dict[str, Any]:
    response = diagnostics_result.get("response", {}) if isinstance(diagnostics_result, dict) else {}
    if not isinstance(response, dict):
        response = {}
    result: dict[str, Any] = {
        "elapsed_ms": diagnostics_result.get("elapsed_ms") if isinstance(diagnostics_result, dict) else None,
        "seen_count": diagnostics_result.get("seen_count") if isinstance(diagnostics_result, dict) else None,
        "response": response,
    }
    if isinstance(diagnostics_result, dict) and diagnostics_result.get("error"):
        result["error"] = diagnostics_result.get("error")
    return result


def compact_smaps_categories(summary: dict[str, Any]) -> list[dict[str, Any]]:
    categories = []
    for name, values in summary["categories"].items():
        pss = values.get("Pss", 0)
        if not pss:
            continue
        categories.append(
            {
                "category": name,
                "pss_mib": round(pss / 1024 / 1024, 1),
                "rss_mib": round(values.get("Rss", 0) / 1024 / 1024, 1),
                "anonymous_mib": round(values.get("Anonymous", 0) / 1024 / 1024, 1),
            }
        )
    return sorted(categories, key=lambda item: item["pss_mib"], reverse=True)[:5]


def account_messages(counter: dict[str, Any], messages: list[dict[str, Any]]) -> None:
    for message in messages:
        method = message.get("method")
        if isinstance(method, str):
            counter["methods"][method] += 1
        params = message.get("params") if isinstance(message.get("params"), dict) else {}
        if method == "Network.responseReceived":
            resource_type = str(params.get("type"))
            counter["resource_types"][resource_type] += 1
            response = params.get("response") if isinstance(params.get("response"), dict) else {}
            url = response.get("url")
            if isinstance(url, str) and len(counter["urls_by_type"][resource_type]) < 12:
                counter["urls_by_type"][resource_type].append(url)
        elif method == "Network.loadingFinished":
            counter["loading_finished_count"] += 1
            encoded = params.get("encodedDataLength")
            if isinstance(encoded, (int, float)):
                counter["encoded_data_length_total"] += int(encoded)
        elif method == "Network.loadingFailed":
            counter["loading_failed_errors"][str(params.get("errorText"))] += 1


def new_counter() -> dict[str, Any]:
    return {
        "methods": Counter(),
        "resource_types": Counter(),
        "urls_by_type": defaultdict(list),
        "loading_failed_errors": Counter(),
        "loading_finished_count": 0,
        "encoded_data_length_total": 0,
    }


def serialize_counter(counter: dict[str, Any]) -> dict[str, Any]:
    return {
        "methods": dict(counter["methods"]),
        "resource_types": dict(counter["resource_types"]),
        "urls_by_type": {key: list(value) for key, value in counter["urls_by_type"].items()},
        "loading_failed_errors": dict(counter["loading_failed_errors"]),
        "loading_finished_count": counter["loading_finished_count"],
        "encoded_data_length_total": counter["encoded_data_length_total"],
    }


def is_dcl_event(message: dict[str, Any], session_id: str, frame_id: str | None) -> bool:
    if message.get("sessionId") != session_id:
        return False
    if message.get("method") == "Page.domContentEventFired":
        return True
    if message.get("method") != "Page.lifecycleEvent":
        return False
    params = message.get("params") if isinstance(message.get("params"), dict) else {}
    if frame_id is not None and params.get("frameId") != frame_id:
        return False
    return params.get("name") in {"DOMContentLoaded", "domContentLoaded"}


async def recv_until_dcl(
    client: Any,
    session_id: str,
    frame_id: str | None,
    deadline: float,
    seen: list[dict[str, Any]],
    counter: dict[str, Any],
) -> int:
    message_count = 0
    account_messages(counter, seen)
    if any(is_dcl_event(message, session_id, frame_id) for message in seen):
        return len(seen)
    while True:
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            raise TimeoutError("timed out waiting for DCL")
        message = await asyncio.wait_for(client.recv(), timeout=remaining)
        message_count += 1
        account_messages(counter, [message])
        if is_dcl_event(message, session_id, frame_id):
            return message_count


async def drain_for(client: Any, seconds: float, counter: dict[str, Any]) -> int:
    deadline = time.perf_counter() + seconds
    message_count = 0
    while True:
        remaining = deadline - time.perf_counter()
        if remaining <= 0:
            return message_count
        try:
            message = await asyncio.wait_for(client.recv(), timeout=min(0.1, remaining))
        except TimeoutError:
            continue
        message_count += 1
        account_messages(counter, [message])


async def drain_for_timeline(
    client: Any,
    seconds: float,
    counter: dict[str, Any],
    root_pid: int,
    interval: float,
    include_smaps: bool,
    *,
    session_id: str | None = None,
    include_runtime: bool = False,
    include_diagnostics: bool = False,
    command_deadline: float | None = None,
) -> tuple[int, list[dict[str, Any]]]:
    started = time.perf_counter()
    deadline = started + seconds
    next_sample = started
    message_count = 0
    samples: list[dict[str, Any]] = []
    slice_counter = new_counter()

    while True:
        now = time.perf_counter()
        if now >= next_sample:
            extra: dict[str, Any] = {}
            cdp_deadline = command_deadline if command_deadline is not None else time.perf_counter() + 5.0
            if include_runtime and session_id is not None:
                extra["runtime_heap"] = compact_heap_diagnostic(
                    await runtime_heap_usage(client, session_id, cdp_deadline, slice_counter)
                )
            if include_diagnostics:
                extra["moli_diagnostics"] = compact_moli_diagnostic(
                    await moli_diagnostics(client, cdp_deadline, slice_counter)
                )
            samples.append(
                timeline_sample(
                    "idle_timeline",
                    root_pid,
                    (now - started) * 1000.0,
                    slice_counter,
                    include_smaps,
                    extra,
                )
            )
            slice_counter = new_counter()
            next_sample = now + interval

        remaining = deadline - now
        if remaining <= 0:
            break
        timeout = min(0.05, remaining, max(0.001, next_sample - now))
        try:
            message = await asyncio.wait_for(client.recv(), timeout=timeout)
        except TimeoutError:
            continue
        message_count += 1
        account_messages(counter, [message])
        account_messages(slice_counter, [message])

    extra = {}
    cdp_deadline = command_deadline if command_deadline is not None else time.perf_counter() + 5.0
    if include_runtime and session_id is not None:
        extra["runtime_heap"] = compact_heap_diagnostic(
            await runtime_heap_usage(client, session_id, cdp_deadline, slice_counter)
        )
    if include_diagnostics:
        extra["moli_diagnostics"] = compact_moli_diagnostic(
            await moli_diagnostics(client, cdp_deadline, slice_counter)
        )
    samples.append(
        timeline_sample(
            "idle_timeline_final",
            root_pid,
            (time.perf_counter() - started) * 1000.0,
            slice_counter,
            include_smaps,
            extra,
        )
    )
    return message_count, samples


async def runtime_heap_usage(
    client: Any,
    session_id: str,
    deadline: float,
    counter: dict[str, Any] | None = None,
) -> dict[str, Any]:
    started = time.perf_counter()
    message_id = await client.send("Runtime.getHeapUsage", session_id=session_id)
    try:
        response, seen = await _recv_command_response(
            client,
            message_id,
            deadline=deadline,
            stage="Runtime.getHeapUsage",
        )
    except Exception as error:  # noqa: BLE001 - optional cross-engine diagnostic.
        return {
            "elapsed_ms": (time.perf_counter() - started) * 1000.0,
            "response": {},
            "seen_count": 0,
            "error": repr(error),
        }
    if counter is not None:
        account_messages(counter, seen)
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "response": response.get("result", {}),
        "seen_count": len(seen),
    }


async def target_infos(
    client: Any,
    deadline: float,
    counter: dict[str, Any] | None = None,
) -> dict[str, Any]:
    started = time.perf_counter()
    message_id = await client.send("Target.getTargets")
    try:
        response, seen = await _recv_command_response(
            client,
            message_id,
            deadline=deadline,
            stage="Target.getTargets",
        )
    except Exception as error:  # noqa: BLE001 - optional diagnostic.
        return {
            "elapsed_ms": (time.perf_counter() - started) * 1000.0,
            "targetInfos": [],
            "targetCount": 0,
            "attachedCount": 0,
            "pageCount": 0,
            "seen_count": 0,
            "error": repr(error),
        }
    if counter is not None:
        account_messages(counter, seen)
    infos = response.get("result", {}).get("targetInfos", [])
    if not isinstance(infos, list):
        infos = []
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "targetInfos": infos,
        "targetCount": len(infos),
        "attachedCount": sum(1 for info in infos if isinstance(info, dict) and info.get("attached")),
        "pageCount": sum(1 for info in infos if isinstance(info, dict) and info.get("type") == "page"),
        "seen_count": len(seen),
    }


async def heap_profiler_collect_garbage(
    client: Any,
    session_id: str,
    deadline: float,
    repeat: int,
    counter: dict[str, Any] | None = None,
) -> dict[str, Any]:
    started = time.perf_counter()
    seen_count = 0
    results: list[dict[str, Any]] = []
    for index in range(repeat):
        message_id = await client.send("HeapProfiler.collectGarbage", session_id=session_id)
        response, seen = await _recv_command_response(
            client,
            message_id,
            deadline=deadline,
            stage=f"HeapProfiler.collectGarbage[{index + 1}]",
        )
        if counter is not None:
            account_messages(counter, seen)
        seen_count += len(seen)
        results.append(response.get("result", {}))
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "repeat": repeat,
        "results": results,
        "seen_count": seen_count,
    }


async def moli_diagnostics(
    client: Any,
    deadline: float,
    counter: dict[str, Any] | None = None,
) -> dict[str, Any]:
    started = time.perf_counter()
    message_id = await client.send("HeapProfiler.moliDiagnostics")
    try:
        response, seen = await _recv_command_response(
            client,
            message_id,
            deadline=deadline,
            stage="HeapProfiler.moliDiagnostics",
        )
    except Exception as error:  # noqa: BLE001 - non-Moli targets may not expose this probe.
        return {
            "elapsed_ms": (time.perf_counter() - started) * 1000.0,
            "response": {},
            "seen_count": 0,
            "error": repr(error),
        }
    if counter is not None:
        account_messages(counter, seen)
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "response": response.get("result", {}),
        "seen_count": len(seen),
    }


async def moli_reset_idle_engine(
    client: Any,
    deadline: float,
    counter: dict[str, Any] | None = None,
) -> dict[str, Any]:
    started = time.perf_counter()
    message_id = await client.send("HeapProfiler.moliResetIdleEngine")
    try:
        response, seen = await _recv_command_response(
            client,
            message_id,
            deadline=deadline,
            stage="HeapProfiler.moliResetIdleEngine",
        )
    except Exception as error:  # noqa: BLE001 - diagnostic probe should preserve failed state.
        return {
            "elapsed_ms": (time.perf_counter() - started) * 1000.0,
            "response": {},
            "seen_count": 0,
            "error": repr(error),
        }
    if counter is not None:
        account_messages(counter, seen)
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "response": response.get("result", {}),
        "seen_count": len(seen),
    }


async def runtime_evaluate_json(
    client: Any,
    session_id: str,
    expression: str,
    deadline: float,
    stage: str,
) -> dict[str, Any]:
    started = time.perf_counter()
    message_id = await client.send(
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True},
        session_id=session_id,
    )
    response, seen = await _recv_command_response(client, message_id, deadline=deadline, stage=stage)
    value = response.get("result", {}).get("result", {}).get("value")
    try:
        parsed = json.loads(value) if isinstance(value, str) else value
    except json.JSONDecodeError:
        parsed = value
    return {
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "value": parsed,
        "seen_count": len(seen),
    }


async def enable_fixed_page_domains(
    client: Any,
    session_id: str,
    deadline: float,
    counter: dict[str, Any],
    *,
    runtime: bool,
    network: bool,
) -> None:
    for method in ("Page.enable", "Runtime.enable" if runtime else None, "Network.enable" if network else None):
        if method is None:
            continue
        message_id = await client.send(method, session_id=session_id)
        _, seen = await _recv_command_response(client, message_id, deadline=deadline, stage=method)
        account_messages(counter, seen)
    lifecycle_id = await client.send(
        "Page.setLifecycleEventsEnabled",
        {"enabled": True},
        session_id=session_id,
    )
    _, seen = await _recv_command_response(
        client,
        lifecycle_id,
        deadline=deadline,
        stage="Page.setLifecycleEventsEnabled",
    )
    account_messages(counter, seen)


async def create_fixed_browser_context(
    client: Any,
    deadline: float,
    counter: dict[str, Any],
) -> str:
    create_id = await client.send("Target.createBrowserContext")
    create_response, seen = await _recv_command_response(
        client,
        create_id,
        deadline=deadline,
        stage="Target.createBrowserContext",
    )
    account_messages(counter, seen)
    return str(create_response["result"]["browserContextId"])


async def dispose_fixed_browser_contexts(
    client: Any,
    browser_context_ids: list[str],
    deadline: float,
    counter: dict[str, Any],
) -> None:
    for browser_context_id in reversed(browser_context_ids):
        dispose_id = await client.send(
            "Target.disposeBrowserContext",
            {"browserContextId": browser_context_id},
        )
        _, seen = await _recv_command_response(
            client,
            dispose_id,
            deadline=deadline,
            stage="Target.disposeBrowserContext",
        )
        account_messages(counter, seen)


async def create_attached_target(
    client: Any,
    deadline: float,
    counter: dict[str, Any],
    *,
    index: int,
    runtime: bool,
    network: bool,
    browser_context_id: str | None = None,
) -> FixedPageTarget:
    create_params = {"url": "about:blank"}
    if browser_context_id is not None:
        create_params["browserContextId"] = browser_context_id
    create_id = await client.send("Target.createTarget", create_params)
    create_response, seen = await _recv_command_response(
        client,
        create_id,
        deadline=deadline,
        stage="Target.createTarget",
    )
    account_messages(counter, seen)
    target_id = str(create_response["result"]["targetId"])

    attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
    attach_response, seen = await _recv_command_response(
        client,
        attach_id,
        deadline=deadline,
        stage="Target.attachToTarget",
    )
    account_messages(counter, seen)
    session_id = str(attach_response["result"]["sessionId"])
    await enable_fixed_page_domains(
        client,
        session_id,
        deadline,
        counter,
        runtime=runtime,
        network=network,
    )
    return FixedPageTarget(
        index=index,
        target_id=target_id,
        session_id=session_id,
        frame_id=None,
        url="about:blank",
        browser_context_id=browser_context_id,
    )


def page_target_ids(targets: dict[str, Any]) -> set[str]:
    infos = targets.get("targetInfos", [])
    if not isinstance(infos, list):
        return set()
    return {
        str(info["targetId"])
        for info in infos
        if isinstance(info, dict) and info.get("type") == "page" and info.get("targetId") is not None
    }


async def wait_for_popup_target_id(
    client: Any,
    before_target_ids: set[str],
    opener_target_id: str,
    deadline: float,
    counter: dict[str, Any],
) -> str:
    last_targets: dict[str, Any] = {}
    while True:
        targets = await target_infos(client, deadline, counter)
        last_targets = targets
        infos = targets.get("targetInfos", [])
        if isinstance(infos, list):
            for info in infos:
                if not isinstance(info, dict):
                    continue
                target_id = info.get("targetId")
                if (
                    target_id is not None
                    and str(target_id) not in before_target_ids
                    and info.get("type") == "page"
                    and info.get("openerId") == opener_target_id
                ):
                    return str(target_id)
        if time.perf_counter() >= deadline:
            raise TimeoutError(
                "popup target was not created before the deadline: "
                f"opener={opener_target_id!r} targets={last_targets!r}"
            )
        await asyncio.sleep(0.02)


async def create_popup_target(
    client: Any,
    opener: FixedPageTarget,
    url: str,
    deadline: float,
    counter: dict[str, Any],
    *,
    index: int,
    runtime: bool,
    network: bool,
) -> FixedPageTarget:
    before = page_target_ids(await target_infos(client, deadline, counter))
    url_literal = json.dumps(url)
    target_literal = json.dumps("_blank")
    expression = (
        "(() => {"
        f"const popup = window.open({url_literal}, {target_literal});"
        "return popup ? 'opened' : 'blocked';"
        "})()"
    )
    open_id = await client.send(
        "Runtime.evaluate",
        {"expression": expression, "returnByValue": True},
        session_id=opener.session_id,
    )
    response, seen = await _recv_command_response(
        client,
        open_id,
        deadline=deadline,
        stage=f"Runtime.evaluate(window.open)[{index}]",
    )
    account_messages(counter, seen)
    opened = response.get("result", {}).get("result", {}).get("value")
    if opened != "opened":
        raise RuntimeError(f"window.open did not create a popup target: {response!r}")

    target_id = await wait_for_popup_target_id(
        client,
        before,
        opener.target_id,
        deadline,
        counter,
    )
    attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
    attach_response, seen = await _recv_command_response(
        client,
        attach_id,
        deadline=deadline,
        stage="Target.attachToTarget(popup)",
    )
    account_messages(counter, seen)
    session_id = str(attach_response["result"]["sessionId"])
    await enable_fixed_page_domains(
        client,
        session_id,
        deadline,
        counter,
        runtime=runtime,
        network=network,
    )
    return FixedPageTarget(
        index=index,
        target_id=target_id,
        session_id=session_id,
        frame_id=None,
        url=url,
        browser_context_id=opener.browser_context_id,
    )


async def navigate_fixed_target(
    client: Any,
    target: FixedPageTarget,
    url: str,
    deadline: float,
    counter: dict[str, Any],
) -> FixedPageTarget:
    navigate_id = await client.send("Page.navigate", {"url": url}, session_id=target.session_id)
    navigate_response, seen = await _recv_command_response(
        client,
        navigate_id,
        deadline=deadline,
        stage="Page.navigate",
        late_error_grace_seconds=CDP_LATE_ERROR_GRACE_SECONDS,
    )
    frame_id = navigate_response.get("result", {}).get("frameId")
    frame_id = str(frame_id) if frame_id is not None else None
    await recv_until_dcl(client, target.session_id, frame_id, deadline, seen, counter)
    return FixedPageTarget(
        index=target.index,
        target_id=target.target_id,
        session_id=target.session_id,
        frame_id=frame_id,
        url=url,
        browser_context_id=target.browser_context_id,
    )


def fixed_probe_worker_source() -> str:
    return """
let connectionCount = 0;
onconnect = event => {
  connectionCount += 1;
  const port = event.ports[0];
  port.postMessage("ready:" + connectionCount);
};
"""


def fixed_probe_dedicated_worker_source() -> str:
    return """
postMessage("ready");
self.onmessage = () => {};
setInterval(() => {}, 1000);
"""


def fixed_probe_worker_name(probe: str, index: int) -> str:
    if probe == FIXED_PROBE_SHARED_WORKER_SAME_KEY:
        return "fixed-shared-worker"
    if probe == FIXED_PROBE_SHARED_WORKER_DISTINCT_KEY:
        return f"fixed-shared-worker-{index}"
    return ""


def fixed_probe_page_html(
    probe: str,
    index: int,
    payload_kib: int,
    *,
    worker_script_url: str | None = None,
) -> str:
    body_payload = "x" * max(0, payload_kib * 1024)
    worker_name = fixed_probe_worker_name(probe, index)
    worker_script = ""
    if probe == FIXED_PROBE_DEDICATED_WORKER:
        source_literal = json.dumps(fixed_probe_dedicated_worker_source())
        worker_script = f"""
<script>
(() => {{
  const source = {source_literal};
  const url = "data:text/javascript," + encodeURIComponent(source);
  const worker = new Worker(url);
  globalThis.__fixedWorkerReady = false;
  globalThis.__fixedWorkerMessages = [];
  globalThis.__fixedWorker = worker;
  worker.onmessage = event => {{
    globalThis.__fixedWorkerMessages.push(String(event.data));
    globalThis.__fixedWorkerReady = true;
  }};
}})();
</script>
"""
    elif worker_name:
        if worker_script_url is None:
            source_literal = json.dumps(fixed_probe_worker_source())
            worker_url_expression = '"data:text/javascript," + encodeURIComponent(source)'
        else:
            source_literal = "null"
            worker_url_expression = json.dumps(worker_script_url)
        name_literal = json.dumps(worker_name)
        worker_script = f"""
<script>
(() => {{
  const source = {source_literal};
  const url = {worker_url_expression};
  const worker = new SharedWorker(url, {name_literal});
  globalThis.__fixedWorkerReady = false;
  globalThis.__fixedWorkerMessages = [];
  globalThis.__fixedWorker = worker;
  worker.port.onmessage = event => {{
    globalThis.__fixedWorkerMessages.push(String(event.data));
    globalThis.__fixedWorkerReady = true;
  }};
  worker.port.start();
}})();
</script>
"""
    return f"""<!doctype html>
<meta charset="utf-8">
<title>fixed-probe-{probe}-{index}</title>
<body data-fixed-probe="{probe}" data-index="{index}">
<main id="payload">{body_payload}</main>
{worker_script}
</body>
"""


def fixed_probe_page_url(
    probe: str,
    index: int,
    payload_kib: int,
    *,
    base_url: str | None = None,
) -> str:
    if base_url is not None:
        query = urllib.parse.urlencode(
            {
                "probe": probe,
                "index": str(index),
                "payload_kib": str(payload_kib),
            }
        )
        return f"{base_url}/fixed-page?{query}"
    html = fixed_probe_page_html(probe, index, payload_kib)
    return "data:text/html," + urllib.parse.quote(html, safe="")


def start_fixed_probe_fixture_server() -> FixedProbeFixtureServer:
    class FixedProbeFixtureHandler(http.server.BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: Any) -> None:  # noqa: A002 - stdlib signature.
            return

        def do_GET(self) -> None:
            parsed = urllib.parse.urlparse(self.path)
            if parsed.path == "/fixed-shared-worker.js":
                self.send_text(
                    "application/javascript; charset=utf-8",
                    fixed_probe_worker_source(),
                )
                return
            if parsed.path == "/fixed-page":
                query = urllib.parse.parse_qs(parsed.query)
                probe = query.get("probe", [FIXED_PROBE_DATA_PAGES])[0]
                try:
                    index = int(query.get("index", ["1"])[0])
                    payload_kib = int(query.get("payload_kib", ["0"])[0])
                except ValueError:
                    self.send_error(400, "invalid fixed probe query")
                    return
                html = fixed_probe_page_html(
                    probe,
                    index,
                    payload_kib,
                    worker_script_url="/fixed-shared-worker.js",
                )
                self.send_text("text/html; charset=utf-8", html)
                return
            self.send_error(404)

        def send_text(self, content_type: str, body: str) -> None:
            encoded = body.encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", content_type)
            self.send_header("content-length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), FixedProbeFixtureHandler)
    host, port = server.server_address[:2]
    thread = threading.Thread(
        target=server.serve_forever,
        name="fixed-probe-fixture",
        daemon=True,
    )
    thread.start()
    return FixedProbeFixtureServer(
        server=server,
        thread=thread,
        base_url=f"http://{host}:{port}",
    )


async def wait_for_fixed_target_ready(
    client: Any,
    target: FixedPageTarget,
    probe: str,
    deadline: float,
    counter: dict[str, Any],
) -> dict[str, Any]:
    expression = (
        "JSON.stringify({"
        "readyState:document.readyState,"
        "title:document.title,"
        "nodeCount:document.querySelectorAll('*').length,"
        "textLength:document.body?document.body.innerText.length:0,"
        "workerReady:globalThis.__fixedWorkerReady === true,"
        "workerMessages:globalThis.__fixedWorkerMessages || []"
        "})"
    )
    last: dict[str, Any] = {}
    while True:
        result = await runtime_evaluate_json(
            client,
            target.session_id,
            expression,
            deadline,
            f"fixed-target-ready-{target.index}",
        )
        last = result
        value = result.get("value")
        if isinstance(value, dict) and value.get("readyState") in {"interactive", "complete"}:
            if probe not in FIXED_PROBE_WORKER_MODES or value.get("workerReady") is True:
                return result
        if time.perf_counter() >= deadline:
            raise TimeoutError(f"fixed target {target.index} did not become ready: {last!r}")
        await asyncio.sleep(0.02)


async def fixed_probe_heap_rows(
    client: Any,
    targets: list[FixedPageTarget],
    deadline: float,
    counter: dict[str, Any],
) -> list[dict[str, Any]]:
    rows = []
    for target in targets:
        rows.append(
            {
                "index": target.index,
                "targetId": target.target_id,
                "sessionId": target.session_id,
                "browserContextId": target.browser_context_id,
                "heap": await runtime_heap_usage(client, target.session_id, deadline, counter),
            }
        )
    return rows


async def close_fixed_targets(
    client: Any,
    targets: list[FixedPageTarget],
    deadline: float,
    counter: dict[str, Any],
) -> None:
    for target in reversed(targets):
        close_id = await client.send("Target.closeTarget", {"targetId": target.target_id})
        _, seen = await _recv_command_response(
            client,
            close_id,
            deadline=deadline,
            stage="Target.closeTarget",
        )
        account_messages(counter, seen)


async def run_fixed_probe_count(
    args: argparse.Namespace,
    probe: str,
    count: int,
    run_index: int,
) -> dict[str, Any]:
    serve = start_target_serve(args.target, args.binary, args.timeout, tuple(args.serve_arg))
    client = None
    targets: list[FixedPageTarget] = []
    cleanup_targets: list[FixedPageTarget] = []
    cleanup_browser_context_ids: list[str] = []
    fixture_server: FixedProbeFixtureServer | None = None
    counter = new_counter()
    snapshots: list[dict[str, Any]] = []
    started = time.perf_counter()
    try:
        if probe in FIXED_PROBE_WORKER_MODES:
            fixture_server = start_fixed_probe_fixture_server()
        snapshots.append(
            snapshot(
                "fixed_server_ready",
                serve.process.pid,
                {
                    "command": serve.command,
                    "probe": probe,
                    "count": count,
                    "fixtureBaseUrl": fixture_server.base_url if fixture_server else None,
                },
            )
        )
        client = await _wait_for_cdp(serve.endpoint, serve.process, min(5.0, args.timeout))
        deadline = time.perf_counter() + args.timeout
        if probe == FIXED_PROBE_POPUP_TARGETS:
            opener = await create_attached_target(
                client,
                deadline,
                counter,
                index=0,
                runtime=True,
                network=args.fixed_network,
            )
            cleanup_targets.append(opener)
            opener_url = fixed_probe_page_url(
                probe,
                0,
                0,
                base_url=fixture_server.base_url if fixture_server else None,
            )
            opener = await navigate_fixed_target(client, opener, opener_url, deadline, counter)
            cleanup_targets[-1] = opener
            opener_ready = await wait_for_fixed_target_ready(
                client,
                opener,
                probe,
                deadline,
                counter,
            )
            snapshots.append(
                snapshot(
                    "fixed_popup_opener_ready",
                    serve.process.pid,
                    {
                        "probe": probe,
                        "count": count,
                        "index": 0,
                        "ready": opener_ready,
                        "targets": await target_infos(client, deadline, counter),
                        "moliDiagnostics": await moli_diagnostics(
                            client, deadline, counter
                        ),
                    },
                )
            )
            for index in range(1, count + 1):
                url = fixed_probe_page_url(
                    probe,
                    index,
                    args.fixed_payload_kib,
                    base_url=fixture_server.base_url if fixture_server else None,
                )
                target = await create_popup_target(
                    client,
                    opener,
                    url,
                    deadline,
                    counter,
                    index=index,
                    runtime=True,
                    network=args.fixed_network,
                )
                ready = await wait_for_fixed_target_ready(client, target, probe, deadline, counter)
                targets.append(target)
                cleanup_targets.append(target)
                snapshots.append(
                    snapshot(
                        "fixed_after_target_ready",
                        serve.process.pid,
                        {
                            "probe": probe,
                            "count": count,
                            "index": index,
                            "ready": ready,
                            "openerTargetId": opener.target_id,
                            "targets": await target_infos(client, deadline, counter),
                            "moliDiagnostics": await moli_diagnostics(
                                client, deadline, counter
                            ),
                        },
                    )
                )
        else:
            for index in range(1, count + 1):
                browser_context_id = None
                if probe == FIXED_PROBE_DIFFERENT_BROWSER_CONTEXTS:
                    browser_context_id = await create_fixed_browser_context(client, deadline, counter)
                    cleanup_browser_context_ids.append(browser_context_id)
                target = await create_attached_target(
                    client,
                    deadline,
                    counter,
                    index=index,
                    runtime=True,
                    network=args.fixed_network,
                    browser_context_id=browser_context_id,
                )
                cleanup_targets.append(target)
                url = fixed_probe_page_url(
                    probe,
                    index,
                    args.fixed_payload_kib,
                    base_url=fixture_server.base_url if fixture_server else None,
                )
                target = await navigate_fixed_target(client, target, url, deadline, counter)
                cleanup_targets[-1] = target
                ready = await wait_for_fixed_target_ready(client, target, probe, deadline, counter)
                targets.append(target)
                snapshots.append(
                    snapshot(
                        "fixed_after_target_ready",
                        serve.process.pid,
                        {
                            "probe": probe,
                            "count": count,
                            "index": index,
                            "ready": ready,
                            "targets": await target_infos(client, deadline, counter),
                            "moliDiagnostics": await moli_diagnostics(
                                client, deadline, counter
                            ),
                        },
                    )
                )

        if args.idle_seconds > 0:
            idle_counter = new_counter()
            idle_messages = await drain_for(client, args.idle_seconds, idle_counter)
        else:
            idle_counter = new_counter()
            idle_messages = 0
        before_close_extra = {
            "probe": probe,
            "count": count,
            "idle_seconds": args.idle_seconds,
            "idle_messages": idle_messages,
            "idle_counter": serialize_counter(idle_counter),
            "targets": await target_infos(client, deadline, counter),
            "browserContextIds": list(cleanup_browser_context_ids),
            "moliDiagnostics": await moli_diagnostics(client, deadline, counter),
            "perTargetHeap": await fixed_probe_heap_rows(client, targets, deadline, counter),
        }
        snapshots.append(snapshot("fixed_before_close_targets", serve.process.pid, before_close_extra))

        await close_fixed_targets(client, cleanup_targets, deadline, counter)
        targets = []
        cleanup_targets = []
        if cleanup_browser_context_ids:
            await dispose_fixed_browser_contexts(
                client,
                cleanup_browser_context_ids,
                deadline,
                counter,
            )
            cleanup_browser_context_ids = []
        await asyncio.sleep(args.post_close_sleep)
        snapshots.append(
            snapshot(
                "fixed_after_close_targets",
                serve.process.pid,
                {
                    "probe": probe,
                    "count": count,
                    "targets": await target_infos(client, deadline, counter),
                    "browserContextIds": list(cleanup_browser_context_ids),
                    "moliDiagnostics": await moli_diagnostics(
                        client, deadline, counter
                    ),
                },
            )
        )
        return {
            "case": f"fixed-{probe}",
            "run": run_index,
            "fixed_probe": probe,
            "fixed_count": count,
            "ok": True,
            "events": serialize_counter(counter),
            "snapshots": snapshots,
            "total_ms": (time.perf_counter() - started) * 1000.0,
        }
    except Exception as error:  # noqa: BLE001 - investigation script should preserve failed probe state.
        failure_deadline = (
            max(deadline, time.perf_counter() + 2.0)
            if "deadline" in locals()
            else time.perf_counter() + 2.0
        )
        failure_extra: dict[str, Any] = {
            "probe": probe,
            "count": count,
            "error": repr(error),
            "targets": {},
            "moliDiagnostics": {},
            "perTargetHeap": [],
        }
        if cleanup_browser_context_ids:
            failure_extra["browserContextIds"] = list(cleanup_browser_context_ids)
        if client is not None:
            with contextlib.suppress(Exception):
                failure_extra["targets"] = await target_infos(client, failure_deadline, counter)
            with contextlib.suppress(Exception):
                failure_extra["moliDiagnostics"] = await moli_diagnostics(
                    client,
                    failure_deadline,
                    counter,
                )
            if targets:
                with contextlib.suppress(Exception):
                    failure_extra["perTargetHeap"] = await fixed_probe_heap_rows(
                        client,
                        targets,
                        failure_deadline,
                        counter,
                    )
        snapshots.append(snapshot("fixed_probe_failure", serve.process.pid, failure_extra))
        return {
            "case": f"fixed-{probe}",
            "run": run_index,
            "fixed_probe": probe,
            "fixed_count": count,
            "ok": False,
            "error": repr(error),
            "events": serialize_counter(counter),
            "snapshots": snapshots,
            "total_ms": (time.perf_counter() - started) * 1000.0,
        }
    finally:
        if client is not None:
            if cleanup_targets:
                try:
                    deadline = time.perf_counter() + 2.0
                    await close_fixed_targets(client, cleanup_targets, deadline, counter)
                except Exception:
                    pass
            if cleanup_browser_context_ids:
                try:
                    deadline = time.perf_counter() + 2.0
                    await dispose_fixed_browser_contexts(
                        client,
                        cleanup_browser_context_ids,
                        deadline,
                        counter,
                    )
                except Exception:
                    pass
            try:
                await client.websocket.close()
            except Exception:
                pass
        if fixture_server is not None:
            with contextlib.suppress(Exception):
                fixture_server.stop()
        stopped = stop_target_serve(serve)
        print(
            f"STOP fixed probe={probe} count={count} run={run_index}: "
            f"{stopped.get('returncode')} {stopped.get('resources')}"
        )


async def run_case(args: argparse.Namespace, case: ProbeCase, run_index: int) -> dict[str, Any]:
    serve = start_target_serve(args.target, args.binary, args.timeout, tuple(args.serve_arg))
    try:
        return await run_case_on_serve(args, case, run_index, serve)
    finally:
        stopped = stop_target_serve(serve)
        print(f"STOP {case.name} run={run_index}: {stopped.get('returncode')} {stopped.get('resources')}")


async def run_case_on_serve(
    args: argparse.Namespace,
    case: ProbeCase,
    run_index: int,
    serve: Any,
    sequence_index: int | None = None,
) -> dict[str, Any]:
    client = None
    target_id = None
    snapshots: list[dict[str, Any]] = []
    counter = new_counter()
    timings: dict[str, float] = {"serve_ready_ms": serve.ready_ms or 0.0}
    started = time.perf_counter()
    try:
        snapshots.append(
            snapshot(
                "server_ready",
                serve.process.pid,
                {"command": serve.command, "sequence_index": sequence_index},
            )
        )
        client = await _wait_for_cdp(serve.endpoint, serve.process, min(5.0, args.timeout))
        deadline = time.perf_counter() + args.timeout

        create_started = time.perf_counter()
        create_id = await client.send("Target.createTarget", {"url": "about:blank"})
        create_response, seen = await _recv_command_response(
            client,
            create_id,
            deadline=deadline,
            stage="Target.createTarget",
        )
        account_messages(counter, seen)
        timings["create_target_ms"] = (time.perf_counter() - create_started) * 1000.0
        target_id = str(create_response["result"]["targetId"])

        attach_started = time.perf_counter()
        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, seen = await _recv_command_response(
            client,
            attach_id,
            deadline=deadline,
            stage="Target.attachToTarget",
        )
        account_messages(counter, seen)
        timings["attach_target_ms"] = (time.perf_counter() - attach_started) * 1000.0
        session_id = str(attach_response["result"]["sessionId"])

        enable_started = time.perf_counter()
        methods = []
        if case.page:
            methods.append("Page.enable")
        if case.runtime:
            methods.append("Runtime.enable")
        if case.network:
            methods.append("Network.enable")
        for method in methods:
            message_id = await client.send(method, session_id=session_id)
            _, seen = await _recv_command_response(client, message_id, deadline=deadline, stage=method)
            account_messages(counter, seen)
        lifecycle_id = await client.send(
            "Page.setLifecycleEventsEnabled",
            {"enabled": True},
            session_id=session_id,
        )
        _, seen = await _recv_command_response(
            client,
            lifecycle_id,
            deadline=deadline,
            stage="Page.setLifecycleEventsEnabled",
        )
        account_messages(counter, seen)
        timings["enable_domains_ms"] = (time.perf_counter() - enable_started) * 1000.0
        snapshots.append(snapshot("domains_enabled", serve.process.pid, {"enabled_methods": methods}))

        nav_started = time.perf_counter()
        navigate_id = await client.send("Page.navigate", {"url": args.url}, session_id=session_id)
        navigate_response, seen = await _recv_command_response(
            client,
            navigate_id,
            deadline=deadline,
            stage="Page.navigate",
            late_error_grace_seconds=CDP_LATE_ERROR_GRACE_SECONDS,
        )
        nav_ack = time.perf_counter()
        timings["navigate_ack_ms"] = (nav_ack - nav_started) * 1000.0
        frame_id = navigate_response.get("result", {}).get("frameId")
        frame_id = str(frame_id) if frame_id is not None else None
        snapshots.append(snapshot("navigate_ack", serve.process.pid, {"frame_id": frame_id}))

        dcl_message_count = await recv_until_dcl(client, session_id, frame_id, deadline, seen, counter)
        dcl_at = time.perf_counter()
        timings["navigate_to_dcl_ms"] = (dcl_at - nav_started) * 1000.0
        timings["ack_to_dcl_ms"] = (dcl_at - nav_ack) * 1000.0
        dcl_extra: dict[str, Any] = {"dcl_message_count": dcl_message_count}
        if case.runtime:
            dcl_extra["heap"] = await runtime_heap_usage(client, session_id, deadline)
            dcl_extra["counts"] = await runtime_evaluate_json(
                client,
                session_id,
                "JSON.stringify({"
                "title:document.title,"
                "readyState:document.readyState,"
                "textLength:document.body?document.body.innerText.length:0,"
                "nodeCount:document.querySelectorAll('*').length,"
                "scriptCount:document.scripts.length,"
                "imgCount:document.images.length,"
                "iframeCount:document.querySelectorAll('iframe,frame').length,"
                "finalUrl:location.href"
                "})",
                deadline,
                "dcl-counts",
            )
        dcl_extra["moliDiagnostics"] = await moli_diagnostics(
            client, deadline, counter
        )
        snapshots.append(snapshot("domcontentloaded", serve.process.pid, dcl_extra))

        if case.gc_after_dcl:
            gc_extra: dict[str, Any] = {
                "gc": await heap_profiler_collect_garbage(
                    client, session_id, deadline, args.gc_repeat, counter
                )
            }
            if case.runtime:
                gc_extra["heap"] = await runtime_heap_usage(client, session_id, deadline)
            snapshots.append(snapshot("after_gc_dcl", serve.process.pid, gc_extra))

        if case.outer_html:
            outer_started = time.perf_counter()
            outer = await runtime_evaluate_json(
                client,
                session_id,
                "document.documentElement ? document.documentElement.outerHTML : ''",
                deadline,
                "outerHTML",
            )
            timings["outer_html_ms"] = (time.perf_counter() - outer_started) * 1000.0
            value = outer.get("value")
            html_bytes = len(value.encode("utf-8", errors="replace")) if isinstance(value, str) else 0
            snapshots.append(
                snapshot(
                    "after_outer_html",
                    serve.process.pid,
                    {"html_bytes": html_bytes, "outer_eval": {k: v for k, v in outer.items() if k != "value"}},
                )
            )

        if case.gc_after_outer:
            gc_extra = {
                "gc": await heap_profiler_collect_garbage(
                    client, session_id, deadline, args.gc_repeat, counter
                )
            }
            if case.runtime:
                gc_extra["heap"] = await runtime_heap_usage(client, session_id, deadline)
            snapshots.append(snapshot("after_gc_outer", serve.process.pid, gc_extra))

        if case.idle:
            idle_counter = new_counter()
            if args.timeline_interval > 0:
                idle_messages, idle_timeline = await drain_for_timeline(
                    client,
                    args.idle_seconds,
                    idle_counter,
                    serve.process.pid,
                    args.timeline_interval,
                    args.timeline_smaps,
                    session_id=session_id,
                    include_runtime=case.runtime and args.timeline_runtime,
                    include_diagnostics=args.timeline_diagnostics,
                    command_deadline=deadline,
                )
            else:
                idle_messages = await drain_for(client, args.idle_seconds, idle_counter)
                idle_timeline = []
            idle_extra: dict[str, Any] = {
                "idle_seconds": args.idle_seconds,
                "idle_messages": idle_messages,
                "idle_counter": serialize_counter(idle_counter),
            }
            if idle_timeline:
                idle_extra["timeline"] = idle_timeline
            if case.runtime:
                idle_extra["heap"] = await runtime_heap_usage(client, session_id, deadline)
                idle_extra["counts"] = await runtime_evaluate_json(
                    client,
                    session_id,
                    "JSON.stringify({"
                    "readyState:document.readyState,"
                    "textLength:document.body?document.body.innerText.length:0,"
                    "nodeCount:document.querySelectorAll('*').length,"
                    "scriptCount:document.scripts.length,"
                    "imgCount:document.images.length,"
                    "iframeCount:document.querySelectorAll('iframe,frame').length"
                    "})",
                    deadline,
                    "idle-counts",
                )
            idle_extra["moliDiagnostics"] = await moli_diagnostics(
                client, deadline, counter
            )
            snapshots.append(snapshot("idle_after", serve.process.pid, idle_extra))

        if case.gc_after_idle:
            gc_extra = {
                "gc": await heap_profiler_collect_garbage(
                    client, session_id, deadline, args.gc_repeat, counter
                )
            }
            if case.runtime:
                gc_extra["heap"] = await runtime_heap_usage(client, session_id, deadline)
            snapshots.append(snapshot("after_gc_idle", serve.process.pid, gc_extra))

        before_close_extra = {
            "targets": await target_infos(client, deadline, counter),
            "moliDiagnostics": await moli_diagnostics(client, deadline, counter),
        }
        if case.runtime:
            before_close_extra["heap"] = await runtime_heap_usage(client, session_id, deadline)
        snapshots.append(snapshot("before_close_target", serve.process.pid, before_close_extra))

        close_started = time.perf_counter()
        close_id = await client.send("Target.closeTarget", {"targetId": target_id})
        _, seen = await _recv_command_response(
            client,
            close_id,
            deadline=deadline,
            stage="Target.closeTarget",
        )
        account_messages(counter, seen)
        target_id = None
        timings["close_target_ms"] = (time.perf_counter() - close_started) * 1000.0
        await asyncio.sleep(args.post_close_sleep)
        snapshots.append(
            snapshot(
                "after_close_target",
                serve.process.pid,
                {
                    "targets": await target_infos(client, deadline, counter),
                    "moliDiagnostics": await moli_diagnostics(
                        client, deadline, counter
                    ),
                },
            )
        )

        if case.reset_engine_after_close:
            reset_extra = {
                "resetIdleEngine": await moli_reset_idle_engine(
                    client, deadline, counter
                ),
                "targets": await target_infos(client, deadline, counter),
                "moliDiagnostics": await moli_diagnostics(
                    client, deadline, counter
                ),
            }
            snapshots.append(snapshot("after_reset_idle_engine", serve.process.pid, reset_extra))

        return {
            "case": case.name,
            "run": run_index,
            "sequence_index": sequence_index,
            "ok": True,
            "timings": timings,
            "events": serialize_counter(counter),
            "snapshots": snapshots,
            "total_ms": (time.perf_counter() - started) * 1000.0,
        }
    except Exception as error:  # noqa: BLE001 - investigation script should preserve failed case state.
        return {
            "case": case.name,
            "run": run_index,
            "sequence_index": sequence_index,
            "ok": False,
            "error": repr(error),
            "timings": timings,
            "events": serialize_counter(counter),
            "snapshots": snapshots,
            "total_ms": (time.perf_counter() - started) * 1000.0,
        }
    finally:
        if client is not None:
            if target_id is not None:
                try:
                    close_id = await client.send("Target.closeTarget", {"targetId": target_id})
                    await client.recv_until_id(close_id, timeout=1.0)
                except Exception:
                    pass
            try:
                await client.websocket.close()
            except Exception:
                pass


def compact_snapshot(row: dict[str, Any]) -> dict[str, Any]:
    resources = row["resources"]
    categories = []
    for name, values in row["smaps"]["categories"].items():
        pss = values.get("Pss", 0)
        if pss:
            categories.append(
                {
                    "category": name,
                    "pss_mib": round(pss / 1024 / 1024, 1),
                    "rss_mib": round(values.get("Rss", 0) / 1024 / 1024, 1),
                    "anonymous_mib": round(values.get("Anonymous", 0) / 1024 / 1024, 1),
                }
            )
    extra = row.get("extra") or {}
    result: dict[str, Any] = {
        "label": row["label"],
        "rss_mib": round(mib(resources.get("rss_bytes")) or 0.0, 1),
        "pss_mib": round(mib(resources.get("pss_bytes")) or 0.0, 1),
        "threads": resources.get("thread_count"),
        "top_categories": sorted(categories, key=lambda item: item["pss_mib"], reverse=True)[:5],
    }
    top_anonymous = [
        {
            "range": mapping.get("range"),
            "perms": mapping.get("perms"),
            "size_mib": round(mib(mapping.get("Size")) or 0.0, 1),
            "rss_mib": round(mib(mapping.get("Rss")) or 0.0, 1),
            "pss_mib": round(mib(mapping.get("Pss")) or 0.0, 1),
            "private_dirty_mib": round(mib(mapping.get("Private_Dirty")) or 0.0, 1),
            "path": mapping.get("path") or "<anon>",
        }
        for mapping in row["smaps"].get("top_mappings", [])
        if mapping.get("category") == "anonymous_unnamed"
    ][:12]
    if top_anonymous:
        result["top_anonymous_mappings"] = top_anonymous
    anonymous_histogram = row["smaps"].get("anonymous_histogram_by_vma_size", {})
    if anonymous_histogram:
        result["anonymous_histogram_by_vma_size"] = {
            name: {
                "count": values["count"],
                "pss_mib": round(values["pss"] / 1024 / 1024, 1),
                "rss_mib": round(values["rss"] / 1024 / 1024, 1),
                "size_mib": round(values["size"] / 1024 / 1024, 1),
            }
            for name, values in anonymous_histogram.items()
        }
    if "heap" in extra:
        heap = extra["heap"].get("response", {})
        result["v8_heap_used_mib"] = round(mib(heap.get("usedSize")) or 0.0, 1)
        result["v8_heap_total_mib"] = round(mib(heap.get("totalSize")) or 0.0, 1)
        result["v8_heap_physical_mib"] = round(mib(heap.get("totalPhysicalSize")) or 0.0, 1)
        result["v8_malloced_mib"] = round(mib(heap.get("mallocedMemory")) or 0.0, 1)
        result["v8_external_mib"] = round(mib(heap.get("externalMemory")) or 0.0, 1)
        result["v8_native_contexts"] = heap.get("numberOfNativeContexts")
        spaces = heap.get("heapSpaces")
        if isinstance(spaces, list):
            top_spaces = sorted(
                [
                    {
                        "name": str(space.get("name")),
                        "used_mib": round(mib(space.get("usedSize")) or 0.0, 1),
                        "physical_mib": round(mib(space.get("physicalSize")) or 0.0, 1),
                        "size_mib": round(mib(space.get("size")) or 0.0, 1),
                    }
                    for space in spaces
                    if isinstance(space, dict)
                ],
                key=lambda item: (item["physical_mib"], item["used_mib"]),
                reverse=True,
            )
            result["v8_top_spaces"] = top_spaces[:8]
        moli = heap.get("moli")
        if isinstance(moli, dict):
            result["moli_counters"] = moli
    if "counts" in extra:
        result["counts"] = extra["counts"].get("value")
    if "gc" in extra:
        result["gc"] = extra["gc"]
    if "trim" in extra:
        result["trim"] = extra["trim"]
    if "resetIdleEngine" in extra:
        result["reset_idle_engine"] = extra["resetIdleEngine"]
    if "targets" in extra:
        targets = extra["targets"]
        target_infos_value = targets.get("targetInfos", []) if isinstance(targets, dict) else []
        result["targets"] = {
            "targetCount": targets.get("targetCount") if isinstance(targets, dict) else None,
            "attachedCount": targets.get("attachedCount") if isinstance(targets, dict) else None,
            "pageCount": targets.get("pageCount") if isinstance(targets, dict) else None,
            "elapsed_ms": targets.get("elapsed_ms") if isinstance(targets, dict) else None,
            "error": targets.get("error") if isinstance(targets, dict) else None,
            "targetInfos": target_infos_value[:8] if isinstance(target_infos_value, list) else [],
        }
    if "browserContextIds" in extra:
        result["browser_context_ids"] = extra["browserContextIds"]
    if "moliDiagnostics" in extra:
        diagnostics = extra["moliDiagnostics"]
        response = diagnostics.get("response", {}) if isinstance(diagnostics, dict) else {}
        result["moli_diagnostics"] = response if isinstance(response, dict) else {}
        if isinstance(diagnostics, dict) and diagnostics.get("error"):
            result["moli_diagnostics_error"] = diagnostics.get("error")
    if "idle_counter" in extra:
        result["idle_counter"] = extra["idle_counter"]
    if "timeline" in extra:
        result["timeline"] = extra["timeline"]
    if "perTargetHeap" in extra:
        result["per_target_heap"] = [
            {
                "index": row.get("index"),
                "targetId": row.get("targetId"),
                "sessionId": row.get("sessionId"),
                "browserContextId": row.get("browserContextId"),
                "heap": compact_heap_diagnostic(row.get("heap", {})),
            }
            for row in extra["perTargetHeap"]
            if isinstance(row, dict)
        ]
    return result


def compact_case(row: dict[str, Any]) -> dict[str, Any]:
    return {
        "case": row["case"],
        "run": row["run"],
        "sequence_index": row.get("sequence_index"),
        "ok": row["ok"],
        "error": row.get("error"),
        "timings": row.get("timings", {}),
        "events": row.get("events", {}),
        "snapshots": [compact_snapshot(snapshot_row) for snapshot_row in row.get("snapshots", [])],
    }


async def run_sequence(args: argparse.Namespace) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    case = CASES[args.sequence_case]
    serve = start_target_serve(args.target, args.binary, args.timeout, tuple(args.serve_arg))
    rows: list[dict[str, Any]] = []
    sequence_snapshots: list[dict[str, Any]] = []
    try:
        sequence_snapshots.append(
            snapshot(
                "sequence_server_ready",
                serve.process.pid,
                {"command": serve.command, "case": case.name},
            )
        )
        for sequence_index in range(1, args.sequence_targets + 1):
            print(
                f"\n=== sequence case={case.name} target={sequence_index}/{args.sequence_targets} ===",
                flush=True,
            )
            rows.append(
                await run_case_on_serve(
                    args,
                    case,
                    run_index=1,
                    serve=serve,
                    sequence_index=sequence_index,
                )
            )
            sequence_snapshots.append(
                snapshot(
                    "sequence_after_target",
                    serve.process.pid,
                    {
                        "case": case.name,
                        "sequence_index": sequence_index,
                    },
                )
            )
        return rows, sequence_snapshots
    finally:
        stopped = stop_target_serve(serve)
        print(
            f"STOP sequence {case.name}: {stopped.get('returncode')} {stopped.get('resources')}"
        )


async def run_fixed_probes(args: argparse.Namespace) -> list[dict[str, Any]]:
    rows = []
    for run_index in range(1, args.runs + 1):
        for count in args.fixed_counts:
            print(
                f"\n=== fixed probe={args.fixed_probe} count={count} run={run_index}/{args.runs} ===",
                flush=True,
            )
            rows.append(await run_fixed_probe_count(args, args.fixed_probe, count, run_index))
    return rows


async def async_main(args: argparse.Namespace) -> dict[str, Any]:
    rows = []
    sequence_snapshots: list[dict[str, Any]] = []
    if args.fixed_probe != FIXED_PROBE_NONE:
        rows = await run_fixed_probes(args)
    elif args.sequence_targets > 0:
        rows, sequence_snapshots = await run_sequence(args)
    else:
        selected = [CASES[name] for name in args.cases]
        for run_index in range(1, args.runs + 1):
            for case in selected:
                print(f"\n=== case={case.name} run={run_index}/{args.runs} ===", flush=True)
                rows.append(await run_case(args, case, run_index))
    return {
        "created_at": datetime.now(timezone.utc).astimezone().isoformat(),
        "url": args.url,
        "target": args.target,
        "binary": str(args.binary),
        "serve_args": args.serve_arg,
        "timeout_seconds": args.timeout,
        "idle_seconds": args.idle_seconds,
        "timeline_interval_seconds": args.timeline_interval,
        "timeline_smaps": args.timeline_smaps,
        "timeline_runtime": args.timeline_runtime,
        "timeline_diagnostics": args.timeline_diagnostics,
        "gc_repeat": args.gc_repeat,
        "sequence_targets": args.sequence_targets,
        "sequence_case": args.sequence_case,
        "fixed_probe": args.fixed_probe,
        "fixed_counts": args.fixed_counts,
        "fixed_payload_kib": args.fixed_payload_kib,
        "fixed_network": args.fixed_network,
        "runs": args.runs,
        "cases": args.cases,
        "sequence_snapshots": sequence_snapshots,
        "rows": rows,
        "compact_rows": [compact_case(row) for row in rows],
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Probe Moli CDP memory by domain ablation.")
    parser.add_argument("--url", default=DEFAULT_URL)
    parser.add_argument("--target", default="moli-cdp")
    parser.add_argument("--binary", type=Path, default=REPO_ROOT / "target/release/moli")
    parser.add_argument(
        "--serve-arg",
        action="append",
        default=[],
        help="Extra argument appended to the target serve command; repeat for multiple args.",
    )
    parser.add_argument("--timeout", type=float, default=45.0)
    parser.add_argument("--idle-seconds", type=float, default=2.0)
    parser.add_argument("--gc-repeat", type=int, default=1)
    parser.add_argument("--post-close-sleep", type=float, default=0.3)
    parser.add_argument(
        "--timeline-interval",
        type=float,
        default=0.0,
        help="During idle cases, record RSS/CDP event deltas at this interval in seconds.",
    )
    parser.add_argument(
        "--timeline-smaps",
        action="store_true",
        help="Include compact smaps categories in each idle timeline sample.",
    )
    parser.add_argument(
        "--timeline-runtime",
        action="store_true",
        help="Include compact Runtime.getHeapUsage data in each idle timeline sample.",
    )
    parser.add_argument(
        "--timeline-diagnostics",
        action="store_true",
        help="Include HeapProfiler.moliDiagnostics in each idle timeline sample.",
    )
    parser.add_argument(
        "--sequence-targets",
        type=int,
        default=0,
        help="Reuse one serve process and open/close this many targets with --sequence-case.",
    )
    parser.add_argument(
        "--sequence-case",
        choices=sorted(CASES),
        default="full-idle",
        help="Case to run for --sequence-targets.",
    )
    parser.add_argument(
        "--fixed-probe",
        choices=FIXED_PROBE_CHOICES,
        default=FIXED_PROBE_NONE,
        help=(
            "Run a Phase 5 fixed-cost live-target probe instead of the normal case matrix. "
            "Use data-pages for document isolate slope, background-targets for parked target "
            "slope, popup-targets for opener-created target slope, different-browser-contexts "
            "for browser-context isolate boundaries, dedicated-worker for worker isolate slope, "
            "shared-worker-same-key for client/context slope, or shared-worker-distinct-key for "
            "SharedWorker isolate slope."
        ),
    )
    parser.add_argument(
        "--fixed-counts",
        type=int,
        nargs="+",
        default=[1, 2, 4, 8, 16],
        help="Live page counts for --fixed-probe.",
    )
    parser.add_argument(
        "--fixed-payload-kib",
        type=int,
        default=4,
        help="Payload text size per fixed data page, in KiB.",
    )
    parser.add_argument(
        "--fixed-network",
        action="store_true",
        help="Enable Network domain during --fixed-probe runs.",
    )
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument(
        "--cases",
        nargs="+",
        choices=sorted(CASES),
        default=["page", "page-runtime", "page-network", "full", "full-outer", "full-idle"],
    )
    parser.add_argument("--output", type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    output = args.output
    if output is None:
        stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        output = Path("/tmp") / f"ifeng-cdp-memory-ablation-{stamp}.json"
    result = asyncio.run(async_main(args))
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"\nWROTE {output}")
    print(json.dumps(result["compact_rows"], ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
