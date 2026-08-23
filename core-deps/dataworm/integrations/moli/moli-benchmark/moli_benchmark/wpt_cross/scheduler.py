"""Execution-order scheduling for wpt_cross case runs."""

from __future__ import annotations

import hashlib
import json
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any, TypeVar

T = TypeVar("T")

FIXED_RUN_SHUFFLE_SEED = "moli-wpt-cross-run-order-v1"
BUCKET_DEPTH = 1


def build_run_schedule(
    cases: Sequence[T],
    *,
    case_path: Callable[[T], str],
) -> tuple[list[T], dict[str, Any]]:
    """Return execution-ordered cases plus metadata.

    The input order remains the canonical result order. The returned list is a
    fixed pseudo-random order used only to submit work to the runner.
    """

    canonical_paths = [case_path(case) for case in cases]
    scheduled_cases, bucket_count = _prefix_balanced_shuffle(cases, case_path)

    scheduled_paths = [case_path(case) for case in scheduled_cases]
    metadata: dict[str, Any] = {
        "mode": "fixed-prefix-balanced-shuffle",
        "seed": FIXED_RUN_SHUFFLE_SEED,
        "bucket_depth": BUCKET_DEPTH,
        "bucket_count": bucket_count,
        "case_count": len(canonical_paths),
        "canonical_cases_sha256": _sha256_lines(canonical_paths),
        "scheduled_cases_sha256": _sha256_lines(scheduled_paths),
        "output_order": "canonical-case-path",
        "execution_order": "fixed-prefix-balanced-shuffle",
    }
    return scheduled_cases, metadata


def write_run_schedule(
    output_dir: Path,
    *,
    metadata: dict[str, Any],
    scheduled_case_paths: Sequence[str],
) -> None:
    """Write reproducibility artifacts for the execution schedule."""

    (output_dir / "schedule.txt").write_text(
        "\n".join(scheduled_case_paths) + "\n",
        encoding="utf-8",
    )
    (output_dir / "schedule.json").write_text(
        json.dumps(metadata, indent=2, sort_keys=True),
        encoding="utf-8",
    )


def _shuffle_key(case_path: str) -> str:
    payload = f"{FIXED_RUN_SHUFFLE_SEED}\0{case_path}".encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _prefix_balanced_shuffle(
    cases: Sequence[T],
    case_path: Callable[[T], str],
) -> tuple[list[T], int]:
    groups: dict[str, list[T]] = {}
    for case in cases:
        groups.setdefault(_case_bucket(case_path(case)), []).append(case)

    states: list[dict[str, Any]] = []
    for bucket in sorted(groups):
        group = groups[bucket]
        group.sort(key=lambda case: (_shuffle_key(case_path(case)), case_path(case)))
        group.reverse()
        states.append(
            {
                "bucket": bucket,
                "items": group,
                "weight": len(group),
                "current": 0,
                "tie": _shuffle_key(bucket),
            }
        )

    scheduled: list[T] = []
    total_weight = sum(state["weight"] for state in states)
    while states:
        for state in states:
            state["current"] += state["weight"]
        best_index = max(
            range(len(states)),
            key=lambda index: (states[index]["current"], states[index]["tie"]),
        )
        best = states[best_index]
        scheduled.append(best["items"].pop())
        best["current"] -= total_weight
        if not best["items"]:
            total_weight -= best["weight"]
            states.pop(best_index)

    return scheduled, len(groups)


def _case_bucket(case_path: str) -> str:
    path_only = case_path.split("?", 1)[0].split("#", 1)[0]
    parts = [part for part in path_only.split("/") if part]
    if len(parts) >= BUCKET_DEPTH:
        return "/".join(parts[:BUCKET_DEPTH])
    if parts:
        return parts[0]
    return "<root>"


def _sha256_lines(paths: Sequence[str]) -> str:
    payload = "".join(f"{path}\n" for path in paths).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()
