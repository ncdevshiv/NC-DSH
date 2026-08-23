from __future__ import annotations

import os
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json
from .config import REPO_ROOT, clear_proxy_env
from .process import run_process
from .stats import summarize
from .synthetic import SYNTHETIC_CASES, SyntheticServer
from .synthetic_compare import FETCH_TARGETS, _command_for_target, target_metadata, target_uses_external_fixture


def run_crawler_suite(
    *,
    output_dir: Path,
    target_matrix: dict[str, Any],
    targets: tuple[str, ...],
    pages: int,
    runs: int,
    timeout_seconds: float,
    gate_target: str,
) -> dict[str, Any]:
    unknown_targets = [target for target in targets if target not in FETCH_TARGETS]
    if unknown_targets:
        raise RuntimeError(f"unknown fetch target(s): {', '.join(unknown_targets)}; use cdp-session for CDP targets")
    if gate_target not in FETCH_TARGETS:
        raise RuntimeError(f"unknown gate target: {gate_target}")

    suite_dir = output_dir / "crawler"
    rows: list[dict[str, Any]] = []
    cases = list(SYNTHETIC_CASES)
    with SyntheticServer() as server:
        for target in targets:
            metadata = target_metadata(target)
            info = target_matrix.get(metadata["binary_key"], {})
            path = info.get("path")
            urls = [
                f"{server.url_for_path(cases[index % len(cases)], external=target_uses_external_fixture(target))}?crawl={index}"
                for index in range(pages)
            ]
            for run_id in range(1, runs + 1):
                for index, url in enumerate(urls):
                    case = cases[index % len(cases)]
                    if not info.get("available") or not path:
                        rows.append(
                            {
                                "target": target,
                                **metadata,
                                "run": run_id,
                                "page": index,
                                "case": case,
                                "ok": False,
                                "elapsed_ms": None,
                                "error": "target binary unavailable",
                            }
                        )
                        continue
                    if target_uses_external_fixture(target) and server.external_base_url is None:
                        rows.append(
                            {
                                "target": target,
                                **metadata,
                                "run": run_id,
                                "page": index,
                                "case": case,
                                "ok": False,
                                "elapsed_ms": None,
                                "error": "external fixture address unavailable for Obscura",
                            }
                        )
                        continue
                    result = run_process(
                        _command_for_target(target, Path(path), url, timeout_seconds),
                        cwd=REPO_ROOT,
                        timeout_seconds=timeout_seconds + 2,
                        env=clear_proxy_env(os.environ),
                    )
                    ok = result.returncode == 0 and not result.timed_out and b"data-benchmark-status=\"ok\"" in result.stdout
                    rows.append(
                        {
                            "target": target,
                            **metadata,
                            "run": run_id,
                            "page": index,
                            "case": case,
                            "ok": ok,
                            "elapsed_ms": result.elapsed_ms,
                            "returncode": result.returncode,
                            "timed_out": result.timed_out,
                            "peak_pss_bytes": result.resources.get("peak_pss_bytes"),
                            "error": None if ok else "marker missing or process failed",
                        }
                    )

    gate_failures = sum(1 for row in rows if row["target"] == gate_target and not row.get("ok"))
    summary: dict[str, Any] = {
        "suite": "crawler",
        "runs": runs,
        "pages": pages,
        "timeout_seconds": timeout_seconds,
        "gate_target": gate_target,
        "gate_failures": gate_failures,
        "targets": {},
        "total_failures": sum(1 for row in rows if not row.get("ok")),
    }
    for target in targets:
        target_rows = [row for row in rows if row["target"] == target]
        summary["targets"][target] = {
            **target_metadata(target),
            "pages": len(target_rows),
            "runs": runs,
            "passes": sum(1 for row in target_rows if row.get("ok")),
            "failures": sum(1 for row in target_rows if not row.get("ok")),
            "elapsed_ms": summarize(row["elapsed_ms"] for row in target_rows if row.get("ok") and row.get("elapsed_ms") is not None),
            "peak_pss_bytes": summarize(row["peak_pss_bytes"] for row in target_rows if row.get("peak_pss_bytes") is not None),
        }
    write_csv(suite_dir / "raw-runs.csv", rows)
    write_json(suite_dir / "runs.json", rows)
    write_json(suite_dir / "summary.json", summary)
    return summary
