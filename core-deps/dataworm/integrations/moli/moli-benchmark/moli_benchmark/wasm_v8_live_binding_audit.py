"""Audit V8 source readiness for wasm global live-binding work.

This is deliberately a source-tree audit, not a runtime compatibility shim. It
helps distinguish a stock V8 tree from a candidate tree that has started to
carry Moli-owned wasm module-binding changes.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any


VERSION_HEADER = "include/v8-version.h"


@dataclass(frozen=True)
class SourceProbe:
    path: str
    name: str
    pattern: str
    description: str
    scope_pattern: str | None = None


UNPATCHED_READ_PROBES = [
    SourceProbe(
        "src/objects/module.cc",
        "namespace_get_export_reads_cell_value",
        r"JSModuleNamespace::GetExport[\s\S]*?Cell::value|JSModuleNamespace::GetExport[\s\S]*?->value\(",
        "module namespace export access still reads ordinary Cell::value",
        scope_pattern=r"JSModuleNamespace::GetExport",
    ),
    SourceProbe(
        "src/objects/module.cc",
        "namespace_get_property_attributes_reads_cell_value",
        r"JSModuleNamespace::GetPropertyAttributes[\s\S]*?Cell::value|JSModuleNamespace::GetPropertyAttributes[\s\S]*?->value\(",
        "module namespace property attributes still read ordinary Cell::value",
        scope_pattern=r"JSModuleNamespace::GetPropertyAttributes",
    ),
    SourceProbe(
        "src/objects/source-text-module.cc",
        "source_text_load_variable_reads_cell_value",
        r"SourceTextModule::LoadVariable[\s\S]*?->value\(",
        "SourceTextModule::LoadVariable still reads ordinary Cell::value",
        scope_pattern=r"SourceTextModule::LoadVariable",
    ),
    SourceProbe(
        "src/interpreter/interpreter-generator.cc",
        "ignition_lda_module_variable_direct_cell_offset",
        r"IGNITION_HANDLER\(LdaModuleVariable[\s\S]*?Cell::kValueOffset",
        "Ignition LdaModuleVariable still loads Cell::kValueOffset directly",
        scope_pattern=r"IGNITION_HANDLER\(LdaModuleVariable",
    ),
    SourceProbe(
        "src/baseline",
        "baseline_lda_module_variable_direct_cell_offset",
        r"LdaModuleVariable[\s\S]*?Cell::kValueOffset",
        "Baseline LdaModuleVariable still loads Cell::kValueOffset directly",
        scope_pattern=r"LdaModuleVariable",
    ),
    SourceProbe(
        "src/maglev/maglev-graph-builder.cc",
        "maglev_lda_module_variable_direct_cell_offset",
        r"VisitLdaModuleVariable[\s\S]*?Cell::kValueOffset",
        "Maglev LdaModuleVariable still loads Cell::kValueOffset directly",
        scope_pattern=r"VisitLdaModuleVariable",
    ),
    SourceProbe(
        "src/compiler/bytecode-graph-builder.cc",
        "turbofan_bytecode_graph_builder_load_module",
        r"BytecodeGraphBuilder::VisitLdaModuleVariable[\s\S]*?javascript\(\)->LoadModule",
        "TurboFan bytecode graph builder still emits JSLoadModule for module binding reads",
        scope_pattern=r"BytecodeGraphBuilder::VisitLdaModuleVariable",
    ),
    SourceProbe(
        "src/compiler/js-typed-lowering.cc",
        "turbofan_jsloadmodule_for_cell_value",
        r"ReduceJSLoadModule[\s\S]*?AccessBuilder::ForCellValue",
        "TurboFan JSLoadModule lowering still lowers to ForCellValue",
        scope_pattern=r"ReduceJSLoadModule",
    ),
    SourceProbe(
        "src/compiler/js-native-context-specialization.cc",
        "namespace_specialization_for_cell_value",
        r"AccessBuilder::ForCellValue",
        "optimized module namespace specialization still reads ForCellValue",
    ),
]


PATCH_MARKER_PROBES = [
    SourceProbe(
        "include/v8-script.h",
        "public_wasm_global_export_api",
        r"SetSyntheticModuleExportWasmGlobal",
        "public V8 API exposes a wasm-global synthetic export setter",
    ),
    SourceProbe(
        "src/objects/synthetic-module.cc",
        "synthetic_module_wasm_global_setter",
        r"SetExportWasmGlobal|SetSyntheticModuleExportWasmGlobal",
        "SyntheticModule has an internal wasm-global export setter",
    ),
    SourceProbe(
        "src/objects",
        "internal_wasm_global_binding_object",
        r"WasmGlobalExportBinding",
        "V8 has a distinct wasm global export binding object or descriptor",
    ),
    SourceProbe(
        "src/wasm",
        "wasm_global_js_value_materializer",
        r"GetJSValue|Materialize.*WasmGlobal|WasmGlobal.*Materialize",
        "wasm global storage can be materialized as a JS module binding value",
    ),
]

PATCHED_READ_PATH_PROBES = [
    SourceProbe(
        "src/objects/module.cc",
        "namespace_get_export_materializes_wasm_binding",
        r"JSModuleNamespace::GetExport[\s\S]*?(MaterializeModuleBinding|WasmGlobalExportBinding)",
        "module namespace export access has an explicit wasm/materialized binding path",
        scope_pattern=r"JSModuleNamespace::GetExport",
    ),
    SourceProbe(
        "src/objects/module.cc",
        "namespace_get_property_attributes_materializes_wasm_binding",
        r"JSModuleNamespace::GetPropertyAttributes[\s\S]*?(MaterializeModuleBinding|WasmGlobalExportBinding)",
        "module namespace property attributes have an explicit wasm/materialized binding path",
        scope_pattern=r"JSModuleNamespace::GetPropertyAttributes",
    ),
    SourceProbe(
        "src/objects/source-text-module.cc",
        "source_text_load_variable_materializes_wasm_binding",
        r"SourceTextModule::LoadVariable[\s\S]*?(MaterializeModuleBinding|WasmGlobalExportBinding)",
        "SourceTextModule::LoadVariable has an explicit wasm/materialized binding path",
        scope_pattern=r"SourceTextModule::LoadVariable",
    ),
    SourceProbe(
        "src/interpreter/interpreter-generator.cc",
        "ignition_lda_module_variable_materializes_wasm_binding",
        r"IGNITION_HANDLER\(LdaModuleVariable[\s\S]*?(MaterializeModuleBinding|WasmGlobalExportBinding|kMaterializeModuleBindingValue|LoadMaterializedModuleBinding)",
        "Ignition LdaModuleVariable materializes wasm-aware module bindings",
        scope_pattern=r"IGNITION_HANDLER\(LdaModuleVariable",
    ),
    SourceProbe(
        "src/baseline",
        "baseline_lda_module_variable_materializes_wasm_binding",
        r"LdaModuleVariable[\s\S]*?(MaterializeModuleBinding|WasmGlobalExportBinding|kMaterializeModuleBindingValue|LoadMaterializedModuleBinding)",
        "Baseline LdaModuleVariable materializes wasm-aware module bindings",
        scope_pattern=r"LdaModuleVariable",
    ),
    SourceProbe(
        "src/maglev/maglev-graph-builder.cc",
        "maglev_lda_module_variable_materializes_wasm_binding",
        r"VisitLdaModuleVariable[\s\S]*?(MaterializeModuleBinding|WasmGlobalExportBinding|kMaterializeModuleBindingValue|LoadMaterializedModuleBinding)",
        "Maglev LdaModuleVariable materializes wasm-aware module bindings",
        scope_pattern=r"VisitLdaModuleVariable",
    ),
    SourceProbe(
        "src/compiler/bytecode-graph-builder.cc",
        "turbofan_bytecode_graph_builder_materialized_load_module",
        r"BytecodeGraphBuilder::VisitLdaModuleVariable[\s\S]*?(LoadMaterializedModuleBinding|MaterializeModuleBinding|WasmGlobalExportBinding)",
        "TurboFan bytecode graph builder emits a materialized module-binding read",
        scope_pattern=r"BytecodeGraphBuilder::VisitLdaModuleVariable",
    ),
    SourceProbe(
        "src/compiler/js-typed-lowering.cc",
        "turbofan_jsloadmodule_materialized_binding",
        r"ReduceJSLoadModule[\s\S]*?(MaterializedModuleBinding|MaterializeModuleBinding|WasmGlobalExportBinding)",
        "TurboFan JSLoadModule lowering recognizes wasm/materialized module bindings",
        scope_pattern=r"ReduceJSLoadModule",
    ),
    SourceProbe(
        "src/compiler/js-native-context-specialization.cc",
        "namespace_specialization_materialized_binding",
        r"(MaterializedModuleBinding|MaterializeModuleBinding|WasmGlobalExportBinding)",
        "optimized namespace specialization recognizes wasm/materialized module bindings",
    ),
]


def _read_text(path: Path) -> str | None:
    try:
        return path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return None


def _mask_cpp_comments_and_literals_preserving_lines(text: str) -> str:
    """Mask C++ comments and literals before source probes, preserving lines."""

    def masked(fragment: str) -> str:
        return "".join("\n" if ch == "\n" else " " for ch in fragment)

    out: list[str] = []
    i = 0
    length = len(text)
    while i < length:
        ch = text[i]
        next_ch = text[i + 1] if i + 1 < length else ""

        if ch == "R" and next_ch == '"':
            open_paren = text.find("(", i + 2)
            if open_paren != -1:
                delimiter = text[i + 2 : open_paren]
                end_marker = ")" + delimiter + '"'
                end = text.find(end_marker, open_paren + 1)
                if end != -1:
                    end += len(end_marker)
                    out.append(masked(text[i:end]))
                    i = end
                    continue

        if ch in {'"', "'"}:
            quote = ch
            literal_start = i
            i += 1
            while i < length:
                current = text[i]
                i += 1
                if current == "\\" and i < length:
                    i += 1
                    continue
                if current == quote:
                    break
            out.append(masked(text[literal_start:i]))
            continue

        if ch == "/" and next_ch == "/":
            comment_start = i
            i += 2
            while i < length and text[i] != "\n":
                i += 1
            out.append(masked(text[comment_start:i]))
            continue

        if ch == "/" and next_ch == "*":
            comment_start = i
            i += 2
            while i < length:
                if text[i] == "*" and i + 1 < length and text[i + 1] == "/":
                    i += 2
                    break
                i += 1
            out.append(masked(text[comment_start:i]))
            continue

        out.append(ch)
        i += 1

    return "".join(out)


def _iter_source_texts(root: Path, relative: str) -> list[tuple[str, str]]:
    target = root / relative
    if target.is_file():
        text = _read_text(target)
        return [] if text is None else [(relative, text)]
    if not target.is_dir():
        return []
    texts: list[tuple[str, str]] = []
    for path in sorted(target.rglob("*")):
        if not path.is_file() or path.suffix not in {".cc", ".h", ".inc", ".tq"}:
            continue
        text = _read_text(path)
        if text is not None:
            texts.append((path.relative_to(root).as_posix(), text))
    return texts


def _find_matching_brace(text: str, open_brace: int) -> int | None:
    depth = 0
    for index in range(open_brace, len(text)):
        ch = text[index]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return index + 1
    return None


def _probe_regions(text: str, probe: SourceProbe) -> list[tuple[int, str]]:
    if probe.scope_pattern is None:
        return [(0, text)]

    regions: list[tuple[int, str]] = []
    for match in re.finditer(probe.scope_pattern, text, flags=re.MULTILINE):
        open_brace = text.find("{", match.end())
        if open_brace == -1:
            continue
        if ";" in text[match.end() : open_brace]:
            continue
        end = _find_matching_brace(text, open_brace)
        if end is None:
            continue
        regions.append((match.start(), text[match.start() : end]))
    return regions


def _probe(root: Path, probe: SourceProbe) -> dict[str, Any]:
    matches: list[dict[str, Any]] = []
    for relative, text in _iter_source_texts(root, probe.path):
        probe_text = _mask_cpp_comments_and_literals_preserving_lines(text)
        for region_start, region in _probe_regions(probe_text, probe):
            for match in re.finditer(probe.pattern, region, flags=re.MULTILINE):
                line = probe_text.count("\n", 0, region_start + match.start()) + 1
                matches.append({"path": relative, "line": line})
    return {
        "name": probe.name,
        "path": probe.path,
        "description": probe.description,
        "found": bool(matches),
        "count": len(matches),
        "matches": matches,
    }


def _parse_v8_version(root: Path) -> dict[str, int] | None:
    text = _read_text(root / VERSION_HEADER)
    if text is None:
        return None
    version: dict[str, int] = {}
    for name, value in re.findall(r"#define\s+V8_(MAJOR|MINOR|BUILD|PATCH)_\w*\s+(\d+)", text):
        version[name.lower()] = int(value)
    required = {"major", "minor", "build", "patch"}
    return version if required.issubset(version) else None


def audit_v8_source(root: Path) -> dict[str, Any]:
    root = root.resolve()
    version = _parse_v8_version(root)
    missing_files = [
        relative
        for relative in {
            VERSION_HEADER,
            *(probe.path for probe in UNPATCHED_READ_PROBES if not probe.path.endswith("objects")),
        }
        if not (root / relative).exists()
    ]
    missing_probe_inputs = [
        probe.path
        for probe in UNPATCHED_READ_PROBES
        if not _iter_source_texts(root, probe.path)
    ]
    direct_read_sites = [_probe(root, probe) for probe in UNPATCHED_READ_PROBES]
    patch_markers = [_probe(root, probe) for probe in PATCH_MARKER_PROBES]
    patched_read_paths = [_probe(root, probe) for probe in PATCHED_READ_PATH_PROBES]
    found_direct_reads = sum(probe["count"] for probe in direct_read_sites)
    found_patch_markers = sum(probe["count"] for probe in patch_markers)
    found_patched_read_paths = sum(probe["count"] for probe in patched_read_paths)
    patch_markers_complete = all(probe["found"] for probe in patch_markers)
    read_paths_rewritten = found_direct_reads == 0
    patched_read_paths_complete = all(probe["found"] for probe in patched_read_paths)

    if missing_files or missing_probe_inputs:
        diagnosis = "missing-source-files"
    elif patch_markers_complete and read_paths_rewritten and patched_read_paths_complete:
        diagnosis = "candidate-patched-read-paths-rewritten"
    elif patch_markers_complete and read_paths_rewritten:
        diagnosis = "candidate-patched-read-path-markers-missing"
    elif patch_markers_complete:
        diagnosis = "candidate-patched-direct-reads-remain"
    elif found_patch_markers:
        diagnosis = "partial-patch-markers"
    elif found_direct_reads:
        diagnosis = "unpatched-stock-v8"
    else:
        diagnosis = "unknown"

    return {
        "v8_root": str(root),
        "version": version,
        "diagnosis": diagnosis,
        "missing_files": missing_files,
        "missing_probe_inputs": missing_probe_inputs,
        "direct_read_sites": direct_read_sites,
        "patch_markers": patch_markers,
        "patched_read_paths": patched_read_paths,
        "counts": {
            "direct_read_sites": found_direct_reads,
            "patch_markers": found_patch_markers,
            "patched_read_paths": found_patched_read_paths,
            "missing_files": len(missing_files),
            "missing_probe_inputs": len(missing_probe_inputs),
        },
        "patch_markers_complete": patch_markers_complete,
        "read_paths_rewritten": read_paths_rewritten,
        "patched_read_paths_complete": patched_read_paths_complete,
        "notes": [
            "This audit is not proof that a V8 fork implements wasm global live bindings correctly.",
            "A passing runtime fix still needs focused Moli regressions and WPT evidence.",
        ],
    }


def _format_human(audit: dict[str, Any]) -> str:
    version = audit.get("version")
    version_text = "unknown"
    if isinstance(version, dict):
        version_text = (
            f"{version.get('major')}.{version.get('minor')}"
            f".{version.get('build')}.{version.get('patch')}"
        )
    lines = [
        f"v8 root: {audit['v8_root']}",
        f"version: {version_text}",
        f"diagnosis: {audit['diagnosis']}",
        f"missing files: {audit['counts']['missing_files']}",
        f"missing probe inputs: {audit['counts']['missing_probe_inputs']}",
        f"direct module cell read sites: {audit['counts']['direct_read_sites']}",
        f"patch markers: {audit['counts']['patch_markers']}",
        f"patched read paths: {audit['counts']['patched_read_paths']}",
    ]
    for group in ("direct_read_sites", "patch_markers", "patched_read_paths"):
        lines.append(f"{group}:")
        for entry in audit[group]:
            state = "found" if entry["found"] else "missing"
            lines.append(f"  - {entry['name']}: {state} ({entry['count']})")
            for match in entry["matches"][:5]:
                lines.append(f"    {match['path']}:{match['line']}")
            if entry["count"] > 5:
                lines.append(f"    ... {entry['count'] - 5} more")
    return "\n".join(lines)


def build_arg_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--v8-root", required=True, type=Path, help="V8 source root to audit")
    parser.add_argument("--json-output", type=Path, help="write machine-readable audit JSON")
    parser.add_argument(
        "--json",
        action="store_true",
        help="print machine-readable audit JSON to stdout instead of a text summary",
    )
    parser.add_argument(
        "--require-patched",
        action="store_true",
        help=(
            "return nonzero unless patch markers are present, source files are "
            "complete, and stock module cell read probes no longer match"
        ),
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_arg_parser().parse_args(argv)
    audit = audit_v8_source(args.v8_root)
    if args.json_output:
        args.json_output.write_text(json.dumps(audit, indent=2, sort_keys=True), encoding="utf-8")
    if args.json:
        print(json.dumps(audit, indent=2, sort_keys=True))
    else:
        print(_format_human(audit))
    if audit["missing_files"] or audit["missing_probe_inputs"]:
        return 2
    if args.require_patched and (
        not audit["patch_markers_complete"]
        or not audit["read_paths_rewritten"]
        or not audit["patched_read_paths_complete"]
    ):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
