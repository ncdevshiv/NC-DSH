use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

const MANIFEST: &str = include_str!("../fixtures/wpt/manifest.toml");
const EXPECTED: &str = include_str!("../fixtures/wpt/expected.toml");
pub const LAYOUT_GEOMETRY_BEHAVIOR_TAG: &str = "layout-geometry-behavior";
pub const VISUAL_RENDERING_BEHAVIOR_TAG: &str = "visual-rendering-behavior";
pub const REAL_LAYOUT_BEHAVIOR_TAG: &str = "real-layout-behavior";
pub const COMPOSITOR_BEHAVIOR_TAG: &str = "compositor-behavior";
pub const MEDIA_FIDELITY_BEHAVIOR_TAG: &str = "media-fidelity-behavior";
pub const HARNESS_BLOCKED_TAG: &str = "harness-blocked";

pub const DEFAULT_EXCLUDED_WPT_TAGS: &[&str] = &[
    LAYOUT_GEOMETRY_BEHAVIOR_TAG,
    VISUAL_RENDERING_BEHAVIOR_TAG,
    REAL_LAYOUT_BEHAVIOR_TAG,
    COMPOSITOR_BEHAVIOR_TAG,
    MEDIA_FIDELITY_BEHAVIOR_TAG,
    HARNESS_BLOCKED_TAG,
];

#[derive(Debug, Deserialize)]
struct RawManifest {
    #[serde(rename = "test")]
    tests: Vec<WptManifestTest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WptManifestTest {
    pub id: String,
    pub upstream: String,
    pub upstream_commit: String,
    pub local_path: String,
    #[serde(default)]
    pub query: String,
    #[serde(rename = "type")]
    pub test_type: String,
    pub global: String,
    #[serde(default)]
    pub origin: WptManifestOrigin,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default = "default_wait_until")]
    pub wait_until: String,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    pub suite: WptManifestSuite,
    #[serde(default)]
    pub source: Option<WptManifestSource>,
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub source_commit: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub actions: Vec<WptManifestAction>,
    #[serde(default)]
    pub notes: String,
}

impl WptManifestTest {
    pub fn effective_source(&self) -> WptManifestSource {
        self.source.unwrap_or(WptManifestSource::Manual)
    }

    pub fn requires_source_metadata(&self) -> bool {
        !matches!(self.effective_source(), WptManifestSource::Manual)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WptManifestSuite {
    Smoke,
    Broad,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WptManifestSource {
    Manual,
    UpstreamWpt,
    ChromiumWpt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WptManifestOrigin {
    #[default]
    Trusted,
    Insecure,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WptManifestAction {
    Evaluate {
        expression: String,
    },
    Delay {
        ms: u64,
    },
    InsertText {
        text: String,
    },
    DispatchDrag {
        event: String,
        x: f64,
        y: f64,
        #[serde(default)]
        modifiers: u8,
        #[serde(default)]
        items: Vec<WptManifestDragItem>,
        #[serde(default)]
        files: Vec<WptManifestDragFile>,
        #[serde(default)]
        directories: Vec<WptManifestDragDirectory>,
        #[serde(default = "default_drag_operations_mask")]
        drag_operations_mask: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WptManifestDragItem {
    pub mime_type: String,
    pub data: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WptManifestDragFile {
    pub name: String,
    pub mime_type: String,
    pub text: String,
    #[serde(default)]
    pub last_modified: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WptManifestDragDirectory {
    pub name: String,
    #[serde(default)]
    pub files: Vec<WptManifestDragFile>,
    #[serde(default)]
    pub directories: Vec<WptManifestDragDirectory>,
    #[serde(default)]
    pub generated_files: Option<WptManifestGeneratedDragFiles>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WptManifestGeneratedDragFiles {
    pub count: usize,
    pub name_prefix: String,
    #[serde(default)]
    pub extension: String,
    pub mime_type: String,
    pub text_prefix: String,
    #[serde(default)]
    pub last_modified_start: f64,
}

fn default_drag_operations_mask() -> i32 {
    1
}

#[derive(Debug, Deserialize)]
struct RawExpectedFile {
    #[serde(default)]
    expectations: BTreeMap<String, WptExpectation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WptExpectation {
    pub status: ExpectedStatus,
    #[serde(default)]
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExpectedStatus {
    Pass,
    Fail,
    Skip,
}

impl ExpectedStatus {
    pub fn as_report_str(self) -> &'static str {
        match self {
            ExpectedStatus::Pass => "pass",
            ExpectedStatus::Fail => "fail",
            ExpectedStatus::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WptManifestScope {
    Smoke,
    Broad,
    Experimental,
    All,
}

impl WptManifestScope {
    pub fn from_env() -> Result<Self> {
        match std::env::var("MOLI_WPT_COMPAT") {
            Ok(value) => Self::parse(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::All),
            Err(error) => Err(anyhow!("failed to read MOLI_WPT_COMPAT: {error}")),
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "" | "all" => Ok(Self::All),
            "smoke" => Ok(Self::Smoke),
            "broad" => Ok(Self::Broad),
            "experimental" => Ok(Self::Experimental),
            other => Err(anyhow!(
                "unsupported MOLI_WPT_COMPAT value '{other}', expected 'smoke', 'broad', 'experimental', or 'all'"
            )),
        }
    }

    fn includes(self, suite: WptManifestSuite) -> bool {
        match self {
            Self::Smoke => suite == WptManifestSuite::Smoke,
            Self::Broad => matches!(suite, WptManifestSuite::Smoke | WptManifestSuite::Broad),
            Self::Experimental | Self::All => true,
        }
    }
}

pub fn parse_manifest(source: &str) -> Result<Vec<WptManifestTest>> {
    let manifest = toml::from_str::<RawManifest>(source).context("failed to parse WPT manifest")?;
    Ok(manifest.tests)
}

pub fn load_selected_manifest_from_str(source: &str) -> Result<Vec<WptManifestTest>> {
    select_manifest_tests(
        parse_manifest(source)?,
        WptManifestScope::from_env()?,
        parse_tag_filter_from_env()?,
        parse_case_filter_from_env()?,
    )
}

pub fn load_selected_manifest() -> Result<Vec<WptManifestTest>> {
    load_selected_manifest_from_str(MANIFEST)
}

pub fn load_manifest_case(case_id: &str) -> Result<Option<WptManifestTest>> {
    Ok(parse_manifest(MANIFEST)?
        .into_iter()
        .find(|test| test.id == case_id))
}

pub fn parse_expected(source: &str) -> Result<BTreeMap<String, WptExpectation>> {
    let expected =
        toml::from_str::<RawExpectedFile>(source).context("failed to parse WPT expected file")?;
    Ok(expected.expectations)
}

pub fn load_expected() -> Result<BTreeMap<String, WptExpectation>> {
    parse_expected(EXPECTED)
}

pub fn parse_tag_filter(value: &str) -> Result<BTreeSet<String>> {
    let tags = value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if !value.trim().is_empty() && tags.is_empty() {
        return Err(anyhow!("MOLI_WPT_TAG did not contain a usable tag"));
    }
    Ok(tags)
}

pub fn parse_case_filter(value: &str) -> Result<BTreeSet<String>> {
    let cases = value
        .split(',')
        .map(str::trim)
        .filter(|case| !case.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    if !value.trim().is_empty() && cases.is_empty() {
        return Err(anyhow!("MOLI_WPT_CASE did not contain a usable case id"));
    }
    Ok(cases)
}

pub fn select_manifest_tests(
    tests: Vec<WptManifestTest>,
    scope: WptManifestScope,
    tag_filter: BTreeSet<String>,
    case_filter: BTreeSet<String>,
) -> Result<Vec<WptManifestTest>> {
    let selected = tests
        .into_iter()
        .filter(|test| scope.includes(test.suite))
        .filter(|test| case_filter.is_empty() || case_filter.contains(&test.id))
        .filter(|test| {
            tag_filter.is_empty() || test.tags.iter().any(|tag| tag_filter.contains(tag))
        })
        .filter(|test| {
            // Some WPT tags describe product-scope or harness areas that are
            // useful to keep in the manifest, but should not drive ordinary
            // nextest health. They remain runnable by exact case id or by the
            // matching explicit-only tag itself.
            !has_default_excluded_tag(test)
                || case_filter.contains(&test.id)
                || has_selected_default_excluded_tag(test, &tag_filter)
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(anyhow!("WPT manifest selection produced no tests"));
    }
    Ok(selected)
}

fn has_default_excluded_tag(test: &WptManifestTest) -> bool {
    test.tags
        .iter()
        .any(|tag| DEFAULT_EXCLUDED_WPT_TAGS.contains(&tag.as_str()))
}

fn has_selected_default_excluded_tag(
    test: &WptManifestTest,
    tag_filter: &BTreeSet<String>,
) -> bool {
    test.tags
        .iter()
        .any(|tag| DEFAULT_EXCLUDED_WPT_TAGS.contains(&tag.as_str()) && tag_filter.contains(tag))
}

fn parse_case_filter_from_env() -> Result<BTreeSet<String>> {
    match std::env::var("MOLI_WPT_CASE") {
        Ok(value) => parse_case_filter(&value),
        Err(std::env::VarError::NotPresent) => Ok(BTreeSet::new()),
        Err(error) => Err(anyhow!("failed to read MOLI_WPT_CASE: {error}")),
    }
}

fn parse_tag_filter_from_env() -> Result<BTreeSet<String>> {
    match std::env::var("MOLI_WPT_TAG") {
        Ok(value) => parse_tag_filter(&value),
        Err(std::env::VarError::NotPresent) => Ok(BTreeSet::new()),
        Err(error) => Err(anyhow!("failed to read MOLI_WPT_TAG: {error}")),
    }
}

fn default_status() -> String {
    "pass".to_owned()
}

fn default_wait_until() -> String {
    "load".to_owned()
}

fn default_timeout_ms() -> u64 {
    5000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_scope_defaults_to_all_for_empty_value() -> Result<()> {
        assert_eq!(WptManifestScope::parse("")?, WptManifestScope::All);
        assert_eq!(WptManifestScope::parse("smoke")?, WptManifestScope::Smoke);
        assert_eq!(WptManifestScope::parse("broad")?, WptManifestScope::Broad);
        assert_eq!(
            WptManifestScope::parse("experimental")?,
            WptManifestScope::Experimental
        );
        assert_eq!(WptManifestScope::parse("all")?, WptManifestScope::All);
        assert!(WptManifestScope::parse("full").is_err());
        Ok(())
    }

    #[test]
    fn tag_filter_accepts_comma_separated_tags() -> Result<()> {
        assert_eq!(
            parse_tag_filter(" cssom, fetch,,cssom ")?,
            BTreeSet::from(["cssom".to_owned(), "fetch".to_owned()])
        );
        assert!(parse_tag_filter("").is_ok_and(|tags| tags.is_empty()));
        Ok(())
    }

    #[test]
    fn case_filter_accepts_comma_separated_case_ids() -> Result<()> {
        assert_eq!(
            parse_case_filter(" element-basic,fetch-basic,,element-basic ")?,
            BTreeSet::from(["element-basic".to_owned(), "fetch-basic".to_owned()])
        );
        assert!(parse_case_filter("").is_ok_and(|cases| cases.is_empty()));
        Ok(())
    }

    #[test]
    fn selection_applies_scope_before_tag_filter() -> Result<()> {
        let selected = select_manifest_tests(
            vec![
                test_case("smoke-cssom", WptManifestSuite::Smoke, &["cssom"]),
                test_case("full-cssom", WptManifestSuite::Broad, &["cssom"]),
                test_case("smoke-fetch", WptManifestSuite::Smoke, &["fetch"]),
            ],
            WptManifestScope::Smoke,
            BTreeSet::from(["cssom".to_owned()]),
            BTreeSet::new(),
        )?;
        assert_eq!(
            selected
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>(),
            ["smoke-cssom"]
        );

        let selected = select_manifest_tests(
            vec![
                test_case("smoke-cssom", WptManifestSuite::Smoke, &["cssom"]),
                test_case("full-cssom", WptManifestSuite::Broad, &["cssom"]),
                test_case("smoke-fetch", WptManifestSuite::Smoke, &["fetch"]),
            ],
            WptManifestScope::All,
            BTreeSet::from(["cssom".to_owned()]),
            BTreeSet::new(),
        )?;
        assert_eq!(
            selected
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>(),
            ["smoke-cssom", "full-cssom"]
        );
        Ok(())
    }

    #[test]
    fn selection_uses_manifest_suite() -> Result<()> {
        let tests = vec![
            test_case_with_suite("smoke", WptManifestSuite::Smoke),
            test_case_with_suite("broad", WptManifestSuite::Broad),
            test_case_with_suite("experimental", WptManifestSuite::Experimental),
        ];

        let ids_for_scope = |scope| -> Result<Vec<String>> {
            Ok(
                select_manifest_tests(tests.clone(), scope, BTreeSet::new(), BTreeSet::new())?
                    .into_iter()
                    .map(|test| test.id)
                    .collect(),
            )
        };

        assert_eq!(ids_for_scope(WptManifestScope::Smoke)?, ["smoke"]);
        assert_eq!(ids_for_scope(WptManifestScope::Broad)?, ["smoke", "broad"]);
        assert_eq!(
            ids_for_scope(WptManifestScope::Experimental)?,
            ["smoke", "broad", "experimental"]
        );
        Ok(())
    }

    #[test]
    fn selection_applies_case_filter_within_scope_and_tags() -> Result<()> {
        let selected = select_manifest_tests(
            vec![
                test_case("smoke-cssom-a", WptManifestSuite::Smoke, &["cssom"]),
                test_case("smoke-cssom-b", WptManifestSuite::Smoke, &["cssom"]),
                test_case("full-cssom-b", WptManifestSuite::Broad, &["cssom"]),
                test_case("smoke-fetch-b", WptManifestSuite::Smoke, &["fetch"]),
            ],
            WptManifestScope::Smoke,
            BTreeSet::from(["cssom".to_owned()]),
            BTreeSet::from(["smoke-cssom-b".to_owned(), "full-cssom-b".to_owned()]),
        )?;
        assert_eq!(
            selected
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>(),
            ["smoke-cssom-b"]
        );
        Ok(())
    }

    #[test]
    fn selection_excludes_default_excluded_tags_unless_selected_explicitly() -> Result<()> {
        let tests = vec![
            test_case("domrect-idl", WptManifestSuite::Smoke, &["geometry", "idl"]),
            test_case(
                "observer-flow",
                WptManifestSuite::Smoke,
                &["intersection-observer", LAYOUT_GEOMETRY_BEHAVIOR_TAG],
            ),
            test_case(
                "visual-ref",
                WptManifestSuite::Smoke,
                &["css", VISUAL_RENDERING_BEHAVIOR_TAG],
            ),
            test_case(
                "real-layout-flow",
                WptManifestSuite::Smoke,
                &["css", REAL_LAYOUT_BEHAVIOR_TAG],
            ),
            test_case(
                "compositor-flow",
                WptManifestSuite::Smoke,
                &["html", COMPOSITOR_BEHAVIOR_TAG],
            ),
            test_case(
                "media-flow",
                WptManifestSuite::Smoke,
                &["html", MEDIA_FIDELITY_BEHAVIOR_TAG],
            ),
            test_case(
                "harness-blocked-flow",
                WptManifestSuite::Smoke,
                &["html", HARNESS_BLOCKED_TAG],
            ),
        ];

        assert!(
            DEFAULT_EXCLUDED_WPT_TAGS.contains(&LAYOUT_GEOMETRY_BEHAVIOR_TAG),
            "layout/observer geometry behavior must stay explicit-only until Moli has a real layout tree"
        );

        let selected = select_manifest_tests(
            tests.clone(),
            WptManifestScope::All,
            BTreeSet::new(),
            BTreeSet::new(),
        )?;
        assert_eq!(
            selected
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>(),
            ["domrect-idl"]
        );

        let ordinary_observer_tag = select_manifest_tests(
            tests.clone(),
            WptManifestScope::All,
            BTreeSet::from(["intersection-observer".to_owned()]),
            BTreeSet::new(),
        );
        assert!(ordinary_observer_tag.is_err());

        let ordinary_css_tag = select_manifest_tests(
            tests.clone(),
            WptManifestScope::All,
            BTreeSet::from(["css".to_owned()]),
            BTreeSet::new(),
        );
        assert!(
            ordinary_css_tag.is_err(),
            "ordinary tags must not implicitly unlock visual/layout explicit-only cases"
        );

        let selected = select_manifest_tests(
            tests.clone(),
            WptManifestScope::All,
            BTreeSet::from([LAYOUT_GEOMETRY_BEHAVIOR_TAG.to_owned()]),
            BTreeSet::new(),
        )?;
        assert_eq!(
            selected
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>(),
            ["observer-flow"]
        );

        let explicit_tag_cases = [
            (VISUAL_RENDERING_BEHAVIOR_TAG, "visual-ref"),
            (REAL_LAYOUT_BEHAVIOR_TAG, "real-layout-flow"),
            (COMPOSITOR_BEHAVIOR_TAG, "compositor-flow"),
            (MEDIA_FIDELITY_BEHAVIOR_TAG, "media-flow"),
            (HARNESS_BLOCKED_TAG, "harness-blocked-flow"),
        ];
        for (tag, expected_id) in explicit_tag_cases {
            let selected = select_manifest_tests(
                tests.clone(),
                WptManifestScope::All,
                BTreeSet::from([tag.to_owned()]),
                BTreeSet::new(),
            )?;
            assert_eq!(
                selected
                    .iter()
                    .map(|test| test.id.as_str())
                    .collect::<Vec<_>>(),
                [expected_id],
                "explicit-only tag '{tag}' should remain runnable by exact tag"
            );
        }

        let selected = select_manifest_tests(
            tests,
            WptManifestScope::All,
            BTreeSet::new(),
            BTreeSet::from(["observer-flow".to_owned()]),
        )?;
        assert_eq!(
            selected
                .iter()
                .map(|test| test.id.as_str())
                .collect::<Vec<_>>(),
            ["observer-flow"]
        );
        Ok(())
    }

    #[test]
    fn manifest_and_expected_parse_from_toml_sources() -> Result<()> {
        let tests = parse_manifest(
            r#"
[[test]]
id = "headers-basic"
upstream = "web-platform-tests/fetch/api/headers/headers-basic.any.js"
upstream_commit = "local"
local_path = "ported/fetch/headers-basic.html"
type = "testharness"
global = "window"
suite = "broad"
source = "upstream-wpt"
source_path = "fetch/api/headers/headers-basic.any.js"
source_commit = "59c8d70847593453e2e611b184d6e9072a527ba0"
tags = ["fetch", "headers"]
notes = "unit test fixture"
[[test.actions]]
type = "evaluate"
expression = "document.body.dataset.ready = '1'"
[[test.actions]]
type = "delay"
ms = 25
[[test.actions]]
type = "insert-text"
text = "abc"
[[test.actions]]
type = "dispatch-drag"
event = "drop"
x = 12.5
y = 34.5
modifiers = 10
drag_operations_mask = 16
items = [{ mime_type = "text/plain", data = "drag text" }]
files = [{ name = "note.txt", mime_type = "text/plain", text = "file text", last_modified = 7 }]
directories = [{ name = "photos", files = [{ name = "one.txt", mime_type = "text/plain", text = "one", last_modified = 9 }], directories = [{ name = "nested", files = [{ name = "deep.txt", mime_type = "text/plain", text = "deep", last_modified = 10 }] }], generated_files = { count = 3, name_prefix = "generated", extension = ".txt", mime_type = "text/plain", text_prefix = "payload ", last_modified_start = 20 } }]
"#,
        )?;
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].id, "headers-basic");
        assert_eq!(tests[0].status, "pass");
        assert_eq!(tests[0].wait_until, "load");
        assert_eq!(tests[0].timeout_ms, 5000);
        assert_eq!(tests[0].suite, WptManifestSuite::Broad);
        assert_eq!(tests[0].source, Some(WptManifestSource::UpstreamWpt));
        assert_eq!(tests[0].effective_source(), WptManifestSource::UpstreamWpt);
        assert!(tests[0].requires_source_metadata());
        assert_eq!(
            tests[0].source_path.as_deref(),
            Some("fetch/api/headers/headers-basic.any.js")
        );
        assert_eq!(
            tests[0].source_commit.as_deref(),
            Some("59c8d70847593453e2e611b184d6e9072a527ba0")
        );
        assert_eq!(
            tests[0].actions,
            [
                WptManifestAction::Evaluate {
                    expression: "document.body.dataset.ready = '1'".to_owned()
                },
                WptManifestAction::Delay { ms: 25 },
                WptManifestAction::InsertText {
                    text: "abc".to_owned()
                },
                WptManifestAction::DispatchDrag {
                    event: "drop".to_owned(),
                    x: 12.5,
                    y: 34.5,
                    modifiers: 10,
                    items: vec![WptManifestDragItem {
                        mime_type: "text/plain".to_owned(),
                        data: "drag text".to_owned(),
                        title: None,
                        base_url: None,
                    }],
                    files: vec![WptManifestDragFile {
                        name: "note.txt".to_owned(),
                        mime_type: "text/plain".to_owned(),
                        text: "file text".to_owned(),
                        last_modified: 7.0,
                    }],
                    directories: vec![WptManifestDragDirectory {
                        name: "photos".to_owned(),
                        files: vec![WptManifestDragFile {
                            name: "one.txt".to_owned(),
                            mime_type: "text/plain".to_owned(),
                            text: "one".to_owned(),
                            last_modified: 9.0,
                        }],
                        directories: vec![WptManifestDragDirectory {
                            name: "nested".to_owned(),
                            files: vec![WptManifestDragFile {
                                name: "deep.txt".to_owned(),
                                mime_type: "text/plain".to_owned(),
                                text: "deep".to_owned(),
                                last_modified: 10.0,
                            }],
                            directories: Vec::new(),
                            generated_files: None,
                        }],
                        generated_files: Some(WptManifestGeneratedDragFiles {
                            count: 3,
                            name_prefix: "generated".to_owned(),
                            extension: ".txt".to_owned(),
                            mime_type: "text/plain".to_owned(),
                            text_prefix: "payload ".to_owned(),
                            last_modified_start: 20.0,
                        }),
                    }],
                    drag_operations_mask: 16,
                },
            ]
        );

        let expected = parse_expected(
            r#"
[expectations.headers-basic]
status = "fail"
reason = "tracked gap"
"#,
        )?;
        assert_eq!(
            expected.get("headers-basic").map(|entry| entry.status),
            Some(ExpectedStatus::Fail)
        );
        Ok(())
    }

    fn test_case(id: &str, suite: WptManifestSuite, tags: &[&str]) -> WptManifestTest {
        let mut test = test_case_with_suite(id, suite);
        test.tags = tags.iter().map(|tag| (*tag).to_owned()).collect();
        test
    }

    fn test_case_with_suite(id: &str, suite: WptManifestSuite) -> WptManifestTest {
        WptManifestTest {
            id: id.to_owned(),
            upstream: format!("web-platform-tests/{id}.html"),
            upstream_commit: "test".to_owned(),
            local_path: format!("ported/{id}.html"),
            query: String::new(),
            test_type: "testharness".to_owned(),
            global: "window".to_owned(),
            origin: WptManifestOrigin::Trusted,
            status: "pass".to_owned(),
            wait_until: "load".to_owned(),
            timeout_ms: 5000,
            suite,
            source: None,
            source_path: None,
            source_commit: None,
            tags: Vec::new(),
            actions: Vec::new(),
            notes: "unit test fixture".to_owned(),
        }
    }
}
