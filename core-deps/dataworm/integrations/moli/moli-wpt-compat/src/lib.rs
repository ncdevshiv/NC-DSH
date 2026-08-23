//! WPT compatibility test metadata and selection helpers for Moli.
//!
//! This crate intentionally owns WPT-specific metadata, reports, and fixtures.
//! Browser execution still lives in the core integration runner until that
//! boundary is stable enough to move as a whole.

mod importer;
mod manifest;
mod meta;
mod report;
mod runner;
mod server;

pub use importer::{
    WptImportCopiedFile, WptImportCopyConfig, WptImportCopyReport, WptImportDryRunConfig,
    WptImportDryRunReport, WptImportDryRunSummary, WptImportSupportedCase,
    WptImportUnsupportedCase, copy_wpt_import, dry_run_wpt_import,
};
pub use manifest::{
    COMPOSITOR_BEHAVIOR_TAG, DEFAULT_EXCLUDED_WPT_TAGS, ExpectedStatus, HARNESS_BLOCKED_TAG,
    LAYOUT_GEOMETRY_BEHAVIOR_TAG, MEDIA_FIDELITY_BEHAVIOR_TAG, REAL_LAYOUT_BEHAVIOR_TAG,
    VISUAL_RENDERING_BEHAVIOR_TAG, WptExpectation, WptManifestAction, WptManifestDragDirectory,
    WptManifestDragFile, WptManifestDragItem, WptManifestOrigin, WptManifestScope,
    WptManifestSource, WptManifestSuite, WptManifestTest, load_expected, load_manifest_case,
    load_selected_manifest, load_selected_manifest_from_str, parse_case_filter, parse_expected,
    parse_manifest, parse_tag_filter, select_manifest_tests,
};
pub use report::{
    WptActualStatus, WptCaseRun, WptOverallStatus, WptPageReport, WptRunCategory, WptSubtest,
    WptSuiteReport, WptSuiteSummary, WptTagSummary,
};
pub use runner::{
    WPT_REPORT_COMPLETE_EXPRESSION, WptCasePlan, WptCasePlanAction, WptCaseRequest, WptWaitUntil,
    case_request, collect_wpt_report_snapshot_expression, completed_case_plan_from_report,
    decode_page_report_value, load_case_plan, load_selected_case_plans, prepare_case_plan,
};
pub use server::WptFixtureServer;
