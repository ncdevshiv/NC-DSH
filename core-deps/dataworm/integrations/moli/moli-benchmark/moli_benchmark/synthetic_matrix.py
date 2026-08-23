from __future__ import annotations

from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json
from .stats import summarize
from .synthetic import SYNTHETIC_CASES, run_synthetic_suite


DEFAULT_SYNTHETIC_CONCURRENCY_MATRIX = (1, 5, 10, 25, 100)
DEFAULT_STABILITY_THRESHOLD_PERCENT = 10.0
FORMAL_SYNTHETIC_RUNS = 100
FORMAL_SYNTHETIC_REPEATS = 5
SYNTHETIC_MATRIX_PROFILES = ("smoke", "formal")


def _median_drift_percent(values: list[float]) -> float | None:
    if len(values) < 2:
        return None
    summary = summarize(values)
    median = summary.get("median")
    minimum = summary.get("min")
    maximum = summary.get("max")
    if median in (None, 0) or minimum is None or maximum is None:
        return None
    return ((float(maximum) - float(minimum)) / float(median)) * 100.0


def _formal_gate_rows(
    *,
    profile: str,
    formal_requirements: dict[str, dict[str, Any]],
    total_failures: int,
    stability_failures: int,
) -> list[dict[str, Any]]:
    rows = [
        {
            "gate": "profile",
            "actual": profile,
            "required": "formal",
            "ok": profile == "formal",
            "failure_kind": None if profile == "formal" else "profile-not-formal",
        }
    ]
    for name, requirement in formal_requirements.items():
        ok = bool(requirement.get("ok"))
        rows.append(
            {
                "gate": name,
                "actual": requirement.get("actual"),
                "required": requirement.get("required"),
                "ok": ok,
                "failure_kind": None if ok else "formal-requirement",
            }
        )
    rows.extend(
        [
            {
                "gate": "workload_failures",
                "actual": total_failures,
                "required": 0,
                "ok": total_failures == 0,
                "failure_kind": None if total_failures == 0 else "workload-failure",
            },
            {
                "gate": "stability_failures",
                "actual": stability_failures,
                "required": 0,
                "ok": stability_failures == 0,
                "failure_kind": None if stability_failures == 0 else "stability-drift",
            },
        ]
    )
    return rows


def run_synthetic_matrix_suite(
    *,
    moli_bin: Path,
    output_dir: Path,
    profile: str,
    runs: int,
    timeout_seconds: float,
    cases: tuple[str, ...],
    concurrency_levels: tuple[int, ...],
    repeats: int,
    stability_threshold_percent: float,
) -> dict[str, Any]:
    unknown = [case for case in cases if case not in SYNTHETIC_CASES]
    if unknown:
        raise RuntimeError(f"unknown synthetic case(s): {', '.join(unknown)}")
    if not concurrency_levels:
        raise RuntimeError("synthetic matrix requires at least one concurrency level")
    invalid_concurrency = [value for value in concurrency_levels if value < 1]
    if invalid_concurrency:
        raise RuntimeError(f"invalid synthetic concurrency level(s): {invalid_concurrency}")
    if repeats < 1:
        raise RuntimeError("synthetic matrix repeats must be at least 1")
    if profile not in SYNTHETIC_MATRIX_PROFILES:
        raise RuntimeError(f"unknown synthetic matrix profile: {profile}")

    suite_dir = output_dir / "synthetic-matrix"
    matrix_rows: list[dict[str, Any]] = []
    summaries: list[dict[str, Any]] = []

    for repeat in range(1, repeats + 1):
        for concurrency in concurrency_levels:
            run_output_dir = suite_dir / f"repeat-{repeat}" / f"concurrency-{concurrency}"
            summary = run_synthetic_suite(
                moli_bin=moli_bin,
                output_dir=run_output_dir,
                runs=runs,
                timeout_seconds=timeout_seconds,
                cases=cases,
                concurrency=concurrency,
            )
            summaries.append({**summary, "repeat": repeat})
            for case, case_summary in summary.get("cases", {}).items():
                elapsed = case_summary.get("elapsed_ms", {})
                pss = case_summary.get("peak_pss_bytes", {})
                matrix_rows.append(
                    {
                        "repeat": repeat,
                        "concurrency": concurrency,
                        "case": case,
                        "runs": runs,
                        "failures": case_summary.get("failures", 0),
                        "elapsed_p50_ms": elapsed.get("p50"),
                        "elapsed_p90_ms": elapsed.get("p90"),
                        "elapsed_p95_ms": elapsed.get("p95"),
                        "peak_pss_p50_bytes": pss.get("p50"),
                        "peak_pss_p95_bytes": pss.get("p95"),
                    }
                )

    cases_summary: dict[str, Any] = {}
    stability_failures = 0
    for case in cases:
        by_concurrency: dict[str, Any] = {}
        for concurrency in concurrency_levels:
            rows = [row for row in matrix_rows if row["case"] == case and row["concurrency"] == concurrency]
            medians = [
                float(row["elapsed_p50_ms"])
                for row in rows
                if row.get("failures") == 0 and row.get("elapsed_p50_ms") is not None
            ]
            drift = _median_drift_percent(medians)
            stable = drift is None or drift <= stability_threshold_percent
            if drift is not None and not stable:
                stability_failures += 1
            by_concurrency[str(concurrency)] = {
                "elapsed_p50_ms": summarize(medians),
                "median_drift_percent": drift,
                "stable": stable,
                "failures": sum(int(row.get("failures", 0) or 0) for row in rows),
            }
        cases_summary[case] = by_concurrency

    total_failures = sum(int(row.get("failures", 0) or 0) for row in matrix_rows)
    required_cases = set(SYNTHETIC_CASES)
    selected_cases = set(cases)
    required_concurrency = set(DEFAULT_SYNTHETIC_CONCURRENCY_MATRIX)
    selected_concurrency = set(concurrency_levels)
    formal_requirements = {
        "runs": {"actual": runs, "required": FORMAL_SYNTHETIC_RUNS, "ok": runs >= FORMAL_SYNTHETIC_RUNS},
        "repeats": {"actual": repeats, "required": FORMAL_SYNTHETIC_REPEATS, "ok": repeats >= FORMAL_SYNTHETIC_REPEATS},
        "concurrency_levels": {
            "actual": list(concurrency_levels),
            "required": list(DEFAULT_SYNTHETIC_CONCURRENCY_MATRIX),
            "ok": required_concurrency.issubset(selected_concurrency),
        },
        "cases": {
            "actual": list(cases),
            "required": list(SYNTHETIC_CASES),
            "ok": required_cases.issubset(selected_cases),
        },
    }
    profile_failures = (
        sum(1 for requirement in formal_requirements.values() if not bool(requirement["ok"]))
        if profile == "formal"
        else 0
    )
    formal_gate_rows = _formal_gate_rows(
        profile=profile,
        formal_requirements=formal_requirements,
        total_failures=total_failures,
        stability_failures=stability_failures,
    )
    summary = {
        "suite": "synthetic-matrix",
        "profile": profile,
        "runs": runs,
        "repeats": repeats,
        "timeout_seconds": timeout_seconds,
        "concurrency_levels": list(concurrency_levels),
        "stability_threshold_percent": stability_threshold_percent,
        "stability_failures": stability_failures,
        "formal_gate_rows": formal_gate_rows,
        "formal_requirements": formal_requirements,
        "profile_failures": profile_failures,
        "gate_target": "moli",
        "gate_failures": total_failures + stability_failures + profile_failures,
        "cases": cases_summary,
        "total_failures": total_failures,
    }

    write_csv(suite_dir / "matrix.csv", matrix_rows)
    write_json(suite_dir / "matrix.json", matrix_rows)
    write_json(suite_dir / "gate-rows.json", {"rows": formal_gate_rows})
    write_json(suite_dir / "run-summaries.json", summaries)
    write_json(suite_dir / "summary.json", summary)
    return summary
