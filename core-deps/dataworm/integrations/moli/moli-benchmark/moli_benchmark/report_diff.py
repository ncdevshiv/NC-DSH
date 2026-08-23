from __future__ import annotations

import json
from pathlib import Path
from typing import Any


def _summary_path(path: Path) -> Path:
    if path.is_dir():
        return path / "summary.json"
    return path


def load_baseline_summary(path: Path) -> dict[str, Any]:
    summary_path = _summary_path(path)
    try:
        data = json.loads(summary_path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise RuntimeError(f"missing baseline report summary: {summary_path}") from error
    except json.JSONDecodeError as error:
        raise RuntimeError(f"invalid baseline report summary JSON: {summary_path}") from error
    if not isinstance(data, dict):
        raise RuntimeError(f"baseline report summary must be an object: {summary_path}")
    suites = data.get("suites")
    if suites is not None and not isinstance(suites, list):
        raise RuntimeError(f"baseline report summary suites must be a list: {summary_path}")
    return data


def _suite_key(summary: dict[str, Any]) -> str | None:
    suite = summary.get("suite")
    return str(suite) if suite else None


def _by_suite(summaries: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {key: summary for summary in summaries if (key := _suite_key(summary)) is not None}


def _int_value(value: Any) -> int | None:
    if value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _total_failures(summary: dict[str, Any] | None) -> int | None:
    if summary is None:
        return None
    return _int_value(summary.get("total_failures"))


def _gate_failures(summary: dict[str, Any] | None) -> int | None:
    if summary is None:
        return None
    return _int_value(summary.get("gate_failures", summary.get("total_failures")))


def _case_count(summary: dict[str, Any] | None) -> int | None:
    if summary is None:
        return None
    cases = summary.get("cases")
    if isinstance(cases, (dict, list, tuple)):
        return len(cases)
    if cases is not None:
        return _int_value(cases)
    runs = summary.get("runs")
    if isinstance(runs, (dict, list, tuple)):
        return len(runs)
    return None


def _delta(current: int | None, baseline: int | None) -> int | None:
    if current is None or baseline is None:
        return None
    return current - baseline


def _suite_status(current: dict[str, Any] | None, baseline: dict[str, Any] | None) -> str:
    if baseline is None:
        return "added"
    if current is None:
        return "removed"
    fields = (
        _total_failures(current) != _total_failures(baseline),
        _gate_failures(current) != _gate_failures(baseline),
        _case_count(current) != _case_count(baseline),
    )
    return "changed" if any(fields) else "unchanged"


def build_report_diff(
    *,
    current_summaries: list[dict[str, Any]],
    baseline_summary: dict[str, Any],
    baseline_path: Path,
) -> dict[str, Any]:
    baseline_summaries = [summary for summary in baseline_summary.get("suites", []) if isinstance(summary, dict)]
    current_by_suite = _by_suite(current_summaries)
    baseline_by_suite = _by_suite(baseline_summaries)
    rows: list[dict[str, Any]] = []

    for suite in sorted(current_by_suite.keys() | baseline_by_suite.keys()):
        current = current_by_suite.get(suite)
        baseline = baseline_by_suite.get(suite)
        current_total = _total_failures(current)
        baseline_total = _total_failures(baseline)
        current_gate = _gate_failures(current)
        baseline_gate = _gate_failures(baseline)
        current_cases = _case_count(current)
        baseline_cases = _case_count(baseline)
        rows.append(
            {
                "suite": suite,
                "status": _suite_status(current, baseline),
                "baseline_total_failures": baseline_total,
                "current_total_failures": current_total,
                "total_failures_delta": _delta(current_total, baseline_total),
                "baseline_gate_failures": baseline_gate,
                "current_gate_failures": current_gate,
                "gate_failures_delta": _delta(current_gate, baseline_gate),
                "baseline_cases": baseline_cases,
                "current_cases": current_cases,
                "cases_delta": _delta(current_cases, baseline_cases),
            }
        )

    baseline_total_failures = sum(_total_failures(summary) or 0 for summary in baseline_summaries)
    current_total_failures = sum(_total_failures(summary) or 0 for summary in current_summaries)
    baseline_gate_failures = sum(_gate_failures(summary) or 0 for summary in baseline_summaries)
    current_gate_failures = sum(_gate_failures(summary) or 0 for summary in current_summaries)
    by_status = {status: sum(1 for row in rows if row["status"] == status) for status in ("added", "removed", "changed", "unchanged")}

    return {
        "schema_version": 1,
        "baseline": str(_summary_path(baseline_path)),
        "summary": {
            **by_status,
            "total_changes": by_status["added"] + by_status["removed"] + by_status["changed"],
            "baseline_total_failures": baseline_total_failures,
            "current_total_failures": current_total_failures,
            "total_failures_delta": current_total_failures - baseline_total_failures,
            "baseline_gate_failures": baseline_gate_failures,
            "current_gate_failures": current_gate_failures,
            "gate_failures_delta": current_gate_failures - baseline_gate_failures,
        },
        "suites": rows,
    }
