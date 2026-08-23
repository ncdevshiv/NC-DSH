from __future__ import annotations

import asyncio
import gzip
import hashlib
import os
import shutil
import subprocess
import tarfile
import tempfile
import time
from pathlib import Path
from typing import Any
from urllib.parse import quote

from .artifacts import write_csv, write_json
from .config import REPO_ROOT, clear_proxy_env
from .process import run_process
from .sampling import snapshot_resources
from .serve import start_moli_serve, stop_moli_serve
from .stats import summarize
from .synthetic import SyntheticServer
from .versions import sha256_file


STARTUP_PROFILES = ("smoke", "formal")
FORMAL_STARTUP_RUNS = 10
FORMAL_STARTUP_WARM_PAGES = 10
FORMAL_STARTUP_IDLE_SECONDS = (1.0, 5.0, 30.0)


def _hash_output(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _cache_mode_fields(process_cache_mode: str, kernel_cache_mode: str) -> dict[str, str]:
    return {
        "process_cache_mode": process_cache_mode,
        "kernel_cache_mode": kernel_cache_mode,
    }


def _drop_os_cache_artifact(suite_dir: Path, case: str, run_id: int) -> dict[str, Any]:
    artifact_dir = suite_dir / "cache" / f"{case}-run-{run_id}"
    artifact_dir.mkdir(parents=True, exist_ok=True)
    artifact_path = artifact_dir / "drop-caches.txt"
    if os.geteuid() != 0:
        artifact_path.write_text("unavailable: root privileges required for drop_caches\n", encoding="utf-8")
        return {
            "requested": True,
            "ok": False,
            "mode": "warm-kernel",
            "error": "root privileges required for drop_caches",
            "artifact_path": str(artifact_path),
        }
    try:
        subprocess.run(["sync"], check=False)
        Path("/proc/sys/vm/drop_caches").write_text("3\n", encoding="utf-8")
    except OSError as error:
        artifact_path.write_text(f"unavailable: {error}\n", encoding="utf-8")
        return {
            "requested": True,
            "ok": False,
            "mode": "warm-kernel",
            "error": str(error),
            "artifact_path": str(artifact_path),
        }
    artifact_path.write_text("ok: wrote 3 to /proc/sys/vm/drop_caches\n", encoding="utf-8")
    return {
        "requested": True,
        "ok": True,
        "mode": "cold-os-cache",
        "artifact_path": str(artifact_path),
    }


def _compressed_binary_size(binary: Path) -> int:
    with tempfile.TemporaryDirectory(prefix="moli-benchmark-size-") as temp_dir:
        archive = Path(temp_dir) / "moli.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(binary, arcname=binary.name)
        return archive.stat().st_size


def _ldd_dependency_paths(binary: Path) -> tuple[list[Path], str | None]:
    ldd_bin = shutil.which("ldd")
    if ldd_bin is None:
        return [], "ldd executable unavailable"
    result = subprocess.run(
        [ldd_bin, str(binary)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    output = result.stdout + result.stderr
    if result.returncode != 0 and "not a dynamic executable" not in output:
        return [], output.strip() or f"ldd exited with {result.returncode}"

    paths: list[Path] = []
    seen: set[Path] = set()
    for raw_line in output.splitlines():
        line = raw_line.strip()
        if not line or line.startswith("linux-vdso"):
            continue
        candidate = ""
        if "=>" in line:
            candidate = line.split("=>", 1)[1].strip().split(" ", 1)[0]
        else:
            candidate = line.split(" ", 1)[0]
        if not candidate.startswith("/"):
            continue
        path = Path(candidate)
        if path.exists() and path not in seen:
            seen.add(path)
            paths.append(path)
    return paths, None


def _add_path_to_tar(tar: tarfile.TarFile, source: Path, arcname: str) -> None:
    tar.add(source, arcname=arcname, recursive=False)
    if source.is_symlink():
        resolved = source.resolve()
        if resolved.exists():
            tar.add(resolved, arcname=str(resolved).lstrip("/"), recursive=False)


def _build_minimal_rootfs_image_artifacts(binary: Path, suite_dir: Path) -> dict[str, Any]:
    image_dir = suite_dir / "image-size"
    image_dir.mkdir(parents=True, exist_ok=True)
    dependencies, dependency_error = _ldd_dependency_paths(binary)
    rootfs_tar = image_dir / "moli-rootfs.tar"
    rootfs_targz = image_dir / "moli-rootfs.tar.gz"
    manifest = {
        "format": "minimal-rootfs-tar",
        "binary": str(binary),
        "binary_arcname": "usr/local/bin/moli",
        "dependencies": [str(path) for path in dependencies],
        "dependency_error": dependency_error,
        "note": "Daemonless startup/deploy size artifact. Container image measurement is intentionally out of scope.",
    }
    write_json(image_dir / "manifest.json", manifest)
    with tarfile.open(rootfs_tar, "w") as tar:
        _add_path_to_tar(tar, binary, "usr/local/bin/moli")
        for dependency in dependencies:
            _add_path_to_tar(tar, dependency, str(dependency).lstrip("/"))
    with rootfs_tar.open("rb") as source, rootfs_targz.open("wb") as target:
        with gzip.GzipFile(fileobj=target, mode="wb", compresslevel=1, mtime=0) as gzip_file:
            shutil.copyfileobj(source, gzip_file)
    return {
        "format": manifest["format"],
        "uncompressed_bytes": rootfs_tar.stat().st_size,
        "compressed_bytes": rootfs_targz.stat().st_size,
        "dependency_count": len(dependencies),
        "dependency_error": dependency_error,
        "rootfs_tar_path": str(rootfs_tar),
        "rootfs_targz_path": str(rootfs_targz),
        "manifest_path": str(image_dir / "manifest.json"),
    }


def _stripped_binary_size(binary: Path) -> tuple[int | None, str | None]:
    strip_bin = shutil.which("strip")
    if strip_bin is None:
        return None, "strip executable unavailable"
    with tempfile.TemporaryDirectory(prefix="moli-benchmark-strip-") as temp_dir:
        candidate = Path(temp_dir) / binary.name
        shutil.copy2(binary, candidate)
        result = subprocess.run(
            [strip_bin, "--strip-unneeded", str(candidate)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0:
            stderr = result.stderr.decode("utf-8", errors="replace").strip()
            return None, stderr or f"strip exited with {result.returncode}"
        return candidate.stat().st_size, None


def _time_verbose_path(suite_dir: Path, case: str, run_id: int) -> Path:
    return suite_dir / "time" / f"{case}-run-{run_id}.time.txt"


def _cgroup_artifact_dir(suite_dir: Path, case: str, run_id: int) -> Path:
    return suite_dir / "cgroup" / f"{case}-run-{run_id}"


def _time_verbose_row_fields(time_verbose: dict[str, Any] | None) -> dict[str, Any]:
    if not time_verbose:
        return {
            "time_max_rss_bytes": None,
            "time_elapsed_seconds": None,
            "time_user_seconds": None,
            "time_system_seconds": None,
            "time_raw_path": None,
        }
    return {
        "time_max_rss_bytes": time_verbose.get("max_rss_bytes"),
        "time_elapsed_seconds": time_verbose.get("elapsed_seconds"),
        "time_user_seconds": time_verbose.get("user_seconds"),
        "time_system_seconds": time_verbose.get("system_seconds"),
        "time_raw_path": time_verbose.get("raw_path"),
    }


def _startup_formal_gate_rows(
    *,
    profile: str,
    runs: int,
    include_cdp_first_page: bool,
    include_cdp_warm_pages: bool,
    cdp_warm_pages: int,
    idle_seconds: tuple[float, ...],
    total_failures: int,
) -> list[dict[str, Any]]:
    idle_set = {float(seconds) for seconds in idle_seconds}
    requirements = [
        {
            "gate": "profile",
            "ok": profile == "formal",
            "actual": profile,
            "required": "formal",
            "failure_kind": "profile-requirement",
        },
        {
            "gate": "runs",
            "ok": runs >= FORMAL_STARTUP_RUNS,
            "actual": runs,
            "required": FORMAL_STARTUP_RUNS,
            "failure_kind": "profile-requirement",
        },
        {
            "gate": "cdp-first-page",
            "ok": include_cdp_first_page,
            "actual": include_cdp_first_page,
            "required": True,
            "failure_kind": "profile-requirement",
        },
        {
            "gate": "cdp-warm-pages",
            "ok": include_cdp_warm_pages and cdp_warm_pages >= FORMAL_STARTUP_WARM_PAGES,
            "actual": {"enabled": include_cdp_warm_pages, "pages": cdp_warm_pages if include_cdp_warm_pages else 0},
            "required": {"enabled": True, "pages": FORMAL_STARTUP_WARM_PAGES},
            "failure_kind": "profile-requirement",
        },
        {
            "gate": "idle-footprint",
            "ok": all(required in idle_set for required in FORMAL_STARTUP_IDLE_SECONDS),
            "actual": sorted(idle_set),
            "required": list(FORMAL_STARTUP_IDLE_SECONDS),
            "failure_kind": "profile-requirement",
        },
        {
            "gate": "workload-failures",
            "ok": total_failures == 0,
            "actual": total_failures,
            "required": 0,
            "failure_kind": "workload-failure",
        },
    ]
    return requirements


async def _cdp_first_page_flow(endpoint: str, timeout_seconds: float) -> dict[str, Any]:
    from .raw_cdp import connect_raw_cdp

    client = await connect_raw_cdp(endpoint)
    try:
        target_command_id = await client.send("Target.createTarget", {"url": "about:blank"})
        target_response, seen = await client.recv_until_id(target_command_id, timeout=timeout_seconds)
        target_id = target_response.get("result", {}).get("targetId")
        if not isinstance(target_id, str) or not target_id:
            raise RuntimeError(f"missing targetId in {target_response}")

        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, attach_seen = await client.recv_until_id(attach_id, timeout=timeout_seconds)
        seen.extend(attach_seen)
        session_id = attach_response.get("result", {}).get("sessionId")
        if not isinstance(session_id, str) or not session_id:
            raise RuntimeError(f"missing sessionId in {attach_response}")

        for method in ("Page.enable", "Runtime.enable"):
            command_id = await client.send(method, session_id=session_id)
            _, method_seen = await client.recv_until_id(command_id, timeout=timeout_seconds)
            seen.extend(method_seen)

        html = "<!doctype html><title>cdp-first-page</title><body data-benchmark-status='ok'>ok</body>"
        navigate_id = await client.send(
            "Page.navigate",
            {"url": "data:text/html;charset=utf-8," + quote(html, safe=":/;=,'")},
            session_id=session_id,
        )
        _, navigate_seen = await client.recv_until_id(navigate_id, timeout=timeout_seconds)
        seen.extend(navigate_seen)

        expression = """
        new Promise(resolve => {
          const deadline = Date.now() + 10000;
          function tick() {
            if (document.body && document.body.dataset.benchmarkStatus === 'ok') {
              resolve(document.title + ':' + document.body.textContent.trim());
            } else if (Date.now() > deadline) {
              resolve('timeout');
            } else {
              setTimeout(tick, 10);
            }
          }
          tick();
        })
        """
        evaluate_id = await client.send(
            "Runtime.evaluate",
            {"expression": expression, "awaitPromise": True, "returnByValue": True},
            session_id=session_id,
        )
        evaluate_response, evaluate_seen = await client.recv_until_id(evaluate_id, timeout=timeout_seconds + 1)
        seen.extend(evaluate_seen)
        value = evaluate_response.get("result", {}).get("result", {}).get("value")
        return {
            "value": value,
            "ok": value == "cdp-first-page:ok",
            "messages": len(seen),
            "command_count": client.command_count,
        }
    finally:
        await client.websocket.close()


async def _cdp_create_page(client: Any, timeout_seconds: float, page_index: int) -> dict[str, Any]:
    started = time.perf_counter()
    seen: list[dict[str, Any]] = []
    html = f"<!doctype html><title>cdp-warm-page-{page_index}</title><body data-benchmark-status='ok'>ok</body>"

    target_command_id = await client.send("Target.createTarget", {"url": "about:blank"})
    target_response, target_seen = await client.recv_until_id(target_command_id, timeout=timeout_seconds)
    seen.extend(target_seen)
    target_id = target_response.get("result", {}).get("targetId")
    if not isinstance(target_id, str) or not target_id:
        raise RuntimeError(f"missing targetId in {target_response}")

    try:
        attach_id = await client.send("Target.attachToTarget", {"targetId": target_id, "flatten": True})
        attach_response, attach_seen = await client.recv_until_id(attach_id, timeout=timeout_seconds)
        seen.extend(attach_seen)
        session_id = attach_response.get("result", {}).get("sessionId")
        if not isinstance(session_id, str) or not session_id:
            raise RuntimeError(f"missing sessionId in {attach_response}")

        for method in ("Page.enable", "Runtime.enable"):
            command_id = await client.send(method, session_id=session_id)
            _, method_seen = await client.recv_until_id(command_id, timeout=timeout_seconds)
            seen.extend(method_seen)

        navigate_id = await client.send(
            "Page.navigate",
            {"url": "data:text/html;charset=utf-8," + quote(html, safe=":/;=,'")},
            session_id=session_id,
        )
        _, navigate_seen = await client.recv_until_id(navigate_id, timeout=timeout_seconds)
        seen.extend(navigate_seen)

        evaluate_id = await client.send(
            "Runtime.evaluate",
            {
                "expression": "document.body && document.body.dataset.benchmarkStatus === 'ok'",
                "returnByValue": True,
            },
            session_id=session_id,
        )
        evaluate_response, evaluate_seen = await client.recv_until_id(evaluate_id, timeout=timeout_seconds)
        seen.extend(evaluate_seen)
        ok = evaluate_response.get("result", {}).get("result", {}).get("value") is True
        return {
            "page_index": page_index,
            "ok": ok,
            "elapsed_ms": (time.perf_counter() - started) * 1000.0,
            "target_id": target_id,
            "messages": len(seen),
        }
    finally:
        try:
            close_id = await client.send("Target.closeTarget", {"targetId": target_id})
            await client.recv_until_id(close_id, timeout=3)
        except Exception:
            pass


async def _cdp_warm_pages_flow(endpoint: str, timeout_seconds: float, pages: int) -> dict[str, Any]:
    from .raw_cdp import connect_raw_cdp

    client = await connect_raw_cdp(endpoint)
    try:
        page_results = []
        for page_index in range(1, pages + 1):
            page_results.append(await _cdp_create_page(client, timeout_seconds, page_index))
        return {
            "ok": all(bool(page.get("ok")) for page in page_results),
            "pages": page_results,
            "command_count": client.command_count,
        }
    finally:
        await client.websocket.close()


def _run_cdp_first_page(
    moli_bin: Path,
    timeout_seconds: float,
    *,
    time_verbose_path: Path | None = None,
    cgroup_artifact_dir: Path | None = None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    handle = None
    started = time.perf_counter()
    try:
        handle = start_moli_serve(
            moli_bin,
            timeout_seconds,
            time_verbose_path=time_verbose_path,
            cgroup_artifact_dir=cgroup_artifact_dir,
        )
        result = asyncio.run(_cdp_first_page_flow(handle.endpoint, timeout_seconds))
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        stop_details = stop_moli_serve(handle)
        row = {
            "case": "cdp-first-page",
            "ok": bool(result.get("ok")),
            "elapsed_ms": elapsed_ms,
            "serve_ready_ms": handle.ready_ms,
            "peak_pss_bytes": stop_details.get("resources", {}).get("peak_pss_bytes"),
            "peak_rss_bytes": stop_details.get("resources", {}).get("peak_rss_bytes"),
            "peak_cpu_percent": stop_details.get("resources", {}).get("peak_cpu_percent"),
            "peak_process_count": stop_details.get("resources", {}).get("peak_process_count"),
            "peak_thread_count": stop_details.get("resources", {}).get("peak_thread_count"),
            "peak_fd_count": stop_details.get("resources", {}).get("peak_fd_count"),
            "cdp_value": result.get("value"),
            "cdp_messages": result.get("messages"),
            "cdp_command_count": result.get("command_count"),
            **_time_verbose_row_fields(stop_details.get("time_verbose")),
        }
        return row, {**row, "serve": stop_details}
    except Exception as error:
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        stop_details = stop_moli_serve(handle)
        row = {
            "case": "cdp-first-page",
            "ok": False,
            "elapsed_ms": elapsed_ms,
            "error": str(error),
            **_time_verbose_row_fields(stop_details.get("time_verbose")),
        }
        return row, {**row, "serve": stop_details}


def _run_cdp_warm_pages(
    moli_bin: Path,
    timeout_seconds: float,
    *,
    pages: int,
    time_verbose_path: Path | None = None,
    cgroup_artifact_dir: Path | None = None,
) -> tuple[dict[str, Any], dict[str, Any]]:
    handle = None
    started = time.perf_counter()
    try:
        handle = start_moli_serve(
            moli_bin,
            timeout_seconds,
            time_verbose_path=time_verbose_path,
            cgroup_artifact_dir=cgroup_artifact_dir,
        )
        result = asyncio.run(_cdp_warm_pages_flow(handle.endpoint, timeout_seconds, pages))
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        page_results = [page for page in result.get("pages", []) if isinstance(page, dict)]
        page_elapsed = [
            float(page["elapsed_ms"])
            for page in page_results
            if page.get("ok") and page.get("elapsed_ms") is not None
        ]
        page_elapsed_summary = summarize(page_elapsed)
        stop_details = stop_moli_serve(handle)
        row = {
            "case": "cdp-warm-pages",
            "ok": bool(result.get("ok")) and len(page_results) == pages,
            "elapsed_ms": elapsed_ms,
            "serve_ready_ms": handle.ready_ms,
            "cdp_pages": pages,
            "cdp_page_passes": sum(1 for page in page_results if page.get("ok")),
            "cdp_page_elapsed_p50_ms": page_elapsed_summary.get("p50"),
            "cdp_page_elapsed_p95_ms": page_elapsed_summary.get("p95"),
            "peak_pss_bytes": stop_details.get("resources", {}).get("peak_pss_bytes"),
            "peak_rss_bytes": stop_details.get("resources", {}).get("peak_rss_bytes"),
            "peak_cpu_percent": stop_details.get("resources", {}).get("peak_cpu_percent"),
            "peak_process_count": stop_details.get("resources", {}).get("peak_process_count"),
            "peak_thread_count": stop_details.get("resources", {}).get("peak_thread_count"),
            "peak_fd_count": stop_details.get("resources", {}).get("peak_fd_count"),
            "cdp_command_count": result.get("command_count"),
            **_time_verbose_row_fields(stop_details.get("time_verbose")),
        }
        return row, {**row, "serve": stop_details, "pages": page_results}
    except Exception as error:
        elapsed_ms = (time.perf_counter() - started) * 1000.0
        stop_details = stop_moli_serve(handle)
        row = {
            "case": "cdp-warm-pages",
            "ok": False,
            "elapsed_ms": elapsed_ms,
            "cdp_pages": pages,
            "error": str(error),
            **_time_verbose_row_fields(stop_details.get("time_verbose")),
        }
        return row, {**row, "serve": stop_details}


def run_startup_suite(
    *,
    moli_bin: Path,
    output_dir: Path,
    profile: str = "smoke",
    runs: int,
    timeout_seconds: float,
    include_cdp_first_page: bool = False,
    include_cdp_warm_pages: bool = False,
    cdp_warm_pages: int = 10,
    idle_seconds: tuple[float, ...] = (),
    drop_os_cache: bool = False,
) -> dict[str, Any]:
    if profile not in STARTUP_PROFILES:
        raise RuntimeError(f"startup profile must be one of: {', '.join(STARTUP_PROFILES)}")
    if include_cdp_warm_pages and cdp_warm_pages < 1:
        raise RuntimeError("cdp_warm_pages must be at least 1 when warm page startup is enabled")
    suite_dir = output_dir / "startup"
    rows: list[dict[str, Any]] = []
    details: list[dict[str, Any]] = []
    cache_events: list[dict[str, Any]] = []

    def prepare_cache(case: str, run_id: int) -> dict[str, str]:
        if not drop_os_cache:
            return _cache_mode_fields("cold-process", "warm-kernel")
        event = _drop_os_cache_artifact(suite_dir, case, run_id)
        cache_events.append({"case": case, "run": run_id, **event})
        return _cache_mode_fields("cold-process", str(event["mode"]))

    stripped_bytes, stripped_error = _stripped_binary_size(moli_bin)
    binary_row = {
        "case": "binary-size",
        "run": 1,
        "ok": True,
        "elapsed_ms": 0.0,
        "binary_bytes": moli_bin.stat().st_size,
        "stripped_binary_bytes": stripped_bytes,
        "stripped_available": stripped_bytes is not None,
        "stripped_error": stripped_error,
        "tar_gz_bytes": _compressed_binary_size(moli_bin),
        "sha256": sha256_file(moli_bin),
        **_cache_mode_fields("artifact", "not-applicable"),
    }
    rows.append(binary_row)
    details.append(binary_row)

    image_started = time.perf_counter()
    image_artifacts = _build_minimal_rootfs_image_artifacts(moli_bin, suite_dir)
    image_row = {
        "case": "image-size",
        "run": 1,
        "ok": True,
        "elapsed_ms": (time.perf_counter() - image_started) * 1000.0,
        "image_format": image_artifacts["format"],
        "image_uncompressed_bytes": image_artifacts["uncompressed_bytes"],
        "image_compressed_bytes": image_artifacts["compressed_bytes"],
        "image_dependency_count": image_artifacts["dependency_count"],
        "image_dependency_error": image_artifacts["dependency_error"],
        "image_rootfs_tar_path": image_artifacts["rootfs_tar_path"],
        "image_rootfs_targz_path": image_artifacts["rootfs_targz_path"],
        "image_manifest_path": image_artifacts["manifest_path"],
        **_cache_mode_fields("artifact", "not-applicable"),
    }
    rows.append(image_row)
    details.append({**image_row, "image": image_artifacts})

    for run_id in range(1, runs + 1):
        handle = None
        cache_fields = prepare_cache("serve-ready", run_id)
        try:
            handle = start_moli_serve(
                moli_bin,
                timeout_seconds,
                time_verbose_path=_time_verbose_path(suite_dir, "serve-ready", run_id),
                cgroup_artifact_dir=_cgroup_artifact_dir(suite_dir, "serve-ready", run_id),
            )
            stop_details = stop_moli_serve(handle)
            row = {
                "case": "serve-ready",
                "run": run_id,
                "ok": True,
                "elapsed_ms": handle.ready_ms,
                "peak_pss_bytes": stop_details.get("resources", {}).get("peak_pss_bytes"),
                "peak_rss_bytes": stop_details.get("resources", {}).get("peak_rss_bytes"),
                "peak_cpu_percent": stop_details.get("resources", {}).get("peak_cpu_percent"),
                "peak_process_count": stop_details.get("resources", {}).get("peak_process_count"),
                "peak_thread_count": stop_details.get("resources", {}).get("peak_thread_count"),
                "peak_fd_count": stop_details.get("resources", {}).get("peak_fd_count"),
                **cache_fields,
                **_time_verbose_row_fields(stop_details.get("time_verbose")),
            }
            rows.append(row)
            details.append({**row, "serve": stop_details})
        except Exception as error:
            stop_details = stop_moli_serve(handle)
            row = {
                "case": "serve-ready",
                "run": run_id,
                "ok": False,
                "elapsed_ms": None,
                "error": str(error),
                **cache_fields,
                **_time_verbose_row_fields(stop_details.get("time_verbose")),
            }
            rows.append(row)
            details.append({**row, "serve": stop_details})

    for run_id in range(1, runs + 1):
        cache_fields = prepare_cache("cli-fetch-aboutblank", run_id)
        result = run_process(
            [
                str(moli_bin),
                "fetch",
                "--dump",
                "html",
                "--wait-until",
                "load",
                "--timeout",
                str(int(timeout_seconds * 1000)),
                "about:blank",
            ],
            cwd=REPO_ROOT,
            timeout_seconds=timeout_seconds,
            env=clear_proxy_env(os.environ),
            time_verbose_path=_time_verbose_path(suite_dir, "cli-fetch-aboutblank", run_id),
            cgroup_artifact_dir=_cgroup_artifact_dir(suite_dir, "cli-fetch-aboutblank", run_id),
        )
        row = {
            "case": "cli-fetch-aboutblank",
            "run": run_id,
            "ok": result.returncode == 0 and not result.timed_out,
            "elapsed_ms": result.elapsed_ms,
            "returncode": result.returncode,
            "timed_out": result.timed_out,
            "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
            "peak_rss_bytes": result.resources.get("peak_rss_bytes"),
            "peak_cpu_percent": result.resources.get("peak_cpu_percent"),
            "peak_process_count": result.resources.get("peak_process_count"),
            "peak_thread_count": result.resources.get("peak_thread_count"),
            "peak_fd_count": result.resources.get("peak_fd_count"),
            "output_sha256": _hash_output(result.output_digest_material()),
            **cache_fields,
            **_time_verbose_row_fields(result.time_verbose),
        }
        rows.append(row)
        details.append({**row, "process": result.json_summary(include_output=True)})

    if include_cdp_first_page:
        for run_id in range(1, runs + 1):
            cache_fields = prepare_cache("cdp-first-page", run_id)
            row, detail = _run_cdp_first_page(
                moli_bin,
                timeout_seconds,
                time_verbose_path=_time_verbose_path(suite_dir, "cdp-first-page", run_id),
                cgroup_artifact_dir=_cgroup_artifact_dir(suite_dir, "cdp-first-page", run_id),
            )
            row["run"] = run_id
            row.update(cache_fields)
            detail["run"] = run_id
            detail.update(cache_fields)
            rows.append(row)
            details.append(detail)

    if include_cdp_warm_pages:
        for run_id in range(1, runs + 1):
            cache_fields = prepare_cache("cdp-warm-pages", run_id)
            row, detail = _run_cdp_warm_pages(
                moli_bin,
                timeout_seconds,
                pages=cdp_warm_pages,
                time_verbose_path=_time_verbose_path(suite_dir, "cdp-warm-pages", run_id),
                cgroup_artifact_dir=_cgroup_artifact_dir(suite_dir, "cdp-warm-pages", run_id),
            )
            row["run"] = run_id
            row.update(cache_fields)
            detail["run"] = run_id
            detail.update(cache_fields)
            rows.append(row)
            details.append(detail)

    for seconds in idle_seconds:
        for run_id in range(1, runs + 1):
            handle = None
            started = time.perf_counter()
            cache_fields = prepare_cache(f"idle-footprint-{seconds:g}s", run_id)
            try:
                handle = start_moli_serve(
                    moli_bin,
                    timeout_seconds,
                    time_verbose_path=_time_verbose_path(suite_dir, f"idle-footprint-{seconds:g}s", run_id),
                    cgroup_artifact_dir=_cgroup_artifact_dir(suite_dir, f"idle-footprint-{seconds:g}s", run_id),
                )
                time.sleep(seconds)
                snapshot = snapshot_resources(handle.process.pid)
                elapsed_ms = (time.perf_counter() - started) * 1000.0
                stop_details = stop_moli_serve(handle)
                row = {
                    "case": f"idle-footprint-{seconds:g}s",
                    "run": run_id,
                    "ok": True,
                    "elapsed_ms": elapsed_ms,
                    "idle_seconds": seconds,
                    "serve_ready_ms": handle.ready_ms,
                    "pss_bytes": snapshot.get("pss_bytes"),
                    "rss_bytes": snapshot.get("rss_bytes"),
                    "cpu_percent": snapshot.get("cpu_percent"),
                    "process_count": snapshot.get("process_count"),
                    "thread_count": snapshot.get("thread_count"),
                    "fd_count": snapshot.get("fd_count"),
                    "peak_pss_bytes": stop_details.get("resources", {}).get("peak_pss_bytes"),
                    "peak_rss_bytes": stop_details.get("resources", {}).get("peak_rss_bytes"),
                    "peak_cpu_percent": stop_details.get("resources", {}).get("peak_cpu_percent"),
                    "peak_process_count": stop_details.get("resources", {}).get("peak_process_count"),
                    "peak_thread_count": stop_details.get("resources", {}).get("peak_thread_count"),
                    "peak_fd_count": stop_details.get("resources", {}).get("peak_fd_count"),
                    **cache_fields,
                    **_time_verbose_row_fields(stop_details.get("time_verbose")),
                }
                rows.append(row)
                details.append({**row, "snapshot": snapshot, "serve": stop_details})
            except Exception as error:
                elapsed_ms = (time.perf_counter() - started) * 1000.0
                stop_details = stop_moli_serve(handle)
                row = {
                    "case": f"idle-footprint-{seconds:g}s",
                    "run": run_id,
                    "ok": False,
                    "elapsed_ms": elapsed_ms,
                    "idle_seconds": seconds,
                    "error": str(error),
                    **cache_fields,
                    **_time_verbose_row_fields(stop_details.get("time_verbose")),
                }
                rows.append(row)
                details.append({**row, "serve": stop_details})

    with SyntheticServer() as server:
        for run_id in range(1, runs + 1):
            cache_fields = prepare_cache("cli-fetch-local-js", run_id)
            result = run_process(
                [
                    str(moli_bin),
                    "fetch",
                    "--dump",
                    "html",
                    "--wait-until",
                    "done",
                    "--wait-script",
                    "document.querySelector('[data-benchmark-status=\"ok\"]') !== null",
                    "--timeout",
                    str(int(timeout_seconds * 1000)),
                    f"{server.base_url}/dynamic-script",
                ],
                cwd=REPO_ROOT,
                timeout_seconds=timeout_seconds + 1,
                env=clear_proxy_env(os.environ),
                time_verbose_path=_time_verbose_path(suite_dir, "cli-fetch-local-js", run_id),
                cgroup_artifact_dir=_cgroup_artifact_dir(suite_dir, "cli-fetch-local-js", run_id),
            )
            row = {
                "case": "cli-fetch-local-js",
                "run": run_id,
                "ok": result.returncode == 0 and not result.timed_out,
                "elapsed_ms": result.elapsed_ms,
                "returncode": result.returncode,
                "timed_out": result.timed_out,
                "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
                "peak_rss_bytes": result.resources.get("peak_rss_bytes"),
                "peak_cpu_percent": result.resources.get("peak_cpu_percent"),
                "peak_process_count": result.resources.get("peak_process_count"),
                "peak_thread_count": result.resources.get("peak_thread_count"),
                "peak_fd_count": result.resources.get("peak_fd_count"),
                "output_sha256": _hash_output(result.output_digest_material()),
                **cache_fields,
                **_time_verbose_row_fields(result.time_verbose),
            }
            rows.append(row)
            details.append({**row, "process": result.json_summary(include_output=not row["ok"])})

    cases = sorted({str(row["case"]) for row in rows})
    elapsed_by_case: dict[str, list[float]] = {case: [] for case in cases}
    pss_by_case: dict[str, list[float]] = {case: [] for case in cases}
    rss_by_case: dict[str, list[float]] = {case: [] for case in cases}
    process_by_case: dict[str, list[float]] = {case: [] for case in cases}
    thread_by_case: dict[str, list[float]] = {case: [] for case in cases}
    fd_by_case: dict[str, list[float]] = {case: [] for case in cases}
    time_rss_by_case: dict[str, list[float]] = {case: [] for case in cases}
    for row in rows:
        case = str(row["case"])
        if row.get("ok") and row.get("elapsed_ms") is not None:
            elapsed_by_case.setdefault(case, []).append(float(row["elapsed_ms"]))
        for target, keys in (
            (pss_by_case, ("pss_bytes", "peak_pss_bytes")),
            (rss_by_case, ("rss_bytes", "peak_rss_bytes")),
            (process_by_case, ("process_count", "peak_process_count")),
            (thread_by_case, ("thread_count", "peak_thread_count")),
            (fd_by_case, ("fd_count", "peak_fd_count")),
            (time_rss_by_case, ("time_max_rss_bytes",)),
        ):
            for key in keys:
                if row.get("ok") and row.get(key) is not None:
                    target.setdefault(case, []).append(float(row[key]))
                    break

    time_verbose_entries = []
    cgroup_entries = []
    for detail in details:
        process_time = (
            detail.get("process", {}).get("time_verbose")
            if isinstance(detail.get("process"), dict)
            else None
        )
        process_cgroup = detail.get("process", {}).get("cgroup") if isinstance(detail.get("process"), dict) else None
        serve_time = (
            detail.get("serve", {}).get("time_verbose")
            if isinstance(detail.get("serve"), dict)
            else None
        )
        serve_cgroup = detail.get("serve", {}).get("cgroup") if isinstance(detail.get("serve"), dict) else None
        for entry in (process_time, serve_time):
            if isinstance(entry, dict):
                time_verbose_entries.append(entry)
        for entry in (process_cgroup, serve_cgroup):
            if isinstance(entry, dict):
                cgroup_entries.append(entry)

    total_failures = sum(1 for row in rows if not row.get("ok"))
    formal_gate_rows = _startup_formal_gate_rows(
        profile=profile,
        runs=runs,
        include_cdp_first_page=include_cdp_first_page,
        include_cdp_warm_pages=include_cdp_warm_pages,
        cdp_warm_pages=cdp_warm_pages,
        idle_seconds=tuple(float(seconds) for seconds in idle_seconds),
        total_failures=total_failures,
    )
    gate_failures = sum(1 for row in formal_gate_rows if row.get("ok") is not True)
    summary = {
        "suite": "startup",
        "profile": profile,
        "runs": runs,
        "timeout_seconds": timeout_seconds,
        "include_cdp_first_page": include_cdp_first_page,
        "include_cdp_warm_pages": include_cdp_warm_pages,
        "cdp_warm_pages": cdp_warm_pages if include_cdp_warm_pages else 0,
        "idle_seconds": list(idle_seconds),
        "drop_os_cache": drop_os_cache,
        "cache_events": cache_events,
        "cache_artifacts": [
            event.get("artifact_path") for event in cache_events if isinstance(event.get("artifact_path"), str)
        ],
        "time_verbose_available": any(entry.get("available") for entry in time_verbose_entries),
        "time_verbose_artifacts": [
            entry.get("raw_path") for entry in time_verbose_entries if isinstance(entry.get("raw_path"), str)
        ],
        "cgroup_available": any(entry.get("available") for entry in cgroup_entries),
        "cgroup_artifacts": [
            artifact
            for entry in cgroup_entries
            for artifact in entry.get("artifacts", [])
            if isinstance(artifact, str)
        ],
        "cases": {
            case: {
                "elapsed_ms": summarize(values),
                "pss_bytes": summarize(pss_by_case.get(case, [])),
                "rss_bytes": summarize(rss_by_case.get(case, [])),
                "process_count": summarize(process_by_case.get(case, [])),
                "thread_count": summarize(thread_by_case.get(case, [])),
                "fd_count": summarize(fd_by_case.get(case, [])),
                "time_max_rss_bytes": summarize(time_rss_by_case.get(case, [])),
                "binary_bytes": next((row.get("binary_bytes") for row in rows if row.get("case") == case), None),
                "stripped_binary_bytes": next(
                    (row.get("stripped_binary_bytes") for row in rows if row.get("case") == case),
                    None,
                ),
                "stripped_available": next(
                    (row.get("stripped_available") for row in rows if row.get("case") == case),
                    None,
                ),
                "tar_gz_bytes": next((row.get("tar_gz_bytes") for row in rows if row.get("case") == case), None),
                "image_format": next((row.get("image_format") for row in rows if row.get("case") == case), None),
                "image_uncompressed_bytes": next(
                    (row.get("image_uncompressed_bytes") for row in rows if row.get("case") == case),
                    None,
                ),
                "image_compressed_bytes": next(
                    (row.get("image_compressed_bytes") for row in rows if row.get("case") == case),
                    None,
                ),
                "image_dependency_count": next(
                    (row.get("image_dependency_count") for row in rows if row.get("case") == case),
                    None,
                ),
                "cdp_page_elapsed_p50_ms": next(
                    (row.get("cdp_page_elapsed_p50_ms") for row in rows if row.get("case") == case),
                    None,
                ),
                "cdp_page_elapsed_p95_ms": next(
                    (row.get("cdp_page_elapsed_p95_ms") for row in rows if row.get("case") == case),
                    None,
                ),
                "cdp_pages": next((row.get("cdp_pages") for row in rows if row.get("case") == case), None),
                "cdp_page_passes": next(
                    (row.get("cdp_page_passes") for row in rows if row.get("case") == case),
                    None,
                ),
                "process_cache_modes": sorted(
                    {
                        str(row.get("process_cache_mode"))
                        for row in rows
                        if row.get("case") == case and row.get("process_cache_mode") is not None
                    }
                ),
                "kernel_cache_modes": sorted(
                    {
                        str(row.get("kernel_cache_mode"))
                        for row in rows
                        if row.get("case") == case and row.get("kernel_cache_mode") is not None
                    }
                ),
                "failures": sum(1 for row in rows if row.get("case") == case and not row.get("ok")),
            }
            for case, values in elapsed_by_case.items()
        },
        "formal_gate_rows": formal_gate_rows,
        "gate_failures": gate_failures,
        "total_failures": total_failures,
    }
    write_csv(suite_dir / "runs.csv", rows)
    write_json(suite_dir / "runs.json", details)
    write_json(suite_dir / "gate-rows.json", formal_gate_rows)
    write_json(suite_dir / "summary.json", summary)
    return summary
