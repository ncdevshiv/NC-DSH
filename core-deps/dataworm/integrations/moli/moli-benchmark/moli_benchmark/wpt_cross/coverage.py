"""Audit focused WPT-cross discovery coverage for wasm cases."""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Any

from .audit import load_known_failure_manifest
from .case_set import enumerate_cases


WASM_CASE_SUFFIXES = (".html", ".any.js", ".window.js", ".worker.js")


def _is_wasm_source_case(path: Path) -> bool:
    return path.name.endswith(WASM_CASE_SUFFIXES)


def _is_support_resource_case(rel: str) -> bool:
    return "resources" in rel.split("/")[:-1]


def _case_base(case_path: str) -> str:
    return case_path.split("?", 1)[0]


def _rule_category(rule: dict[str, Any]) -> str:
    category = rule.get("category")
    return category if isinstance(category, str) and category else "uncategorized"


def wasm_source_case_paths(wpt_root: Path) -> list[str]:
    root = wpt_root.resolve()
    wasm_root = root / "wasm"
    if not wasm_root.exists():
        raise RuntimeError(f"WPT wasm directory does not exist: {wasm_root}")
    cases = [
        path.relative_to(root).as_posix()
        for path in sorted(wasm_root.rglob("*"))
        if path.is_file() and _is_wasm_source_case(path)
    ]
    return cases


def audit_wasm_focused_coverage(
    wpt_root: Path,
    *,
    known_failures: Path | None = None,
) -> dict[str, Any]:
    source_cases = wasm_source_case_paths(wpt_root)
    enumerated_cases = enumerate_cases(
        wpt_root,
        dir_prefixes=("wasm",),
        include_tentative=True,
        any_js_global="both",
    )
    enumerated_case_paths = sorted(case.case_path for case in enumerated_cases)
    duplicate_enumerated_case_paths = [
        case_path
        for case_path, count in sorted(Counter(enumerated_case_paths).items())
        if count > 1
    ]
    enumerated_case_set = set(enumerated_case_paths)
    enumerated_base_paths = sorted(
        {_case_base(case.case_path) for case in enumerated_cases}
    )
    enumerated_base_set = set(enumerated_base_paths)
    ignored_support_resources = [
        case for case in source_cases if _is_support_resource_case(case)
    ]
    top_level_cases = [
        case for case in source_cases if not _is_support_resource_case(case)
    ]
    missing_top_level_cases = [
        case for case in top_level_cases if case not in enumerated_base_set
    ]
    enumerated_support_resource_paths = [
        case for case in enumerated_base_paths if _is_support_resource_case(case)
    ]
    extra_enumerated_base_paths = [
        case for case in enumerated_base_paths if case not in source_cases
    ]
    missing_known_failure_rules: list[str] = []
    missing_known_failure_rule_details: list[dict[str, str]] = []
    known_failure_rule_count = 0
    known_failure_category_counts: dict[str, int] = {}
    missing_known_failure_category_counts: dict[str, int] = {}
    if known_failures is not None:
        rules = load_known_failure_manifest(known_failures)["rules"]
        known_failure_rule_count = len(rules)
        known_failure_category_counts = dict(
            sorted(Counter(_rule_category(rule) for rule in rules).items())
        )
        missing_rules = [
            rule for rule in rules if rule["case_path"] not in enumerated_case_set
        ]
        missing_known_failure_rules = [rule["case_path"] for rule in missing_rules]
        missing_known_failure_rule_details = [
            {
                "case_path": rule["case_path"],
                "category": _rule_category(rule),
            }
            for rule in missing_rules
        ]
        missing_known_failure_category_counts = dict(
            sorted(Counter(_rule_category(rule) for rule in missing_rules).items())
        )
    return {
        "ok": (
            not missing_top_level_cases
            and not missing_known_failure_rules
            and not duplicate_enumerated_case_paths
            and not enumerated_support_resource_paths
            and not extra_enumerated_base_paths
        ),
        "wpt_root": str(wpt_root.resolve()),
        "source_case_count": len(source_cases),
        "top_level_case_count": len(top_level_cases),
        "ignored_support_resource_count": len(ignored_support_resources),
        "enumerated_case_count": len(enumerated_cases),
        "enumerated_base_count": len(enumerated_base_paths),
        "known_failure_rule_count": known_failure_rule_count,
        "known_failure_category_counts": known_failure_category_counts,
        "missing_known_failure_category_counts": missing_known_failure_category_counts,
        "duplicate_enumerated_case_paths": duplicate_enumerated_case_paths,
        "enumerated_support_resource_paths": enumerated_support_resource_paths,
        "missing_top_level_cases": missing_top_level_cases,
        "missing_known_failure_rules": missing_known_failure_rules,
        "missing_known_failure_rule_details": missing_known_failure_rule_details,
        "ignored_support_resources": ignored_support_resources,
        "extra_enumerated_base_paths": extra_enumerated_base_paths,
    }


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m moli_benchmark.wpt_cross.coverage",
        description="Audit focused WPT-cross discovery coverage for wasm cases.",
    )
    parser.add_argument("--wpt-root", required=True, type=Path)
    parser.add_argument(
        "--known-failures",
        type=Path,
        help="optional wasm known-failure manifest; every rule must be in the focused case set",
    )
    parser.add_argument("--json-output", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _build_parser().parse_args(argv)
    audit = audit_wasm_focused_coverage(
        args.wpt_root,
        known_failures=args.known_failures,
    )
    payload = json.dumps(audit, indent=2, sort_keys=True)
    if args.json_output:
        args.json_output.write_text(payload, encoding="utf-8")
    print(payload)
    return 0 if audit["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
