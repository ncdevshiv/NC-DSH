//! Rust heavy-ops core for DataWorm.
//!
//! This crate is the single source of truth for the CPU-bound passes:
//!   - `crawl_tree`        downward traversal + `contains` edges + sha256
//!   - `hash_pass`         exact (sha256) + near (simhash) `duplicate_of` edges
//!   - `extract_text_refs` per-language import/link extraction
//!   - `compute_impact`    full reverse-references BFS (blast radius)
//!   - `compute_signature` deterministic graph fingerprint
//!
//! Every public op is also reachable through `dispatch(method, params)`, which is
//! the JSON contract shared by the PyO3 module and the standalone binary — so
//! Python (in-process), the CLI binary (out-of-process), and the daemon all drive
//! the exact same code path.
//!
//! Semantics intentionally mirror the Python implementation in
//! `dataworm/crawler.py`, `extractors/{hashing,references}.py`, `query.py`, and
//! `graph.py` so the parity tests hold.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// Public submodules: these are the crate's real surface. The PyO3 module
// (feature `python`) mirrors them as methods; without that feature they are
// still legitimate public rlib API, so their items must not be flagged dead.
pub mod pass;
pub mod query;
pub mod refs;
pub mod semantic;
pub mod store;

// ---- shared wire types ---------------------------------------------------

/// Serialisable view of a graph node. Field names match the Python `Node.to_dict`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeData {
    pub id: String,
    pub path: String,
    pub kind: String, // "dir" | "file"
    pub size: u64,
    pub mtime: f64,
    pub mime: String,
    pub content_hash: String,
    #[serde(default)]
    pub root: String, // provenance: which crawl root this node came from
    pub attrs: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EdgeData {
    pub src: String,
    pub dst: String,
    pub edge_type: String,
    pub weight: f64,
    pub attrs: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphSnapshot {
    pub root: String,
    pub nodes: Vec<NodeData>,
    pub edges: Vec<EdgeData>,
    pub meta: HashMap<String, Value>,
    /// Non-fatal filesystem failures (stat) met while producing this
    /// snapshot. ALWAYS serialized — an empty vec emits `[]` — so consumers
    /// can rely on the key. `#[serde(default)]` keeps older snapshots that
    /// lack the key deserializable.
    #[serde(default)]
    pub warnings: Vec<PassWarning>,
}

/// One structured, non-fatal filesystem failure recorded during a pass or
/// traversal. Warnings are pure additive metadata: the pass proceeds exactly
/// as before WITHOUT the data, so graph outputs stay byte-identical while a
/// corrupt/missing input stops being indistinguishable from an empty one.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PassWarning {
    /// Absolute path (or directory path for enumeration failures).
    pub path: String,
    /// Exactly one of "hash" | "read" | "stat" (plus the synthetic
    /// "truncated" tail emitted by `cap_warnings`).
    pub op: String,
    /// Human-readable OS error text (never empty).
    pub error: String,
}

/// Cap an aggregated warning list at 1000 oldest-first entries; any overflow
/// collapses into ONE synthetic trailing marker:
/// `{"path":"","op":"truncated","error":"<N> more"}`.
pub(crate) fn cap_warnings(all: Vec<PassWarning>) -> Vec<PassWarning> {
    const MAX_WARNINGS: usize = 1000;
    if all.len() <= MAX_WARNINGS {
        return all;
    }
    let dropped = all.len() - MAX_WARNINGS;
    let mut kept: Vec<PassWarning> = all.into_iter().take(MAX_WARNINGS).collect();
    kept.push(PassWarning {
        path: String::new(),
        op: "truncated".to_string(),
        error: format!("{dropped} more"),
    });
    kept
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrawlRequest {
    pub root: String,
    #[serde(default)]
    pub ignore_dirs: Vec<String>,
    #[serde(default)]
    pub ignore_globs: Vec<String>,
    #[serde(default)]
    pub text_extensions: Vec<String>,
    #[serde(default)]
    pub max_content_bytes: u64,
    /// When true, crawl only the entries *directly* in `root` (no recursion):
    /// immediate subdirs are recorded as nodes + `contains` edges and returned
    /// in `meta["subdirs"]` so the caller can give each its own fragment store
    /// (the federated split). Parity with Python's `crawl_shallow`.
    #[serde(default)]
    pub shallow: bool,
    /// id -> prior {mtime, size, hash}. When a file's mtime+size are unchanged
    /// we reuse the cached hash instead of re-reading/re-hashing (incremental
    /// re-crawl). Parity with Python crawler's mtime cache.
    #[serde(default)]
    pub existing: HashMap<String, ExistingNode>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExistingNode {
    #[serde(default)]
    pub mtime: f64,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImpactRequest {
    pub target: String,
    pub reference_edges: Vec<EdgeData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureRequest {
    pub nodes: Vec<NodeData>,
    pub edges: Vec<EdgeData>,
}

// ---- config defaults (mirror dataworm/config.py) -------------------------

const DEFAULT_IGNORE_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    "node_modules",
    ".venv",
    "venv",
    "env",
    ".idea",
    ".vscode",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "target",
    ".dataworm",
];

const DEFAULT_IGNORE_GLOBS: &[&str] = &[
    "*.pyc", "*.pyo", "*.class", "*.o", "*.so", "*.dll", "*.exe", "*.png", "*.jpg", "*.jpeg",
    "*.gif", "*.ico", "*.webp", "*.mp3", "*.mp4", "*.avi", "*.mov", "*.wav", "*.zip", "*.tar",
    "*.gz", "*.7z", "*.rar", "*.pdf", "*.lock",
];

// Text-extension defaults live on the Python side (config.py); Rust receives
// `text_extensions` per request, so no Rust-side default list is kept here.

const DEFAULT_MAX_CONTENT_BYTES: u64 = 2 * 1024 * 1024;

struct CrawlConfig {
    ignore_dirs: HashSet<String>,
    ignore_globs: Vec<String>,
    max_content_bytes: u64,
    shallow: bool,
    existing: HashMap<String, ExistingNode>,
}

impl CrawlConfig {
    fn from_request(req: &CrawlRequest) -> Self {
        let ignore_dirs = if req.ignore_dirs.is_empty() {
            DEFAULT_IGNORE_DIRS.iter().map(|s| s.to_string()).collect()
        } else {
            req.ignore_dirs.iter().cloned().collect()
        };
        let ignore_globs = if req.ignore_globs.is_empty() {
            DEFAULT_IGNORE_GLOBS.iter().map(|s| s.to_string()).collect()
        } else {
            req.ignore_globs.clone()
        };
        let max_content_bytes = if req.max_content_bytes == 0 {
            DEFAULT_MAX_CONTENT_BYTES
        } else {
            req.max_content_bytes
        };
        CrawlConfig {
            ignore_dirs,
            ignore_globs,
            max_content_bytes,
            shallow: req.shallow,
            existing: req.existing.clone(),
        }
    }

    fn should_ignore_dir(&self, name: &str) -> bool {
        self.ignore_dirs.contains(name)
    }

    fn should_ignore_file(&self, rel_id: &str, name: &str) -> bool {
        for pat in &self.ignore_globs {
            if glob_match(pat, name) || glob_match(pat, rel_id) {
                return true;
            }
        }
        false
    }
}

/// Lowercased ".ext" suffix of `name` ("" when it has none). Single source of
/// truth shared by the text filter and every pass — was duplicated per module.
pub(crate) fn lower_ext(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default()
}

/// Whether `name`'s extension is listed in `text_extensions` (entries look
/// like ".py"). The one shared text filter — refs.rs / semantic.rs / pass.rs
/// used to carry identical private copies of this function.
pub(crate) fn is_text(name: &str, text_extensions: &[String]) -> bool {
    let ext = lower_ext(name);
    text_extensions.iter().any(|t| t == &ext)
}

/// Minimal shell-style glob matcher (only `*` and `?`, no character classes) —
/// parity with Python's `fnmatch.fnmatch` for the default patterns.
fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    glob_match_inner(&p, &n)
}

fn glob_match_inner(p: &[char], n: &[char]) -> bool {
    let mut pi = 0;
    let mut ni = 0;
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            star = Some((sp, sn + 1));
            ni = sn + 1;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

// ---- hashing: sha256 of file bytes (capped) -------------------------------

/// sha256 of file bytes (capped). Failure is no longer silent: the error
/// propagates so the caller can record a {"op":"hash"} PassWarning while
/// proceeding WITHOUT a hash — output-wise identical to the old "" result.
fn sha256_file(path: &Path, limit: u64) -> Result<String, std::io::Error> {
    let mut hasher = Sha256::new();
    let mut reader = fs::File::open(path)?;
    let mut buf = [0u8; 65536];
    let mut read_total: u64 = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        read_total += n as u64;
        if limit > 0 && read_total > limit {
            break;
        }
    }
    let bytes = hasher.finalize();
    // hex
    let mut s = String::with_capacity(64);
    for b in bytes.iter() {
        s.push_str(&format!("{:02x}", b));
    }
    Ok(s)
}

// ---- crawl: downward traversal + contains edges ---------------------------

pub fn crawl_tree(req: CrawlRequest) -> GraphSnapshot {
    let cfg = CrawlConfig::from_request(&req);
    let root_path = PathBuf::from(&req.root);
    let mut sink = CrawlSink {
        nodes: Vec::new(),
        edges: Vec::new(),
        subdirs: Vec::new(),
        warnings: Vec::new(),
    };

    // Root node. Its id is globally unique per fragment — "#root:" + the root
    // as a forward-slash absolute path — so federated fragments never collide
    // on id "" (which collapsed every fragment's root into one canvas node).
    let root_mtime = fs::metadata(&root_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let root_id = format!("#root:{}", req.root.replace('\\', "/"));
    sink.nodes.push(NodeData {
        id: root_id.clone(),
        path: root_path.to_string_lossy().to_string(),
        kind: "dir".to_string(),
        size: 0,
        mtime: root_mtime,
        mime: String::new(),
        content_hash: String::new(),
        root: req.root.clone(),
        attrs: HashMap::new(),
    });

    visit_down(&root_path, &req.root, &root_path, &root_id, &cfg, &mut sink, 0);

    let mut meta = HashMap::new();
    meta.insert("root".to_string(), Value::String(req.root.clone()));
    if cfg.shallow {
        // Immediate subdir absolute paths, so the caller can crawl each fully
        // into its own fragment store (federated split).
        meta.insert(
            "subdirs".to_string(),
            Value::Array(sink.subdirs.into_iter().map(Value::String).collect()),
        );
    }
    GraphSnapshot {
        root: req.root,
        nodes: sink.nodes,
        edges: sink.edges,
        meta,
        warnings: sink.warnings,
    }
}

/// Accumulates traversal output so `visit_down` stays under clippy's argument
/// limit and call sites stay readable.
struct CrawlSink {
    nodes: Vec<NodeData>,
    edges: Vec<EdgeData>,
    subdirs: Vec<String>,
    /// Stat failures met during traversal; drained into `GraphSnapshot::
    /// warnings`. Recording never changes which nodes/edges are emitted.
    warnings: Vec<PassWarning>,
}

/// Hard recursion bound: never descend deeper than this many directory levels.
/// Purely defensive (the symlink guard already bounds real trees) — a
/// pathological or future-regressed tree can no longer blow the stack.
const MAX_DEPTH: usize = 256;

fn visit_down(
    root: &Path,
    root_str: &str,
    current: &Path,
    parent_id: &str,
    cfg: &CrawlConfig,
    sink: &mut CrawlSink,
    depth: usize,
) {
    if depth >= MAX_DEPTH {
        // Beyond the defensive cap: stop recursing (skip silently — stdout
        // carries the JSON contract for the standalone binary).
        return;
    }
    let entries = match fs::read_dir(current) {
        Ok(e) => e,
        Err(e) => {
            // The whole directory is unreadable/vanished: everything below it
            // would silently disappear from the graph. Record, then skip the
            // subtree exactly as before (outputs unchanged).
            sink.warnings.push(PassWarning {
                path: current.to_string_lossy().to_string(),
                op: "stat".to_string(),
                error: e.to_string(),
            });
            return;
        }
    };
    // Sorted by name for deterministic output (parity with Python's os.scandir sort).
    let mut kids: Vec<_> = entries.flatten().collect();
    kids.sort_by_key(|e| e.file_name());

    for entry in kids {
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        let full_path = entry.path();
        // Link-safe traversal stat: `symlink_metadata` never follows the final
        // path component, so a symlinked entry is seen AS a link here. (The
        // previous `fs::metadata` reported the *target*'s type, so this loop
        // recursed into symlinked dirs and a cyclic link pair overflowed the
        // stack.)
        let link_meta = match fs::symlink_metadata(&full_path) {
            Ok(m) => m,
            Err(e) => {
                // The entry itself cannot be stat'ed (vanished mid-crawl,
                // permission denied): it would silently vanish from the
                // graph. Record, then skip exactly as before.
                sink.warnings.push(PassWarning {
                    path: full_path.to_string_lossy().to_string(),
                    op: "stat".to_string(),
                    error: e.to_string(),
                });
                continue;
            }
        };
        let is_link = link_meta.is_symlink();
        // Node attributes come from ONE bounded follow stat that is never
        // recursed into: a resolvable link records its target's kind/size/
        // mtime, a broken or unreadable link its own. For plain files/dirs
        // both stats agree, so symlink-free trees behave exactly as before.
        let meta = if is_link {
            match fs::metadata(&full_path) {
                Ok(m) => m,
                Err(_) => link_meta,
            }
        } else {
            link_meta
        };
        // Strict confinement: rel_id returns None when full_path is outside root.
        // Out-of-root entries are never recorded.
        let node_id = match rel_id(root, &full_path) {
            Some(id) => id,
            None => continue, // out-of-root; never record
        };
        // Skip hidden (parity: Python skips names starting with "." inside
        // visit? — actually crawler.py only skips via ignore rules. But the
        // legacy Rust impl skipped dotfiles; keep parity with crawler.py by
        // NOT skipping dotfiles here, only ignore_dirs/ignore_globs.)
        let file_type = meta.file_type();
        if file_type.is_dir() {
            if cfg.should_ignore_dir(&name_str) {
                continue;
            }
            let mtime = mtime_of(&meta);
            sink.nodes.push(NodeData {
                id: node_id.clone(),
                path: full_path.to_string_lossy().to_string(),
                kind: "dir".to_string(),
                size: 0,
                mtime,
                mime: String::new(),
                content_hash: String::new(),
                root: root_str.to_string(),
                attrs: HashMap::new(),
            });
            sink.edges.push(EdgeData {
                src: parent_id.to_string(),
                dst: node_id.clone(),
                edge_type: "contains".to_string(),
                weight: 1.0,
                attrs: HashMap::new(),
            });
            if is_link {
                // Symlinked dir: recorded above, NEVER descended into. This
                // gate is what makes cyclic symlink pairs terminate.
                continue;
            }
            if cfg.shallow {
                // Record the subdir but do NOT descend — the caller gives each
                // immediate subdir its own fragment store (federated split).
                sink.subdirs.push(full_path.to_string_lossy().to_string());
            } else {
                visit_down(root, root_str, &full_path, &node_id, cfg, sink, depth + 1);
            }
        } else if file_type.is_file() {
            if cfg.should_ignore_file(&node_id, &name_str) {
                continue;
            }
            let size = meta.len();
            let mtime = mtime_of(&meta);
            // Reuse the cached hash when mtime+size are unchanged (incremental
            // re-crawl — mirrors crawler.py's mtime cache). Otherwise sha256 if
            // within the size cap (crawler.py hashes when st.st_size <= cap).
            let content_hash = match cfg.existing.get(&node_id) {
                Some(ex)
                    if !ex.hash.is_empty()
                        && ex.size == size
                        && (ex.mtime - mtime).abs() < 1e-6 =>
                {
                    ex.hash.clone()
                }
                _ if size <= cfg.max_content_bytes => {
                    match sha256_file(&full_path, cfg.max_content_bytes) {
                        Ok(h) => h,
                        Err(e) => {
                            // Hash failed (vanished/unreadable between stat and
                            // open): record it, keep the node with an empty
                            // content_hash — byte-identical to the old "" path.
                            sink.warnings.push(PassWarning {
                                path: full_path.to_string_lossy().to_string(),
                                op: "hash".to_string(),
                                error: e.to_string(),
                            });
                            String::new()
                        }
                    }
                }
                _ => String::new(),
            };
            let mime = mime_guess(&name_str);
            sink.nodes.push(NodeData {
                id: node_id.clone(),
                path: full_path.to_string_lossy().to_string(),
                kind: "file".to_string(),
                size,
                mtime,
                mime,
                content_hash,
                root: root_str.to_string(),
                attrs: HashMap::new(),
            });
            sink.edges.push(EdgeData {
                src: parent_id.to_string(),
                dst: node_id,
                edge_type: "contains".to_string(),
                weight: 1.0,
                attrs: HashMap::new(),
            });
        } else {
            // Neither a regular file nor a directory (broken symlink target,
            // Unix socket/fifo/device, ...): this entry used to vanish
            // silently. Record the skip — outputs stay identical.
            sink.warnings.push(PassWarning {
                path: full_path.to_string_lossy().to_string(),
                op: "stat".to_string(),
                error: format!(
                    "entry is neither a regular file nor a directory ({file_type:?}); skipped"
                ),
            });
        }
    }
}

fn mtime_of(meta: &fs::Metadata) -> f64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn rel_id(root: &Path, path: &Path) -> Option<String> {
    // Canonical node id: root-relative path with forward slashes. Empty for root.
    //
    // Strict confinement: returns None if `path` is not inside `root`. The worm
    // never mints a node for anything outside the crawl root (no basename
    // fallback). Callers must skip None rather than record an out-of-root node.
    match path.strip_prefix(root) {
        Ok(rel) => {
            let id = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join("/");
            Some(id)
        }
        Err(_) => None,
    }
}

/// Tiny built-in mime table mirroring Python's mimetypes.guess_type for the
/// extensions we care about. Avoids pulling in a mime crate.
fn mime_guess(name: &str) -> String {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html".to_string(),
        "css" => "text/css".to_string(),
        "js" | "mjs" | "cjs" => "application/javascript".to_string(),
        "json" => "application/json".to_string(),
        "xml" => "application/xml".to_string(),
        "md" => "text/markdown".to_string(),
        "txt" => "text/plain".to_string(),
        "py" => "text/x-python".to_string(),
        "rs" => "text/rust".to_string(),
        "go" => "text/x-go".to_string(),
        "java" => "text/x-java".to_string(),
        "c" => "text/x-c".to_string(),
        "cpp" | "cxx" => "text/x-c++".to_string(),
        "ts" => "application/typescript".to_string(),
        "tsx" | "jsx" => "text/jsx".to_string(),
        "sh" => "application/x-sh".to_string(),
        "yaml" | "yml" => "application/yaml".to_string(),
        "toml" => "application/toml".to_string(),
        "csv" => "text/csv".to_string(),
        "svg" => "image/svg+xml".to_string(),
        _ => String::new(),
    }
}

// ---- hashing pass: simhash + near-duplicate detection --------------------
// Parity with dataworm/extractors/hashing.py.

const HASHBITS: usize = 64;

fn token_hash(token: &str) -> u64 {
    use md5::{Digest, Md5};
    let mut h = Md5::new();
    h.update(token.as_bytes());
    let digest = h.finalize();
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

pub fn simhash(text: &str) -> u64 {
    let tokens: Vec<String> = tokenize(text);
    if tokens.is_empty() {
        return 0;
    }
    let mut v = vec![0i64; HASHBITS];
    for tok in &tokens {
        let h = token_hash(tok);
        for (i, count) in v.iter_mut().enumerate() {
            if h & (1u64 << i) != 0 {
                *count += 1;
            } else {
                *count -= 1;
            }
        }
    }
    let mut fp: u64 = 0;
    for (i, count) in v.iter().enumerate() {
        if *count > 0 {
            fp |= 1u64 << i;
        }
    }
    fp
}

/// Word tokenizer mirroring Python's `\w+` regex over lowercased text.
/// Returns owned strings (lowercased, split on non-word chars).
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

pub fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn is_near_duplicate(a: u64, b: u64, max_distance: u32) -> bool {
    if a == 0 || b == 0 {
        return false;
    }
    hamming_distance(a, b) <= max_distance
}

// Simhash banding (parity with extractors/hashing.py): a 64-bit fingerprint
// splits into BANDS disjoint slices of BAND_BITS bits. Pigeonhole guarantee
// for exact recall of hamming <= 3: with 4 bands, at most 3 differing bits
// touch at most 3 bands, so any true near-duplicate pair agrees on at least
// one full band value. Indexing fingerprints by band value therefore recalls
// EVERY qualifying pair; the exact hamming check stays as the verifier.
const BANDS: usize = 4;
const BAND_BITS: usize = 16;

#[inline]
fn band_value(fp: u64, band: usize) -> u64 {
    (fp >> (band * BAND_BITS)) & 0xFFFF
}

/// Index pairs `(i, j)`, `i < j`, sharing at least one band value —
/// exact-recall candidate generation for the O(n^2) near-duplicate compare.
/// Pairs may repeat (once per shared band) and the order is grouped-by-band;
/// consumers dedupe and re-sort before emitting edges.
fn near_duplicate_candidates(fps: &[u64]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for band in 0..BANDS {
        let mut buckets: HashMap<u64, Vec<usize>> = HashMap::new();
        for (idx, fp) in fps.iter().enumerate() {
            buckets.entry(band_value(*fp, band)).or_default().push(idx);
        }
        for members in buckets.values() {
            if members.len() < 2 {
                continue;
            }
            for a in 0..(members.len() - 1) {
                for b in (a + 1)..members.len() {
                    out.push((members[a], members[b]));
                }
            }
        }
    }
    out
}

// ---- duplicate-detection core ---------------------------------------------
// Shared by the stateless `hash_pass` (below) and the stateful
// `pass::hashing_pass`: both build their inputs exactly as before, then hand
// them to these builders so the edge-construction/pairing logic exists once.

/// Exact-duplicate `duplicate_of` edges from a hash -> ids grouping. The
/// canonical target is the lexicographically smallest id in each group; every
/// other id points at it (weight 1.0, reason "exact", sha256 truncated to 12
/// hex chars — parity with Python).
pub(crate) fn exact_duplicate_edges(by_hash: &HashMap<String, Vec<String>>) -> Vec<EdgeData> {
    let mut out: Vec<EdgeData> = Vec::new();
    for (_h, ids) in by_hash.iter() {
        if ids.len() < 2 {
            continue;
        }
        let mut sorted = ids.clone();
        sorted.sort();
        let canonical = sorted[0].clone();
        for other in &sorted[1..] {
            let mut attrs = HashMap::new();
            attrs.insert("reason".to_string(), Value::String("exact".to_string()));
            // Parity with Python: stores first 12 hex chars of the sha.
            attrs.insert(
                "sha256".to_string(),
                Value::String(_h.chars().take(12).collect()),
            );
            out.push(EdgeData {
                src: other.clone(),
                dst: canonical.clone(),
                edge_type: "duplicate_of".to_string(),
                weight: 1.0,
                attrs,
            });
        }
    }
    out
}

/// Near-duplicate `duplicate_of` edges over `(id, simhash)` pairs already in
/// final order: callers pre-sort / pre-truncate (the stateful pass sorts by id
/// and caps; the stateless pass keeps snapshot order), so this core never
/// reorders its input. Candidates come from 16-bit band indexing (pigeonhole
/// over 4 bands: <=3 differing bits touch <=3 bands, so every true pair shares
/// at least one full band value); verification stays the exact hamming check
/// and each candidate is verified once even when several bands surface it.
/// Pairs within hamming distance 3 that aren't already joined by an exact
/// edge become weight-0.9 edges with reason "near", emitted ascending (i, j)
/// — byte-identical to the previous full nested loop's insertion order.
pub(crate) fn near_duplicate_edges(
    fingerprints: &[(String, u64)],
    already_linked: &HashSet<(String, String)>,
) -> Vec<EdgeData> {
    let fps: Vec<u64> = fingerprints.iter().map(|(_, fp)| *fp).collect();
    // Dedupe banding candidates (a qualifying pair may surface from several
    // shared bands), verify each survivor once.
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut verified: Vec<(usize, usize)> = Vec::new();
    for (i, j) in near_duplicate_candidates(&fps) {
        if !seen.insert((i, j)) {
            continue;
        }
        if is_near_duplicate(fps[i], fps[j], 3) {
            verified.push((i, j));
        }
    }
    // Emit in ascending (i, j) == lexicographic id-pair order over the
    // caller's id-sorted input — identical to the old nested i<j loop.
    verified.sort_unstable();

    let mut out: Vec<EdgeData> = Vec::new();
    for (i, j) in verified {
        let (id_a, fp_a) = &fingerprints[i];
        let (id_b, fp_b) = &fingerprints[j];
        if already_linked.contains(&(id_a.clone(), id_b.clone()))
            || already_linked.contains(&(id_b.clone(), id_a.clone()))
        {
            continue;
        }
        let mut attrs = HashMap::new();
        attrs.insert("reason".to_string(), Value::String("near".to_string()));
        attrs.insert(
            "hamming".to_string(),
            Value::Number(serde_json::Number::from(hamming_distance(*fp_a, *fp_b))),
        );
        out.push(EdgeData {
            src: id_a.clone(),
            dst: id_b.clone(),
            edge_type: "duplicate_of".to_string(),
            weight: 0.9,
            attrs,
        });
    }
    out
}

/// Build `duplicate_of` edges for a snapshot: exact (by content_hash) + near
/// (by simhash over text files within hamming distance 3). Mirrors
/// `engine.hashing_pass`. Input is the crawl snapshot; returns new edges only.
pub fn hash_pass(snap: &GraphSnapshot) -> Vec<EdgeData> {
    // Exact duplicates: group file nodes by content_hash.
    let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
    let mut file_nodes: Vec<&NodeData> = Vec::new();
    for n in &snap.nodes {
        if n.kind == "file" && !n.content_hash.is_empty() {
            by_hash
                .entry(n.content_hash.clone())
                .or_default()
                .push(n.id.clone());
            file_nodes.push(n);
        }
    }
    let mut out = exact_duplicate_edges(&by_hash);

    // Near duplicates: simhash over text files, compare pairwise within hamming<=3.
    let mut fingerprints: Vec<(String, u64)> = Vec::new();
    for n in &file_nodes {
        let text = read_text_node(n);
        if text.is_empty() {
            continue;
        }
        let fp = simhash(&text);
        fingerprints.push((n.id.clone(), fp));
    }
    // Track already-linked pairs to skip (parity with hashing_pass dedup logic).
    let linked: HashSet<(String, String)> = out
        .iter()
        .flat_map(|e| {
            [
                (e.src.clone(), e.dst.clone()),
                (e.dst.clone(), e.src.clone()),
            ]
        })
        .collect();
    out.extend(near_duplicate_edges(&fingerprints, &linked));
    out
}

fn read_text_node(n: &NodeData) -> String {
    fs::read_to_string(&n.path).unwrap_or_default()
}

// ---- reference extraction -------------------------------------------------
// Parity with dataworm/extractors/references.py.

static PY_IMPORT: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
static PY_FROM: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
static JS_FROM: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
static JS_CALL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
static MD_LINK: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
static GENERIC_REL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

fn py_import_re() -> &'static Regex {
    // (?m) so `^`/`$` match line boundaries (parity with Python's re.MULTILINE);
    // capture the whole import list so `import a, b` yields both modules.
    PY_IMPORT.get_or_init(|| Regex::new(r"(?m)^\s*import\s+(.+)$").unwrap())
}
fn py_from_re() -> &'static Regex {
    PY_FROM.get_or_init(|| Regex::new(r"(?m)^\s*from\s+(\.{0,3}[\w\.]*)\s+import").unwrap())
}
fn js_from_re() -> &'static Regex {
    JS_FROM
        .get_or_init(|| Regex::new(r#"(?:import|export)[^'"]*?from\s*['"]([^'"]+)['"]"#).unwrap())
}
fn js_call_re() -> &'static Regex {
    JS_CALL
        .get_or_init(|| Regex::new(r#"(?:require|import)\s*\(\s*['"]([^'"]+)['"]\s*\)"#).unwrap())
}
fn md_link_re() -> &'static Regex {
    MD_LINK.get_or_init(|| Regex::new(r"\]\(\s*<?([^)>\s]+)>?(?:\s+[^)]*)?\)").unwrap())
}
fn generic_rel_re() -> &'static Regex {
    GENERIC_REL.get_or_init(|| Regex::new(r#"['"](\.{1,2}/[^'"\s]+)['"]"#).unwrap())
}

/// Split a captured ``import`` statement into module names: ``import a, b as x,
/// c.d`` -> ["a", "b", "c.d"]. Strips a trailing ``# comment`` and any ``as``
/// alias; keeps only a leading ``[A-Za-z0-9_.]`` token per comma-separated part
/// (parity with references.py's `_split_import_list`).
fn split_import_list(stmt: &str) -> Vec<String> {
    let stmt = stmt.split('#').next().unwrap_or("");
    let mut out = Vec::new();
    for part in stmt.split(',') {
        let part = part.trim();
        let m: String = part
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.')
            .collect();
        if !m.is_empty() {
            out.push(m);
        }
    }
    out
}

// RESOLVE_EXTS / INDEX_FILES live in refs.rs (their only consumer); the copies
// that used to sit here were dead weight.

/// Extract raw reference strings from a file. Parity with `extract_raw_references`.
/// `node_id` is the root-relative id; `path` is absolute (for reading).
pub fn extract_text_refs(path: String) -> Vec<String> {
    let content = fs::read_to_string(&path).unwrap_or_default();
    let node_id = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    extract_raw_references(&node_id, &content)
}

pub fn extract_raw_references(node_id: &str, text: &str) -> Vec<String> {
    let suffix = Path::new(node_id)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    let mut refs: Vec<String> = Vec::new();
    match suffix.as_str() {
        ".py" => {
            for c in py_import_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    for module in split_import_list(m.as_str()) {
                        refs.push(module);
                    }
                }
            }
            for c in py_from_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    refs.push(m.as_str().to_string());
                }
            }
        }
        ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" => {
            for c in js_from_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    refs.push(m.as_str().to_string());
                }
            }
            for c in js_call_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    refs.push(m.as_str().to_string());
                }
            }
        }
        ".md" | ".markdown" | ".rst" => {
            for c in md_link_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    refs.push(m.as_str().to_string());
                }
            }
            for c in generic_rel_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    refs.push(m.as_str().to_string());
                }
            }
        }
        _ => {
            for c in generic_rel_re().captures_iter(text) {
                if let Some(m) = c.get(1) {
                    refs.push(m.as_str().to_string());
                }
            }
        }
    }

    // De-duplicate while preserving order; drop noise (parity with references.py).
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for raw in refs {
        let raw = raw.trim().to_string();
        if raw.is_empty() || seen.contains(&raw) {
            continue;
        }
        if raw.starts_with("http://")
            || raw.starts_with("https://")
            || raw.starts_with("mailto:")
            || raw.starts_with("#")
            || raw.starts_with("data:")
        {
            continue;
        }
        seen.insert(raw.clone());
        out.push(raw);
    }
    out
}

// ---- impact: full reverse-references BFS (blast radius) -------------------
// Parity with dataworm/query.py QueryAPI.impact_of — walks `references` edges
// backwards from the target. This is a *full* BFS (the legacy impl was 1-level).
// The BFS itself is the shared core in query.rs; this stateless variant ships
// the uncapped result while query::impact_of caps/truncates its response.

pub fn compute_impact(req: ImpactRequest) -> Value {
    let target = req.target.clone();
    // Build reverse adjacency: dst -> set(src) over references edges.
    let mut rev: HashMap<String, Vec<String>> = HashMap::new();
    for e in req.reference_edges.iter() {
        if e.edge_type == "references" {
            rev.entry(e.dst.clone()).or_default().push(e.src.clone());
        }
    }

    let query::ImpactBfs { direct, transitive } = query::impact_bfs(&target, &rev);
    json!({
        "target": target,
        "direct": direct,
        "transitive": transitive,
        "total_affected": direct.len() + transitive.len(),
    })
}

// ---- signature: deterministic fingerprint ---------------------------------
// Parity with dataworm/graph.py GraphStore.signature() — sha256 over
// (num_nodes | sorted(src,dst,type,round(weight,6))).

pub fn compute_signature(req: SignatureRequest) -> Value {
    let mut h = Sha256::new();
    h.update(req.nodes.len().to_string().as_bytes());
    h.update(b"|");
    let mut edge_tuples: Vec<(String, String, String, f64)> = req
        .edges
        .iter()
        .map(|e| {
            (
                e.src.clone(),
                e.dst.clone(),
                e.edge_type.clone(),
                round6(e.weight),
            )
        })
        .collect();
    // f64 isn't Ord, so sort with a total order (parity with Python's tuple sort,
    // which compares element-wise; weights are rounded to 6dp so ties are stable).
    edge_tuples.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
    });
    for t in &edge_tuples {
        // Parity: Python does h.update(repr(tup).encode()) per edge + b";".
        // repr of a 4-tuple of (str,str,str,float) -> ('a','b','contains',1.0)
        // with a space after each comma (Python's tuple repr).
        h.update(format!("('{}', '{}', '{}', {})", t.0, t.1, t.2, repr_float(t.3)).as_bytes());
        h.update(b";");
    }
    let bytes = h.finalize();
    let hex: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    json!({ "hash_hex": hex })
}

// Shared with store::GraphStore::signature (its private copies were folded
// into these — one definition of the Python-parity float formatting).
pub(crate) fn round6(x: f64) -> f64 {
    (x * 1_000_000.0).round() / 1_000_000.0
}

/// Mirror Python's `repr(round(x,6))` for the signature: avoid scientific
/// notation, show a decimal point. Python `repr(1.0)` => "1.0".
pub(crate) fn repr_float(x: f64) -> String {
    if x == x.trunc() {
        format!("{:.1}", x)
    } else {
        format!("{}", x)
    }
}

// ---- the single JSON contract: dispatch(method, params) -------------------
//
// Every public op is reachable here. Both the PyO3 `#[pymodule]` and the
// standalone binary call this — one entrypoint, identical behaviour.

pub fn dispatch(method: &str, params: Value) -> Value {
    match method {
        "crawl" => {
            let req: CrawlRequest = match serde_json::from_value(params) {
                Ok(r) => r,
                Err(e) => return error_json(&format!("bad crawl params: {}", e)),
            };
            let snap = crawl_tree(req);
            serde_json::to_value(snap).unwrap_or_else(|_| error_json("serialize failed"))
        }
        "hash_pass" => {
            let snap: GraphSnapshot = match serde_json::from_value(params) {
                Ok(s) => s,
                Err(e) => return error_json(&format!("bad hash_pass params: {}", e)),
            };
            let edges = hash_pass(&snap);
            serde_json::to_value(edges).unwrap_or_else(|_| error_json("serialize failed"))
        }
        "extract_refs" => {
            let path = params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let refs = extract_text_refs(path);
            json!({ "refs": refs })
        }
        "extract_refs_text" => {
            // For tests: extract from in-memory text given a node_id.
            let node_id = params
                .get("node_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let text = params
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let refs = extract_raw_references(&node_id, &text);
            json!({ "refs": refs })
        }
        "impact" => {
            let req: ImpactRequest = match serde_json::from_value(params) {
                Ok(r) => r,
                Err(e) => return error_json(&format!("bad impact params: {}", e)),
            };
            compute_impact(req)
        }
        "signature" => {
            let req: SignatureRequest = match serde_json::from_value(params) {
                Ok(r) => r,
                Err(e) => return error_json(&format!("bad signature params: {}", e)),
            };
            compute_signature(req)
        }
        "simhash" => {
            let text = params.get("text").and_then(|v| v.as_str()).unwrap_or("");
            json!({ "fingerprint": simhash(text) })
        }
        "hamming" => {
            let a = params.get("a").and_then(|v| v.as_u64()).unwrap_or(0);
            let b = params.get("b").and_then(|v| v.as_u64()).unwrap_or(0);
            json!({ "distance": hamming_distance(a, b) })
        }
        "ping" => json!({ "ok": true, "backend": "rust" }),
        "reference_pass" => {
            // Stateless variant for the standalone binary: takes a snapshot,
            // runs the pass, returns the added edges + dangling. The daemon
            // uses the stateful PyGraphStore.reference_pass method instead.
            let snap: GraphSnapshot = match serde_json::from_value(params) {
                Ok(s) => s,
                Err(e) => return error_json(&format!("bad reference_pass params: {}", e)),
            };
            let mut store = store::GraphStore::from_snapshot(&snap);
            let max_content_bytes = snap
                .meta
                .get("max_content_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(2 * 1024 * 1024);
            let text_extensions: Vec<String> = snap
                .meta
                .get("text_extensions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let mut warnings: Vec<PassWarning> = Vec::new();
            let mut result = refs::reference_pass(
                &mut store,
                max_content_bytes,
                &text_extensions,
                &mut warnings,
            );
            if let Some(obj) = result.as_object_mut() {
                obj.insert(
                    "warnings".to_string(),
                    serde_json::to_value(&warnings).unwrap_or_default(),
                );
            }
            result
        }
        "semantic_pass_from_vectors" => {
            // For the sentence-transformers path: embedding is Python, the
            // O(n^2) compare is Rust. Takes a snapshot + items [{id, vec}]
            // + threshold; applies similar_to edges and returns the added list.
            let snap: GraphSnapshot = match serde_json::from_value(
                params.get("snapshot").cloned().unwrap_or(Value::Null),
            ) {
                Ok(s) => s,
                Err(e) => return error_json(&format!("bad snapshot: {}", e)),
            };
            let threshold = params
                .get("threshold")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.35);
            let items: Vec<(String, semantic::Vector)> = match params.get("items") {
                Some(arr) => arr
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|it| {
                                let id = it.get("id")?.as_str()?.to_string();
                                let vec = semantic::parse_vector(it.get("vec")?);
                                Some((id, vec))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            let mut store = store::GraphStore::from_snapshot(&snap);
            semantic::semantic_pass_from_vectors(&mut store, &items, threshold)
        }
        _ => error_json(&format!("unknown method: {}", method)),
    }
}

fn error_json(msg: &str) -> Value {
    json!({ "error": msg })
}

// ---- PyO3 module (feature-gated) ------------------------------------------
// Compiled only with --features python (set by maturin via pyo3/extension-module).
// The standalone rlib + binary build without this feature compiles cleanly.
//
// PyO3 0.29 API: arguments use `Bound<PyAny>`; the module fn takes `&Bound<PyModule>`.

#[cfg(feature = "python")]
mod python {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::types::PyModule;

    /// A stateful graph store owned by Rust. Python holds a thin handle and
    /// crosses the boundary once per mutation/query; the whole graph never
    /// materialises in Python. Bus-event emission stays on the Python side
    /// (the wrapper in dataworm/graph.py emits node/edge/reset_dim after each
    /// call so the live dashboard keeps animating).
    #[pyclass]
    #[pyo3(name = "RustGraphStore")]
    struct PyGraphStore {
        inner: store::GraphStore,
    }

    #[pymethods]
    impl PyGraphStore {
        #[new]
        #[pyo3(signature = (root="".to_string()))]
        fn new(root: String) -> Self {
            PyGraphStore {
                inner: store::GraphStore::with_root(root),
            }
        }

        /// add_node(dict) -> bool (True if newly added). Dict matches Node.to_dict().
        fn add_node(&mut self, node: Bound<'_, PyAny>) -> PyResult<bool> {
            let nd = py_to_node(&node)?;
            Ok(self.inner.add_node(nd))
        }

        fn has_node(&self, id: String) -> bool {
            self.inner.has_node(&id)
        }

        /// get_node(id) -> dict or None.
        fn get_node(&self, id: String) -> Option<String> {
            self.inner
                .get_node(&id)
                .map(|n| serde_json::to_string(&n).unwrap_or_default())
        }

        /// node_ids() -> list[str]
        fn node_ids(&self) -> Vec<String> {
            self.inner.node_ids()
        }

        /// all_nodes() -> list[dict]
        fn all_nodes(&self) -> Vec<String> {
            self.inner
                .all_nodes()
                .iter()
                .map(|n| serde_json::to_string(n).unwrap_or_default())
                .collect()
        }

        /// all_edges() -> list[dict]
        fn all_edges(&self) -> Vec<String> {
            self.inner
                .all_edges()
                .iter()
                .map(|e| serde_json::to_string(e).unwrap_or_default())
                .collect()
        }

        /// add_edge(dict). Dict matches Edge.to_dict() but uses "edge_type".
        fn add_edge(&mut self, edge: Bound<'_, PyAny>) -> PyResult<()> {
            let ed = py_to_edge(&edge)?;
            self.inner.add_edge(ed);
            Ok(())
        }

        fn get_edge(&self, src: String, dst: String, edge_type: String) -> Option<String> {
            self.inner
                .get_edge(&src, &dst, &edge_type)
                .map(|e| serde_json::to_string(e).unwrap_or_default())
        }

        /// clear_edges(type) -> count removed.
        fn clear_edges(&mut self, edge_type: String) -> usize {
            self.inner.clear_edges(&edge_type)
        }

        /// remove_node(id) -> bool.
        fn remove_node(&mut self, id: String) -> bool {
            self.inner.remove_node(&id)
        }

        /// remove_nodes_batch(ids) -> count removed. One index cleanup for the
        /// whole batch (per-node remove_node costs an O(V) index retain each).
        fn remove_nodes_batch(&mut self, ids: Vec<String>) -> usize {
            self.inner.remove_nodes_batch(&ids)
        }

        /// out_edges(node_id, edge_type) -> list[dict-json]. O(degree) via the
        /// endpoint index (edge_type "" = all types). Mirrors graph.py's
        /// out_edges so the Python wrapper can stop full-scanning all edges.
        fn out_edges(&self, node_id: String, edge_type: String) -> Vec<String> {
            let t = if edge_type.is_empty() {
                None
            } else {
                Some(edge_type.as_str())
            };
            self.inner
                .out_edges(&node_id, t)
                .into_iter()
                .map(|e| serde_json::to_string(e).unwrap_or_default())
                .collect()
        }

        /// in_edges(node_id, edge_type) -> list[dict-json]. O(degree).
        fn in_edges(&self, node_id: String, edge_type: String) -> Vec<String> {
            let t = if edge_type.is_empty() {
                None
            } else {
                Some(edge_type.as_str())
            };
            self.inner
                .in_edges(&node_id, t)
                .into_iter()
                .map(|e| serde_json::to_string(e).unwrap_or_default())
                .collect()
        }

        /// counts() -> dict (nodes/edges/edges_<type>).
        fn counts(&self) -> String {
            serde_json::to_string(&self.inner.counts()).unwrap_or_default()
        }

        /// signature() -> hex string.
        fn signature(&self) -> String {
            self.inner.signature()
        }

        /// merge(other) -> summary dict.
        fn merge(&mut self, other: &PyGraphStore) -> String {
            serde_json::to_string(&self.inner.merge(&other.inner)).unwrap_or_default()
        }

        fn attach_root(&mut self, root: String) {
            self.inner.attach_root(&root);
        }

        #[getter]
        fn root(&self) -> String {
            self.inner.root.clone()
        }

        #[setter]
        fn set_root(&mut self, root: String) {
            self.inner.root = root.clone();
            if !root.is_empty() {
                self.inner.roots.insert(root);
            }
        }

        #[getter]
        fn roots(&self) -> Vec<String> {
            self.inner.roots.iter().cloned().collect()
        }

        #[setter]
        fn set_roots(&mut self, roots: Vec<String>) {
            self.inner.roots = roots.into_iter().collect();
        }

        /// Load a crawl snapshot (dict with nodes/edges/root/meta) into this store.
        fn load_snapshot(&mut self, snap: Bound<'_, PyAny>) -> PyResult<()> {
            let json_module = PyModule::import(snap.py(), "json")?;
            let dumped: String = json_module.getattr("dumps")?.call1((&snap,))?.extract()?;
            let value: Value = serde_json::from_str(&dumped).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("bad snapshot: {}", e))
            })?;
            let graph_snap: GraphSnapshot = serde_json::from_value(value).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("bad snapshot: {}", e))
            })?;
            self.inner = store::GraphStore::from_snapshot(&graph_snap);
            Ok(())
        }

        /// to_snapshot() -> dict (nodes/edges/root/meta).
        fn to_snapshot(&self) -> String {
            serde_json::to_string(&self.inner.to_snapshot()).unwrap_or_default()
        }

        /// reference_pass(max_content_bytes, text_extensions) -> dict
        /// {removed, added_edges, dangling, warnings}. Mutates the store in
        /// place; the Python wrapper emits reset_dim + edge bus events from
        /// this result. `warnings` carries {"op":"read"} fs failures.
        fn reference_pass(
            &mut self,
            max_content_bytes: u64,
            text_extensions: Vec<String>,
        ) -> String {
            let mut warnings: Vec<PassWarning> = Vec::new();
            let mut result = refs::reference_pass(
                &mut self.inner,
                max_content_bytes,
                &text_extensions,
                &mut warnings,
            );
            if let Some(obj) = result.as_object_mut() {
                obj.insert(
                    "warnings".to_string(),
                    serde_json::to_value(&warnings).unwrap_or_default(),
                );
            }
            serde_json::to_string(&result).unwrap_or_default()
        }

        /// semantic_pass(max_content_bytes, text_extensions, max_semantic_nodes,
        /// threshold) -> dict {removed, added_edges, warnings}. TF-IDF embed +
        /// pairwise cosine, all in Rust.
        fn semantic_pass(
            &mut self,
            max_content_bytes: u64,
            text_extensions: Vec<String>,
            max_semantic_nodes: usize,
            threshold: f64,
        ) -> String {
            let mut warnings: Vec<PassWarning> = Vec::new();
            let mut result = semantic::semantic_pass(
                &mut self.inner,
                max_content_bytes,
                &text_extensions,
                max_semantic_nodes,
                threshold,
                &mut warnings,
            );
            if let Some(obj) = result.as_object_mut() {
                obj.insert(
                    "warnings".to_string(),
                    serde_json::to_value(&warnings).unwrap_or_default(),
                );
            }
            serde_json::to_string(&result).unwrap_or_default()
        }

        /// hashing_pass(max_content_bytes, text_extensions, max_hashing_nodes)
        /// -> dict {removed, added_edges, warnings}. Exact + near duplicate_of,
        /// all in Rust.
        fn hashing_pass(
            &mut self,
            max_content_bytes: u64,
            text_extensions: Vec<String>,
            max_hashing_nodes: usize,
        ) -> String {
            let mut warnings: Vec<PassWarning> = Vec::new();
            let mut result = pass::hashing_pass(
                &mut self.inner,
                max_content_bytes,
                &text_extensions,
                max_hashing_nodes,
                &mut warnings,
            );
            if let Some(obj) = result.as_object_mut() {
                obj.insert(
                    "warnings".to_string(),
                    serde_json::to_value(&warnings).unwrap_or_default(),
                );
            }
            serde_json::to_string(&result).unwrap_or_default()
        }

        /// run_convergence(max_cycles, ...) -> dict
        /// {converged, cycles, root, counts, events}. Runs the whole convergence
        /// loop in Rust on THIS store (mutated in place — no snapshot round-trip);
        /// Python replays `events` to animate the dashboard.
        // Argument list mirrors the engine's convergence kwargs 1:1 (the
        // JSON/wire contract) — restructuring it would ripple across both sides.
        #[allow(clippy::too_many_arguments)]
        fn run_convergence(
            &mut self,
            max_cycles: usize,
            max_content_bytes: u64,
            text_extensions: Vec<String>,
            max_semantic_nodes: usize,
            similarity_threshold: f64,
            enable_semantic: bool,
            enable_hashing: bool,
            max_hashing_nodes: usize,
        ) -> String {
            let result = pass::run_convergence(
                &mut self.inner,
                max_cycles,
                max_content_bytes,
                text_extensions,
                max_semantic_nodes,
                similarity_threshold,
                enable_semantic,
                enable_hashing,
                max_hashing_nodes,
            );
            serde_json::to_string(&result).unwrap_or_default()
        }

        // ---- content-addressed memo transport (bulk) ----
        // The Python side (graph.py::_RustBackedStore.memo) owns the persistent
        // memo; these two methods move it across the boundary wholesale so the
        // Rust passes can consult/extend it natively. Shape:
        //   {"refs": {key: [str,...]}, "simhash": {hash: int},
        //    "embed": {hash: {dim: weight}}}

        /// Replace all three native memo maps from a JSON payload. Malformed
        /// JSON clears the maps to empty and never panics.
        fn set_memos(&mut self, json_str: String) {
            let parsed: Result<Value, _> = serde_json::from_str(&json_str);
            let value = match parsed {
                Ok(v) => v,
                Err(_) => {
                    self.inner.memo_refs.clear();
                    self.inner.memo_simhash.clear();
                    self.inner.memo_embed.clear();
                    return;
                }
            };
            let mut refs_map: HashMap<String, Vec<String>> = HashMap::new();
            if let Some(entries) = value.get("refs").and_then(|v| v.as_object()) {
                for (key, list) in entries {
                    if let Some(items) = list.as_array() {
                        refs_map.insert(
                            key.clone(),
                            items
                                .iter()
                                .filter_map(|s| s.as_str().map(|s| s.to_string()))
                                .collect(),
                        );
                    }
                }
            }
            let mut simhash_map: HashMap<String, u64> = HashMap::new();
            if let Some(entries) = value.get("simhash").and_then(|v| v.as_object()) {
                for (key, fp) in entries {
                    if let Some(fp) = fp.as_u64() {
                        simhash_map.insert(key.clone(), fp);
                    }
                }
            }
            let mut embed_map: HashMap<String, HashMap<usize, f64>> = HashMap::new();
            if let Some(entries) = value.get("embed").and_then(|v| v.as_object()) {
                for (key, vec) in entries {
                    if let Some(dims) = vec.as_object() {
                        embed_map.insert(
                            key.clone(),
                            dims.iter()
                                .filter_map(|(dim, w)| dim.parse::<usize>().ok().zip(w.as_f64()))
                                .collect(),
                        );
                    }
                }
            }
            self.inner.memo_refs = refs_map;
            self.inner.memo_simhash = simhash_map;
            self.inner.memo_embed = embed_map;
        }

        /// Serialize all three native memo maps as JSON. On a serialization
        /// failure returns "{}" (the Python pull helper tolerates it).
        fn get_memos(&self) -> String {
            let payload = json!({
                "refs": self.inner.memo_refs,
                "simhash": self.inner.memo_simhash,
                "embed": self.inner.memo_embed,
            });
            serde_json::to_string(&payload).unwrap_or_default()
        }

        // ---- query ops (all in Rust over the in-memory store) ----

        /// to_id(path) -> id or None. Maps a user path to a node id.
        fn to_id(&self, path: String) -> Option<String> {
            query::to_id(&self.inner, &path)
        }

        /// impact_of(path) -> dict {target, direct, transitive, total_affected}.
        fn impact_of(&self, path: String) -> String {
            serde_json::to_string(&query::impact_of(&self.inner, &path)).unwrap_or_default()
        }

        /// neighbors(path, edge_types, depth) -> dict. edge_types is a list[str]
        /// (empty = all types).
        fn neighbors(&self, path: String, edge_types: Vec<String>, depth: usize) -> String {
            serde_json::to_string(&query::neighbors(&self.inner, &path, &edge_types, depth))
                .unwrap_or_default()
        }

        /// context_for(path) -> dict {node, link_counts, links, dangling, impact}.
        fn context_for(&self, path: String) -> String {
            serde_json::to_string(&query::context_for(&self.inner, &path)).unwrap_or_default()
        }

        /// search(text, limit) -> dict {results: [{id, kind, path}]}.
        fn search(&self, text: String, limit: usize) -> String {
            serde_json::to_string(&query::search(&self.inner, &text, limit)).unwrap_or_default()
        }

        /// summary() -> dict {root, meta, node_kinds, ...counts}.
        fn summary(&self) -> String {
            serde_json::to_string(&query::summary(&self.inner)).unwrap_or_default()
        }
    }

    /// Parse a Python dict (Node.to_dict shape) into a NodeData.
    fn py_to_node(node: &Bound<'_, PyAny>) -> PyResult<NodeData> {
        let json_module = PyModule::import(node.py(), "json")?;
        let dumped: String = json_module.getattr("dumps")?.call1((node,))?.extract()?;
        let value: Value = serde_json::from_str(&dumped)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad node: {}", e)))?;
        let nd: NodeData = serde_json::from_value(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad node: {}", e)))?;
        Ok(nd)
    }

    /// Parse a Python dict (Edge.to_dict shape, with "edge_type") into an EdgeData.
    fn py_to_edge(edge: &Bound<'_, PyAny>) -> PyResult<EdgeData> {
        let json_module = PyModule::import(edge.py(), "json")?;
        let dumped: String = json_module.getattr("dumps")?.call1((edge,))?.extract()?;
        let value: Value = serde_json::from_str(&dumped)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad edge: {}", e)))?;
        let ed: EdgeData = serde_json::from_value(value)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad edge: {}", e)))?;
        Ok(ed)
    }

    /// dispatch(method, params) -> result
    /// Accepts a Python dict (or any json-serialisable object) as `params`,
    /// runs the Rust op, and returns a Python dict. This is the in-process
    /// mirror of the standalone binary's JSON contract.
    #[pyfunction]
    #[pyo3(name = "dispatch")]
    fn py_dispatch(py: Python, method: &str, params: Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let json_module = PyModule::import(py, "json")?;
        let dumped: String = json_module.getattr("dumps")?.call1((&params,))?.extract()?;
        let value: Value = serde_json::from_str(&dumped)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("bad params: {}", e)))?;
        let result = super::dispatch(method, value);
        let result_str = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
        let loaded = json_module
            .getattr("loads")?
            .call1((result_str,))?
            .into_pyobject(py)?;
        Ok(loaded.unbind())
    }

    #[pyfunction]
    fn crawl_tree_py(_py: Python, root: String) -> PyResult<String> {
        let req = CrawlRequest {
            root,
            ignore_dirs: vec![],
            ignore_globs: vec![],
            text_extensions: vec![],
            max_content_bytes: 0,
            shallow: false,
            existing: HashMap::new(),
        };
        let snap = super::crawl_tree(req);
        Ok(serde_json::to_string(&snap).unwrap_or_default())
    }

    #[pyfunction]
    fn simhash_py(text: String) -> u64 {
        super::simhash(&text)
    }

    #[pyfunction]
    fn hamming_py(a: u64, b: u64) -> u32 {
        super::hamming_distance(a, b)
    }

    #[pymodule]
    fn _rust(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_function(wrap_pyfunction!(py_dispatch, m)?)?;
        m.add_function(wrap_pyfunction!(crawl_tree_py, m)?)?;
        m.add_function(wrap_pyfunction!(simhash_py, m)?)?;
        m.add_function(wrap_pyfunction!(hamming_py, m)?)?;
        m.add_class::<PyGraphStore>()?;
        m.add("__version__", env!("CARGO_PKG_VERSION"))?;
        Ok(())
    }
}

// ---- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_SEQ: AtomicUsize = AtomicUsize::new(0);

    /// Unique per-call temp dir (no tempfile dependency; tests clean up).
    fn temp_root(tag: &str) -> PathBuf {
        let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "dw_rs_test_{}_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            n
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &Path) {
        // Try both the plain and (on Windows) the extended-length form; one
        // of them succeeds even when the tree grew past MAX_PATH.
        let _ = fs::remove_dir_all(dir);
        #[cfg(windows)]
        if let Some(s) = dir.to_str() {
            let verbatim = PathBuf::from(format!(r"\\?\{}", s));
            let _ = fs::remove_dir_all(&verbatim);
        }
    }

    fn crawl(root: &Path) -> GraphSnapshot {
        crawl_tree(CrawlRequest {
            root: root.to_string_lossy().to_string(),
            ignore_dirs: vec![],
            ignore_globs: vec![],
            text_extensions: vec![],
            max_content_bytes: 0,
            shallow: false,
            existing: HashMap::new(),
        })
    }

    fn node_ids(snap: &GraphSnapshot) -> HashSet<String> {
        snap.nodes.iter().map(|n| n.id.clone()).collect()
    }

    /// Platform-appropriate directory symlink; on Unix `std::os::unix::fs::
    /// symlink` creates a dir link when the target is a dir.
    #[cfg(windows)]
    fn make_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }

    #[cfg(unix)]
    fn make_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    /// Skips gracefully when the OS denies symlink creation (Windows needs
    /// admin or Developer Mode) — the test still passes.
    macro_rules! skip_without_symlinks {
        ($res:expr, $root:expr) => {
            match $res {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("skipping: cannot create symlink: {}", e);
                    cleanup(&$root);
                    return;
                }
            }
        };
    }

    #[test]
    fn mutual_symlink_cycle_is_recorded_but_not_descended() {
        let root = temp_root("cycle");
        let a = root.join("a");
        let b = root.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("a.txt"), "alpha").unwrap();
        fs::write(b.join("b.txt"), "beta").unwrap();

        // Mutual directory-symlink cycle: a/link_to_b -> b, b/link_to_a -> a.
        skip_without_symlinks!(make_dir_symlink(&b, &a.join("link_to_b")), root);
        skip_without_symlinks!(make_dir_symlink(&a, &b.join("link_to_a")), root);

        // Termination IS the core assertion here: before the fix this crawled
        // through the pair until the stack overflowed.
        let snap = crawl(&root);
        let ids = node_ids(&snap);

        assert!(ids.contains("a") && ids.contains("b"), "real dirs recorded");
        assert!(
            ids.contains("a/link_to_b") && ids.contains("b/link_to_a"),
            "symlinked dirs recorded as nodes"
        );
        assert!(
            ids.contains("a/a.txt") && ids.contains("b/b.txt"),
            "regular files recorded"
        );
        for id in &ids {
            assert!(
                !id.contains("link_to_b/") && !id.contains("link_to_a/"),
                "descended through a symlink: {}",
                id
            );
        }
        // contains edges: every node except the root has exactly one parent.
        assert_eq!(snap.edges.len(), snap.nodes.len() - 1);
        cleanup(&root);
    }

    #[test]
    fn self_referential_symlink_terminates() {
        let root = temp_root("selfcycle");
        let d = root.join("d");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("f.txt"), "x").unwrap();
        skip_without_symlinks!(make_dir_symlink(&d, &d.join("self")), root);

        let snap = crawl(&root);
        let ids = node_ids(&snap);
        assert!(ids.contains("d/self"), "self-link recorded as a node");
        assert!(ids.contains("d/f.txt"));
        for id in &ids {
            assert!(
                !id.starts_with("d/self/"),
                "descended into self-link: {}",
                id
            );
        }
        cleanup(&root);
    }

    #[test]
    fn depth_cap_bounds_recursion() {
        // Extended-length prefix so 300+ nested dirs don't hit MAX_PATH on
        // Windows (%TEMP% alone eats ~90 chars of the 260 budget).
        let base = temp_root("deep");
        #[cfg(windows)]
        let root = match base.to_str() {
            Some(s) => PathBuf::from(format!(r"\\?\{}", s)),
            None => {
                cleanup(&base);
                return;
            }
        };
        #[cfg(not(windows))]
        let root = base.clone();

        let mut p = root.clone();
        for i in 0..(MAX_DEPTH + 64) {
            p = p.join(format!("d{}", i));
            if fs::create_dir(&p).is_err() {
                cleanup(&base);
                return;
            }
        }
        fs::write(p.join("bottom.txt"), "x").unwrap();

        // 256 nested visit_down frames (debug builds have fat frames) plus
        // remove_dir_all's own deep recursion both exceed a stock libtest
        // thread stack, so run them on a dedicated large-stack worker.
        let worker_root = root.clone();
        let snap = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || crawl(&worker_root))
            .unwrap()
            .join()
            .unwrap();
        let ids = node_ids(&snap);

        assert!(ids.contains("d0"));
        let deepest_allowed = (0..MAX_DEPTH)
            .map(|i| format!("d{}", i))
            .collect::<Vec<_>>()
            .join("/");
        assert!(
            ids.contains(&deepest_allowed),
            "level MAX_DEPTH-1 reachable"
        );
        let capped = format!("d{}", MAX_DEPTH);
        assert!(
            !ids.iter()
                .any(|id| id.split('/').any(|seg| seg == capped.as_str())),
            "nothing recorded below the depth cap"
        );
        assert!(!ids.iter().any(|id| id.ends_with("/bottom.txt")));
        // Root + exactly one node per level d0..=d{MAX_DEPTH-1}.
        assert_eq!(snap.nodes.len(), MAX_DEPTH + 1);
        let worker_cleanup = base.clone();
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || cleanup(&worker_cleanup))
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn symlink_free_tree_layout_is_unchanged() {
        let root = temp_root("plain");
        let src = root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.py"), "print('hi')\n").unwrap();
        fs::write(root.join("readme.md"), "# doc\n").unwrap();

        let snap = crawl(&root);
        let root_id = format!("#root:{}", root.to_string_lossy().replace('\\', "/"));
        let expected: HashSet<String> = [
            root_id.clone(),
            "src".to_string(),
            "src/a.py".to_string(),
            "readme.md".to_string(),
        ]
        .into_iter()
        .collect();
        assert_eq!(node_ids(&snap), expected);

        let pairs: HashSet<(String, String)> = snap
            .edges
            .iter()
            .map(|e| (e.src.clone(), e.dst.clone()))
            .collect();
        assert_eq!(pairs.len(), 3);
        assert!(pairs.contains(&(root_id.clone(), "src".to_string())));
        assert!(pairs.contains(&(root_id, "readme.md".to_string())));
        assert!(pairs.contains(&("src".to_string(), "src/a.py".to_string())));

        let py = snap.nodes.iter().find(|n| n.id == "src/a.py").unwrap();
        assert_eq!(py.kind, "file");
        assert_eq!(py.content_hash.len(), 64, "sha256 hex");
        assert_eq!(py.mime, "text/x-python");
        cleanup(&root);
    }

    #[test]
    fn exact_duplicate_edges_builder_canonicalizes() {
        let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
        by_hash.insert("aa".to_string(), vec!["b".into(), "a".into(), "c".into()]);
        by_hash.insert("solo".to_string(), vec!["only".into()]);
        let out = exact_duplicate_edges(&by_hash);
        assert_eq!(out.len(), 2, "single-member group emits nothing");
        let pairs: HashSet<(String, String)> =
            out.iter().map(|e| (e.src.clone(), e.dst.clone())).collect();
        assert!(pairs.contains(&("b".to_string(), "a".to_string())));
        assert!(pairs.contains(&("c".to_string(), "a".to_string())));
        for e in &out {
            assert_eq!(e.edge_type, "duplicate_of");
            assert_eq!(e.weight, 1.0);
            assert_eq!(
                e.attrs.get("reason").and_then(|v| v.as_str()),
                Some("exact")
            );
            assert_eq!(e.attrs.get("sha256").and_then(|v| v.as_str()), Some("aa"));
        }
    }

    #[test]
    fn near_duplicate_edges_builder_skips_already_linked() {
        let fps = vec![
            ("x".to_string(), 0b1010u64),
            ("y".to_string(), 0b1011u64), // hamming 1 apart
        ];
        let fresh = HashSet::new();
        let out = near_duplicate_edges(&fps, &fresh);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].src, "x");
        assert_eq!(out[0].dst, "y");
        assert_eq!(out[0].weight, 0.9);
        assert_eq!(
            out[0].attrs.get("hamming").and_then(|v| v.as_u64()),
            Some(1)
        );

        let mut linked = HashSet::new();
        linked.insert(("x".to_string(), "y".to_string()));
        linked.insert(("y".to_string(), "x".to_string()));
        assert!(near_duplicate_edges(&fps, &linked).is_empty());
    }

    #[test]
    fn hash_pass_finds_exact_duplicates_in_snapshot() {
        let root = temp_root("dups");
        fs::write(root.join("one.txt"), "same bytes").unwrap();
        fs::write(root.join("two.txt"), "same bytes").unwrap();
        let snap = crawl(&root);
        let edges = hash_pass(&snap);
        assert_eq!(edges.len(), 1, "one duplicate_of edge between two copies");
        assert_eq!(edges[0].edge_type, "duplicate_of");
        assert_eq!(edges[0].weight, 1.0);
        let mut pair = [edges[0].src.clone(), edges[0].dst.clone()];
        pair.sort();
        assert_eq!(pair, ["one.txt".to_string(), "two.txt".to_string()]);
        cleanup(&root);
    }

    #[test]
    fn impact_core_agrees_across_both_call_shapes() {
        // b references a; c references b; d->z is unrelated noise; an
        // a->q edge is NOT a reference and must be ignored.
        let mk_edge = |src: &str, dst: &str, t: &str| EdgeData {
            src: src.to_string(),
            dst: dst.to_string(),
            edge_type: t.to_string(),
            weight: 1.0,
            attrs: HashMap::new(),
        };
        let edges = vec![
            mk_edge("b", "a", "references"),
            mk_edge("c", "b", "references"),
            mk_edge("d", "z", "references"),
            mk_edge("a", "q", "duplicate_of"),
        ];

        // Stateless shape: uncapped compute_impact over raw edges.
        let res = compute_impact(ImpactRequest {
            target: "a".to_string(),
            reference_edges: edges.clone(),
        });
        assert_eq!(res["target"], "a");
        assert_eq!(res["direct"], serde_json::json!(["b"]));
        assert_eq!(res["transitive"], serde_json::json!(["c"]));
        assert_eq!(res["total_affected"], 2);

        // Stateful shape: same BFS core behind query::impact_of.
        let mut store = store::GraphStore::with_root(String::new());
        for id in ["a", "b", "c", "d", "z", "q"] {
            store.add_node(NodeData {
                id: id.to_string(),
                path: id.to_string(),
                kind: "file".to_string(),
                size: 0,
                mtime: 0.0,
                mime: String::new(),
                content_hash: String::new(),
                root: String::new(),
                attrs: HashMap::new(),
            });
        }
        for e in &edges {
            store.add_edge(e.clone());
        }
        let q = query::impact_of(&store, "a");
        assert_eq!(q["direct"], res["direct"]);
        assert_eq!(q["transitive"], res["transitive"]);
        assert_eq!(q["truncated"], false);

        // Unknown target keeps the error contract.
        let miss = query::impact_of(&store, "nope");
        assert!(miss.get("error").is_some());
    }

    #[test]
    fn signature_is_order_independent_and_weight_rounding_shared() {
        let mk_node = |id: &str| NodeData {
            id: id.to_string(),
            path: id.to_string(),
            kind: "file".to_string(),
            size: 0,
            mtime: 0.0,
            mime: String::new(),
            content_hash: String::new(),
            root: String::new(),
            attrs: HashMap::new(),
        };
        let mk_edge = |src: &str, dst: &str, w: f64| EdgeData {
            src: src.to_string(),
            dst: dst.to_string(),
            edge_type: "contains".to_string(),
            weight: w,
            attrs: HashMap::new(),
        };
        let nodes = vec![mk_node("a"), mk_node("b"), mk_node("c")];
        let edges_fwd = vec![mk_edge("a", "b", 1.0), mk_edge("b", "c", 0.123456789)];
        let edges_rev: Vec<EdgeData> = edges_fwd.iter().rev().cloned().collect();

        let sig = SignatureRequest {
            nodes: nodes.clone(),
            edges: edges_fwd,
        };
        let sig_rev = SignatureRequest {
            nodes,
            edges: edges_rev,
        };
        let h1 = compute_signature(sig)["hash_hex"].clone();
        let h2 = compute_signature(sig_rev)["hash_hex"].clone();
        assert_eq!(h1, h2, "edge insertion order must not change the signature");

        // round6 folds 0.123456789 to 0.123457 — shared with store.signature().
        let rounded = SignatureRequest {
            nodes: vec![],
            edges: vec![mk_edge("a", "b", 0.123457)],
        };
        let h3 = compute_signature(rounded)["hash_hex"].clone();
        assert_ne!(h1, h3, "different rounded weights must hash differently");
    }

    // ---- warning-recording tests (silent-failure elimination) --------------

    fn mk_ghost_node(path: &Path) -> NodeData {
        NodeData {
            id: "ghost.txt".to_string(),
            path: path.to_string_lossy().to_string(),
            kind: "file".to_string(),
            size: 42,
            mtime: 0.0,
            mime: String::new(),
            // Empty hash: both passes take the no-memo read path.
            content_hash: String::new(),
            root: String::new(),
            attrs: HashMap::new(),
        }
    }

    /// w1: a store node pointing at a NONEXISTENT path must surface a
    /// {"op":"read"} warning from each pass while graph counts stay put.
    #[test]
    fn w1_missing_file_records_read_warnings_and_keeps_counts() {
        let dir = temp_root("w1");
        let ghost_path = dir.join("ghost.txt"); // deliberately never created
        let mut store = store::GraphStore::with_root(dir.to_string_lossy().to_string());
        store.add_node(mk_ghost_node(&ghost_path));
        let before = store.counts();

        let mut warnings: Vec<PassWarning> = Vec::new();
        refs::reference_pass(
            &mut store,
            2 * 1024 * 1024,
            &[".txt".to_string()],
            &mut warnings,
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.op == "read" && w.path.ends_with("ghost.txt") && !w.error.is_empty()),
            "reference_pass must record a read warning: {:?}",
            warnings
        );
        assert_eq!(store.counts(), before, "reference_pass changed nothing");

        let mut hash_warnings: Vec<PassWarning> = Vec::new();
        pass::hashing_pass(
            &mut store,
            2 * 1024 * 1024,
            &[".txt".to_string()],
            256,
            &mut hash_warnings,
        );
        assert!(
            hash_warnings
                .iter()
                .any(|w| w.op == "read" && w.path.ends_with("ghost.txt") && !w.error.is_empty()),
            "hashing_pass must record a read warning: {:?}",
            hash_warnings
        );
        assert_eq!(store.counts(), before, "hashing_pass changed nothing");

        // Hash-op unit: the crawl-time hasher now reports failure instead of
        // silently returning ""; the visit_down caller turns Err into a
        // {"op":"hash"} warning plus the old empty-hash output.
        assert!(sha256_file(&ghost_path, 1024).is_err());
        cleanup(&dir);
    }

    /// w2: a healthy tree end-to-end run_convergence must carry the
    /// "warnings" key with EXACTLY an empty list.
    #[test]
    fn w2_healthy_tree_run_convergence_has_empty_warnings() {
        let root = temp_root("w2");
        fs::write(root.join("a.py"), "import b\n").unwrap();
        fs::write(root.join("b.py"), "x = 1\n").unwrap();
        let snap = crawl(&root);
        assert!(snap.warnings.is_empty());
        let mut st = store::GraphStore::from_snapshot(&snap);
        let result = pass::run_convergence(
            &mut st,
            4,
            2 * 1024 * 1024,
            vec![".py".to_string()],
            256,
            0.35,
            true,
            true,
            256,
        );
        let obj = result.as_object().expect("convergence result object");
        assert!(obj.contains_key("warnings"), "key always present");
        assert_eq!(obj["warnings"], serde_json::json!([]));
        cleanup(&root);
    }

    /// w3: stat/read mismatch — a DIRECTORY named like a text file. The crawl
    /// stats it fine (kind "dir", no warnings), but a stale/corrupt store that
    /// believes it is a file hits a hard read failure: assert it's recorded.
    #[test]
    fn w3_dir_named_like_file_surfaces_read_warning() {
        let root = temp_root("w3");
        let weird = root.join("weird.py");
        fs::create_dir_all(&weird).unwrap(); // directory masquerading as .py

        let snap = crawl(&root);
        assert!(
            snap.warnings.is_empty(),
            "crawl-level stat succeeded: {:?}",
            snap.warnings
        );
        // The key must survive serialization even when empty.
        assert_eq!(
            serde_json::to_value(&snap).unwrap()["warnings"],
            serde_json::json!([])
        );

        let mut st = store::GraphStore::from_snapshot(&snap);
        // Simulate stale metadata: flip weird.py to kind "file" (exactly what
        // a corrupted/stale store looks like), then run the pass over it.
        let mut corrupted = st.get_node("weird.py").cloned().unwrap();
        corrupted.kind = "file".to_string();
        corrupted.size = 10;
        st.add_node(corrupted);

        let before = st.counts();
        let mut warnings: Vec<PassWarning> = Vec::new();
        refs::reference_pass(
            &mut st,
            2 * 1024 * 1024,
            &[".py".to_string()],
            &mut warnings,
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.op == "read" && w.path.ends_with("weird.py") && !w.error.is_empty()),
            "unreadable entry must be recorded: {:?}",
            warnings
        );
        assert_eq!(st.counts(), before, "outputs unchanged by warnings");
        cleanup(&root);
    }

    /// Crawl-level stat warning: an unresolvable symlink is neither followed
    /// nor recordable, so it used to vanish silently — now it emits a
    /// {"op":"stat"} warning. Skips where the OS denies symlink creation.
    #[test]
    fn broken_symlink_records_stat_warning() {
        let root = temp_root("brk");
        let d = root.join("d");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("f.txt"), "x").unwrap();
        let missing = d.join("gone.txt");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(d.join("nope"), &missing);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(d.join("nope"), &missing);
        skip_without_symlinks!(made, root);

        let snap = crawl(&root);
        let ids = node_ids(&snap);
        assert!(!ids.contains("d/gone.txt"), "broken link never recorded");
        assert_eq!(snap.warnings.len(), 1, "{:?}", snap.warnings);
        assert_eq!(snap.warnings[0].op, "stat");
        assert!(snap.warnings[0].path.ends_with("gone.txt"));
        assert!(!snap.warnings[0].error.is_empty());
        cleanup(&root);
    }

    /// Contract math: keep the first 1000 oldest-first, append one synthetic
    /// truncated tail counting the rest.
    #[test]
    fn warnings_cap_keeps_oldest_and_appends_truncation_tail() {
        let all: Vec<PassWarning> = (0..1002)
            .map(|i| PassWarning {
                path: format!("p{i}"),
                op: "read".to_string(),
                error: "boom".to_string(),
            })
            .collect();
        let capped = cap_warnings(all);
        assert_eq!(capped.len(), 1001);
        assert_eq!(capped[0].path, "p0", "oldest kept first");
        assert_eq!(capped[999].path, "p999");
        let tail = &capped[1000];
        assert_eq!(tail.path, "");
        assert_eq!(tail.op, "truncated");
        assert_eq!(tail.error, "2 more");

        let under: Vec<PassWarning> = Vec::new();
        assert!(cap_warnings(under).is_empty(), "healthy stays exactly []");
    }
}
