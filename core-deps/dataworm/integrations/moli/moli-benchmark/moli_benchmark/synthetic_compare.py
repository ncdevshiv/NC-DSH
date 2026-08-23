from __future__ import annotations

import os
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json
from .config import REPO_ROOT, clear_proxy_env
from .process import run_process
from .stats import summarize
from .synthetic import SYNTHETIC_CASES, SyntheticServer, _hash_output


BASE_TARGETS = ("moli", "lightpanda", "chrome", "obscura")
FETCH_TARGETS = (*BASE_TARGETS, "moli-full")
CDP_TARGETS = (
    "moli-cdp",
    "lightpanda-cdp",
    "chrome-cdp",
    "obscura-cdp",
    "moli-full-cdp",
)
TARGETS = (*FETCH_TARGETS, *CDP_TARGETS)
WEBFETCH_TARGETS = (
    "moli",
    "moli-cdp",
    "moli-full",
    "moli-full-cdp",
    "lightpanda",
    "lightpanda-cdp",
    "chrome",
    "obscura",
    "obscura-cdp",
)

TARGET_METADATA: dict[str, dict[str, str]] = {
    "moli": {"engine": "moli", "driver": "fetch", "label": "moli / fetch", "binary_key": "moli"},
    "moli-cdp": {"engine": "moli", "driver": "cdp", "label": "moli / cdp", "binary_key": "moli"},
    "moli-full": {"engine": "moli", "driver": "fetch-full", "label": "moli full / fetch", "binary_key": "moli"},
    "moli-full-cdp": {"engine": "moli", "driver": "cdp-full", "label": "moli full / cdp", "binary_key": "moli"},
    "lightpanda": {"engine": "lightpanda", "driver": "fetch", "label": "lightpanda / fetch", "binary_key": "lightpanda"},
    "lightpanda-cdp": {"engine": "lightpanda", "driver": "cdp", "label": "lightpanda / cdp", "binary_key": "lightpanda"},
    "chrome": {"engine": "chrome", "driver": "dump-dom", "label": "chrome / dump-dom", "binary_key": "chrome"},
    "chrome-cdp": {"engine": "chrome", "driver": "cdp", "label": "chrome / cdp", "binary_key": "chrome"},
    "obscura": {"engine": "obscura", "driver": "fetch", "label": "obscura / fetch", "binary_key": "obscura"},
    "obscura-cdp": {"engine": "obscura", "driver": "cdp", "label": "obscura / cdp", "binary_key": "obscura"},
}


def target_metadata(target: str) -> dict[str, str]:
    return TARGET_METADATA.get(target, {"engine": target, "driver": "unknown", "label": target, "binary_key": target})


def target_binary_key(target: str) -> str:
    return target_metadata(target)["binary_key"]


def target_uses_external_fixture(target: str) -> bool:
    return target_metadata(target)["engine"] == "obscura"


def normalize_cdp_target(target: str) -> str:
    if target in CDP_TARGETS:
        return target
    if target == "moli-full":
        return "moli-full-cdp"
    if target in BASE_TARGETS:
        return f"{target}-cdp"
    return target


def target_enables_all_resource_fetch(target: str) -> bool:
    return target in {"moli-full", "moli-full-cdp"}


def target_is_cdp(target: str) -> bool:
    return target in CDP_TARGETS


def _command_for_target(target: str, binary: Path, url: str, timeout_seconds: float) -> list[str]:
    metadata = target_metadata(target)
    if target_is_cdp(target):
        raise RuntimeError(f"{target} is a CDP target; use the cdp-session suite")
    timeout_ms = str(int(timeout_seconds * 1000))
    if target in {"moli", "moli-full"}:
        compatibility_args = (
            ["--layout", "--resource"]
            if target_enables_all_resource_fetch(target)
            else []
        )
        return [
            str(binary),
            "fetch",
            *compatibility_args,
            "--dump",
            "html",
            "--wait-until",
            "done",
            "--wait-script",
            "document.querySelector('[data-benchmark-status=\"ok\"]') !== null",
            "--timeout",
            timeout_ms,
            url,
        ]
    if target == "lightpanda":
        return [
            str(binary),
            "fetch",
            "--dump",
            "html",
            "--wait-until",
            "done",
            "--wait-ms",
            timeout_ms,
            "--http-timeout",
            timeout_ms,
            "--terminate-ms",
            timeout_ms,
            url,
        ]
    if target == "chrome":
        return [
            str(binary),
            "--headless=new",
            "--disable-gpu",
            "--no-sandbox",
            "--disable-dev-shm-usage",
            f"--virtual-time-budget={timeout_ms}",
            "--dump-dom",
            url,
        ]
    if target == "obscura":
        return [
            str(binary),
            "fetch",
            "--dump",
            "html",
            "--wait-until",
            "load",
            "--wait",
            "0",
            "--timeout",
            str(max(1, int(timeout_seconds))),
            url,
        ]
    raise RuntimeError(f"unknown target: {target}")


def run_synthetic_compare_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...],
    runs: int,
    timeout_seconds: float,
    cases: tuple[str, ...],
    concurrency: int,
    gate_target: str,
) -> dict[str, Any]:
    unknown_cases = [case for case in cases if case not in SYNTHETIC_CASES]
    if unknown_cases:
        raise RuntimeError(f"unknown synthetic case(s): {', '.join(unknown_cases)}")
    unknown_targets = [target for target in targets if target not in FETCH_TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown fetch target(s): {', '.join(unknown_targets)}; use cdp-session for CDP targets")
    if gate_target not in FETCH_TARGETS:
        raise RuntimeError(f"unknown gate target: {gate_target}")
    if gate_target not in targets:
        raise RuntimeError(f"gate target `{gate_target}` must be included in selected targets")

    suite_dir = output_dir / "synthetic-compare"
    rows: list[dict[str, Any]] = []
    details: list[dict[str, Any]] = []

    def run_one(server: SyntheticServer, target: str, case: str, run_id: int) -> tuple[dict[str, Any], dict[str, Any]]:
        metadata = target_metadata(target)
        target_info = target_matrix.get(metadata["binary_key"], {})
        path = target_info.get("path")
        if not target_info.get("available") or not path:
            row = {
                "target": target,
                **metadata,
                "case": case,
                "run": run_id,
                "concurrency": concurrency,
                "ok": False,
                "elapsed_ms": None,
                "returncode": None,
                "timed_out": False,
                "peak_pss_bytes": None,
                "peak_rss_bytes": None,
                "peak_cpu_percent": None,
                "output_sha256": None,
                "error": "target binary unavailable",
            }
            return row, dict(row)

        if target_uses_external_fixture(target) and server.external_base_url is None:
            row = {
                "target": target,
                **metadata,
                "case": case,
                "run": run_id,
                "concurrency": concurrency,
                "ok": False,
                "elapsed_ms": None,
                "returncode": None,
                "timed_out": False,
                "peak_pss_bytes": None,
                "peak_rss_bytes": None,
                "peak_cpu_percent": None,
                "output_sha256": None,
                "error": "external fixture address unavailable for Obscura",
            }
            return row, dict(row)

        url = server.url_for_path(case, external=target_uses_external_fixture(target))
        result = run_process(
            _command_for_target(target, Path(path), url, timeout_seconds),
            cwd=REPO_ROOT,
            timeout_seconds=timeout_seconds + 2,
            env=clear_proxy_env(os.environ),
        )
        ok = result.returncode == 0 and not result.timed_out and b"data-benchmark-status=\"ok\"" in result.stdout
        row = {
            "target": target,
            **metadata,
            "case": case,
            "run": run_id,
            "concurrency": concurrency,
            "ok": ok,
            "elapsed_ms": result.elapsed_ms,
            "returncode": result.returncode,
            "timed_out": result.timed_out,
            "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
            "peak_rss_bytes": result.resources.get("peak_rss_bytes"),
            "peak_cpu_percent": result.resources.get("peak_cpu_percent"),
            "output_sha256": _hash_output(result.output_digest_material()),
            "error": None if ok else "benchmark status marker missing or process failed",
        }
        return row, {**row, "url": url, "process": result.json_summary(include_output=not ok)}

    with SyntheticServer() as server:
        with ThreadPoolExecutor(max_workers=max(1, concurrency)) as executor:
            futures = [
                executor.submit(run_one, server, target, case, run_id)
                for target in targets
                for case in cases
                for run_id in range(1, runs + 1)
            ]
            for future in as_completed(futures):
                row, detail = future.result()
                rows.append(row)
                details.append(detail)

    gate_failures = sum(1 for row in rows if row["target"] == gate_target and not row.get("ok"))
    summary: dict[str, Any] = {
        "suite": "synthetic-compare",
        "runs": runs,
        "timeout_seconds": timeout_seconds,
        "concurrency": concurrency,
        "gate_target": gate_target,
        "gate_failures": gate_failures,
        "cases": list(cases),
        "targets": {},
        "total_failures": sum(1 for row in rows if not row.get("ok")),
    }
    for target in targets:
        target_rows = [row for row in rows if row["target"] == target]
        target_summary = {
            **target_metadata(target),
            "failures": sum(1 for row in target_rows if not row.get("ok")),
            "failure_samples": [],
            "cases": {},
        }
        for case in cases:
            case_rows = [row for row in target_rows if row["case"] == case]
            failure_samples = []
            for detail in (detail for detail in details if detail["target"] == target and detail["case"] == case and not detail.get("ok")):
                process = detail.get("process") if isinstance(detail.get("process"), dict) else {}
                tail = str(process.get("stderr_tail") or process.get("stdout_tail") or detail.get("error") or "")
                compact_tail = " ".join(line.strip() for line in tail.splitlines() if line.strip())
                failure_samples.append(compact_tail[:500] if compact_tail else str(detail.get("error") or "unknown failure"))
                if len(failure_samples) >= 3:
                    break
            target_summary["cases"][case] = {
                "elapsed_ms": summarize(row["elapsed_ms"] for row in case_rows if row.get("ok") and row.get("elapsed_ms") is not None),
                "peak_pss_bytes": summarize(row["peak_pss_bytes"] for row in case_rows if row.get("peak_pss_bytes") is not None),
                "peak_rss_bytes": summarize(row["peak_rss_bytes"] for row in case_rows if row.get("peak_rss_bytes") is not None),
                "failures": sum(1 for row in case_rows if not row.get("ok")),
                "failure_samples": failure_samples,
            }
            target_summary["failure_samples"].extend(failure_samples)
        summary["targets"][target] = target_summary

    write_csv(suite_dir / "runs.csv", rows)
    write_json(suite_dir / "runs.json", details)
    write_json(suite_dir / "summary.json", summary)
    return summary
