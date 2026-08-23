use crate::ExpectedStatus;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static REPORT_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Deserialize)]
pub struct WptPageReport {
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub status: WptOverallStatus,
    #[serde(default)]
    pub tests: Vec<WptSubtest>,
}

impl WptPageReport {
    pub fn passed(&self) -> bool {
        self.complete
            && self.status.status == "OK"
            && !self.tests.is_empty()
            && self.tests.iter().all(|test| test.status == "PASS")
    }

    pub fn failure_messages(&self) -> Vec<String> {
        let mut messages = Vec::new();
        if !self.complete {
            messages.push("WPT report did not complete".to_owned());
        }
        if self.tests.is_empty() {
            messages.push("WPT report contains no subtests".to_owned());
        }
        if self.status.status != "OK" {
            messages.push(format!(
                "WPT status was {}{}",
                self.status.status,
                if self.status.message.is_empty() {
                    "".to_owned()
                } else {
                    format!(": {}", self.status.message)
                }
            ));
        }
        for test in &self.tests {
            if test.status != "PASS" {
                messages.push(format!(
                    "{}: {}{}",
                    test.name,
                    test.status,
                    if test.message.is_empty() {
                        "".to_owned()
                    } else {
                        format!(": {}", test.message)
                    }
                ));
            }
        }
        messages
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WptOverallStatus {
    #[serde(default = "default_wpt_pending")]
    pub status: String,
    #[serde(default)]
    pub message: String,
}

impl Default for WptOverallStatus {
    fn default() -> Self {
        Self {
            status: default_wpt_pending(),
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct WptSubtest {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_wpt_notrun")]
    pub status: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct WptSuiteSummary {
    pub total: usize,
    pub pass: usize,
    pub known_fail: usize,
    pub unexpected_fail: usize,
    pub unexpected_pass: usize,
    pub skip: usize,
    pub by_tag: BTreeMap<String, WptTagSummary>,
}

impl WptSuiteSummary {
    pub fn record_category_for_tags(&mut self, category: WptRunCategory, tags: &[String]) {
        self.record_category(category);
        for tag in tags {
            self.by_tag
                .entry(tag.clone())
                .or_default()
                .record_category(category);
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut lines = vec![
            "WPT compat subset:".to_owned(),
            format!("  total: {}", self.total),
            format!("  pass: {}", self.pass),
            format!("  known-fail: {}", self.known_fail),
            format!("  unexpected-fail: {}", self.unexpected_fail),
            format!("  unexpected-pass: {}", self.unexpected_pass),
            format!("  skip: {}", self.skip),
        ];
        if !self.by_tag.is_empty() {
            lines.push(String::new());
            lines.push("By tag:".to_owned());
            for (tag, summary) in &self.by_tag {
                lines.push(format!(
                    "  {tag}: total {}, pass {}, known-fail {}, unexpected-fail {}, unexpected-pass {}, skip {}",
                    summary.total,
                    summary.pass,
                    summary.known_fail,
                    summary.unexpected_fail,
                    summary.unexpected_pass,
                    summary.skip
                ));
            }
        }
        lines.join("\n")
    }

    fn record_category(&mut self, category: WptRunCategory) {
        self.total += 1;
        match category {
            WptRunCategory::Pass => self.pass += 1,
            WptRunCategory::KnownFail => self.known_fail += 1,
            WptRunCategory::UnexpectedFail => self.unexpected_fail += 1,
            WptRunCategory::UnexpectedPass => self.unexpected_pass += 1,
            WptRunCategory::Skip => self.skip += 1,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct WptTagSummary {
    pub total: usize,
    pub pass: usize,
    pub known_fail: usize,
    pub unexpected_fail: usize,
    pub unexpected_pass: usize,
    pub skip: usize,
}

impl WptTagSummary {
    fn record_category(&mut self, category: WptRunCategory) {
        self.total += 1;
        match category {
            WptRunCategory::Pass => self.pass += 1,
            WptRunCategory::KnownFail => self.known_fail += 1,
            WptRunCategory::UnexpectedFail => self.unexpected_fail += 1,
            WptRunCategory::UnexpectedPass => self.unexpected_pass += 1,
            WptRunCategory::Skip => self.skip += 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WptRunCategory {
    Pass,
    KnownFail,
    UnexpectedFail,
    UnexpectedPass,
    Skip,
}

impl WptRunCategory {
    pub fn as_report_str(self) -> &'static str {
        match self {
            WptRunCategory::Pass => "pass",
            WptRunCategory::KnownFail => "known-fail",
            WptRunCategory::UnexpectedFail => "unexpected-fail",
            WptRunCategory::UnexpectedPass => "unexpected-pass",
            WptRunCategory::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WptActualStatus {
    Pass,
    Fail,
    Skipped,
}

impl WptActualStatus {
    pub fn as_report_str(self) -> &'static str {
        match self {
            WptActualStatus::Pass => "pass",
            WptActualStatus::Fail => "fail",
            WptActualStatus::Skipped => "skip",
        }
    }
}

#[derive(Debug, Clone)]
pub struct WptCaseRun {
    pub id: String,
    pub upstream: String,
    pub upstream_commit: String,
    pub local_path: String,
    pub tags: Vec<String>,
    pub expected: ExpectedStatus,
    pub expected_reason: String,
    pub actual: WptActualStatus,
    pub failures: Vec<String>,
}

impl WptCaseRun {
    pub fn category(&self) -> WptRunCategory {
        match (self.expected, self.actual) {
            (ExpectedStatus::Pass, WptActualStatus::Pass) => WptRunCategory::Pass,
            (ExpectedStatus::Pass, WptActualStatus::Fail) => WptRunCategory::UnexpectedFail,
            (ExpectedStatus::Fail, WptActualStatus::Fail) => WptRunCategory::KnownFail,
            (ExpectedStatus::Fail, WptActualStatus::Pass) => WptRunCategory::UnexpectedPass,
            (ExpectedStatus::Skip, _) | (_, WptActualStatus::Skipped) => WptRunCategory::Skip,
        }
    }

    fn unexpected_failure_message(&self) -> Option<String> {
        match self.category() {
            WptRunCategory::UnexpectedFail => Some(format!(
                "WPT case '{}' unexpectedly failed\nupstream: {}\ncommit: {}\nlocal: {}\ntags: {}\n{}",
                self.id,
                self.upstream,
                self.upstream_commit,
                self.local_path,
                self.tags.join(","),
                self.failures.join("\n")
            )),
            WptRunCategory::UnexpectedPass => Some(format!(
                "WPT case '{}' unexpectedly passed\nupstream: {}\ncommit: {}\nlocal: {}\ntags: {}\nexpected-fail reason: {}",
                self.id,
                self.upstream,
                self.upstream_commit,
                self.local_path,
                self.tags.join(","),
                self.expected_reason
            )),
            WptRunCategory::Pass | WptRunCategory::KnownFail | WptRunCategory::Skip => None,
        }
    }

    fn to_json_case(&self) -> WptJsonCase {
        WptJsonCase {
            id: self.id.clone(),
            upstream: self.upstream.clone(),
            upstream_commit: self.upstream_commit.clone(),
            local_path: self.local_path.clone(),
            tags: self.tags.clone(),
            expected: self.expected.as_report_str(),
            actual: self.actual.as_report_str(),
            category: self.category().as_report_str(),
            expected_reason: self.expected_reason.clone(),
            failures: self.failures.clone(),
        }
    }
}

#[derive(Debug)]
pub struct WptSuiteReport {
    runs: Vec<WptCaseRun>,
}

impl WptSuiteReport {
    pub fn new(runs: Vec<WptCaseRun>) -> Self {
        Self { runs }
    }

    pub fn summary(&self) -> WptSuiteSummary {
        let mut summary = WptSuiteSummary::default();
        for run in &self.runs {
            summary.record_category_for_tags(run.category(), &run.tags);
        }
        summary
    }

    pub fn markdown_summary(&self) -> String {
        self.summary().to_markdown()
    }

    pub fn write_summary_json_with_label(&self, label: &str) -> Result<PathBuf> {
        let path = wpt_compat_json_report_path_with_label(label);
        let payload = serde_json::to_string_pretty(&self.to_json_report())
            .context("failed to encode WPT summary report")?;
        write_report_file(&path, payload)?;
        Ok(path)
    }

    pub fn write_markdown_report_with_label(&self, label: &str) -> Result<PathBuf> {
        let path = wpt_compat_markdown_report_path_with_label(label);
        write_report_file(&path, self.to_markdown_report())?;
        Ok(path)
    }

    pub fn assert_no_unexpected_failures(&self) -> Result<()> {
        let failures = self
            .runs
            .iter()
            .filter_map(WptCaseRun::unexpected_failure_message)
            .collect::<Vec<_>>();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "{}\n\n{}",
                self.markdown_summary(),
                failures.join("\n\n")
            ))
        }
    }

    fn to_json_report(&self) -> WptJsonReport {
        WptJsonReport {
            summary: self.summary(),
            cases: self.runs.iter().map(WptCaseRun::to_json_case).collect(),
        }
    }

    fn to_markdown_report(&self) -> String {
        let summary = self.summary();
        let mut lines = vec![
            "# WPT Compat Report".to_owned(),
            String::new(),
            "## Summary".to_owned(),
            String::new(),
            "| total | pass | known-fail | unexpected-fail | unexpected-pass | skip |".to_owned(),
            "|---:|---:|---:|---:|---:|---:|".to_owned(),
            format!(
                "| {} | {} | {} | {} | {} | {} |",
                summary.total,
                summary.pass,
                summary.known_fail,
                summary.unexpected_fail,
                summary.unexpected_pass,
                summary.skip
            ),
        ];

        if !summary.by_tag.is_empty() {
            lines.extend([
                String::new(),
                "## By Tag".to_owned(),
                String::new(),
                "| tag | total | pass | known-fail | unexpected-fail | unexpected-pass | skip |"
                    .to_owned(),
                "|---|---:|---:|---:|---:|---:|---:|".to_owned(),
            ]);
            for (tag, tag_summary) in &summary.by_tag {
                lines.push(format!(
                    "| {} | {} | {} | {} | {} | {} | {} |",
                    markdown_table_cell(tag),
                    tag_summary.total,
                    tag_summary.pass,
                    tag_summary.known_fail,
                    tag_summary.unexpected_fail,
                    tag_summary.unexpected_pass,
                    tag_summary.skip
                ));
            }
        }

        lines.extend([String::new(), "## Non-Pass Cases".to_owned(), String::new()]);
        let non_pass_cases = self
            .runs
            .iter()
            .filter(|run| run.category() != WptRunCategory::Pass)
            .collect::<Vec<_>>();
        if non_pass_cases.is_empty() {
            lines.push("None.".to_owned());
        } else {
            for run in non_pass_cases {
                lines.push(format!(
                    "- `{}`: expected `{}`, actual `{}`, category `{}`",
                    run.id,
                    run.expected.as_report_str(),
                    run.actual.as_report_str(),
                    run.category().as_report_str()
                ));
                if !run.expected_reason.trim().is_empty() {
                    lines.push(format!("  - reason: {}", run.expected_reason));
                }
                for failure in &run.failures {
                    lines.push(format!("  - {}", failure.replace('\n', " ")));
                }
            }
        }

        lines.extend([
            String::new(),
            "## Case Inventory".to_owned(),
            String::new(),
            "| id | category | expected | actual | tags | upstream | local |".to_owned(),
            "|---|---|---|---|---|---|---|".to_owned(),
        ]);
        for run in &self.runs {
            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} |",
                markdown_table_cell(&run.id),
                run.category().as_report_str(),
                run.expected.as_report_str(),
                run.actual.as_report_str(),
                markdown_table_cell(&run.tags.join(", ")),
                markdown_table_cell(&format!("{} @ {}", run.upstream, run.upstream_commit)),
                markdown_table_cell(&run.local_path)
            ));
        }

        lines.push(String::new());
        lines.join("\n")
    }
}

#[derive(Debug, Serialize)]
struct WptJsonReport {
    summary: WptSuiteSummary,
    cases: Vec<WptJsonCase>,
}

#[derive(Debug, Serialize)]
struct WptJsonCase {
    id: String,
    upstream: String,
    upstream_commit: String,
    local_path: String,
    tags: Vec<String>,
    expected: &'static str,
    actual: &'static str,
    category: &'static str,
    expected_reason: String,
    failures: Vec<String>,
}

fn wpt_compat_json_report_path_with_label(label: &str) -> PathBuf {
    wpt_compat_report_path(&format!("moli-wpt-compat-report-{label}.json"))
}

fn wpt_compat_markdown_report_path_with_label(label: &str) -> PathBuf {
    wpt_compat_report_path(&format!("moli-wpt-compat-report-{label}.md"))
}

fn wpt_compat_report_path(filename: &str) -> PathBuf {
    if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir).join(filename);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(
            || PathBuf::from("target"),
            |workspace| workspace.join("target"),
        )
        .join(filename)
}

fn write_report_file(path: &Path, payload: String) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("WPT report path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("failed to create WPT report dir {}", parent.display()))?;

    let temp_path = unique_report_temp_path(path);
    let result = (|| {
        let mut temp = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .with_context(|| {
                format!(
                    "failed to create temporary WPT report {}",
                    temp_path.display()
                )
            })?;
        temp.write_all(payload.as_bytes()).with_context(|| {
            format!(
                "failed to write temporary WPT report {}",
                temp_path.display()
            )
        })?;
        drop(temp);
        std::fs::rename(&temp_path, path).with_context(|| {
            format!(
                "failed to replace WPT report {} from {}",
                path.display(),
                temp_path.display()
            )
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn unique_report_temp_path(path: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = REPORT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("wpt-report"));
    let mut temp_name = OsString::from(".");
    temp_name.push(file_name);
    temp_name.push(format!(".{}.{}.{}.tmp", std::process::id(), nonce, counter));
    path.with_file_name(temp_name)
}

fn markdown_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', "<br>")
}

fn default_wpt_pending() -> String {
    "PENDING".to_owned()
}

fn default_wpt_notrun() -> String {
    "NOTRUN".to_owned()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[test]
    fn page_report_records_failure_messages() {
        let report = WptPageReport {
            complete: false,
            status: WptOverallStatus {
                status: "ERROR".to_owned(),
                message: "boom".to_owned(),
            },
            tests: vec![WptSubtest {
                name: "subtest".to_owned(),
                status: "FAIL".to_owned(),
                message: "bad value".to_owned(),
            }],
        };
        assert!(!report.passed());
        assert_eq!(
            report.failure_messages(),
            [
                "WPT report did not complete",
                "WPT status was ERROR: boom",
                "subtest: FAIL: bad value"
            ]
        );
    }

    #[test]
    fn suite_summary_records_categories_by_tag() {
        let mut summary = WptSuiteSummary::default();
        summary.record_category_for_tags(
            WptRunCategory::UnexpectedFail,
            &["fetch".to_owned(), "headers".to_owned()],
        );
        assert_eq!(summary.total, 1);
        assert_eq!(summary.unexpected_fail, 1);
        assert_eq!(
            summary.by_tag.get("fetch").map(|tag| tag.unexpected_fail),
            Some(1)
        );
        assert!(summary.to_markdown().contains("unexpected-fail: 1"));
    }

    #[test]
    fn concurrent_report_writes_never_expose_partial_payloads() -> Result<()> {
        const WRITERS: usize = 4;
        const WRITES_PER_THREAD: usize = 16;

        let root = std::env::temp_dir().join(format!(
            "moli-wpt-report-atomic-{}-{}",
            std::process::id(),
            REPORT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("report.json");
        write_report_file(
            &path,
            serde_json::json!({ "writer": "initial", "padding": "x".repeat(256 * 1024) })
                .to_string(),
        )?;

        let barrier = Arc::new(Barrier::new(WRITERS + 1));
        let finished = Arc::new(AtomicUsize::new(0));
        let handles = (0..WRITERS)
            .map(|writer| {
                let path = path.clone();
                let barrier = Arc::clone(&barrier);
                let finished = Arc::clone(&finished);
                std::thread::spawn(move || -> Result<()> {
                    let payload = serde_json::json!({
                        "writer": writer,
                        "padding": "x".repeat(256 * 1024),
                    })
                    .to_string();
                    barrier.wait();
                    for _ in 0..WRITES_PER_THREAD {
                        write_report_file(&path, payload.clone())?;
                    }
                    finished.fetch_add(1, Ordering::Release);
                    Ok(())
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();
        while finished.load(Ordering::Acquire) != WRITERS {
            let payload = std::fs::read_to_string(&path)?;
            let report: serde_json::Value = serde_json::from_str(&payload)?;
            assert_eq!(report["padding"].as_str().map(str::len), Some(256 * 1024));
        }
        for handle in handles {
            handle.join().expect("report writer should not panic")?;
        }
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
