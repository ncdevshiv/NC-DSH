//! Doc-truth regression linter.
//!
//! Walks every `.rs` file under `<workspace>/crates/**` at test time and
//! fails when *non-test* source text contains known doc-truth lies — claims
//! about behavior that the code does not have. This guards against the
//! failure mode where documentation drifts into fiction (e.g. advertising a
//! "Real Browser" integration while `execute()` fabricates success strings).
//! Conditionally true claims are handled by proof markers: e.g. "real
//! browser" passes only in files that themselves wire up the real
//! OmniChrome/CDP bridge (see [`REAL_BROWSER_PROOF_MARKERS`]).
//!
//! What counts as scanned source: all crate sources, including doc comments,
//! regular comments, and string literals. Excluded: integration/bench trees
//! (`tests/`, `benches/`, `examples/`) and anything gated behind
//! `#[cfg(test)]` (unit-test modules are stripped before matching).
//!
//! Temporary exceptions live in [`ALLOWLIST`] below. Each entry maps a path
//! fragment to a pattern that is tolerated *in that file only*, together
//! with the owning area and the reason it cannot be fixed here (the lint's
//! author lacks write ownership of those crates). The goal is for this list
//! to shrink back to EMPTY as owning arcs clear their violations.
//!
//! Run: `cargo test -p ai-devtools --test doc_truth_lint`

use std::fs;
use std::path::{Path, PathBuf};

/// A banned-pattern rule. Matching is a case-insensitive substring search
/// over non-test source text.
struct Rule {
    /// Stable name used in reports.
    name: &'static str,
    /// Banned substring (ASCII; compared against an ASCII-lowercased copy).
    pattern: &'static str,
    /// Why this claim is treated as a lie unless proven otherwise.
    rationale: &'static str,
}

const BANNED_RULES: &[Rule] = &[
    Rule {
        name: "real-browser",
        pattern: "real browser",
        rationale: "simulated browser tooling (acknowledgement strings) must not \
                    claim a real integration; the claim passes only in files that \
                    prove one (see REAL_BROWSER_PROOF_MARKERS)",
    },
    Rule {
        name: "rrf-style-fusion",
        pattern: "rrf-style",
        rationale: "claimed unless the same file contains an actual reciprocal-rank \
                    fusion implementation (see RRF_IMPLEMENTATION_MARKERS)",
    },
    Rule {
        name: "per-task-deadlines",
        pattern: "per-task deadline",
        rationale: "the runtime enforces batch-level deadlines only; per-task \
                    deadlines do not exist (matches singular and plural)",
    },
    Rule {
        name: "analytics-ingest-incremental-lie",
        pattern: "since the last call",
        rationale: "`MetricsRegistry::ingest` re-reads ALL collector events on \
                    every call (no cursor/watermark); docs claiming incremental \
                    ingestion hide double-counting",
    },
    Rule {
        name: "production-ready-claim",
        pattern: "production-ready",
        rationale: "no doc may declare itself production-ready; readiness is an \
                    organizational judgment, not a doc-comment assertion",
    },
    Rule {
        name: "production-ready-claim-spaced",
        pattern: "production ready",
        rationale: "same guard as production-ready, unhyphenated variant",
    },
];

/// Textual markers proving an actual Reciprocal Rank Fusion implementation
/// exists in the same file. A file mentioning "RRF-style" passes ONLY if one
/// of these markers is present:
/// - `fn reciprocal_rank_fusion` — a function actually implementing RRF; or
/// - `/ (rrf_k` / `/(rrf_k` — the characteristic `score = 1/(k + rank)`
///   computation.
const RRF_IMPLEMENTATION_MARKERS: &[&str] = &["fn reciprocal_rank_fusion", "/ (rrf_k", "/(rrf_k"];

/// Textual markers proving a "real browser" claim is TRUE in the same file.
/// A file mentioning "real browser" passes ONLY if one of these markers is
/// present, i.e. the file itself wires up the actual integration:
/// - `omnichrome` — the OmniChrome Chrome-extension CDP bridge client; or
/// - `cdp` — Chrome DevTools Protocol plumbing.
///
/// This exists because `ai-computer` genuinely drives a real browser through
/// the local OmniChrome bridge, unlike the simulated browser tooling elsewhere
/// (for which the ban was written). A future file that claims "real browser"
/// without containing this proof still fails the lint.
const REAL_BROWSER_PROOF_MARKERS: &[&str] = &["omnichrome", "cdp"];

/// Temporary exceptions: `(path fragment, banned pattern)` pairs tolerated
/// because the violating file is OUTSIDE this lint's write ownership.
/// Each entry must be removed by the owning arc once the doc (or the code)
/// tells the truth. Currently NON-EMPTY precisely because of these two
/// pre-existing violations found by the initial scan:
///
/// 1. `ai-analytics/src/lib.rs` — `MetricsRegistry::ingest` doc says
///    "Ingests all events from a collector since the last call", but the
///    body iterates every event on every call (no cursor): repeated ingests
///    double-count. Owner: ai-analytics arc.
/// 2. `ai-rag/src/hybrid.rs` — `hybrid_fusion` doc says "(RRF-style fusion
///    is available via hybrid_fusion)", but `hybrid_fusion` implements
///    alpha-weighted linear score combination, NOT reciprocal rank fusion,
///    and no RRF implementation marker exists in the file.
///    Owner: ai-rag arc.
const ALLOWLIST: &[(&str, &str)] = &[
    ("ai-analytics/src/lib.rs", "since the last call"),
    ("ai-rag/src/hybrid.rs", "rrf-style"),
];

// ---------------------------------------------------------------------------
// Source-text processing
// ---------------------------------------------------------------------------

/// Recursively collects `.rs` files under `dir`, skipping build artifacts
/// and scratch directories.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(
            name,
            "target" | ".git" | ".cargo" | "proptest-regressions" | "mutants.out"
        ) {
            continue;
        }
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if name.ends_with(".rs") {
            out.push(path);
        }
    }
}

/// Whether a file path is test-only code (`tests/`, `benches/`,
/// `examples/` subtrees) and thus out of scope.
fn is_test_tree(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        matches!(s.as_ref(), "tests" | "benches" | "examples")
    })
}

/// Removes the contents of `#[cfg(test)]`-gated modules from `src`,
/// replacing skipped lines with empty placeholders so that byte offsets and
/// LINE NUMBERS in the returned text still match the ORIGINAL file.
/// Handles both forms:
/// - `#[cfg(test)] mod name;` (declaration — blanks the two lines);
/// - `#[cfg(test)] mod name { ... }` (inline module — blanks through the
///   matching closing brace).
///
/// Brace counting ignores braces inside double-quoted strings and `//`
/// line comments. Known limitation: braces inside raw strings or char
/// literals can skew the balance; none of the banned patterns depend on
/// that region being counted exactly.
fn strip_cfg_test_modules(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut kept: Vec<String> = Vec::with_capacity(lines.len());
    for line in &lines {
        kept.push((*line).to_string());
    }
    let mut i = 0usize;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            // Find the next meaningful line after the attribute.
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if j < lines.len() && lines[j].trim_start().starts_with("mod ") {
                if let Some(semi) = lines[j].find(';') {
                    if lines[j][..semi].find('{').is_none() {
                        // Declaration form: `mod x;` — blank attribute + decl.
                        kept[i].clear();
                        kept[j].clear();
                        i = j + 1;
                        continue;
                    }
                }
                // Inline module form: blank through the balanced close brace.
                let end = skip_balanced_block(&lines, j);
                for kept_line in kept.iter_mut().take(end).skip(i) {
                    kept_line.clear();
                }
                i = end;
                continue;
            }
            // Attribute on a non-module item: blank just the attribute line
            // and keep scanning (its item stays in scope of the lint).
            kept[i].clear();
            i += 1;
            continue;
        }
        i += 1;
    }
    // Join without appending a phantom trailing newline so offsets stay
    // aligned with `src` (line counting only needs '\n' positions).
    kept.join("\n")
}

/// Returns the exclusive end-line index of the brace-balanced block whose
/// opening line is `lines[start]` (expected to contain the first `{`).
fn skip_balanced_block(lines: &[&str], start: usize) -> usize {
    let mut depth = 0i32;
    let mut opened = false;
    for (offset, line) in lines[start..].iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let mut k = 0usize;
        while k < chars.len() {
            let c = chars[k];
            if c == '"' {
                // Skip a double-quoted string (honoring escapes).
                k += 1;
                while k < chars.len() {
                    if chars[k] == '\\' {
                        k += 2;
                        continue;
                    }
                    if chars[k] == '"' {
                        k += 1;
                        break;
                    }
                    k += 1;
                }
                continue;
            }
            if c == '/' && chars.get(k + 1) == Some(&'/') {
                break; // line comment — ignore the rest of the line
            }
            match c {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => {
                    depth -= 1;
                    if opened && depth == 0 {
                        return start + offset + 1;
                    }
                }
                _ => {}
            }
            k += 1;
        }
    }
    lines.len()
}

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

/// A single banned-pattern hit.
#[derive(Debug)]
struct Hit {
    rule: &'static str,
    pattern: String,
    line: usize,
    excerpt: String,
}

/// Finds all hits of `BANNED_RULES` in non-test `text` from `rel_path`,
/// honoring the ALLOWLIST. `rel_path` uses forward slashes.
fn violations_in(rel_path: &str, text: &str) -> Vec<Hit> {
    let stripped = strip_cfg_test_modules(text);
    let lowered = stripped.to_ascii_lowercase();

    let line_of =
        |offset: usize| -> usize { 1 + stripped[..offset].bytes().filter(|&b| b == b'\n').count() };
    let excerpt_of = |offset: usize| -> String {
        stripped
            .lines()
            .nth(line_of(offset) - 1)
            .unwrap_or("")
            .trim()
            .chars()
            .take(140)
            .collect()
    };

    let mut hits = Vec::new();
    for rule in BANNED_RULES {
        // "rrf-style" is handled by the conditional implementation-marker
        // check below; a plain substring hit here would double-report.
        if rule.name == "rrf-style-fusion" {
            continue;
        }
        let needle = rule.pattern;
        let mut from = 0usize;
        while let Some(pos) = lowered[from..].find(needle) {
            let abs = from + pos;
            let allowed = ALLOWLIST
                .iter()
                .any(|(fragment, pattern)| rel_path.contains(fragment) && *pattern == needle)
                // Evidence-gated claim: "real browser" is tolerated in files
                // that themselves contain the real-bridge proof markers
                // (mirrors the conditional RRF handling below).
                || (rule.name == "real-browser"
                    && REAL_BROWSER_PROOF_MARKERS.iter().any(|m| lowered.contains(m)));
            if !allowed {
                hits.push(Hit {
                    rule: rule.name,
                    pattern: needle.to_string(),
                    line: line_of(abs),
                    excerpt: excerpt_of(abs),
                });
            }
            from = abs + needle.len();
        }
    }

    // Conditional rule: "RRF-style" requires a real RRF implementation in
    // the same file (checked AFTER test-module stripping).
    if lowered.contains("rrf-style")
        && !RRF_IMPLEMENTATION_MARKERS
            .iter()
            .any(|m| lowered.contains(m))
    {
        let allowed = ALLOWLIST
            .iter()
            .any(|(fragment, pattern)| rel_path.contains(fragment) && *pattern == "rrf-style");
        if !allowed {
            // Report at the first occurrence.
            let pos = lowered.find("rrf-style").unwrap_or(0);
            hits.push(Hit {
                rule: "rrf-style-without-implementation",
                pattern: "rrf-style".to_string(),
                line: line_of(pos),
                excerpt: excerpt_of(pos),
            });
        }
    }

    hits
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

#[test]
fn workspace_docs_tell_the_truth() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    // No canonicalize(): it transiently fails (Win32 ERROR_FILE_NOT_FOUND)
    // under post-build AV/journal load on Windows, and the walk only needs
    // a consistent base for strip_prefix. A short retry guards against
    // directory-enumeration transience.
    let crates_dir = {
        let base = Path::new(manifest).join("../../crates");
        let mut attempt = 0;
        loop {
            if base.is_dir() {
                break base;
            }
            attempt += 1;
            assert!(
                attempt <= 3,
                "crates directory not found at {}",
                base.display()
            );
            std::thread::sleep(std::time::Duration::from_millis(150 * attempt as u64));
        }
    };

    let mut files = Vec::new();
    collect_rs_files(&crates_dir, &mut files);
    // Sanity: the workspace currently holds ~94 .rs files under crates/
    // (82 outside tests/benches) and is growing toward 100+. A floor far
    // below that catches a broken walk (wrong root, skipped dirs) without
    // making this lint flaky while sibling arcs land new files.
    assert!(
        files.len() >= 80,
        "sanity: expected a substantial crate tree (>=80 .rs files), found {} — \
         did the walk root resolve correctly? ({})",
        files.len(),
        crates_dir.display()
    );

    let mut total_hits = 0usize;
    let mut allowlisted_files = 0usize;
    let mut failures = String::new();
    for path in &files {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        if is_test_tree(path) {
            continue;
        }
        let rel = path
            .strip_prefix(&crates_dir)
            .expect("walked paths are under crates/")
            .to_string_lossy()
            .replace('\\', "/");
        let hits = violations_in(&rel, &src);
        if !hits.is_empty() {
            total_hits += hits.len();
            failures.push_str(&format!("\n  {}:\n", crates_dir.join(&rel).display()));
            for hit in hits {
                failures.push_str(&format!(
                    "    [{}] line {}: banned `{}` — {}\n      | {}",
                    hit.rule,
                    hit.line,
                    hit.pattern,
                    BANNED_RULES
                        .iter()
                        .find(|r| r.name == hit.rule)
                        .map(|r| r.rationale)
                        .unwrap_or("conditional RRF rule"),
                    hit.excerpt
                ));
            }
        }
        if ALLOWLIST.iter().any(|(fragment, _)| rel.contains(fragment)) {
            allowlisted_files += 1;
        }
    }

    println!(
        "doc-truth lint: scanned {} non-test .rs files under {}",
        files.len(),
        crates_dir.display()
    );
    println!(
        "doc-truth lint: {} temporary ALLOWLIST entr{} cover{} known violations outside this lint's ownership",
        ALLOWLIST.len(),
        if ALLOWLIST.len() == 1 { "y" } else { "ies" },
        if ALLOWLIST.len() == 1 { "es" } else { "" },
    );
    println!("doc-truth lint: allowlisted files seen during walk: {allowlisted_files}");

    assert!(
        failures.is_empty(),
        "doc-truth violations found ({total_hits}):{failures}\n\
         Fix the documentation to describe what the code ACTUALLY does, or \
         add a precise, justified ALLOWLIST entry if the file is outside \
         this linter's write ownership."
    );
}

// ---------------------------------------------------------------------------
// Detector self-tests (keep the linter honest)
// ---------------------------------------------------------------------------

#[test]
fn detector_flags_each_banned_pattern() {
    for rule in BANNED_RULES {
        // The RRF rule is conditional (implementation-marker gated) and is
        // covered by `rrf_style_requires_an_implementation_marker`.
        if rule.name == "rrf-style-fusion" {
            continue;
        }
        let sample = format!("//! docs claiming {} here\nfn main() {{}}", rule.pattern);
        let hits = violations_in("some-crate/src/lib.rs", &sample);
        assert_eq!(hits.len(), 1, "rule {} must fire exactly once", rule.name);
        assert_eq!(hits[0].rule, rule.name);
        assert_eq!(hits[0].line, 1);
    }
}

#[test]
fn detector_is_case_insensitive() {
    let hits = violations_in(
        "x/src/a.rs",
        "//! REAL Browser Computer Use — production-Ready since forever",
    );
    assert!(hits.iter().any(|h| h.rule == "real-browser"));
    assert!(hits.iter().any(|h| h.rule == "production-ready-claim"));
}

#[test]
fn cfg_test_modules_are_stripped_but_declarations_kept_safe() {
    // A lie hidden inside #[cfg(test)] code must NOT fail the lint…
    let with_test_mod = "\
//! public docs
pub fn f() -> u32 { 1 }

#[cfg(test)]
mod tests {
    fn helper_since_the_last_call() {}
    struct S { field: u32 }
}
";
    assert!(violations_in("x/src/a.rs", with_test_mod).is_empty());

    // …while the same lie in normal code still fails, even when the test
    // module precedes it and its braces include strings/comments.
    let mixed = "\
//! public docs
#[cfg(test)]
mod tests {
    // braces in comments } { and strings \"}\" must not break counting
    const S: &str = \"}{\";
    fn t() { assert!(true); }
}

/// Ingests events since the last call.
pub fn g() {}
";
    let hits = violations_in("x/src/a.rs", mixed);
    assert_eq!(hits.len(), 1, "lie after test module must be found");
    // Line numbers must reference the ORIGINAL file, not the stripped text.
    assert_eq!(hits[0].line, 9);

    // Declaration form (`mod proptests;`) is skipped without breaking the scan.
    let decl_form = "\
//! docs
#[cfg(test)]
mod proptests;

pub fn h() {}
";
    assert!(violations_in("x/src/a.rs", decl_form).is_empty());
}

#[test]
fn rrf_style_requires_an_implementation_marker() {
    // Claim without implementation → flagged. (Use a NON-allowlisted path:
    // ai-rag/src/hybrid.rs currently carries a temporary exception.)
    let liar = "/// Combines scores (RRF-style fusion available).\npub fn fuse() {}\n";
    let hits = violations_in("ai-rag-other/src/hybrid.rs", liar);
    assert!(
        hits.iter()
            .any(|h| h.rule == "rrf-style-without-implementation"),
        "unimplemented RRF-style claim must be flagged, got {hits:?}"
    );

    // Claim WITH an actual implementation marker → accepted.
    let real = "\
/// Combines scores using RRF-style fusion.
pub fn fuse(ranks: &[usize]) -> Vec<f32> {
    ranks.iter().map(|&rank| 1.0 / (rrf_k + rank as f32)).collect()
}
";
    assert!(violations_in("ai-rag/src/hybrid.rs", real).is_empty());

    // Marker via named function also accepted.
    let named = "/// RRF-style.\nfn reciprocal_rank_fusion(k: f32, rank: usize) -> f32 { 1.0 / (k + rank as f32) }\n";
    assert!(violations_in("ai-rag/src/hybrid.rs", named).is_empty());
}

#[test]
fn allowlist_scopes_hits_to_the_exact_file_and_pattern() {
    // Allowlisted file + matching pattern → tolerated.
    assert!(
        violations_in(
            "ai-analytics/src/lib.rs",
            "/// Ingests all events from a collector since the last call.\n"
        )
        .is_empty()
    );

    // Same pattern in ANY OTHER file → still flagged.
    let hits = violations_in(
        "ai-other/src/lib.rs",
        "/// Ingests all events from a collector since the last call.\n",
    );
    assert_eq!(hits.len(), 1);

    // Allowlisted file but a DIFFERENT banned pattern → still flagged.
    let hits = violations_in(
        "ai-analytics/src/lib.rs",
        "/// Real Browser support since the last call.\n",
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].rule, "real-browser");
}
