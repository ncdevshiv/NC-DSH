"""Audit a wpt_cross matrix against an explicit known-failure manifest."""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any
from urllib.parse import unquote, urlsplit


PASS_STATUS = "pass"
ALLOWED_EVIDENCE_KINDS = {"chromium", "doc", "local-test", "wpt"}
ALLOWED_EXPECTED_FAILURE_STATUSES = {
    "crash",
    "error",
    "fail",
    "harness-stalled",
    "missing",
    "timeout",
}


def _load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def _resolve_existing_manifest_path(manifest_path: Path, relative: str) -> Path | None:
    path_without_anchor = _split_manifest_reference(relative)[0]
    if not path_without_anchor:
        return None
    relative_path = Path(path_without_anchor)
    if relative_path.is_absolute():
        return None
    for base in (manifest_path.parent, *manifest_path.parents):
        candidate = base / relative_path
        if candidate.is_file():
            return candidate
    return None


def _is_absolute_manifest_path(path: str) -> bool:
    path_without_anchor = _split_manifest_reference(path)[0]
    return bool(path_without_anchor) and Path(path_without_anchor).is_absolute()


def _case_base(case_path: str) -> str:
    return case_path.split("?", 1)[0]


def _validate_manifest_case_path(path: Path, case_path: str) -> None:
    parsed = urlsplit(case_path)
    path_parts = parsed.path.split("/")
    invalid = (
        bool(parsed.scheme)
        or bool(parsed.netloc)
        or bool(parsed.fragment)
        or not parsed.path
        or parsed.path.startswith("/")
        or "\\" in parsed.path
        or any(part in {"", ".", ".."} for part in path_parts)
    )
    if invalid:
        raise ValueError(
            f"{path}: rule case_path {case_path!r} must be a relative WPT case path"
        )


def _split_manifest_reference(reference: str) -> tuple[str, str | None]:
    path, separator, fragment = reference.partition("#")
    return path, unquote(fragment) if separator else None


def _wpt_case_path_from_manifest_reference(reference: str) -> str | None:
    path_without_anchor = _split_manifest_reference(reference)[0]
    parts = Path(path_without_anchor).parts
    if not parts:
        return None
    if "wpt" in parts:
        root_index = parts.index("wpt") + 1
        if root_index >= len(parts):
            return None
        return Path(*parts[root_index:]).as_posix()
    return None


def _markdown_slug(heading: str) -> str:
    heading = re.sub(r"<[^>]*>", "", heading)
    heading = heading.replace("`", "")
    chars: list[str] = []
    for char in heading.strip().lower():
        if char.isalnum() or char in {" ", "-", "_"}:
            chars.append(char)
    slug = re.sub(r"\s+", "-", "".join(chars).strip())
    return re.sub(r"-+", "-", slug).strip("-")


def _markdown_heading_anchors(path: Path) -> set[str]:
    anchors: set[str] = set()
    seen: Counter[str] = Counter()
    text = path.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        match = re.match(r"^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        heading = match.group(1).strip()
        candidates: list[str] = []
        numbered = re.match(r"^(\d+)(?:[.\u3001\uff0e]|$)", heading)
        if numbered:
            candidates.append(numbered.group(1))
        slug = _markdown_slug(heading)
        if slug:
            candidates.append(slug)
        for candidate in candidates:
            suffix = seen[candidate]
            anchors.add(candidate if suffix == 0 else f"{candidate}-{suffix}")
            seen[candidate] += 1
    return anchors


def _validate_markdown_reference_anchor(
    manifest_path: Path,
    reference: str,
    resolved_path: Path,
    context: str,
) -> None:
    _, fragment = _split_manifest_reference(reference)
    if fragment is None:
        return
    anchor = fragment.strip()
    if not anchor:
        raise ValueError(f"{manifest_path}: {context} has an empty document anchor")
    anchors = _markdown_heading_anchors(resolved_path)
    if anchor not in anchors:
        raise ValueError(
            f"{manifest_path}: {context} document anchor does not exist: "
            f"{reference}"
        )


def _manifest_categories(path: Path, payload: dict[str, Any]) -> dict[str, Any]:
    explicit = payload.get("categories")
    if explicit is None:
        return {}
    if not isinstance(explicit, dict):
        raise ValueError(f"{path}: top-level 'categories' must be an object")
    categories: dict[str, Any] = {}
    for category, metadata in explicit.items():
        if not isinstance(category, str) or not category:
            raise ValueError(f"{path}: category names must be non-empty strings")
        if not isinstance(metadata, dict):
            raise ValueError(f"{path}: category {category!r} metadata must be an object")
        tracking_doc = metadata.get("tracking_doc")
        if not isinstance(tracking_doc, str) or not tracking_doc:
            raise ValueError(
                f"{path}: category {category!r} tracking_doc must be a string"
            )
        if _is_absolute_manifest_path(tracking_doc):
            raise ValueError(
                f"{path}: category {category!r} tracking_doc must be relative"
            )
        resolved_tracking_doc = _resolve_existing_manifest_path(path, tracking_doc)
        if resolved_tracking_doc is None:
            raise ValueError(
                f"{path}: category {category!r} tracking_doc does not exist: "
                f"{tracking_doc}"
            )
        _validate_markdown_reference_anchor(
            path,
            tracking_doc,
            resolved_tracking_doc,
            f"category {category!r} tracking_doc",
        )
        scope = metadata.get("scope")
        if not isinstance(scope, str) or not scope.strip():
            raise ValueError(f"{path}: category {category!r} scope must be a string")
        evidence = metadata.get("evidence")
        if not isinstance(evidence, list) or not evidence:
            raise ValueError(
                f"{path}: category {category!r} evidence must be a non-empty list"
            )
        for idx, item in enumerate(evidence):
            if not isinstance(item, dict):
                raise ValueError(
                    f"{path}: category {category!r} evidence #{idx + 1} must be an object"
                )
            evidence_path = item.get("path")
            if not isinstance(evidence_path, str) or not evidence_path:
                raise ValueError(
                    f"{path}: category {category!r} evidence #{idx + 1} path must be a string"
                )
            if _is_absolute_manifest_path(evidence_path):
                raise ValueError(
                    f"{path}: category {category!r} evidence #{idx + 1} path must be relative"
                )
            resolved_evidence_path = _resolve_existing_manifest_path(path, evidence_path)
            if resolved_evidence_path is None:
                raise ValueError(
                    f"{path}: category {category!r} evidence #{idx + 1} path does not exist: "
                    f"{evidence_path}"
                )
            kind = item.get("kind")
            if not isinstance(kind, str) or kind not in ALLOWED_EVIDENCE_KINDS:
                raise ValueError(
                    f"{path}: category {category!r} evidence #{idx + 1} kind must be one of "
                    f"{sorted(ALLOWED_EVIDENCE_KINDS)!r}"
                )
            if kind == "doc":
                _validate_markdown_reference_anchor(
                    path,
                    evidence_path,
                    resolved_evidence_path,
                    f"category {category!r} evidence #{idx + 1}",
                )
            note = item.get("note")
            if not isinstance(note, str) or not note.strip():
                raise ValueError(
                    f"{path}: category {category!r} evidence #{idx + 1} note must be a string"
                )
        categories[category] = metadata
    return categories


def load_known_failure_manifest(path: Path) -> dict[str, Any]:
    payload = _load_json(path)
    rules = payload.get("rules") if isinstance(payload, dict) else None
    if not isinstance(rules, list):
        raise ValueError(f"{path} must contain a top-level 'rules' list")
    categories = _manifest_categories(path, payload)
    seen_case_paths: set[str] = set()
    for idx, rule in enumerate(rules):
        if not isinstance(rule, dict):
            raise ValueError(f"{path}: rule #{idx + 1} must be an object")
        case_path = rule.get("case_path")
        if not isinstance(case_path, str) or not case_path:
            raise ValueError(f"{path}: rule #{idx + 1} must have case_path")
        _validate_manifest_case_path(path, case_path)
        if case_path in seen_case_paths:
            raise ValueError(f"{path}: duplicate known-failure rule for {case_path!r}")
        seen_case_paths.add(case_path)
        category = rule.get("category")
        if categories:
            if not isinstance(category, str) or not category:
                raise ValueError(
                    f"{path}: rule for {case_path!r} must reference a category"
                )
            if category not in categories:
                raise ValueError(
                    f"{path}: rule for {case_path!r} references unknown category "
                    f"{category!r}"
                )
            _validate_categorized_rule_evidence(path, rule)
            _validate_rule_has_wpt_source_evidence(path, rule, categories[category])
        _expected_statuses(rule)
        _message_needles(rule)
    if categories:
        referenced_categories = {
            rule["category"]
            for rule in rules
            if isinstance(rule.get("category"), str) and rule["category"]
        }
        unused_categories = sorted(set(categories) - referenced_categories)
        if unused_categories:
            raise ValueError(
                f"{path}: categories are not referenced by any rule: "
                f"{unused_categories!r}"
            )
    return payload


def load_known_failures(path: Path) -> list[dict[str, Any]]:
    return load_known_failure_manifest(path)["rules"]


def _expected_statuses(rule: dict[str, Any]) -> set[str]:
    explicit = rule.get("expected_statuses")
    if explicit is None:
        explicit = rule.get("expected_status")
    if explicit is None:
        return set()
    if isinstance(explicit, str) and explicit.strip():
        statuses = {explicit}
        _validate_expected_failure_statuses(rule, statuses)
        return statuses
    if (
        isinstance(explicit, list)
        and explicit
        and all(isinstance(item, str) and item.strip() for item in explicit)
    ):
        statuses = set(explicit)
        _validate_expected_failure_statuses(rule, statuses)
        return statuses
    raise ValueError(
        f"rule for {rule.get('case_path', '<unknown>')} has invalid expected_status"
    )


def _validate_expected_failure_statuses(
    rule: dict[str, Any],
    statuses: set[str],
) -> None:
    invalid = statuses - ALLOWED_EXPECTED_FAILURE_STATUSES
    if invalid:
        raise ValueError(
            f"rule for {rule.get('case_path', '<unknown>')} has invalid expected_status "
            f"{sorted(invalid)!r}; expected one of "
            f"{sorted(ALLOWED_EXPECTED_FAILURE_STATUSES)!r}"
        )


def _validate_categorized_rule_evidence(path: Path, rule: dict[str, Any]) -> None:
    case_path = rule.get("case_path", "<unknown>")
    reason = rule.get("reason")
    if not isinstance(reason, str) or not reason.strip():
        raise ValueError(f"{path}: categorized rule for {case_path!r} must have reason")
    if not _expected_statuses(rule):
        raise ValueError(
            f"{path}: categorized rule for {case_path!r} must have expected_status"
        )
    if not _message_needles(rule):
        raise ValueError(
            f"{path}: categorized rule for {case_path!r} must have message_contains"
        )


def _validate_rule_has_wpt_source_evidence(
    path: Path,
    rule: dict[str, Any],
    category_metadata: dict[str, Any],
) -> None:
    case_path = str(rule.get("case_path", "<unknown>"))
    case_base = _case_base(case_path)
    evidence = category_metadata.get("evidence")
    wpt_evidence_paths = {
        normalized
        for item in evidence
        if isinstance(item, dict) and item.get("kind") == "wpt"
        for normalized in [_wpt_case_path_from_manifest_reference(str(item.get("path", "")))]
        if normalized is not None
    }
    if case_base not in wpt_evidence_paths:
        raise ValueError(
            f"{path}: categorized rule for {case_path!r} must have matching "
            f"wpt evidence for {case_base!r}"
        )


def _message_needles(rule: dict[str, Any]) -> list[str]:
    explicit = rule.get("message_contains")
    if explicit is None:
        return []
    if isinstance(explicit, str) and explicit.strip():
        return [explicit]
    if (
        isinstance(explicit, list)
        and all(isinstance(item, str) and item.strip() for item in explicit)
    ):
        return explicit
    raise ValueError(
        f"rule for {rule.get('case_path', '<unknown>')} has invalid message_contains"
    )


def _failure_text(result: dict[str, Any]) -> str:
    parts: list[str] = []
    error = result.get("error")
    if error:
        parts.append(str(error))
    harness = result.get("harness_status_name")
    if harness:
        parts.append(str(harness))
    harness_message = result.get("harness_message")
    if harness_message:
        parts.append(str(harness_message))
    for failure in result.get("failures") or []:
        if isinstance(failure, dict):
            for key in ("name", "message", "status_name"):
                value = failure.get(key)
                if value:
                    parts.append(str(value))
        elif failure:
            parts.append(str(failure))
    return "\n".join(parts)


def _audit_entry(
    case_path: str,
    result: dict[str, Any] | None,
    rule: dict[str, Any] | None,
    *,
    note: str | None = None,
) -> dict[str, Any]:
    entry: dict[str, Any] = {"case_path": case_path}
    if result is not None:
        entry["status"] = result.get("status", "missing")
        for key in ("error", "harness_status_name", "harness_message"):
            value = result.get(key)
            if value:
                entry[key] = value
        failures = result.get("failures") or []
        if failures:
            entry["failures"] = failures
    if rule is not None:
        for key in (
            "category",
            "reason",
            "expected_status",
            "expected_statuses",
            "message_contains",
        ):
            if key in rule:
                entry[key] = rule[key]
    if note:
        entry["note"] = note
    return entry


def _category_counts(entries: list[dict[str, Any]]) -> dict[str, int]:
    counter: Counter[str] = Counter()
    for entry in entries:
        category = entry.get("category")
        counter[str(category) if category else "uncategorized"] += 1
    return dict(sorted(counter.items()))


def _matrix_rows_by_case_path(matrix: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    rows: dict[str, dict[str, Any]] = {}
    for idx, row in enumerate(matrix):
        case_path = row.get("case_path")
        if not isinstance(case_path, str) or not case_path:
            raise ValueError(f"matrix row #{idx + 1} must have case_path")
        if case_path in rows:
            raise ValueError(f"matrix contains duplicate case_path {case_path!r}")
        rows[case_path] = row
    return rows


def audit_matrix(
    matrix: list[dict[str, Any]],
    engine: str,
    rules: list[dict[str, Any]],
    *,
    categories: dict[str, Any] | None = None,
    allow_missing_known_failures: bool = False,
) -> dict[str, Any]:
    rows_by_case = _matrix_rows_by_case_path(matrix)
    rules_by_case = {str(rule["case_path"]): rule for rule in rules}

    known_failures: list[dict[str, Any]] = []
    resolved_known_failures: list[dict[str, Any]] = []
    mismatched_known_failures: list[dict[str, Any]] = []
    missing_expected_failures: list[dict[str, Any]] = []
    skipped_known_failures: list[dict[str, Any]] = []
    unexpected_failures: list[dict[str, Any]] = []

    for case_path, rule in rules_by_case.items():
        row = rows_by_case.get(case_path)
        result = None
        if row is not None:
            raw_result = (row.get("results") or {}).get(engine)
            if isinstance(raw_result, dict):
                result = raw_result
        if result is None:
            entry = _audit_entry(
                case_path,
                None,
                rule,
                note=f"matrix has no result for engine {engine}",
            )
            if allow_missing_known_failures:
                skipped_known_failures.append(entry)
            else:
                missing_expected_failures.append(entry)
            continue

        status = str(result.get("status", "missing"))
        if status == PASS_STATUS:
            resolved_known_failures.append(_audit_entry(case_path, result, rule))
            continue

        expected_statuses = _expected_statuses(rule)
        if expected_statuses and status not in expected_statuses:
            mismatched_known_failures.append(
                _audit_entry(
                    case_path,
                    result,
                    rule,
                    note=f"status {status!r} did not match {sorted(expected_statuses)!r}",
                )
            )
            continue

        text = _failure_text(result)
        missing_needles = [needle for needle in _message_needles(rule) if needle not in text]
        if missing_needles:
            mismatched_known_failures.append(
                _audit_entry(
                    case_path,
                    result,
                    rule,
                    note=f"missing expected failure text: {missing_needles!r}",
                )
            )
            continue

        known_failures.append(_audit_entry(case_path, result, rule))

    for row in matrix:
        case_path = str(row.get("case_path", ""))
        if case_path in rules_by_case:
            continue
        result = (row.get("results") or {}).get(engine)
        if not isinstance(result, dict):
            continue
        if str(result.get("status", "missing")) != PASS_STATUS:
            unexpected_failures.append(_audit_entry(case_path, result, None))

    counts = {
        "known_failures": len(known_failures),
        "resolved_known_failures": len(resolved_known_failures),
        "mismatched_known_failures": len(mismatched_known_failures),
        "missing_expected_failures": len(missing_expected_failures),
        "skipped_known_failures": len(skipped_known_failures),
        "unexpected_failures": len(unexpected_failures),
    }
    category_counts = {
        "known_failures": _category_counts(known_failures),
        "resolved_known_failures": _category_counts(resolved_known_failures),
        "mismatched_known_failures": _category_counts(mismatched_known_failures),
        "missing_expected_failures": _category_counts(missing_expected_failures),
        "skipped_known_failures": _category_counts(skipped_known_failures),
        "unexpected_failures": _category_counts(unexpected_failures),
    }
    return {
        "engine": engine,
        "allow_missing_known_failures": allow_missing_known_failures,
        "categories": categories or {},
        "counts": counts,
        "category_counts": category_counts,
        "known_failures": known_failures,
        "resolved_known_failures": resolved_known_failures,
        "mismatched_known_failures": mismatched_known_failures,
        "missing_expected_failures": missing_expected_failures,
        "skipped_known_failures": skipped_known_failures,
        "unexpected_failures": unexpected_failures,
        "ok": (
            counts["mismatched_known_failures"] == 0
            and counts["missing_expected_failures"] == 0
            and counts["resolved_known_failures"] == 0
            and counts["unexpected_failures"] == 0
        ),
    }


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m moli_benchmark.wpt_cross.audit",
        description="Audit a wpt_cross matrix against an explicit known-failure manifest.",
    )
    parser.add_argument("--matrix", type=Path, required=True, help="Path to matrix.json")
    parser.add_argument("--engine", required=True, help="Engine key to audit, e.g. moli")
    parser.add_argument(
        "--known-failures",
        type=Path,
        required=True,
        help="JSON manifest containing known failure rules",
    )
    parser.add_argument("--output", type=Path, default=None, help="Optional JSON output path")
    parser.add_argument(
        "--allow-missing-known-failures",
        action="store_true",
        help=(
            "Allow manifest rules that are absent from the matrix. Use only "
            "for focused single-case or small-slice investigations; full "
            "baseline audits should keep the default strict behavior."
        ),
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = _build_parser()
    args = parser.parse_args(argv)
    try:
        matrix = _load_json(args.matrix)
        if not isinstance(matrix, list):
            raise ValueError(f"{args.matrix} must contain a matrix row list")
        manifest = load_known_failure_manifest(args.known_failures)
        rules = manifest["rules"]
        audit = audit_matrix(
            matrix,
            args.engine,
            rules,
            categories=manifest.get("categories"),
            allow_missing_known_failures=args.allow_missing_known_failures,
        )
    except Exception as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.output:
        args.output.write_text(json.dumps(audit, indent=2, sort_keys=True), encoding="utf-8")

    counts = audit["counts"]
    print(
        "known={known_failures} resolved={resolved_known_failures} "
        "mismatched={mismatched_known_failures} missing={missing_expected_failures} "
        "skipped={skipped_known_failures} unexpected={unexpected_failures}".format(
            **counts
        )
    )
    for section in (
        "unexpected_failures",
        "mismatched_known_failures",
        "missing_expected_failures",
        "skipped_known_failures",
        "resolved_known_failures",
    ):
        entries = audit[section]
        if entries:
            print(f"{section}:")
            for entry in entries:
                print(f"  - {entry['case_path']}: {entry.get('status', 'missing')}")
                note = entry.get("note")
                if note:
                    print(f"    {note}")

    return 0 if audit["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
