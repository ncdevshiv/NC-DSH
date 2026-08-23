use crate::meta::{
    extract_wpt_html_static_references, extract_wpt_js_import_scripts_references,
    extract_wpt_js_new_url_references, extract_wpt_js_shared_worker_constructor_references,
    extract_wpt_js_worker_constructor_references, extract_wpt_meta_global_values,
    extract_wpt_meta_script_references, resolve_wpt_static_resource_reference,
};
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WptImportDryRunConfig {
    pub wpt_root: PathBuf,
    pub source: String,
    pub source_commit: String,
    pub target_suite: String,
    pub extra_tags: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WptImportCopyConfig {
    pub dry_run: WptImportDryRunConfig,
    pub fixture_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WptImportDryRunReport {
    pub wpt_root: String,
    pub source: String,
    pub source_commit: String,
    pub target_suite: String,
    pub extra_tags: Vec<String>,
    pub summary: WptImportDryRunSummary,
    pub supported: Vec<WptImportSupportedCase>,
    pub unsupported: Vec<WptImportUnsupportedCase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WptImportDryRunSummary {
    pub total: usize,
    pub supported: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WptImportSupportedCase {
    pub source_path: String,
    pub local_path: String,
    pub dependencies: Vec<String>,
    pub test_type: String,
    pub global: String,
    pub suite: String,
    pub source: String,
    pub source_commit: String,
    pub extra_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WptImportUnsupportedCase {
    pub source_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WptImportCopyReport {
    pub dry_run: WptImportDryRunReport,
    pub copied: Vec<WptImportCopiedFile>,
    pub manifest_draft: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WptImportCopiedFile {
    pub source_path: String,
    pub local_path: String,
}

pub fn dry_run_wpt_import(config: &WptImportDryRunConfig) -> Result<WptImportDryRunReport> {
    if config.paths.is_empty() {
        return Err(anyhow!("at least one upstream WPT path is required"));
    }

    let mut supported = Vec::new();
    let mut unsupported = Vec::new();
    for requested_path in &config.paths {
        let relative_path = normalize_requested_wpt_path(requested_path)?;
        let absolute_path = config.wpt_root.join(&relative_path);
        if absolute_path.is_dir() {
            for discovered in discover_wpt_test_candidates(&absolute_path, &config.wpt_root)? {
                classify_path(&discovered, config, &mut supported, &mut unsupported)?;
            }
        } else {
            classify_path(&relative_path, config, &mut supported, &mut unsupported)?;
        }
    }

    supported.sort_by(|left, right| {
        left.source_path
            .cmp(&right.source_path)
            .then_with(|| global_sort_rank(&left.global).cmp(&global_sort_rank(&right.global)))
            .then_with(|| left.global.cmp(&right.global))
    });
    unsupported.sort_by(|left, right| left.source_path.cmp(&right.source_path));
    supported.dedup_by(|left, right| {
        left.source_path == right.source_path && left.global == right.global
    });
    unsupported.dedup_by(|left, right| left.source_path == right.source_path);

    Ok(WptImportDryRunReport {
        wpt_root: config.wpt_root.display().to_string(),
        source: config.source.clone(),
        source_commit: config.source_commit.clone(),
        target_suite: config.target_suite.clone(),
        extra_tags: config.extra_tags.clone(),
        summary: WptImportDryRunSummary {
            total: supported.len() + unsupported.len(),
            supported: supported.len(),
            unsupported: unsupported.len(),
        },
        supported,
        unsupported,
    })
}

pub fn copy_wpt_import(config: &WptImportCopyConfig) -> Result<WptImportCopyReport> {
    let dry_run = dry_run_wpt_import(&config.dry_run)?;
    let mut copied = Vec::new();
    let mut copied_paths = BTreeSet::new();

    for case in &dry_run.supported {
        copy_imported_wpt_fixture(
            &config.dry_run.wpt_root,
            &config.fixture_root,
            &case.source_path,
            &case.local_path,
            &mut copied_paths,
            &mut copied,
        )?;

        for dependency in &case.dependencies {
            copy_imported_wpt_fixture(
                &config.dry_run.wpt_root,
                &config.fixture_root,
                dependency,
                &format!("upstream/{dependency}"),
                &mut copied_paths,
                &mut copied,
            )?;
        }
    }

    Ok(WptImportCopyReport {
        manifest_draft: manifest_draft_for_supported_cases(&dry_run.supported),
        dry_run,
        copied,
    })
}

fn copy_imported_wpt_fixture(
    wpt_root: &Path,
    fixture_root: &Path,
    source_path: &str,
    local_path: &str,
    copied_paths: &mut BTreeSet<String>,
    copied: &mut Vec<WptImportCopiedFile>,
) -> Result<()> {
    if !copied_paths.insert(local_path.to_owned()) {
        return Ok(());
    }

    let source = wpt_root.join(source_path);
    let destination = fixture_root.join(local_path);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to copy WPT fixture {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    copied.push(WptImportCopiedFile {
        source_path: source_path.to_owned(),
        local_path: local_path.to_owned(),
    });
    Ok(())
}

fn classify_path(
    relative_path: &Path,
    config: &WptImportDryRunConfig,
    supported: &mut Vec<WptImportSupportedCase>,
    unsupported: &mut Vec<WptImportUnsupportedCase>,
) -> Result<()> {
    let source_path = path_to_wpt_string(relative_path)?;
    let absolute_path = config.wpt_root.join(relative_path);
    let classification = classify_wpt_test_path(relative_path, &absolute_path)?;
    match classification {
        WptImportPathClassification::Supported { test_type, globals } => {
            let dependencies = discover_static_dependencies(&config.wpt_root, &source_path)?;
            for global in globals {
                supported.push(WptImportSupportedCase {
                    local_path: format!("upstream/{source_path}"),
                    source_path: source_path.clone(),
                    dependencies: dependencies.clone(),
                    test_type: test_type.to_owned(),
                    global: global.to_owned(),
                    suite: config.target_suite.clone(),
                    source: config.source.clone(),
                    source_commit: config.source_commit.clone(),
                    extra_tags: config.extra_tags.clone(),
                });
            }
        }
        WptImportPathClassification::Unsupported { reason } => {
            unsupported.push(WptImportUnsupportedCase {
                source_path,
                reason: reason.to_owned(),
            });
        }
    }
    Ok(())
}

fn discover_static_dependencies(wpt_root: &Path, source_path: &str) -> Result<Vec<String>> {
    let mut dependencies = Vec::new();
    let mut seen = BTreeSet::new();
    discover_static_dependencies_inner(wpt_root, source_path, &mut seen, &mut dependencies)?;
    Ok(dependencies)
}

fn discover_static_dependencies_inner(
    wpt_root: &Path,
    source_path: &str,
    seen: &mut BTreeSet<String>,
    dependencies: &mut Vec<String>,
) -> Result<()> {
    let absolute_path = wpt_root.join(source_path);
    for dependency in discover_direct_static_dependencies(source_path, &absolute_path)? {
        if is_local_wpt_harness_resource(&dependency) || !wpt_root.join(&dependency).is_file() {
            continue;
        }
        if !seen.insert(dependency.clone()) {
            continue;
        }
        dependencies.push(dependency.clone());
        if should_discover_transitive_static_dependencies(&dependency) {
            discover_static_dependencies_inner(wpt_root, &dependency, seen, dependencies)?;
        }
    }
    Ok(())
}

fn discover_direct_static_dependencies(
    source_path: &str,
    absolute_path: &Path,
) -> Result<Vec<String>> {
    let discover_meta_scripts = source_path.ends_with(".any.js")
        || source_path.ends_with(".window.js")
        || source_path.ends_with(".worker.js")
        || source_path.ends_with(".sharedworker.js");
    let discover_html_scripts = source_path.ends_with(".html") || source_path.ends_with(".htm");
    let discover_worker_constructors =
        discover_meta_scripts || discover_html_scripts || source_path.ends_with(".js");
    if !discover_meta_scripts && !discover_html_scripts && !discover_worker_constructors {
        return Ok(Vec::new());
    }

    let source = std::fs::read_to_string(absolute_path)
        .with_context(|| format!("failed to read WPT fixture {}", absolute_path.display()))?;
    let mut dependencies = Vec::new();
    let mut references = if discover_meta_scripts {
        extract_wpt_meta_script_references(&source)
    } else if discover_html_scripts {
        extract_wpt_html_static_references(&source)
    } else {
        Vec::new()
    };
    references.extend(extract_wpt_js_worker_constructor_references(&source));
    references.extend(extract_wpt_js_shared_worker_constructor_references(&source));
    references.extend(extract_wpt_js_import_scripts_references(&source));
    references.extend(extract_wpt_js_new_url_references(&source));
    for reference in references {
        let Some(resolved) = resolve_wpt_static_resource_reference(source_path, &reference, "")?
        else {
            continue;
        };
        if is_local_wpt_harness_resource(&resolved.path) {
            continue;
        }
        if !dependencies.contains(&resolved.path) {
            dependencies.push(resolved.path);
        }
    }
    Ok(dependencies)
}

fn should_discover_transitive_static_dependencies(path: &str) -> bool {
    path.ends_with(".js") || path.ends_with(".html") || path.ends_with(".htm")
}

fn is_local_wpt_harness_resource(path: &str) -> bool {
    matches!(
        path,
        "resources/WebIDLParser.js"
            | "resources/idlharness.js"
            | "resources/moli-wpt-adapter.js"
            | "resources/testharness.js"
            | "resources/testharnessreport.js"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WptImportPathClassification {
    Supported {
        test_type: &'static str,
        globals: Vec<&'static str>,
    },
    Unsupported {
        reason: &'static str,
    },
}

fn classify_wpt_test_path(
    relative_path: &Path,
    absolute_path: &Path,
) -> Result<WptImportPathClassification> {
    let Some(file_name) = relative_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(unsupported("path has no UTF-8 file name"));
    };
    let path = relative_path.to_string_lossy();
    if !absolute_path.is_file() {
        return Ok(unsupported("path is not a file under the WPT root"));
    }
    if path_contains_component(relative_path, "resources")
        || path_contains_component(relative_path, "support")
    {
        return Ok(unsupported(
            "resource helper paths are not test entrypoints",
        ));
    }
    if file_name.contains("-manual.") || file_name.ends_with("-manual.html") {
        return Ok(unsupported(
            "manual tests are not supported by the automated compat runner",
        ));
    }
    if file_name.contains(".https.") && !is_supported_https_script_harness(file_name) {
        return Ok(unsupported(
            "HTTPS WPT tests require fixture HTTPS origin support",
        ));
    }
    if file_name.ends_with(".worker.js") {
        return Ok(supported(test_type_for_file(file_name), vec!["worker"]));
    }
    if file_name.ends_with(".serviceworker.js")
        || path.contains("service-worker")
        || path.contains("service-workers")
    {
        return Ok(unsupported(
            "service worker harness integration is not supported yet",
        ));
    }
    if file_name.ends_with(".sharedworker.js") {
        return Ok(supported(
            test_type_for_file(file_name),
            vec!["sharedworker"],
        ));
    }
    if file_name.ends_with(".any.js") {
        let globals = any_js_supported_globals(absolute_path)?;
        if !globals.is_empty() {
            return Ok(supported(test_type_for_file(file_name), globals));
        }
        return Ok(unsupported(
            ".any.js fixture does not include a supported window or worker global variant",
        ));
    }
    if file_name.ends_with(".window.js") {
        return Ok(supported(test_type_for_file(file_name), vec!["window"]));
    }
    if file_name.ends_with(".html") || file_name.ends_with(".htm") {
        return Ok(supported(test_type_for_file(file_name), vec!["window"]));
    }
    Ok(unsupported("unsupported test file shape"))
}

fn is_supported_https_script_harness(file_name: &str) -> bool {
    file_name.ends_with(".https.any.js")
        || file_name.ends_with(".https.window.js")
        || file_name.ends_with(".https.worker.js")
        || file_name.ends_with(".https.sharedworker.js")
}

fn any_js_supported_globals(absolute_path: &Path) -> Result<Vec<&'static str>> {
    let source = std::fs::read_to_string(absolute_path)
        .with_context(|| format!("failed to read WPT fixture {}", absolute_path.display()))?;
    let globals = extract_wpt_meta_global_values(&source);
    if globals.is_empty() {
        return Ok(vec!["window", "worker"]);
    }

    let mut supported = Vec::new();
    for global in globals {
        match global.as_str() {
            "window" => push_supported_global(&mut supported, "window"),
            "worker" | "dedicatedworker" => push_supported_global(&mut supported, "worker"),
            "sharedworker" => push_supported_global(&mut supported, "sharedworker"),
            _ => {}
        }
    }
    Ok(supported)
}

fn push_supported_global(globals: &mut Vec<&'static str>, global: &'static str) {
    if !globals.contains(&global) {
        globals.push(global);
    }
}

fn supported(test_type: &'static str, globals: Vec<&'static str>) -> WptImportPathClassification {
    WptImportPathClassification::Supported { test_type, globals }
}

fn unsupported(reason: &'static str) -> WptImportPathClassification {
    WptImportPathClassification::Unsupported { reason }
}

fn test_type_for_file(file_name: &str) -> &'static str {
    if file_name.starts_with("idlharness") || file_name.contains("-idlharness") {
        "idlharness"
    } else {
        "testharness"
    }
}

fn discover_wpt_test_candidates(root: &Path, wpt_root: &Path) -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    discover_wpt_test_candidates_inner(root, wpt_root, &mut candidates)?;
    candidates.sort();
    Ok(candidates)
}

fn discover_wpt_test_candidates_inner(
    directory: &Path,
    wpt_root: &Path,
    candidates: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("failed to read WPT directory {}", directory.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", directory.display()))?;
        let path = entry.path();
        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if matches!(name, "resources" | "support") {
                continue;
            }
            discover_wpt_test_candidates_inner(&path, wpt_root, candidates)?;
            continue;
        }
        let relative_path = path
            .strip_prefix(wpt_root)
            .with_context(|| format!("failed to relativize WPT path {}", path.display()))?;
        if looks_like_wpt_test_candidate(relative_path) {
            candidates.push(relative_path.to_path_buf());
        }
    }
    Ok(())
}

fn looks_like_wpt_test_candidate(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    file_name.ends_with(".html")
        || file_name.ends_with(".htm")
        || file_name.ends_with(".any.js")
        || file_name.ends_with(".window.js")
        || file_name.ends_with(".worker.js")
        || file_name.ends_with(".serviceworker.js")
        || file_name.ends_with(".sharedworker.js")
}

fn normalize_requested_wpt_path(path: &str) -> Result<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return Err(anyhow!("empty upstream WPT path"));
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(anyhow!("upstream WPT path must be relative: {path}"));
    }

    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(segment) => normalized.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!("upstream WPT path escapes root: {path}"));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(anyhow!("empty upstream WPT path"));
    }
    Ok(normalized)
}

fn path_to_wpt_string(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => {
                let segment = segment
                    .to_str()
                    .ok_or_else(|| anyhow!("WPT path contains non-UTF-8 segment"))?;
                parts.push(segment.to_owned());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(anyhow!("WPT path is not relative to the root"));
            }
        }
    }
    Ok(parts.join("/"))
}

fn path_contains_component(path: &Path, expected: &str) -> bool {
    path.components().any(|component| match component {
        Component::Normal(segment) => segment == expected,
        _ => false,
    })
}

fn manifest_draft_for_supported_cases(cases: &[WptImportSupportedCase]) -> String {
    let mut draft = String::new();
    for case in cases {
        draft.push_str("[[test]]\n");
        draft.push_str(&format!(
            "id = \"{}\"\n",
            toml_string(&manifest_id_for_case(case, cases))
        ));
        draft.push_str(&format!(
            "upstream = \"web-platform-tests/{}\"\n",
            toml_string(&case.source_path)
        ));
        draft.push_str(&format!(
            "upstream_commit = \"{}\"\n",
            toml_string(&case.source_commit)
        ));
        draft.push_str(&format!(
            "local_path = \"{}\"\n",
            toml_string(&case.local_path)
        ));
        draft.push_str(&format!("type = \"{}\"\n", toml_string(&case.test_type)));
        draft.push_str(&format!("global = \"{}\"\n", toml_string(&case.global)));
        draft.push_str("status = \"pass\"\n");
        draft.push_str("wait_until = \"load\"\n");
        draft.push_str("timeout_ms = 5000\n");
        draft.push_str(&format!("suite = \"{}\"\n", toml_string(&case.suite)));
        draft.push_str(&format!("source = \"{}\"\n", toml_string(&case.source)));
        draft.push_str(&format!(
            "source_path = \"{}\"\n",
            toml_string(&case.source_path)
        ));
        draft.push_str(&format!(
            "source_commit = \"{}\"\n",
            toml_string(&case.source_commit)
        ));
        draft.push_str(&format!(
            "tags = [{}]\n",
            manifest_tags_for_case(case)
                .iter()
                .map(|tag| format!("\"{}\"", toml_string(tag)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        draft.push_str("notes = \"Imported upstream WPT test. Keep this in the broad suite until it is proven stable under the Moli harness.\"\n\n");
    }
    draft
}

fn manifest_id_for_case(case: &WptImportSupportedCase, cases: &[WptImportSupportedCase]) -> String {
    let mut id = manifest_id_for_source_path(&case.source_path);
    if cases
        .iter()
        .filter(|other| other.source_path == case.source_path)
        .nth(1)
        .is_some()
    {
        id.push('-');
        id.push_str(&case.global);
    }
    id
}

fn manifest_id_for_source_path(source_path: &str) -> String {
    let stem = source_path
        .strip_suffix(".any.js")
        .or_else(|| source_path.strip_suffix(".worker.js"))
        .or_else(|| source_path.strip_suffix(".sharedworker.js"))
        .or_else(|| source_path.strip_suffix(".window.js"))
        .or_else(|| source_path.strip_suffix(".html"))
        .unwrap_or(source_path);
    let mut id = String::from("upstream-");
    let mut previous_dash = true;
    for character in stem.chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            id.push('-');
            previous_dash = true;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    id
}

fn global_sort_rank(global: &str) -> usize {
    match global {
        "window" => 0,
        "worker" => 1,
        "sharedworker" => 2,
        _ => 3,
    }
}

fn tags_for_source_path(source_path: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut segments = source_path.split('/').collect::<Vec<_>>();
    if segments.len() > 1 {
        segments.pop();
    }
    for segment in segments.into_iter().take(2) {
        let tag = segment
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_owned();
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

fn manifest_tags_for_case(case: &WptImportSupportedCase) -> Vec<String> {
    let mut tags = tags_for_source_path(&case.source_path);
    for tag in &case.extra_tags {
        if !tag.is_empty() && !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    tags
}

fn toml_string(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            character => vec![character],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn dry_run_reports_supported_window_shapes() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write("url/url-origin.any.js", "test(() => {}, 'url origin');")?;
        root.write(
            "dom/events/EventTarget.window.js",
            "test(() => {}, 'event target');",
        )?;
        root.write("FileAPI/idlharness.html", "<!doctype html>")?;

        let report = dry_run_wpt_import(&config(
            &root,
            &[
                "url",
                "dom/events/EventTarget.window.js",
                "FileAPI/idlharness.html",
            ],
        ))?;

        assert_eq!(report.summary.total, 4);
        assert_eq!(report.summary.supported, 4);
        assert_eq!(report.summary.unsupported, 0);
        assert_eq!(
            report
                .supported
                .iter()
                .map(|case| (
                    case.source_path.as_str(),
                    case.local_path.as_str(),
                    case.test_type.as_str(),
                    case.global.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                (
                    "FileAPI/idlharness.html",
                    "upstream/FileAPI/idlharness.html",
                    "idlharness",
                    "window"
                ),
                (
                    "dom/events/EventTarget.window.js",
                    "upstream/dom/events/EventTarget.window.js",
                    "testharness",
                    "window"
                ),
                (
                    "url/url-origin.any.js",
                    "upstream/url/url-origin.any.js",
                    "testharness",
                    "window"
                ),
                (
                    "url/url-origin.any.js",
                    "upstream/url/url-origin.any.js",
                    "testharness",
                    "worker"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn dry_run_reports_unsupported_shapes() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write("fetch/api/basic.https.html", "<!doctype html>")?;

        let report = dry_run_wpt_import(&config(&root, &["fetch/api/basic.https.html"]))?;

        assert_eq!(report.summary.supported, 0);
        assert_eq!(report.summary.unsupported, 1);
        assert!(
            report
                .unsupported
                .iter()
                .any(|case| case.source_path == "fetch/api/basic.https.html"
                    && case.reason.contains("HTTPS"))
        );
        Ok(())
    }

    #[test]
    fn dry_run_supports_https_script_harness_shapes() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "WebCryptoAPI/digest/digest.https.any.js",
            "test(() => {}, 'digest');",
        )?;
        root.write(
            "WebCryptoAPI/randomUUID.https.window.js",
            "test(() => {}, 'uuid');",
        )?;
        root.write(
            "WebCryptoAPI/worker.https.worker.js",
            "importScripts('/resources/testharness.js');\ntest(() => {}, 'worker');\ndone();",
        )?;

        let report = dry_run_wpt_import(&config(
            &root,
            &[
                "WebCryptoAPI/digest/digest.https.any.js",
                "WebCryptoAPI/randomUUID.https.window.js",
                "WebCryptoAPI/worker.https.worker.js",
            ],
        ))?;

        assert_eq!(report.summary.total, 4);
        assert_eq!(report.summary.supported, 4);
        assert_eq!(report.summary.unsupported, 0);
        assert!(report.supported.iter().any(|case| case.source_path
            == "WebCryptoAPI/digest/digest.https.any.js"
            && case.global == "window"));
        assert!(report.supported.iter().any(|case| case.source_path
            == "WebCryptoAPI/digest/digest.https.any.js"
            && case.global == "worker"));
        assert!(report.supported.iter().any(|case| case.source_path
            == "WebCryptoAPI/randomUUID.https.window.js"
            && case.global == "window"));
        assert!(report.supported.iter().any(|case| case.source_path
            == "WebCryptoAPI/worker.https.worker.js"
            && case.global == "worker"));
        Ok(())
    }

    #[test]
    fn dry_run_supports_worker_js_entrypoints() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/worker-basic.worker.js",
            "importScripts('/resources/testharness.js');\ntest(() => {}, 'worker');\ndone();",
        )?;

        let report = dry_run_wpt_import(&config(&root, &["workers/worker-basic.worker.js"]))?;

        assert_eq!(report.summary.supported, 1);
        assert_eq!(report.summary.unsupported, 0);
        let case = &report.supported[0];
        assert_eq!(case.source_path, "workers/worker-basic.worker.js");
        assert_eq!(case.global, "worker");
        assert_eq!(case.local_path, "upstream/workers/worker-basic.worker.js");
        assert_eq!(case.test_type, "testharness");
        Ok(())
    }

    #[test]
    fn dry_run_supports_html_with_shared_worker_constructor() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/semantics/run-a-worker/002.html",
            r#"<!doctype html>
<script>
new SharedWorker("002.js");
</script>
"#,
        )?;
        root.write(
            "workers/semantics/run-a-worker/002.js",
            "onconnect = () => {};",
        )?;

        let report =
            dry_run_wpt_import(&config(&root, &["workers/semantics/run-a-worker/002.html"]))?;

        assert_eq!(report.summary.supported, 1);
        assert_eq!(report.summary.unsupported, 0);
        assert_eq!(
            report.supported[0].source_path,
            "workers/semantics/run-a-worker/002.html"
        );
        assert_eq!(report.supported[0].global, "window");
        assert_eq!(
            report.supported[0].dependencies,
            ["workers/semantics/run-a-worker/002.js"]
        );
        Ok(())
    }

    #[test]
    fn dry_run_supports_shared_worker_harness_entrypoints() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/current/current.sharedworker.js",
            "// META: global=sharedworker\n// META: script=helper.js\n",
        )?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/current/helper.js",
            "globalThis.helperLoaded = true;\n",
        )?;

        let report = dry_run_wpt_import(&config(
            &root,
            &["workers/interfaces/WorkerGlobalScope/current/current.sharedworker.js"],
        ))?;

        assert_eq!(report.summary.supported, 1);
        assert_eq!(report.summary.unsupported, 0);
        assert_eq!(report.supported[0].global, "sharedworker");
        assert_eq!(
            report.supported[0].dependencies,
            ["workers/interfaces/WorkerGlobalScope/current/helper.js"]
        );
        Ok(())
    }

    #[test]
    fn dry_run_supports_worker_only_any_js_global() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/current/current.any.js",
            "// META: global=dedicatedworker\n",
        )?;
        root.write(
            "url/url-origin.any.js",
            "// META: global=window,dedicatedworker\n",
        )?;

        let report = dry_run_wpt_import(&config(
            &root,
            &["workers/current/current.any.js", "url/url-origin.any.js"],
        ))?;

        assert_eq!(report.summary.supported, 3);
        assert_eq!(report.summary.unsupported, 0);
        let worker_case = report
            .supported
            .iter()
            .find(|case| case.source_path == "workers/current/current.any.js")
            .expect("worker .any.js should be supported");
        assert_eq!(worker_case.global, "worker");
        let url_globals = report
            .supported
            .iter()
            .filter(|case| case.source_path == "url/url-origin.any.js")
            .map(|case| case.global.as_str())
            .collect::<Vec<_>>();
        assert_eq!(url_globals, ["window", "worker"]);
        Ok(())
    }

    #[test]
    fn dry_run_supports_shared_worker_any_js_globals() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/examples/onconnect.any.js",
            "// META: global=sharedworker\n",
        )?;
        root.write(
            "workers/importscripts_mime_local.any.js",
            "// META: global=dedicatedworker,sharedworker\n",
        )?;

        let report = dry_run_wpt_import(&config(
            &root,
            &[
                "workers/examples/onconnect.any.js",
                "workers/importscripts_mime_local.any.js",
            ],
        ))?;

        assert_eq!(report.summary.supported, 3);
        assert_eq!(report.summary.unsupported, 0);
        let onconnect_case = report
            .supported
            .iter()
            .find(|case| case.source_path == "workers/examples/onconnect.any.js")
            .expect("sharedworker-only .any.js should be supported");
        assert_eq!(onconnect_case.global, "sharedworker");
        let import_scripts_globals = report
            .supported
            .iter()
            .filter(|case| case.source_path == "workers/importscripts_mime_local.any.js")
            .map(|case| case.global.as_str())
            .collect::<Vec<_>>();
        assert_eq!(import_scripts_globals, ["worker", "sharedworker"]);
        Ok(())
    }

    #[test]
    fn dry_run_reports_supported_substitution_shapes() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write("url/url.sub.html", "<!doctype html>")?;
        root.write("xhr/access-control-basic-allow.sub.htm", "<!doctype html>")?;
        root.write("xhr/event-error.sub.any.js", "test(() => {}, 'sub any');")?;

        let report = dry_run_wpt_import(&config(
            &root,
            &[
                "url/url.sub.html",
                "xhr/access-control-basic-allow.sub.htm",
                "xhr/event-error.sub.any.js",
            ],
        ))?;

        assert_eq!(report.summary.total, 4);
        assert_eq!(report.summary.supported, 4);
        assert_eq!(report.summary.unsupported, 0);
        assert_eq!(
            report
                .supported
                .iter()
                .map(|case| (
                    case.source_path.as_str(),
                    case.local_path.as_str(),
                    case.global.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                ("url/url.sub.html", "upstream/url/url.sub.html", "window"),
                (
                    "xhr/access-control-basic-allow.sub.htm",
                    "upstream/xhr/access-control-basic-allow.sub.htm",
                    "window"
                ),
                (
                    "xhr/event-error.sub.any.js",
                    "upstream/xhr/event-error.sub.any.js",
                    "window"
                ),
                (
                    "xhr/event-error.sub.any.js",
                    "upstream/xhr/event-error.sub.any.js",
                    "worker"
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn directory_discovery_skips_resource_helpers() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "FileAPI/blob/Blob-constructor.any.js",
            "test(() => {}, 'blob');",
        )?;
        root.write("FileAPI/support/Blob.js", "helper();")?;
        root.write("FileAPI/resources/helper.html", "<!doctype html>")?;

        let report = dry_run_wpt_import(&config(&root, &["FileAPI"]))?;

        assert_eq!(report.summary.total, 2);
        assert_eq!(
            report.supported[0].source_path,
            "FileAPI/blob/Blob-constructor.any.js"
        );
        Ok(())
    }

    #[test]
    fn dry_run_reports_direct_meta_script_dependencies() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "FileAPI/blob/Blob-constructor.any.js",
            "// META: script=../support/Blob.js\n// META: script=https://example.test/skip.js\n",
        )?;
        root.write(
            "FileAPI/support/Blob.js",
            "self.test_blob = function () {};",
        )?;

        let report = dry_run_wpt_import(&config(&root, &["FileAPI/blob/Blob-constructor.any.js"]))?;

        assert_eq!(
            report.supported[0].dependencies,
            ["FileAPI/support/Blob.js"]
        );
        Ok(())
    }

    #[test]
    fn dry_run_reports_direct_html_static_dependencies() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "xhr/event-error-order.sub.html",
            r#"<!doctype html>
<script src="/resources/testharness.js"></script>
<script src="/resources/testharnessreport.js"></script>
<script src="resources/xmlhttprequest-event-order.js"></script>
<iframe src="resources/well-formed.xml"></iframe>
"#,
        )?;
        root.write(
            "xhr/resources/xmlhttprequest-event-order.js",
            "self.prepare_xhr_for_event_order_test = function () {};",
        )?;
        root.write("xhr/resources/well-formed.xml", "<x>foo</x>")?;

        let report = dry_run_wpt_import(&config(&root, &["xhr/event-error-order.sub.html"]))?;

        assert_eq!(
            report.supported[0].dependencies,
            [
                "xhr/resources/xmlhttprequest-event-order.js",
                "xhr/resources/well-formed.xml"
            ]
        );
        Ok(())
    }

    #[test]
    fn dry_run_reports_static_worker_constructor_dependencies() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/close/setTimeout.html",
            r#"<!doctype html>
<script src="/resources/testharness.js"></script>
<script>
async_test(t => {
  const worker = new Worker("setTimeout.js?pipe=sub");
  worker.onmessage = t.step_func_done();
});
</script>
"#,
        )?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/close/setTimeout.js",
            "setTimeout(() => postMessage('done'), 0);",
        )?;

        let report = dry_run_wpt_import(&config(
            &root,
            &["workers/interfaces/WorkerGlobalScope/close/setTimeout.html"],
        ))?;

        assert_eq!(
            report.supported[0].dependencies,
            ["workers/interfaces/WorkerGlobalScope/close/setTimeout.js"]
        );
        Ok(())
    }

    #[test]
    fn dry_run_reports_transitive_worker_constructor_dependencies_from_js_helpers() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/onerror/message-classic-Error.html",
            r#"<!doctype html>
<script src="message-helper.js"></script>
"#,
        )?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/onerror/message-helper.js",
            r#"promise_test(async t => {
  const worker = new Worker("throw.js?where=toplevel");
}, "message");
"#,
        )?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/onerror/throw.js",
            "throw new Error('boom');",
        )?;

        let report = dry_run_wpt_import(&config(
            &root,
            &["workers/interfaces/WorkerGlobalScope/onerror/message-classic-Error.html"],
        ))?;

        assert_eq!(
            report.supported[0].dependencies,
            [
                "workers/interfaces/WorkerGlobalScope/onerror/message-helper.js",
                "workers/interfaces/WorkerGlobalScope/onerror/throw.js"
            ]
        );
        Ok(())
    }

    #[test]
    fn dry_run_reports_transitive_import_scripts_dependencies_from_worker_scripts() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/Worker-nested-importScripts-error.html",
            r#"<!doctype html>
<script>
new Worker("support/importScripts-1.js");
</script>
"#,
        )?;
        root.write(
            "workers/support/importScripts-1.js",
            r#"importScripts("importScripts-2.js");"#,
        )?;
        root.write(
            "workers/support/importScripts-2.js",
            r#"importScripts("importScripts-3.js");"#,
        )?;
        root.write(
            "workers/support/importScripts-3.js",
            r#"importScripts("invalidScript.js");"#,
        )?;
        root.write("workers/support/invalidScript.js", "abc def;")?;

        let report = dry_run_wpt_import(&config(
            &root,
            &["workers/Worker-nested-importScripts-error.html"],
        ))?;

        assert_eq!(
            report.supported[0].dependencies,
            [
                "workers/support/importScripts-1.js",
                "workers/support/importScripts-2.js",
                "workers/support/importScripts-3.js",
                "workers/support/invalidScript.js",
            ]
        );
        Ok(())
    }

    #[test]
    fn dry_run_skips_missing_static_worker_constructor_dependencies() -> Result<()> {
        let root = TestWptRoot::new()?;
        root.write(
            "workers/semantics/run-a-worker/003.html",
            r#"<!doctype html>
<script>
new Worker("404_worker");
</script>
"#,
        )?;

        let report =
            dry_run_wpt_import(&config(&root, &["workers/semantics/run-a-worker/003.html"]))?;

        assert_eq!(report.summary.supported, 1);
        assert!(report.supported[0].dependencies.is_empty());
        Ok(())
    }

    #[test]
    fn requested_paths_must_stay_relative() -> Result<()> {
        let root = TestWptRoot::new()?;
        let error = dry_run_wpt_import(&config(&root, &["../outside.html"]))
            .expect_err("escaping path should fail");
        assert!(error.to_string().contains("escapes root"));
        Ok(())
    }

    #[test]
    fn copy_import_copies_supported_entrypoints_and_generates_manifest_draft() -> Result<()> {
        let root = TestWptRoot::new()?;
        let fixture_root = TestWptRoot::new()?;
        root.write("url/url-origin.any.js", "test(() => {}, 'url origin');")?;

        let report = copy_wpt_import(&WptImportCopyConfig {
            dry_run: config(&root, &["url/url-origin.any.js"]),
            fixture_root: fixture_root.path.clone(),
        })?;

        assert_eq!(report.dry_run.summary.supported, 2);
        assert_eq!(
            fs::read_to_string(fixture_root.path.join("upstream/url/url-origin.any.js"))?,
            "test(() => {}, 'url origin');"
        );
        assert_eq!(
            report.copied,
            [WptImportCopiedFile {
                source_path: "url/url-origin.any.js".to_owned(),
                local_path: "upstream/url/url-origin.any.js".to_owned(),
            }]
        );
        assert!(report.manifest_draft.contains("[[test]]"));
        assert!(
            report
                .manifest_draft
                .contains("id = \"upstream-url-url-origin-window\"")
        );
        assert!(
            report
                .manifest_draft
                .contains("id = \"upstream-url-url-origin-worker\"")
        );
        assert!(
            report
                .manifest_draft
                .contains("local_path = \"upstream/url/url-origin.any.js\"")
        );
        assert!(report.manifest_draft.contains("suite = \"broad\""));
        assert!(report.manifest_draft.contains("source = \"upstream-wpt\""));
        assert!(report.manifest_draft.contains("tags = [\"url\"]"));
        Ok(())
    }

    #[test]
    fn copy_import_manifest_draft_includes_extra_tags() -> Result<()> {
        let root = TestWptRoot::new()?;
        let fixture_root = TestWptRoot::new()?;
        root.write(
            "intersection-observer/basic.html",
            "<!doctype html><script>test(() => {}, 'observer');</script>",
        )?;
        let mut dry_run = config(&root, &["intersection-observer/basic.html"]);
        dry_run.extra_tags = vec![
            "real-layout-behavior".to_owned(),
            "harness-blocked".to_owned(),
            "explicit-probe".to_owned(),
        ];

        let report = copy_wpt_import(&WptImportCopyConfig {
            dry_run,
            fixture_root: fixture_root.path.clone(),
        })?;

        assert_eq!(
            report.dry_run.extra_tags,
            ["real-layout-behavior", "harness-blocked", "explicit-probe"]
        );
        assert_eq!(
            report.dry_run.supported[0].extra_tags,
            ["real-layout-behavior", "harness-blocked", "explicit-probe"]
        );
        assert!(
            report.manifest_draft.contains(
                "tags = [\"intersection-observer\", \"real-layout-behavior\", \"harness-blocked\", \"explicit-probe\"]"
            ),
            "manifest draft should preserve automatic source tags and append operator-supplied ROI tags"
        );
        Ok(())
    }

    #[test]
    fn copy_import_generates_unique_manifest_ids_for_any_js_variants() -> Result<()> {
        let root = TestWptRoot::new()?;
        let fixture_root = TestWptRoot::new()?;
        root.write(
            "workers/importscripts_mime_local.any.js",
            "// META: global=dedicatedworker,sharedworker\n",
        )?;

        let report = copy_wpt_import(&WptImportCopyConfig {
            dry_run: config(&root, &["workers/importscripts_mime_local.any.js"]),
            fixture_root: fixture_root.path.clone(),
        })?;

        assert_eq!(report.dry_run.summary.supported, 2);
        assert_eq!(report.copied.len(), 1);
        assert!(
            report
                .manifest_draft
                .contains("id = \"upstream-workers-importscripts-mime-local-worker\"")
        );
        assert!(
            report
                .manifest_draft
                .contains("id = \"upstream-workers-importscripts-mime-local-sharedworker\"")
        );
        assert!(report.manifest_draft.contains("global = \"worker\""));
        assert!(report.manifest_draft.contains("global = \"sharedworker\""));
        Ok(())
    }

    #[test]
    fn copy_import_copies_direct_meta_script_dependencies_once() -> Result<()> {
        let root = TestWptRoot::new()?;
        let fixture_root = TestWptRoot::new()?;
        root.write(
            "FileAPI/blob/Blob-constructor.any.js",
            "// META: script=../support/Blob.js\ntest(() => {}, 'constructor');",
        )?;
        root.write(
            "FileAPI/blob/Blob-slice.any.js",
            "// META: script=../support/Blob.js\ntest(() => {}, 'slice');",
        )?;
        root.write(
            "FileAPI/support/Blob.js",
            "self.test_blob = function () {};",
        )?;

        let report = copy_wpt_import(&WptImportCopyConfig {
            dry_run: config(
                &root,
                &[
                    "FileAPI/blob/Blob-constructor.any.js",
                    "FileAPI/blob/Blob-slice.any.js",
                ],
            ),
            fixture_root: fixture_root.path.clone(),
        })?;

        assert_eq!(
            fs::read_to_string(fixture_root.path.join("upstream/FileAPI/support/Blob.js"))?,
            "self.test_blob = function () {};"
        );
        assert_eq!(
            report
                .copied
                .iter()
                .filter(|file| file.source_path == "FileAPI/support/Blob.js")
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn copy_import_copies_direct_html_script_dependencies_once() -> Result<()> {
        let root = TestWptRoot::new()?;
        let fixture_root = TestWptRoot::new()?;
        root.write(
            "xhr/event-error-order.sub.html",
            r#"<!doctype html>
<script src="/resources/testharness.js"></script>
<script src="resources/xmlhttprequest-event-order.js"></script>
"#,
        )?;
        root.write(
            "xhr/send-network-error-async-events.sub.htm",
            r#"<!doctype html>
<script src="resources/xmlhttprequest-event-order.js"></script>
"#,
        )?;
        root.write(
            "xhr/resources/xmlhttprequest-event-order.js",
            "self.prepare_xhr_for_event_order_test = function () {};",
        )?;

        let report = copy_wpt_import(&WptImportCopyConfig {
            dry_run: config(
                &root,
                &[
                    "xhr/event-error-order.sub.html",
                    "xhr/send-network-error-async-events.sub.htm",
                ],
            ),
            fixture_root: fixture_root.path.clone(),
        })?;

        assert_eq!(
            fs::read_to_string(
                fixture_root
                    .path
                    .join("upstream/xhr/resources/xmlhttprequest-event-order.js")
            )?,
            "self.prepare_xhr_for_event_order_test = function () {};"
        );
        assert_eq!(
            report
                .copied
                .iter()
                .filter(|file| file.source_path == "xhr/resources/xmlhttprequest-event-order.js")
                .count(),
            1
        );
        assert!(
            !fixture_root
                .path
                .join("upstream/resources/testharness.js")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn copy_import_copies_static_worker_constructor_dependencies_once() -> Result<()> {
        let root = TestWptRoot::new()?;
        let fixture_root = TestWptRoot::new()?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/close/setTimeout.html",
            r#"<!doctype html>
<script>new Worker("setTimeout.js");</script>
"#,
        )?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/close/setInterval.html",
            r#"<!doctype html>
<script>new Worker("setTimeout.js");</script>
"#,
        )?;
        root.write(
            "workers/interfaces/WorkerGlobalScope/close/setTimeout.js",
            "setTimeout(() => postMessage('done'), 0);",
        )?;

        let report = copy_wpt_import(&WptImportCopyConfig {
            dry_run: config(
                &root,
                &[
                    "workers/interfaces/WorkerGlobalScope/close/setTimeout.html",
                    "workers/interfaces/WorkerGlobalScope/close/setInterval.html",
                ],
            ),
            fixture_root: fixture_root.path.clone(),
        })?;

        assert_eq!(
            fs::read_to_string(
                fixture_root
                    .path
                    .join("upstream/workers/interfaces/WorkerGlobalScope/close/setTimeout.js")
            )?,
            "setTimeout(() => postMessage('done'), 0);"
        );
        assert_eq!(
            report
                .copied
                .iter()
                .filter(|file| file.source_path
                    == "workers/interfaces/WorkerGlobalScope/close/setTimeout.js")
                .count(),
            1
        );
        Ok(())
    }

    fn config(root: &TestWptRoot, paths: &[&str]) -> WptImportDryRunConfig {
        WptImportDryRunConfig {
            wpt_root: root.path.clone(),
            source: "upstream-wpt".to_owned(),
            source_commit: "test-commit".to_owned(),
            target_suite: "broad".to_owned(),
            extra_tags: Vec::new(),
            paths: paths.iter().map(|path| (*path).to_owned()).collect(),
        }
    }

    struct TestWptRoot {
        path: PathBuf,
    }

    impl TestWptRoot {
        fn new() -> Result<Self> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "moli-wpt-importer-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path)
                .with_context(|| format!("failed to create test WPT root {}", path.display()))?;
            Ok(Self { path })
        }

        fn write(&self, relative_path: &str, contents: &str) -> Result<()> {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            fs::write(&path, contents)
                .with_context(|| format!("failed to write {}", path.display()))
        }
    }

    impl Drop for TestWptRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
