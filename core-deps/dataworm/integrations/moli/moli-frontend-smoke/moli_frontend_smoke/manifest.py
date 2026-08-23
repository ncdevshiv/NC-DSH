from __future__ import annotations

import fnmatch
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

from .models import SmokeCase


@dataclass(frozen=True)
class SmokeManifest:
    path: Path
    schema_version: int
    complete: bool
    total_catalog_cases: int
    fixtures_sha256: str | None
    tools: dict[str, str]
    cases: tuple[SmokeCase, ...]
    raw: dict[str, Any]


def load_manifest(path: Path) -> SmokeManifest:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise RuntimeError(
            f"missing fixture manifest {path}; run `npm run build` first"
        ) from error
    if raw.get("schemaVersion") != 1:
        raise RuntimeError(f"unsupported manifest schema: {raw.get('schemaVersion')!r}")
    values = raw.get("cases")
    if not isinstance(values, list) or not values:
        raise RuntimeError("fixture manifest must contain a non-empty flat cases list")
    cases = tuple(SmokeCase.from_json(value) for value in values)
    validate_cases(cases)
    return SmokeManifest(
        path=path,
        schema_version=1,
        complete=bool(raw.get("complete")),
        total_catalog_cases=int(raw.get("totalCatalogCases", len(cases))),
        fixtures_sha256=(
            str(raw["fixturesSha256"]) if raw.get("fixturesSha256") is not None else None
        ),
        tools={str(key): str(value) for key, value in (raw.get("tools") or {}).items()},
        cases=cases,
        raw=raw,
    )


def validate_cases(cases: Iterable[SmokeCase]) -> None:
    values = tuple(cases)
    ids = [case.id for case in values]
    paths = [case.path for case in values]
    if len(ids) != len(set(ids)):
        raise RuntimeError("fixture manifest contains duplicate case ids")
    if len(paths) != len(set(paths)):
        raise RuntimeError("fixture manifest contains duplicate case paths")
    for case in values:
        expected_prefix = f"{case.framework}/{case.family}/"
        if not case.id.startswith(expected_prefix):
            raise RuntimeError(f"case id does not match metadata: {case.id}")
        if not case.path.startswith("/cases/") or not case.path.endswith("/index.html"):
            raise RuntimeError(f"invalid case path: {case.path}")


def select_cases(
    manifest: SmokeManifest,
    *,
    frameworks: set[str],
    families: set[str],
    complexities: set[str],
    patterns: list[str],
) -> tuple[SmokeCase, ...]:
    selected = []
    for case in manifest.cases:
        if frameworks and case.framework not in frameworks:
            continue
        if families and case.family not in families:
            continue
        if complexities and case.complexity not in complexities:
            continue
        if patterns and not any(fnmatch.fnmatchcase(case.id, pattern) for pattern in patterns):
            continue
        selected.append(case)
    if not selected:
        raise RuntimeError("case selection is empty")
    return tuple(selected)
