from __future__ import annotations

import json
import os
import shutil
from pathlib import Path
from typing import Any

from .artifacts import write_csv, write_json, write_text
from .config import REPO_ROOT, clear_proxy_env
from .process import run_process


REPORT_PREFIX = "moli-wpt-compat-report-wpt-compat-"
WPT_RUNNER = "nextest"


def _target_dir(env: dict[str, str]) -> Path:
    raw = env.get("CARGO_TARGET_DIR")
    return Path(raw).expanduser().resolve() if raw else REPO_ROOT / "target"


def _report_paths(target_dir: Path) -> list[Path]:
    return sorted(target_dir.glob(f"{REPORT_PREFIX}*.json"))


def _clean_reports(target_dir: Path) -> None:
    for path in target_dir.glob(f"{REPORT_PREFIX}*.json"):
        path.unlink(missing_ok=True)
    for path in target_dir.glob(f"{REPORT_PREFIX}*.md"):
        path.unlink(missing_ok=True)


def _normalize_runner(runner: str | None) -> str:
    if runner in (None, WPT_RUNNER):
        return WPT_RUNNER
    raise RuntimeError("WPT compat benchmarks only support the release nextest runner")


def _run_command(runner: str | None = WPT_RUNNER) -> list[str]:
    _normalize_runner(runner)
    return [
        "cargo",
        "nextest",
        "run",
        "-p",
        "moli-core",
        "--test",
        "wpt_compat",
        "--release",
        "--no-fail-fast",
    ]


def _empty_counts() -> dict[str, int]:
    return {
        "total": 0,
        "pass": 0,
        "known_fail": 0,
        "unexpected_fail": 0,
        "unexpected_pass": 0,
        "skip": 0,
    }


def _add_rates(counts: dict[str, Any]) -> None:
    total = int(counts.get("total", 0) or 0)
    if total <= 0:
        counts["pass_rate_percent"] = None
        counts["unexpected_fail_rate_percent"] = None
        counts["skip_rate_percent"] = None
        return
    counts["pass_rate_percent"] = (int(counts.get("pass", 0) or 0) / total) * 100.0
    counts["unexpected_fail_rate_percent"] = (int(counts.get("unexpected_fail", 0) or 0) / total) * 100.0
    counts["skip_rate_percent"] = (int(counts.get("skip", 0) or 0) / total) * 100.0


def _record_category(summary: dict[str, Any], category: str, tags: list[str]) -> None:
    key = category.replace("-", "_")
    summary["total"] += 1
    if key in summary:
        summary[key] += 1
    for tag in tags:
        tag_summary = summary["by_tag"].setdefault(tag, _empty_counts())
        tag_summary["total"] += 1
        if key in tag_summary:
            tag_summary[key] += 1


def _collect_reports(target_dir: Path, suite_dir: Path) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    reports_dir = suite_dir / "reports"
    reports_dir.mkdir(parents=True, exist_ok=True)
    summary: dict[str, Any] = {**_empty_counts(), "by_tag": {}}
    cases: list[dict[str, Any]] = []
    for path in _report_paths(target_dir):
        data = json.loads(path.read_text(encoding="utf-8"))
        shutil.copy2(path, reports_dir / path.name)
        md_path = path.with_suffix(".md")
        if md_path.exists():
            shutil.copy2(md_path, reports_dir / md_path.name)
        for case in data.get("cases", []):
            category = str(case.get("category", "unexpected-fail"))
            tags = [str(tag) for tag in case.get("tags", [])]
            _record_category(summary, category, tags)
            cases.append(
                {
                    "id": case.get("id"),
                    "expected": case.get("expected"),
                    "actual": case.get("actual"),
                    "category": category,
                    "tags": ",".join(tags),
                    "failures": " | ".join(str(item) for item in case.get("failures", [])),
                }
            )
    _add_rates(summary)
    for tag_summary in summary["by_tag"].values():
        _add_rates(tag_summary)
    return summary, cases


def _by_tag_rows(summary: dict[str, Any]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for tag, counts in sorted(summary.get("by_tag", {}).items()):
        rows.append({"tag": tag, **counts})
    return rows


def _case_key(case: dict[str, Any]) -> str:
    return str(case.get("id") or "")


def _case_tags(case: dict[str, Any]) -> str:
    tags = case.get("tags", "")
    if isinstance(tags, list):
        return ",".join(str(tag) for tag in tags)
    return str(tags or "")


def _case_index(cases: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {_case_key(case): case for case in cases if _case_key(case)}


def _load_baseline_cases(path: Path | None) -> list[dict[str, Any]]:
    if path is None:
        return []
    data = json.loads(path.read_text(encoding="utf-8"))
    cases = data.get("cases") if isinstance(data, dict) else data
    if not isinstance(cases, list):
        raise RuntimeError(f"WPT baseline does not contain a cases list: {path}")
    return [case for case in cases if isinstance(case, dict)]


def _wpt_case_diff(current_cases: list[dict[str, Any]], baseline_cases: list[dict[str, Any]]) -> tuple[dict[str, int], list[dict[str, Any]]]:
    current = _case_index(current_cases)
    baseline = _case_index(baseline_cases)
    rows: list[dict[str, Any]] = []

    for case_id in sorted(current.keys() - baseline.keys()):
        case = current[case_id]
        rows.append(
            {
                "kind": "added",
                "id": case_id,
                "baseline_expected": None,
                "current_expected": case.get("expected"),
                "baseline_category": None,
                "current_category": case.get("category"),
                "baseline_actual": None,
                "current_actual": case.get("actual"),
                "tags": _case_tags(case),
            }
        )

    for case_id in sorted(baseline.keys() - current.keys()):
        case = baseline[case_id]
        rows.append(
            {
                "kind": "removed",
                "id": case_id,
                "baseline_expected": case.get("expected"),
                "current_expected": None,
                "baseline_category": case.get("category"),
                "current_category": None,
                "baseline_actual": case.get("actual"),
                "current_actual": None,
                "tags": _case_tags(case),
            }
        )

    for case_id in sorted(current.keys() & baseline.keys()):
        current_case = current[case_id]
        baseline_case = baseline[case_id]
        if baseline_case.get("expected") != current_case.get("expected"):
            rows.append(
                {
                    "kind": "expectation-change",
                    "id": case_id,
                    "baseline_expected": baseline_case.get("expected"),
                    "current_expected": current_case.get("expected"),
                    "baseline_category": baseline_case.get("category"),
                    "current_category": current_case.get("category"),
                    "baseline_actual": baseline_case.get("actual"),
                    "current_actual": current_case.get("actual"),
                    "tags": _case_tags(current_case),
                }
            )
        if baseline_case.get("category") != current_case.get("category"):
            rows.append(
                {
                    "kind": "category-change",
                    "id": case_id,
                    "baseline_expected": baseline_case.get("expected"),
                    "current_expected": current_case.get("expected"),
                    "baseline_category": baseline_case.get("category"),
                    "current_category": current_case.get("category"),
                    "baseline_actual": baseline_case.get("actual"),
                    "current_actual": current_case.get("actual"),
                    "tags": _case_tags(current_case),
                }
            )

    summary = {
        "added": sum(1 for row in rows if row["kind"] == "added"),
        "removed": sum(1 for row in rows if row["kind"] == "removed"),
        "expectation_changes": sum(1 for row in rows if row["kind"] == "expectation-change"),
        "category_changes": sum(1 for row in rows if row["kind"] == "category-change"),
        "total_changes": len(rows),
    }
    return summary, rows


def _rate(value: Any) -> str:
    return "n/a" if value is None else f"{float(value):.2f}%"


def _markdown(summary: dict[str, Any], command_summary: dict[str, Any] | None, diff_summary: dict[str, Any] | None) -> str:
    lines = [
        "# WPT compat benchmark",
        "",
        f"- total: `{summary['total']}`",
        f"- pass: `{summary['pass']}`",
        f"- known-fail: `{summary['known_fail']}`",
        f"- unexpected-fail: `{summary['unexpected_fail']}`",
        f"- unexpected-pass: `{summary['unexpected_pass']}`",
        f"- skip: `{summary['skip']}`",
        f"- pass rate: `{_rate(summary.get('pass_rate_percent'))}`",
        f"- unexpected-fail rate: `{_rate(summary.get('unexpected_fail_rate_percent'))}`",
        f"- skip rate: `{_rate(summary.get('skip_rate_percent'))}`",
        "",
    ]
    if command_summary is not None:
        lines.extend(
            [
                "Run command:",
                "",
                "```text",
                " ".join(command_summary["command"]),
                "```",
                "",
                f"- returncode: `{command_summary['returncode']}`",
                f"- timed out: `{command_summary['timed_out']}`",
                f"- elapsed ms: `{command_summary['elapsed_ms']:.3f}`",
                "",
            ]
        )
    if summary["by_tag"]:
        lines.append("By tag:")
        for tag, counts in sorted(summary["by_tag"].items()):
            lines.append(
                f"- `{tag}`: total {counts['total']}, pass {counts['pass']}, "
                f"unexpected-fail {counts['unexpected_fail']}, skip {counts['skip']}, "
                f"pass rate {_rate(counts.get('pass_rate_percent'))}"
            )
        lines.append("")
    if diff_summary is not None:
        lines.extend(
            [
                "Baseline diff:",
                "",
                f"- added: `{diff_summary['added']}`",
                f"- removed: `{diff_summary['removed']}`",
                f"- expectation changes: `{diff_summary['expectation_changes']}`",
                f"- category changes: `{diff_summary['category_changes']}`",
                "",
            ]
        )
    return "\n".join(lines)


def run_wpt_suite(
    *,
    output_dir: Path,
    timeout_seconds: float,
    compat: str | None,
    case_filter: str | None,
    tag_filter: str | None,
    no_run: bool,
    runner: str | None = WPT_RUNNER,
    baseline: Path | None = None,
) -> dict[str, Any]:
    runner = _normalize_runner(runner)
    suite_dir = output_dir / "wpt"
    env = clear_proxy_env(os.environ)
    if compat:
        env["MOLI_WPT_COMPAT"] = compat
    if case_filter:
        env["MOLI_WPT_CASE"] = case_filter
    if tag_filter:
        env["MOLI_WPT_TAG"] = tag_filter

    target_dir = _target_dir(env)
    command_summary: dict[str, Any] | None = None
    if not no_run:
        _clean_reports(target_dir)
        result = run_process(
            _run_command(runner),
            cwd=REPO_ROOT,
            timeout_seconds=timeout_seconds,
            env=env,
        )
        command_summary = result.json_summary(include_output=result.returncode != 0 or result.timed_out)
        write_json(suite_dir / "process.json", command_summary)

    compat_summary, cases = _collect_reports(target_dir, suite_dir)
    diff_summary = None
    if baseline is not None:
        baseline_cases = _load_baseline_cases(baseline)
        diff_summary, diff_rows = _wpt_case_diff(cases, baseline_cases)
        write_json(suite_dir / "diff.json", {"baseline": str(baseline), "summary": diff_summary, "cases": diff_rows})
        write_csv(suite_dir / "diff.csv", diff_rows)
    total_failures = compat_summary["unexpected_fail"] + compat_summary["unexpected_pass"] + compat_summary["known_fail"] + compat_summary["skip"]
    if command_summary is not None and (command_summary["returncode"] != 0 or command_summary["timed_out"]):
        total_failures += 1
    summary = {
        "suite": "wpt",
        "runner": runner,
        "selection": {
            "compat": compat,
            "case": case_filter,
            "tag": tag_filter,
            "no_run": no_run,
        },
        "summary": compat_summary,
        "cases": len(cases),
        "total_failures": total_failures,
    }
    if baseline is not None:
        summary["baseline"] = str(baseline)
        summary["diff"] = diff_summary
    write_json(suite_dir / "moli-wpt-compat-report.json", {"summary": compat_summary, "cases": cases})
    write_csv(suite_dir / "raw-runs.csv", cases)
    write_csv(suite_dir / "by-tag.csv", _by_tag_rows(compat_summary))
    write_json(suite_dir / "summary.json", summary)
    write_text(suite_dir / "summary.md", _markdown(compat_summary, command_summary, diff_summary))
    return summary
