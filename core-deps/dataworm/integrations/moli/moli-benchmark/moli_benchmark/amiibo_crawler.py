from __future__ import annotations

import asyncio
import contextlib
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import urldefrag, urlparse

from .artifacts import write_csv, write_json
from .stats import summarize
from .synthetic_compare import TARGETS, normalize_cdp_target, target_metadata
from .target_serve import start_target_serve, stop_target_serve


AMIIBO_URL = "https://demo-browser.lightpanda.io/amiibo/"
AMIIBO_EXPECTED_PAGES = 933
AMIIBO_CONCURRENCY_MATRIX = (1, 2, 5, 10, 25, 100)
AMIIBO_SMOKE_CONCURRENCY_MATRIX = (1,)
AMIIBO_SMOKE_LIMIT = 5
DEFAULT_AMIIBO_RUNS = 1
AMIIBO_MODES = ("session", "process")
AMIIBO_PROFILES = ("smoke", "formal")


@dataclass
class PageSession:
    client: Any
    session_id: str


def _expected_pages(limit: int) -> int:
    return limit if limit > 0 else AMIIBO_EXPECTED_PAGES


def _normalize_url(url: str) -> str:
    return urldefrag(url)[0]


def _same_origin(seed: str, candidate: str) -> bool:
    seed_url = urlparse(seed)
    candidate_url = urlparse(candidate)
    return candidate_url.scheme in ("http", "https") and candidate_url.netloc == seed_url.netloc


def _classify_amiibo_error(error: str | None) -> str | None:
    if error is None:
        return None
    lower = error.lower()
    if "target binary unavailable" in lower:
        return "target-unavailable"
    if "timeout" in lower or "timed out" in lower:
        return "timeout"
    if "navigate" in lower or "navigation" in lower:
        return "navigation-error"
    if "evaluate" in lower or "script" in lower or "javascript" in lower:
        return "script-error"
    if "cdp" in lower or "protocol" in lower or "target." in lower or "runtime." in lower or "page." in lower:
        return "protocol-error"
    if "process" in lower or "crash" in lower or "exited" in lower:
        return "process-error"
    return "error"


def _page_assertion_failures(page: dict[str, Any]) -> list[str]:
    failures: list[str] = []
    if not isinstance(page.get("url"), str) or not page["url"]:
        failures.append("missing-url")
    title = page.get("title")
    if not isinstance(title, str) or not title.strip():
        failures.append("missing-title")
    ready_state = page.get("ready_state")
    if ready_state != "complete":
        failures.append("document-not-complete")
    text_length = page.get("text_length")
    if not isinstance(text_length, int | float) or text_length <= 0:
        failures.append("missing-body-text")
    link_count = page.get("link_count")
    if not isinstance(link_count, int) or link_count < 0:
        failures.append("missing-link-count")
    fields = page.get("fields")
    if not isinstance(fields, dict):
        failures.append("missing-fields")
        return failures
    name = fields.get("name")
    if not isinstance(name, str) or not name.strip() or name == "Amiibo Character":
        failures.append("missing-amiibo-name")
    if isinstance(title, str) and isinstance(name, str) and title.strip() and name.strip() and title.strip() != name.strip():
        failures.append("title-name-mismatch")
    game = fields.get("game")
    if not isinstance(game, str) or not game.strip() or game == "Amiibo Game":
        failures.append("missing-amiibo-series")
    serie = fields.get("serie")
    if not isinstance(serie, str) or not serie.strip() or serie == "Amiibo Serie":
        failures.append("missing-game-series")
    image_src = fields.get("imageSrc")
    if not isinstance(image_src, str) or not image_src.strip():
        failures.append("missing-image-src")
    alt_count = fields.get("altCount")
    if not isinstance(alt_count, int) or alt_count < 0:
        failures.append("missing-alt-count")
    return failures


def _collect_page_assertion_failures(pages: list[dict[str, Any]]) -> list[dict[str, Any]]:
    failures: list[dict[str, Any]] = []
    for page in pages:
        page_failures = _page_assertion_failures(page)
        if page_failures:
            failures.append(
                {
                    "url": page.get("url"),
                    "worker": page.get("worker"),
                    "failures": page_failures,
                }
            )
    return failures


def _row_failure_kind(
    *,
    ok: bool,
    pages: int | None,
    expected_pages: int,
    crawler_errors: int,
    assertion_failures: int,
    error: str | None = None,
) -> str | None:
    if ok:
        return None
    classified = _classify_amiibo_error(error)
    if classified is not None:
        return classified
    if crawler_errors:
        return "crawler-error"
    if assertion_failures:
        return "assertion-failure"
    if pages is not None and pages != expected_pages:
        return "page-count-mismatch"
    return "error"


def _row_error_message(
    *,
    ok: bool,
    pages: int,
    expected_pages: int,
    crawler_errors: int,
    assertion_failures: int,
) -> str | None:
    if ok:
        return None
    if crawler_errors:
        return "crawler recorded navigation/protocol errors"
    if assertion_failures:
        return "crawler page assertions failed"
    if pages != expected_pages:
        return f"crawler discovered {pages} of {expected_pages} expected pages"
    return "crawler did not complete expected page count without errors"


async def _create_page_session(endpoint: str, timeout_seconds: float) -> PageSession:
    from .raw_cdp import connect_raw_cdp

    client = await connect_raw_cdp(endpoint)
    try:
        target_command_id = await client.send("Target.createTarget", {"url": "about:blank"})
        target_response, _ = await client.recv_until_id(target_command_id, timeout=timeout_seconds)
        target_id = target_response.get("result", {}).get("targetId")
        if not isinstance(target_id, str) or not target_id:
            raise RuntimeError(f"missing targetId in {target_response}")

        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, _ = await client.recv_until_id(attach_id, timeout=timeout_seconds)
        session_id = attach_response.get("result", {}).get("sessionId")
        if not isinstance(session_id, str) or not session_id:
            raise RuntimeError(f"missing sessionId in {attach_response}")

        for method in ("Page.enable", "Runtime.enable"):
            command_id = await client.send(method, session_id=session_id)
            await client.recv_until_id(command_id, timeout=timeout_seconds)
        return PageSession(client=client, session_id=session_id)
    except Exception:
        await client.websocket.close()
        raise


async def _fetch_links(page: PageSession, url: str, timeout_seconds: float) -> dict[str, Any]:
    started = time.perf_counter()
    navigate_id = await page.client.send("Page.navigate", {"url": url}, session_id=page.session_id)
    await page.client.recv_until_id(navigate_id, timeout=timeout_seconds)
    expression = f"""
    new Promise(resolve => {{
      const deadline = Date.now() + {int(timeout_seconds * 1000)};
      const text = selector => {{
        const element = document.querySelector(selector);
        return element ? element.textContent.trim() : "";
      }};
      function collect() {{
        const anchors = Array.from(document.querySelectorAll('a[href]')).map(anchor => anchor.href);
        if ((document.readyState === 'complete' && anchors.length > 0) || Date.now() > deadline) {{
          const image = document.querySelector('#image');
          resolve({{
            readyState: document.readyState,
            title: document.title,
            textLength: document.body ? document.body.textContent.length : 0,
            links: anchors,
            fields: {{
              name: text('#name'),
              game: text('#game'),
              serie: text('#serie'),
              imageSrc: image ? image.getAttribute('src') || "" : "",
              navText: text('#nav'),
              altCount: document.querySelectorAll('#alt a[href]').length
            }}
          }});
        }} else {{
          setTimeout(collect, 10);
        }}
      }}
      collect();
    }})
    """
    evaluate_id = await page.client.send(
        "Runtime.evaluate",
        {"expression": expression, "awaitPromise": True, "returnByValue": True},
        session_id=page.session_id,
    )
    response, _ = await page.client.recv_until_id(evaluate_id, timeout=timeout_seconds + 1)
    value = response.get("result", {}).get("result", {}).get("value")
    if not isinstance(value, dict):
        raise RuntimeError(f"unexpected crawler evaluate result for {url}: {response}")
    links = value.get("links")
    if not isinstance(links, list):
        links = []
    fields = value.get("fields")
    if not isinstance(fields, dict):
        fields = {}
    return {
        "url": url,
        "elapsed_ms": (time.perf_counter() - started) * 1000.0,
        "ready_state": value.get("readyState"),
        "title": value.get("title"),
        "text_length": value.get("textLength"),
        "links": [str(link) for link in links],
        "fields": fields,
    }


async def _crawl_with_endpoint(
    *,
    endpoint: str,
    pool: int,
    limit: int,
    url: str,
    timeout_seconds: float,
) -> dict[str, Any]:
    sessions: list[tuple[int, PageSession]] = []
    errors: list[dict[str, Any]] = []

    try:
        for worker_id in range(pool):
            try:
                sessions.append((worker_id, await _create_page_session(endpoint, timeout_seconds)))
            except Exception as error:
                errors.append({"worker": worker_id, "url": None, "stage": "create-session", "error": str(error)})

        return await _crawl_with_page_sessions(
            sessions=sessions,
            initial_errors=errors,
            limit=limit,
            url=url,
            timeout_seconds=timeout_seconds,
        )
    except BaseException:
        await _close_page_sessions(sessions)
        raise


async def _close_page_sessions(sessions: list[tuple[int, PageSession]]) -> None:
    for _, page in sessions:
        with contextlib.suppress(Exception):
            await page.client.websocket.close()


async def _crawl_with_page_sessions(
    *,
    sessions: list[tuple[int, PageSession]],
    initial_errors: list[dict[str, Any]],
    limit: int,
    url: str,
    timeout_seconds: float,
) -> dict[str, Any]:
    expected = _expected_pages(limit)
    queue: asyncio.Queue[str] = asyncio.Queue()
    known = {_normalize_url(url)}
    pages: list[dict[str, Any]] = []
    errors = list(initial_errors)
    lock = asyncio.Lock()
    await queue.put(_normalize_url(url))

    workers: list[asyncio.Task[None]] = []

    async def worker(worker_id: int, page: PageSession) -> None:
        try:
            while True:
                current = await queue.get()
                try:
                    result = await _fetch_links(page, current, timeout_seconds)
                    async with lock:
                        pages.append({k: v for k, v in result.items() if k != "links"} | {"worker": worker_id, "link_count": len(result["links"])})
                        for link in result["links"]:
                            if len(known) >= expected:
                                break
                            normalized = _normalize_url(link)
                            if not _same_origin(url, normalized) or normalized in known:
                                continue
                            known.add(normalized)
                            await queue.put(normalized)
                except Exception as error:
                    async with lock:
                        errors.append({"worker": worker_id, "url": current, "error": str(error)})
                finally:
                    queue.task_done()
        except asyncio.CancelledError:
            raise
        finally:
            await page.client.websocket.close()

    try:
        if not sessions:
            return {
                "pages": pages,
                "errors": errors,
                "known_count": len(known),
                "expected_pages": expected,
            }

        workers = [asyncio.create_task(worker(worker_id, page)) for worker_id, page in sessions]
        await queue.join()
    finally:
        for worker_task in workers:
            worker_task.cancel()
        if workers:
            await asyncio.gather(*workers, return_exceptions=True)
        else:
            await _close_page_sessions(sessions)
    return {
        "pages": pages,
        "errors": errors,
        "known_count": len(known),
        "expected_pages": expected,
    }


def _summarize_serve_resources(serve_details: list[dict[str, Any]]) -> dict[str, Any]:
    resources = [detail.get("resources", {}) for detail in serve_details]

    def sum_field(name: str) -> int | float | None:
        values = [resource.get(name) for resource in resources if resource.get(name) is not None]
        return sum(values) if values else None

    def max_field(name: str) -> int | float | None:
        values = [resource.get(name) for resource in resources if resource.get(name) is not None]
        return max(values) if values else None

    return {
        "peak_cpu_percent": sum_field("peak_cpu_percent"),
        "peak_fd_count": sum_field("peak_fd_count"),
        "peak_process_count": sum_field("peak_process_count"),
        "peak_pss_bytes": sum_field("peak_pss_bytes"),
        "peak_rss_bytes": sum_field("peak_rss_bytes"),
        "peak_thread_count": sum_field("peak_thread_count"),
        "sample_count": sum_field("sample_count"),
        "sampling_method": "aggregate_process_tree",
        "worker_count": len(serve_details),
        "max_worker_peak_pss_bytes": max_field("peak_pss_bytes"),
    }


def _summarize_rows(rows: list[dict[str, Any]]) -> dict[str, Any]:
    failure_kinds: dict[str, int] = {}
    for row in rows:
        failure_kind = row.get("failure_kind")
        if isinstance(failure_kind, str) and failure_kind:
            failure_kinds[failure_kind] = failure_kinds.get(failure_kind, 0) + 1
    return {
        "runs": len(rows),
        "passes": sum(1 for row in rows if row.get("ok")),
        "failures": sum(1 for row in rows if not row.get("ok")),
        "failure_kinds": failure_kinds,
        "assertion_failures": sum(int(row.get("assertion_failures") or 0) for row in rows),
        "elapsed_ms": summarize(row.get("elapsed_ms") for row in rows if row.get("ok") and row.get("elapsed_ms") is not None),
        "browser_peak_pss_bytes": summarize(
            row.get("browser_peak_pss_bytes")
            for row in rows
            if row.get("browser_peak_pss_bytes") is not None
        ),
    }


def _row_from_crawl_result(
    *,
    target: str,
    mode: str,
    pool: int,
    limit: int,
    url: str,
    elapsed_ms: float,
    result: dict[str, Any],
    serve_details: dict[str, Any],
) -> tuple[dict[str, Any], dict[str, Any]]:
    pages = int(result["known_count"])
    expected = _expected_pages(limit)
    crawler_errors = len(result["errors"])
    assertion_failures = _collect_page_assertion_failures(result["pages"])
    ok = pages == expected and not crawler_errors and not assertion_failures
    resources = serve_details.get("resources", {})
    row = {
        "target": target,
        "mode": mode,
        "pool": pool,
        "limit": limit,
        "url": url,
        "ok": ok,
        "elapsed_ms": elapsed_ms,
        "pages": pages,
        "expected_pages": expected,
        "errors": crawler_errors,
        "assertion_failures": len(assertion_failures),
        "browser_peak_pss_bytes": resources.get("peak_pss_bytes"),
        "browser_peak_cpu_percent": resources.get("peak_cpu_percent"),
        "error": _row_error_message(
            ok=ok,
            pages=pages,
            expected_pages=expected,
            crawler_errors=crawler_errors,
            assertion_failures=len(assertion_failures),
        ),
    }
    row["failure_kind"] = _row_failure_kind(
        ok=ok,
        pages=pages,
        expected_pages=expected,
        crawler_errors=crawler_errors,
        assertion_failures=len(assertion_failures),
    )
    return row, {
        **row,
        "pages_detail": result["pages"],
        "errors_detail": result["errors"],
        "assertion_failures_detail": assertion_failures,
        "serve": serve_details,
    }


def _run_session_mode(
    *,
    target: str,
    binary: Path,
    pool: int,
    limit: int,
    url: str,
    timeout_seconds: float,
) -> tuple[dict[str, Any], dict[str, Any]]:
    serve = None
    started = time.perf_counter()
    try:
        serve = start_target_serve(target, binary, timeout_seconds)
        result = asyncio.run(
            asyncio.wait_for(
                _crawl_with_endpoint(
                    endpoint=serve.endpoint,
                    pool=pool,
                    limit=limit,
                    url=url,
                    timeout_seconds=timeout_seconds,
                ),
                timeout=timeout_seconds,
            )
        )
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        serve_details = stop_target_serve(serve)
        return _row_from_crawl_result(
            target=target,
            mode="session",
            pool=pool,
            limit=limit,
            url=url,
            elapsed_ms=elapsed_ms,
            result=result,
            serve_details=serve_details,
        )
    except Exception as error:
        serve_details = stop_target_serve(serve)
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        if isinstance(error, TimeoutError):
            error_message = f"crawler run exceeded {timeout_seconds:.1f}s wall timeout"
        else:
            error_message = str(error)
        row = {
            "target": target,
            "mode": "session",
            "pool": pool,
            "limit": limit,
            "url": url,
            "ok": False,
            "elapsed_ms": elapsed_ms,
            "pages": None,
            "expected_pages": _expected_pages(limit),
            "errors": 0,
            "assertion_failures": 0,
            "browser_peak_pss_bytes": serve_details.get("resources", {}).get("peak_pss_bytes"),
            "browser_peak_cpu_percent": serve_details.get("resources", {}).get("peak_cpu_percent"),
            "error": error_message,
            "failure_kind": _row_failure_kind(
                ok=False,
                pages=None,
                expected_pages=_expected_pages(limit),
                crawler_errors=0,
                assertion_failures=0,
                error=error_message,
            ),
        }
        return row, {**row, "serve": serve_details}


def _run_process_mode(
    *,
    target: str,
    binary: Path,
    pool: int,
    limit: int,
    url: str,
    timeout_seconds: float,
) -> tuple[dict[str, Any], dict[str, Any]]:
    serves: list[Any] = []
    started = time.perf_counter()

    async def run_crawl() -> dict[str, Any]:
        sessions: list[tuple[int, PageSession]] = []
        errors: list[dict[str, Any]] = []
        try:
            for worker_id in range(pool):
                try:
                    serve = start_target_serve(target, binary, timeout_seconds)
                    serves.append(serve)
                    sessions.append((worker_id, await _create_page_session(serve.endpoint, timeout_seconds)))
                except Exception as error:
                    errors.append({"worker": worker_id, "url": None, "stage": "create-process-session", "error": str(error)})
            return await _crawl_with_page_sessions(
                sessions=sessions,
                initial_errors=errors,
                limit=limit,
                url=url,
                timeout_seconds=timeout_seconds,
            )
        except BaseException:
            await _close_page_sessions(sessions)
            raise

    try:
        result = asyncio.run(asyncio.wait_for(run_crawl(), timeout=timeout_seconds))
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        stopped = [stop_target_serve(serve) for serve in serves]
        serves = []
        serve_details = {
            "mode": "process",
            "workers": stopped,
            "resources": _summarize_serve_resources(stopped),
        }
        return _row_from_crawl_result(
            target=target,
            mode="process",
            pool=pool,
            limit=limit,
            url=url,
            elapsed_ms=elapsed_ms,
            result=result,
            serve_details=serve_details,
        )
    except Exception as error:
        stopped = [stop_target_serve(serve) for serve in serves]
        serves = []
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        if isinstance(error, TimeoutError):
            error_message = f"crawler run exceeded {timeout_seconds:.1f}s wall timeout"
        else:
            error_message = str(error)
        row = {
            "target": target,
            "mode": "process",
            "pool": pool,
            "limit": limit,
            "url": url,
            "ok": False,
            "elapsed_ms": elapsed_ms,
            "pages": None,
            "expected_pages": _expected_pages(limit),
            "errors": 0,
            "assertion_failures": 0,
            "browser_peak_pss_bytes": None,
            "browser_peak_cpu_percent": None,
            "error": error_message,
            "failure_kind": _row_failure_kind(
                ok=False,
                pages=None,
                expected_pages=_expected_pages(limit),
                crawler_errors=0,
                assertion_failures=0,
                error=error_message,
            ),
        }
        return row, {
            **row,
            "serve": {
                "mode": "process",
                "workers": stopped,
                "resources": _summarize_serve_resources(stopped),
            },
        }


def run_amiibo_crawler_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    profile: str,
    targets: tuple[str, ...],
    pools: tuple[int, ...],
    modes: tuple[str, ...],
    runs: int,
    limit: int,
    timeout_seconds: float,
    gate_target: str,
    url: str = AMIIBO_URL,
) -> dict[str, Any]:
    if profile not in AMIIBO_PROFILES:
        raise RuntimeError(f"unknown Amiibo crawler profile: {profile}")
    targets = tuple(dict.fromkeys(normalize_cdp_target(target) for target in targets))
    gate_target = normalize_cdp_target(gate_target)
    unknown_targets = [target for target in targets if target not in TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown target(s): {', '.join(unknown_targets)}")
    unknown_modes = [mode for mode in modes if mode not in AMIIBO_MODES]
    if unknown_modes:
        raise RuntimeError(f"unknown Amiibo crawler mode(s): {', '.join(unknown_modes)}")
    invalid_pools = [pool for pool in pools if pool < 1]
    if invalid_pools:
        raise RuntimeError(f"invalid Amiibo crawler pool(s): {invalid_pools}")
    if limit < 0:
        raise RuntimeError("Amiibo crawler limit must be 0 or greater")
    if runs < 1:
        raise RuntimeError("Amiibo crawler runs must be at least 1")

    suite_dir = output_dir / "amiibo-crawler"
    rows: list[dict[str, Any]] = []
    details: list[dict[str, Any]] = []

    for target in targets:
        metadata = target_metadata(target)
        info = target_matrix.get(metadata["binary_key"], {})
        path = info.get("path")
        for run_id in range(1, runs + 1):
            for pool in pools:
                for mode in modes:
                    if not info.get("available") or not path:
                        row = {
                            "target": target,
                            **metadata,
                            "mode": mode,
                            "run": run_id,
                            "pool": pool,
                            "limit": limit,
                            "url": url,
                            "ok": False,
                            "elapsed_ms": None,
                            "pages": None,
                            "expected_pages": _expected_pages(limit),
                            "errors": 0,
                            "assertion_failures": 0,
                            "error": "target binary unavailable",
                            "failure_kind": "target-unavailable",
                        }
                        rows.append(row)
                        details.append(row)
                        continue
                    if mode == "session":
                        row, detail = _run_session_mode(
                            target=target,
                            binary=Path(path),
                            pool=pool,
                            limit=limit,
                            url=url,
                            timeout_seconds=timeout_seconds,
                        )
                    elif mode == "process":
                        row, detail = _run_process_mode(
                            target=target,
                            binary=Path(path),
                            pool=pool,
                            limit=limit,
                            url=url,
                            timeout_seconds=timeout_seconds,
                        )
                    else:
                        raise RuntimeError(f"unknown Amiibo crawler mode: {mode}")
                    row["run"] = run_id
                    row.update(metadata)
                    detail["run"] = run_id
                    detail.update(metadata)
                    rows.append(row)
                    details.append(detail)

    gate_failures = sum(1 for row in rows if row.get("target") == gate_target and not row.get("ok"))
    required_pools = set(AMIIBO_CONCURRENCY_MATRIX)
    selected_pools = set(pools)
    required_modes = set(AMIIBO_MODES)
    selected_modes = set(modes)
    formal_requirements = {
        "runs": {"actual": runs, "required": DEFAULT_AMIIBO_RUNS, "ok": runs >= DEFAULT_AMIIBO_RUNS},
        "limit": {"actual": limit, "required": 0, "ok": limit == 0},
        "expected_pages": {"actual": _expected_pages(limit), "required": AMIIBO_EXPECTED_PAGES, "ok": _expected_pages(limit) >= AMIIBO_EXPECTED_PAGES},
        "pool": {
            "actual": list(pools),
            "required": list(AMIIBO_CONCURRENCY_MATRIX),
            "ok": required_pools.issubset(selected_pools),
        },
        "modes": {
            "actual": list(modes),
            "required": list(AMIIBO_MODES),
            "ok": required_modes.issubset(selected_modes),
        },
    }
    profile_failures = (
        sum(1 for requirement in formal_requirements.values() if not bool(requirement["ok"]))
        if profile == "formal"
        else 0
    )
    summary: dict[str, Any] = {
        "suite": "amiibo-crawler",
        "profile": profile,
        "runs": runs,
        "url": url,
        "limit": limit,
        "expected_pages": _expected_pages(limit),
        "pool": list(pools),
        "modes": list(modes),
        "timeout_seconds": timeout_seconds,
        "gate_target": gate_target,
        "gate_failures": gate_failures + profile_failures,
        "profile_failures": profile_failures,
        "formal_requirements": formal_requirements,
        "targets": {},
        "total_failures": sum(1 for row in rows if not row.get("ok")),
    }
    for target in targets:
        target_rows = [row for row in rows if row.get("target") == target]
        target_summary = _summarize_rows(target_rows)
        target_summary.update(target_metadata(target))
        target_summary["modes"] = {
            mode: _summarize_rows([row for row in target_rows if row.get("mode") == mode])
            for mode in modes
        }
        summary["targets"][target] = target_summary

    write_csv(suite_dir / "raw-runs.csv", rows)
    write_json(suite_dir / "runs.json", details)
    write_json(suite_dir / "summary.json", summary)
    return summary
