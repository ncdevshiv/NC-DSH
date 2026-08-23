from __future__ import annotations

import json
from pathlib import Path

import pytest

from moli_frontend_smoke.manifest import load_manifest, select_cases


def _manifest(tmp_path: Path) -> Path:
    cases = []
    for framework in ("react", "vue", "angular"):
        cases.append(
            {
                "id": f"{framework}/family/case",
                "framework": framework,
                "family": "family",
                "complexity": "simple",
                "slug": "case",
                "title": "Case",
                "variant": 0,
                "seed": 1,
                "size": 4,
                "path": f"/cases/{framework}/family/case/index.html",
            }
        )
    path = tmp_path / "manifest.json"
    path.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "complete": False,
                "totalCatalogCases": 300,
                "tools": {},
                "cases": cases,
            }
        )
    )
    return path


def test_load_and_filter_flat_manifest(tmp_path: Path) -> None:
    manifest = load_manifest(_manifest(tmp_path))
    assert [case.framework for case in manifest.cases] == ["react", "vue", "angular"]
    selected = select_cases(
        manifest,
        frameworks={"vue"},
        families=set(),
        complexities={"simple"},
        patterns=["vue/*"],
    )
    assert [case.id for case in selected] == ["vue/family/case"]


def test_duplicate_ids_are_rejected(tmp_path: Path) -> None:
    path = _manifest(tmp_path)
    value = json.loads(path.read_text())
    value["cases"].append(dict(value["cases"][0], path="/cases/react/family/other/index.html"))
    path.write_text(json.dumps(value))
    with pytest.raises(RuntimeError, match="duplicate case ids"):
        load_manifest(path)


def test_empty_selection_is_rejected(tmp_path: Path) -> None:
    manifest = load_manifest(_manifest(tmp_path))
    with pytest.raises(RuntimeError, match="selection is empty"):
        select_cases(
            manifest,
            frameworks={"missing"},
            families=set(),
            complexities=set(),
            patterns=[],
        )
