//! Reference resolution + the full reference_pass, in Rust.
//!
//! Ports `dataworm/extractors/references.py` (the resolver `_resolve_py` /
//! `_resolve_pathlike` / `resolve_reference`, including the bare-import
//! sibling candidate) and `dataworm/engine.py`'s `reference_pass` (the loop
//! that reads each file node's text, extracts refs, resolves them against
//! the store, and builds `references` edges + records `dangling`).
//!
//! `reference_pass` mutates the Rust `GraphStore` in place and returns a
//! summary (added edges + dangling map) so the Python wrapper can emit the
//! `reset_dim` + per-edge bus events that animate the dashboard.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde_json::{json, Value};

use crate::store::GraphStore;
use crate::{EdgeData, NodeData, PassWarning};

const RESOLVE_EXTS: &[&str] = &[
    ".py", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".md", ".json", ".html", ".txt", ".rst",
];
const INDEX_FILES: &[&str] = &[
    "index.js",
    "index.ts",
    "index.jsx",
    "index.tsx",
    "__init__.py",
];

// ---- posixpath helpers (mirror python's posixpath over forward-slash ids) ----

fn dirname(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn join(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else if rel.is_empty() {
        base.to_string()
    } else {
        format!("{}/{}", base, rel)
    }
}

/// Mirror python's posixpath.normpath for the simple cases we hit:
/// collapse "." and ".." segments, strip trailing slash.
fn normpath(p: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn has_suffix(path: &str) -> bool {
    Path::new(path).extension().is_some()
}

// ---- the resolver (parity with references.py) -----------------------------

fn resolve_py(node_id: &str, raw: &str) -> Vec<String> {
    let dots = raw.chars().take_while(|c| *c == '.').count();
    let module = &raw[dots..];
    let base_dir = dirname(node_id);

    let prefix = if dots > 0 {
        // Relative import: climb (dots - 1) package levels.
        let mut start = base_dir.clone();
        for _ in 0..(dots.saturating_sub(1)) {
            start = dirname(&start);
        }
        let rel_module = module.replace('.', "/");
        if rel_module.is_empty() {
            start
        } else {
            join(&start, &rel_module)
        }
    } else {
        module.replace('.', "/")
    };

    let mut candidates: Vec<String> = Vec::new();
    if !prefix.is_empty() {
        candidates.push(format!("{}.py", prefix));
        candidates.push(format!("{}/__init__.py", prefix));
    }
    // Bare single-segment import: also try the sibling in the file's own dir.
    if dots == 0 && !module.contains('/') && !base_dir.is_empty() {
        let sibling = join(&base_dir, module);
        candidates.push(format!("{}.py", sibling));
        candidates.push(format!("{}/__init__.py", sibling));
    }
    candidates
}

fn resolve_pathlike(node_id: &str, raw: &str) -> Vec<String> {
    let raw = raw
        .split('#')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    if raw.is_empty() {
        return Vec::new();
    }
    let base_dir = dirname(node_id);
    let joined = normpath(&join(&base_dir, raw));
    if joined.is_empty() || joined.starts_with("..") {
        return Vec::new();
    }

    let mut candidates = vec![joined.clone()];
    if !has_suffix(&joined) {
        for ext in RESOLVE_EXTS {
            candidates.push(format!("{}{}", joined, ext));
        }
        for idx in INDEX_FILES {
            candidates.push(format!("{}/{}", joined, idx));
        }
    }
    candidates
}

fn suffix_of(node_id: &str) -> String {
    Path::new(node_id)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default()
}

/// Return the node id a raw reference points to, or None if unresolved.
/// Parity with `references.resolve_reference`.
pub fn resolve_reference(store: &GraphStore, node: &NodeData, raw: &str) -> Option<String> {
    let suffix = suffix_of(&node.id);
    let candidates: Vec<String> = if suffix == ".py" {
        // A bare relative path in a .py string literal can still be a file.
        let mut c = resolve_py(&node.id, raw);
        if raw.starts_with('.') {
            c.extend(resolve_pathlike(&node.id, raw));
        }
        c
    } else {
        resolve_pathlike(&node.id, raw)
    };
    candidates
        .into_iter()
        .find(|cand| !cand.is_empty() && store.has_node(cand))
}

// ---- the full reference_pass --------------------------------------------

/// Result of a reference pass: the edges added (src, dst) + per-node dangling
/// refs, returned as JSON so the Python wrapper can emit bus events.
///
/// `warnings` collects {"op":"read"} PassWarnings for files that could not be
/// read_to_string'd (they are skipped exactly as before — outputs unchanged).
pub fn reference_pass(
    store: &mut GraphStore,
    max_content_bytes: u64,
    text_extensions: &[String],
    warnings: &mut Vec<PassWarning>,
) -> Value {
    // 1. Clear existing references edges.
    let removed = store.clear_edges("references");

    // 2. Snapshot the file nodes we'll process (avoid borrowing store across the loop).
    let file_nodes: Vec<NodeData> = store
        .nodes(Some("file"))
        .into_iter()
        .filter(|n| crate::is_text(&n.id, text_extensions))
        .cloned()
        .collect();

    let mut added_edges: Vec<(String, String)> = Vec::new();
    let mut dangling_map: HashMap<String, Vec<String>> = HashMap::new();

    for node in &file_nodes {
        if node.size > max_content_bytes {
            continue;
        }
        // Content-addressed memo (parity with engine.reference_pass): raw
        // reference extraction depends on the bytes (content_hash) AND the
        // extension, so the key carries both. A hit skips the fs read AND the
        // extraction entirely; resolution below always re-runs. A stored entry
        // implies non-empty content: empty-text files are skipped WITHOUT a
        // memo entry, exactly like the Python pass.
        let raw_refs: Vec<String> = if !node.content_hash.is_empty() {
            let memo_key = format!("{}|{}", node.content_hash, suffix_of(&node.id));
            if let Some(cached) = store.memo_refs.get(&memo_key) {
                cached.clone()
            } else {
                let text = match fs::read_to_string(&node.path) {
                    Ok(t) if t.len() as u64 <= max_content_bytes => t,
                    Ok(_) => continue, // over the size cap: policy skip, not a failure
                    Err(e) => {
                        warnings.push(PassWarning {
                            path: node.path.clone(),
                            op: "read".to_string(),
                            error: e.to_string(),
                        });
                        continue;
                    }
                };
                if text.is_empty() {
                    continue;
                }
                let extracted = crate::extract_raw_references(&node.id, &text);
                store.memo_refs.insert(memo_key, extracted.clone());
                extracted
            }
        } else {
            let text = match fs::read_to_string(&node.path) {
                Ok(t) if t.len() as u64 <= max_content_bytes => t,
                Ok(_) => continue, // over the size cap: policy skip, not a failure
                Err(e) => {
                    warnings.push(PassWarning {
                        path: node.path.clone(),
                        op: "read".to_string(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            if text.is_empty() {
                continue;
            }
            crate::extract_raw_references(&node.id, &text)
        };
        let mut resolved: HashSet<String> = HashSet::new();
        let mut dangling: Vec<String> = Vec::new();
        for raw in raw_refs {
            match resolve_reference(store, node, &raw) {
                Some(target) if target != node.id => {
                    resolved.insert(target);
                }
                None => dangling.push(raw),
                _ => {} // self-reference; skip
            }
        }
        for target in &resolved {
            store.add_edge(EdgeData {
                src: node.id.clone(),
                dst: target.clone(),
                edge_type: "references".to_string(),
                weight: 1.0,
                attrs: HashMap::new(),
            });
            added_edges.push((node.id.clone(), target.clone()));
        }
        if !dangling.is_empty() {
            dangling_map.insert(node.id.clone(), dangling);
        }
    }

    json!({
        "removed": removed,
        "added_edges": added_edges,
        "dangling": dangling_map,
    })
}

// The shared text filter is crate::is_text (lib.rs); the per-module copy that
// used to live here was removed with the is_text dedup.
