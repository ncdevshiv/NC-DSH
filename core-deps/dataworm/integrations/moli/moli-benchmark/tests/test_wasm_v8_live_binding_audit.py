from __future__ import annotations

import json
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path

from moli_benchmark.wasm_v8_live_binding_audit import audit_v8_source, main


def _write(root: Path, relative: str, text: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _write_version(root: Path) -> None:
    _write(
        root,
        "include/v8-version.h",
        """
#define V8_MAJOR_VERSION 14
#define V8_MINOR_VERSION 6
#define V8_BUILD_NUMBER 202
#define V8_PATCH_LEVEL 24
""",
    )


def _write_stock_v8_fixture(root: Path) -> None:
    _write_version(root)
    _write(
        root,
        "src/objects/module.cc",
        """
MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return cell->value(); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return cell->value(); }
""",
    )
    _write(
        root,
        "src/objects/source-text-module.cc",
        "Handle<Object> SourceTextModule::LoadVariable() { return cell->value(); }",
    )
    _write(
        root,
        "src/interpreter/interpreter-generator.cc",
        "IGNITION_HANDLER(LdaModuleVariable, InterpreterAssembler) { LoadObjectField(cell, Cell::kValueOffset); }",
    )
    _write(
        root,
        "src/baseline/x64/baseline-assembler-x64-inl.h",
        "void BaselineAssembler::LdaModuleVariable() { LoadTaggedField(acc, cell, Cell::kValueOffset); }",
    )
    _write(
        root,
        "src/maglev/maglev-graph-builder.cc",
        "ReduceResult MaglevGraphBuilder::VisitLdaModuleVariable() { BuildLoadTaggedField(cell, Cell::kValueOffset); }",
    )
    _write(
        root,
        "src/compiler/bytecode-graph-builder.cc",
        "void BytecodeGraphBuilder::VisitLdaModuleVariable() { NewNode(javascript()->LoadModule(cell_index), module); }",
    )
    _write(
        root,
        "src/compiler/js-typed-lowering.cc",
        "Reduction JSTypedLowering::ReduceJSLoadModule(Node* node) { AccessBuilder::ForCellValue(); }",
    )
    _write(
        root,
        "src/compiler/js-native-context-specialization.cc",
        "void Optimize() { AccessBuilder::ForCellValue(); }",
    )


def _write_rewritten_read_path_fixture(root: Path, *, include_baseline: bool = True) -> None:
    _write_version(root)
    _write(
        root,
        "src/objects/module.cc",
        """
MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return MaterializeModuleBindingValue(cell); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return MaterializeModuleBindingValue(cell); }
""",
    )
    _write(
        root,
        "src/objects/source-text-module.cc",
        "Handle<Object> SourceTextModule::LoadVariable() { return MaterializeModuleBindingValue(cell); }",
    )
    _write(
        root,
        "src/interpreter/interpreter-generator.cc",
        "IGNITION_HANDLER(LdaModuleVariable, InterpreterAssembler) { CallBuiltin(Builtin::kMaterializeModuleBindingValue); }",
    )
    if include_baseline:
        _write(
            root,
            "src/baseline/x64/baseline-assembler-x64-inl.h",
            "void BaselineAssembler::LdaModuleVariable() { CallBuiltin(Builtin::kMaterializeModuleBindingValue); }",
        )
    else:
        (root / "src/baseline").mkdir(parents=True, exist_ok=True)
    _write(
        root,
        "src/maglev/maglev-graph-builder.cc",
        "ReduceResult MaglevGraphBuilder::VisitLdaModuleVariable() { BuildCallBuiltin(Builtin::kMaterializeModuleBindingValue); }",
    )
    _write(
        root,
        "src/compiler/bytecode-graph-builder.cc",
        "void BytecodeGraphBuilder::VisitLdaModuleVariable() { NewNode(javascript()->LoadMaterializedModuleBinding(cell_index), module); }",
    )
    _write(
        root,
        "src/compiler/js-typed-lowering.cc",
        "Reduction JSTypedLowering::ReduceJSLoadModule(Node* node) { ReduceToMaterializedModuleBinding(node); }",
    )
    _write(
        root,
        "src/compiler/js-native-context-specialization.cc",
        "void Optimize() { ReduceToMaterializedModuleBinding(); }",
    )


def _write_silenced_read_path_fixture(root: Path) -> None:
    _write_version(root)
    _write(
        root,
        "src/objects/module.cc",
        """
MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return LoadValueThroughNewPath(cell); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return LoadValueThroughNewPath(cell); }
""",
    )
    _write(
        root,
        "src/objects/source-text-module.cc",
        "Handle<Object> SourceTextModule::LoadVariable() { return LoadValueThroughNewPath(cell); }",
    )
    _write(
        root,
        "src/interpreter/interpreter-generator.cc",
        "IGNITION_HANDLER(LdaModuleVariable, InterpreterAssembler) { Dispatch(); }",
    )
    _write(
        root,
        "src/baseline/x64/baseline-assembler-x64-inl.h",
        "void BaselineAssembler::LdaModuleVariable() { LoadAnyValue(acc, cell); }",
    )
    _write(
        root,
        "src/maglev/maglev-graph-builder.cc",
        "ReduceResult MaglevGraphBuilder::VisitLdaModuleVariable() { return ReduceResult::Done(); }",
    )
    _write(
        root,
        "src/compiler/bytecode-graph-builder.cc",
        "void BytecodeGraphBuilder::VisitLdaModuleVariable() { NewNode(javascript()->LoadUnknownModule(cell_index), module); }",
    )
    _write(
        root,
        "src/compiler/js-typed-lowering.cc",
        "Reduction JSTypedLowering::ReduceJSLoadModule(Node* node) { return NoChange(); }",
    )
    _write(
        root,
        "src/compiler/js-native-context-specialization.cc",
        "void Optimize() { KeepGenericNamespaceLoad(); }",
    )


def _write_comment_only_candidate_fixture(root: Path) -> None:
    _write_version(root)
    _write(
        root,
        "src/objects/module.cc",
        """
// MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return cell->value(); }
// MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return MaterializeModuleBindingValue(cell); }
/*
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return cell->value(); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return MaterializeModuleBindingValue(cell); }
*/
const char* ignored = "JSModuleNamespace::GetExport() { return MaterializeModuleBindingValue(cell); }";
MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return LoadValueThroughNewPath(cell); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return LoadValueThroughNewPath(cell); }
""",
    )
    _write(
        root,
        "src/objects/source-text-module.cc",
        """
// Handle<Object> SourceTextModule::LoadVariable() { return cell->value(); }
// Handle<Object> SourceTextModule::LoadVariable() { return MaterializeModuleBindingValue(cell); }
Handle<Object> SourceTextModule::LoadVariable() { return LoadValueThroughNewPath(cell); }
""",
    )
    _write(
        root,
        "src/interpreter/interpreter-generator.cc",
        """
// IGNITION_HANDLER(LdaModuleVariable, InterpreterAssembler) { LoadObjectField(cell, Cell::kValueOffset); }
// IGNITION_HANDLER(LdaModuleVariable, InterpreterAssembler) { CallBuiltin(Builtin::kMaterializeModuleBindingValue); }
IGNITION_HANDLER(LdaModuleVariable, InterpreterAssembler) { Dispatch(); }
""",
    )
    _write(
        root,
        "src/baseline/x64/baseline-assembler-x64-inl.h",
        """
// void BaselineAssembler::LdaModuleVariable() { LoadTaggedField(acc, cell, Cell::kValueOffset); }
// void BaselineAssembler::LdaModuleVariable() { CallBuiltin(Builtin::kMaterializeModuleBindingValue); }
void BaselineAssembler::LdaModuleVariable() { LoadAnyValue(acc, cell); }
""",
    )
    _write(
        root,
        "src/maglev/maglev-graph-builder.cc",
        """
// ReduceResult MaglevGraphBuilder::VisitLdaModuleVariable() { BuildLoadTaggedField(cell, Cell::kValueOffset); }
// ReduceResult MaglevGraphBuilder::VisitLdaModuleVariable() { BuildCallBuiltin(Builtin::kMaterializeModuleBindingValue); }
ReduceResult MaglevGraphBuilder::VisitLdaModuleVariable() { return ReduceResult::Done(); }
""",
    )
    _write(
        root,
        "src/compiler/bytecode-graph-builder.cc",
        """
// void BytecodeGraphBuilder::VisitLdaModuleVariable() { NewNode(javascript()->LoadModule(cell_index), module); }
// void BytecodeGraphBuilder::VisitLdaModuleVariable() { NewNode(javascript()->LoadMaterializedModuleBinding(cell_index), module); }
void BytecodeGraphBuilder::VisitLdaModuleVariable() { NewNode(javascript()->LoadUnknownModule(cell_index), module); }
""",
    )
    _write(
        root,
        "src/compiler/js-typed-lowering.cc",
        """
// Reduction JSTypedLowering::ReduceJSLoadModule(Node* node) { AccessBuilder::ForCellValue(); }
// Reduction JSTypedLowering::ReduceJSLoadModule(Node* node) { ReduceToMaterializedModuleBinding(node); }
Reduction JSTypedLowering::ReduceJSLoadModule(Node* node) { return NoChange(); }
""",
    )
    _write(
        root,
        "src/compiler/js-native-context-specialization.cc",
        """
// void Optimize() { AccessBuilder::ForCellValue(); }
// void Optimize() { ReduceToMaterializedModuleBinding(); }
void Optimize() { KeepGenericNamespaceLoad(); }
""",
    )
    _write(
        root,
        "include/v8-script.h",
        """
// Maybe<bool> SetSyntheticModuleExportWasmGlobal();
constexpr const char* ignored = "SetSyntheticModuleExportWasmGlobal";
""",
    )
    _write(
        root,
        "src/objects/synthetic-module.cc",
        "// Maybe<bool> SyntheticModule::SetExportWasmGlobal() { return Just(true); }",
    )
    _write(
        root,
        "src/objects/wasm-global-export-binding.h",
        """
// class WasmGlobalExportBinding {};
constexpr const char* ignored = R"(WasmGlobalExportBinding)";
""",
    )
    _write(
        root,
        "src/wasm/wasm-global-export.cc",
        "// Object WasmGlobalObject::GetJSValue() { return Object(); }",
    )


def _write_patch_markers(root: Path) -> None:
    _write(
        root,
        "include/v8-script.h",
        "Maybe<bool> SetSyntheticModuleExportWasmGlobal();",
    )
    _write(
        root,
        "src/objects/synthetic-module.cc",
        "Maybe<bool> SyntheticModule::SetExportWasmGlobal() { return Just(true); }",
    )
    _write(
        root,
        "src/objects/wasm-global-export-binding.h",
        "class WasmGlobalExportBinding {};",
    )
    _write(
        root,
        "src/wasm/wasm-global-export.cc",
        "Object WasmGlobalObject::GetJSValue() { return Object(); }",
    )


class WasmV8LiveBindingAuditTests(unittest.TestCase):
    def test_audit_identifies_stock_v8_module_cell_read_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_stock_v8_fixture(root)

            audit = audit_v8_source(root)

        self.assertEqual(audit["diagnosis"], "unpatched-stock-v8")
        self.assertEqual(audit["version"], {"major": 14, "minor": 6, "build": 202, "patch": 24})
        self.assertEqual(audit["counts"]["missing_files"], 0)
        self.assertGreaterEqual(audit["counts"]["direct_read_sites"], 8)
        attributes_probe = next(
            probe
            for probe in audit["direct_read_sites"]
            if probe["name"] == "namespace_get_property_attributes_reads_cell_value"
        )
        self.assertEqual(attributes_probe["count"], 1)
        baseline_probe = next(
            probe
            for probe in audit["direct_read_sites"]
            if probe["name"] == "baseline_lda_module_variable_direct_cell_offset"
        )
        self.assertEqual(baseline_probe["count"], 1)
        graph_builder_probe = next(
            probe
            for probe in audit["direct_read_sites"]
            if probe["name"] == "turbofan_bytecode_graph_builder_load_module"
        )
        self.assertEqual(graph_builder_probe["count"], 1)
        self.assertEqual(audit["counts"]["patch_markers"], 0)
        self.assertEqual(audit["counts"]["patched_read_paths"], 0)
        self.assertFalse(audit["patch_markers_complete"])
        self.assertFalse(audit["patched_read_paths_complete"])

    def test_audit_keeps_marker_only_candidate_out_of_require_patched(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_stock_v8_fixture(root)
            _write_patch_markers(root)

            audit = audit_v8_source(root)
            with redirect_stdout(StringIO()):
                code = main(["--v8-root", str(root), "--require-patched"])

        self.assertEqual(audit["diagnosis"], "candidate-patched-direct-reads-remain")
        self.assertEqual(audit["counts"]["patch_markers"], 4)
        self.assertTrue(audit["patch_markers_complete"])
        self.assertFalse(audit["read_paths_rewritten"])
        self.assertFalse(audit["patched_read_paths_complete"])
        self.assertIn("not proof", audit["notes"][0])
        self.assertEqual(code, 1)

    def test_audit_rejects_silenced_direct_reads_without_materialized_read_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_silenced_read_path_fixture(root)
            _write_patch_markers(root)

            audit = audit_v8_source(root)
            with redirect_stdout(StringIO()):
                code = main(["--v8-root", str(root), "--require-patched"])

        self.assertEqual(audit["diagnosis"], "candidate-patched-read-path-markers-missing")
        self.assertEqual(audit["counts"]["direct_read_sites"], 0)
        self.assertTrue(audit["patch_markers_complete"])
        self.assertTrue(audit["read_paths_rewritten"])
        self.assertFalse(audit["patched_read_paths_complete"])
        self.assertEqual(code, 1)

    def test_audit_ignores_comment_only_patch_markers_and_read_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_comment_only_candidate_fixture(root)

            audit = audit_v8_source(root)
            with redirect_stdout(StringIO()):
                code = main(["--v8-root", str(root), "--require-patched"])

        self.assertEqual(audit["diagnosis"], "unknown")
        self.assertEqual(audit["counts"]["direct_read_sites"], 0)
        self.assertEqual(audit["counts"]["patch_markers"], 0)
        self.assertEqual(audit["counts"]["patched_read_paths"], 0)
        self.assertFalse(audit["patch_markers_complete"])
        self.assertFalse(audit["patched_read_paths_complete"])
        self.assertEqual(code, 1)

    def test_audit_scopes_namespace_probes_to_their_own_function_body(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_stock_v8_fixture(root)
            _write(
                root,
                "src/objects/module.cc",
                """
MaybeDirectHandle<Object> JSModuleNamespace::GetExport();
MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return LoadValueThroughNewPath(cell); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return cell->value(); }
""",
            )

            audit = audit_v8_source(root)

        get_export_probe = next(
            probe
            for probe in audit["direct_read_sites"]
            if probe["name"] == "namespace_get_export_reads_cell_value"
        )
        attributes_probe = next(
            probe
            for probe in audit["direct_read_sites"]
            if probe["name"] == "namespace_get_property_attributes_reads_cell_value"
        )
        self.assertEqual(get_export_probe["count"], 0)
        self.assertEqual(attributes_probe["count"], 1)

        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_stock_v8_fixture(root)
            _write(
                root,
                "src/objects/module.cc",
                """
MaybeDirectHandle<Object> JSModuleNamespace::GetExport();
MaybeDirectHandle<Object> JSModuleNamespace::GetExport() { return LoadValueThroughNewPath(cell); }
Maybe<PropertyAttributes> JSModuleNamespace::GetPropertyAttributes() { return MaterializeModuleBindingValue(cell); }
""",
            )

            audit = audit_v8_source(root)

        get_export_probe = next(
            probe
            for probe in audit["patched_read_paths"]
            if probe["name"] == "namespace_get_export_materializes_wasm_binding"
        )
        attributes_probe = next(
            probe
            for probe in audit["patched_read_paths"]
            if probe["name"] == "namespace_get_property_attributes_materializes_wasm_binding"
        )
        self.assertEqual(get_export_probe["count"], 0)
        self.assertEqual(attributes_probe["count"], 1)

    def test_audit_accepts_candidate_with_markers_and_rewritten_read_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_rewritten_read_path_fixture(root)
            _write_patch_markers(root)

            audit = audit_v8_source(root)
            with redirect_stdout(StringIO()):
                code = main(["--v8-root", str(root), "--require-patched"])

        self.assertEqual(audit["diagnosis"], "candidate-patched-read-paths-rewritten")
        self.assertEqual(audit["counts"]["direct_read_sites"], 0)
        self.assertTrue(audit["patch_markers_complete"])
        self.assertTrue(audit["read_paths_rewritten"])
        self.assertTrue(audit["patched_read_paths_complete"])
        self.assertGreaterEqual(audit["counts"]["patched_read_paths"], 9)
        self.assertEqual(code, 0)

    def test_require_patched_rejects_missing_probe_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_rewritten_read_path_fixture(root, include_baseline=False)
            _write_patch_markers(root)

            audit = audit_v8_source(root)
            with redirect_stdout(StringIO()):
                code = main(["--v8-root", str(root), "--require-patched"])

        self.assertEqual(audit["diagnosis"], "missing-source-files")
        self.assertEqual(audit["missing_files"], [])
        self.assertEqual(audit["missing_probe_inputs"], ["src/baseline"])
        self.assertTrue(audit["patch_markers_complete"])
        self.assertTrue(audit["read_paths_rewritten"])
        self.assertFalse(audit["patched_read_paths_complete"])
        self.assertEqual(code, 2)

    def test_audit_keeps_partial_patch_markers_out_of_require_patched(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            _write_stock_v8_fixture(root)
            _write(
                root,
                "include/v8-script.h",
                "Maybe<bool> SetSyntheticModuleExportWasmGlobal();",
            )

            audit = audit_v8_source(root)
            with redirect_stdout(StringIO()):
                code = main(["--v8-root", str(root), "--require-patched"])

        self.assertEqual(audit["diagnosis"], "partial-patch-markers")
        self.assertFalse(audit["patch_markers_complete"])
        self.assertEqual(code, 1)

    def test_cli_require_patched_rejects_stock_tree_and_writes_json(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            output = root / "audit.json"
            v8_root = root / "v8"
            _write_stock_v8_fixture(v8_root)

            with redirect_stdout(StringIO()):
                code = main(
                    [
                        "--v8-root",
                        str(v8_root),
                        "--json-output",
                        str(output),
                        "--require-patched",
                    ]
                )

            self.assertEqual(code, 1)
            audit = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(audit["diagnosis"], "unpatched-stock-v8")


if __name__ == "__main__":
    unittest.main()
