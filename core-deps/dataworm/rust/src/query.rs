//! Query ops over the Rust GraphStore: to_id, impact_of, neighbors,
//! context_for, search, summary. Ports `dataworm/query.py`'s QueryAPI so
//! every read query runs in Rust over the graph held in Rust memory.
//!
//! Each fn takes `&GraphStore` + params and returns a serde_json::Value; the
//! PyO3 `PyGraphStore` exposes them as methods so Python's `QueryAPI` becomes
//! a thin wrapper (or `Core._op_*` calls them directly).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde_json::{json, Value};

use crate::store::GraphStore;

const ALL_EDGE_TYPES: &[&str] = &["contains", "references", "duplicate_of", "similar_to"];

// ---- to_id: map a user path (id/relative/absolute) to a node id -----------

pub fn to_id(store: &GraphStore, path: &str) -> Option<String> {
    if store.has_node(path) {
        return Some(path.to_string());
    }
    // Try absolute-path relativisation against the store's root. Both paths
    // are normalised to forward slashes so Windows backslashes don't break it.
    if !store.root.is_empty() {
        let root_norm = store.root.replace('\\', "/");
        let path_norm = path.replace('\\', "/");
        // Also try canonicalize (resolves symlinks / short names on Windows).
        let canon_path = Path::new(path).canonicalize().ok();
        let canon_root = Path::new(&store.root).canonicalize().ok();
        if let (Some(cp), Some(cr)) = (canon_path, canon_root) {
            if let Ok(rel) = cp.strip_prefix(&cr) {
                let cand: String = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                if store.has_node(&cand) {
                    return Some(cand);
                }
            }
        }
        // Fallback: string-level relativisation (handles the case where
        // canonicalize failed, e.g. a path under a short-name temp dir).
        if path_norm.starts_with(&root_norm) {
            let rel = &path_norm[root_norm.len()..];
            let rel = rel.trim_start_matches('/');
            if store.has_node(rel) {
                return Some(rel.to_string());
            }
        }
    }
    // Suffix match: needle == id, or id ends with "/needle".
    let needle = path.replace('\\', "/");
    let needle = needle.trim_start_matches("./");
    for node_id in store.node_ids() {
        if node_id == needle || node_id.ends_with(&format!("/{}", needle)) {
            return Some(node_id);
        }
    }
    None
}

// ---- impact_of: reverse-references BFS (blast radius) --------------------

/// Shared reverse-reference BFS core: walks `references` edges backwards from
/// `target`, splitting direct (depth-0) from transitive hits, both sorted.
/// Used by `query::impact_of` (which caps/truncates its response) and by
/// `lib::compute_impact` (uncapped) — one algorithm, two response shapes.
pub(crate) struct ImpactBfs {
    pub direct: Vec<String>,
    pub transitive: Vec<String>,
}

pub(crate) fn impact_bfs(target: &str, rev: &HashMap<String, Vec<String>>) -> ImpactBfs {
    let mut direct: Vec<String> = Vec::new();
    let mut transitive: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(target.to_string());

    // BFS frontier of (node, depth).
    let mut frontier: VecDeque<(String, usize)> = VecDeque::new();
    frontier.push_back((target.to_string(), 0));

    while let Some((current, depth)) = frontier.pop_front() {
        let srcs = match rev.get(&current) {
            Some(s) => s,
            None => continue,
        };
        for src in srcs {
            if seen.contains(src) || src == target {
                continue;
            }
            seen.insert(src.clone());
            if depth == 0 {
                direct.push(src.clone());
            } else {
                transitive.push(src.clone());
            }
            frontier.push_back((src.clone(), depth + 1));
        }
    }
    direct.sort();
    transitive.sort();
    ImpactBfs { direct, transitive }
}

pub fn impact_of(store: &GraphStore, path: &str) -> Value {
    let node_id = match to_id(store, path) {
        Some(id) => id,
        None => {
            return json!({
                "error": format!("unknown path: {}", path),
                "direct": [],
                "transitive": [],
            })
        }
    };

    // Build reverse adjacency: dst -> set(src) over references edges.
    let mut rev: HashMap<String, Vec<String>> = HashMap::new();
    for e in store.all_edges() {
        if e.edge_type == "references" {
            rev.entry(e.dst.clone()).or_default().push(e.src.clone());
        }
    }

    let ImpactBfs { direct, transitive } = impact_bfs(&node_id, &rev);
    // Cap response sizes so a high-in-degree node (e.g. a base class everyone
    // imports) doesn't ship a million-entry list to the browser. The full
    // count is still reported; only the rendered lists are truncated.
    const IMPACT_CAP: usize = 1000;
    let direct_truncated = direct.len() > IMPACT_CAP;
    let transitive_truncated = transitive.len() > IMPACT_CAP;
    let direct_capped: Vec<String> = direct.into_iter().take(IMPACT_CAP).collect();
    let transitive_capped: Vec<String> = transitive.into_iter().take(IMPACT_CAP).collect();
    json!({
        "target": node_id,
        "direct": direct_capped,
        "transitive": transitive_capped,
        "total_affected": direct_capped.len() + transitive_capped.len(),
        "truncated": direct_truncated || transitive_truncated,
    })
}

// ---- neighbors: nodes within N hops --------------------------------------

pub fn neighbors(store: &GraphStore, path: &str, edge_types: &[String], depth: usize) -> Value {
    let node_id = match to_id(store, path) {
        Some(id) => id,
        None => return json!({ "error": format!("unknown path: {}", path), "neighbors": [] }),
    };
    let types: HashSet<&str> = if edge_types.is_empty() {
        ALL_EDGE_TYPES.iter().copied().collect()
    } else {
        edge_types.iter().map(|s| s.as_str()).collect()
    };

    // Build undirected adjacency over the chosen edge types.
    let mut adjacency: HashMap<String, HashSet<String>> = HashMap::new();
    for e in store.all_edges() {
        if !types.contains(e.edge_type.as_str()) {
            continue;
        }
        adjacency
            .entry(e.src.clone())
            .or_default()
            .insert(e.dst.clone());
        adjacency
            .entry(e.dst.clone())
            .or_default()
            .insert(e.src.clone());
    }

    let mut seen: HashMap<String, usize> = HashMap::new();
    seen.insert(node_id.clone(), 0);
    let mut frontier: VecDeque<String> = VecDeque::new();
    frontier.push_back(node_id.clone());
    while let Some(current) = frontier.pop_front() {
        let cur_depth = *seen.get(&current).unwrap_or(&0);
        if cur_depth >= depth {
            continue;
        }
        if let Some(neighbors) = adjacency.get(&current) {
            for nxt in neighbors {
                if !seen.contains_key(nxt) {
                    seen.insert(nxt.clone(), cur_depth + 1);
                    frontier.push_back(nxt.clone());
                }
            }
        }
    }

    let mut result: Vec<Value> = seen
        .iter()
        .filter(|(id, _)| *id != &node_id)
        .map(|(id, d)| json!({ "id": id, "depth": d }))
        .collect();
    result.sort_by(|a, b| {
        let ida = a["id"].as_str().unwrap_or("");
        let idb = b["id"].as_str().unwrap_or("");
        ida.cmp(idb)
    });
    // Cap the response so a deep /api/neighbors call doesn't ship the whole
    // subtree to the browser.
    const NEIGHBORS_CAP: usize = 1000;
    let truncated = result.len() > NEIGHBORS_CAP;
    let total = result.len();
    result.truncate(NEIGHBORS_CAP);
    json!({ "target": node_id, "depth": depth, "neighbors": result,
            "truncated": truncated, "total": total })
}

// ---- context_for: metadata + links + impact ------------------------------

pub fn context_for(store: &GraphStore, path: &str) -> Value {
    let node_id = match to_id(store, path) {
        Some(id) => id,
        None => return json!({ "error": format!("unknown path: {}", path) }),
    };
    let node = match store.get_node(&node_id) {
        Some(n) => n,
        None => return json!({ "error": format!("missing node: {}", node_id) }),
    };
    let node_json = serde_json::to_value(node).unwrap_or(Value::Null);

    // Collect in + out edges.
    let mut links: Vec<Value> = Vec::new();
    for e in store.all_edges() {
        if e.src == node_id || e.dst == node_id {
            let direction = if e.src == node_id { "out" } else { "in" };
            let other = if e.src == node_id { &e.dst } else { &e.src };
            links.push(json!({
                "id": other,
                "type": e.edge_type,
                "weight": (e.weight * 1e4).round() / 1e4,
                "direction": direction,
            }));
        }
    }
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for link in &links {
        if let Some(t) = link["type"].as_str() {
            *by_type.entry(t.to_string()).or_insert(0) += 1;
        }
    }
    let dangling = node
        .attrs
        .get("dangling")
        .cloned()
        .unwrap_or(Value::Array(vec![]));

    json!({
        "node": node_json,
        "link_counts": by_type,
        "links": links,
        "dangling_references": dangling,
        "impact": impact_of(store, path),
    })
}

// ---- search: substring over id + path ------------------------------------

pub fn search(store: &GraphStore, text: &str, limit: usize) -> Value {
    // Clamp the limit server-side so a client can't request unbounded results.
    const MAX_SEARCH_LIMIT: usize = 500;
    let limit = limit.min(MAX_SEARCH_LIMIT);
    let needle = text.to_lowercase().replace('\\', "/");
    let mut hits: Vec<Value> = Vec::new();
    for n in store.all_nodes() {
        let id_lower = n.id.to_lowercase();
        let path_lower = n.path.to_lowercase();
        if id_lower.contains(&needle) || path_lower.contains(&needle) {
            hits.push(json!({
                "id": n.id,
                "kind": n.kind,
                "path": n.path,
            }));
        }
    }
    // Deterministic order: sort by id (parity with the Python fallback, which
    // sorts the same way so results are identical across backends).
    hits.sort_by(|a, b| {
        a["id"]
            .as_str()
            .unwrap_or("")
            .cmp(b["id"].as_str().unwrap_or(""))
    });
    hits.truncate(limit);
    json!({ "results": hits })
}

// ---- summary: counts + kinds + root --------------------------------------

pub fn summary(store: &GraphStore) -> Value {
    let counts = store.counts();
    let mut kinds: HashMap<&str, usize> = [("dir", 0), ("file", 0)].into_iter().collect();
    for n in store.all_nodes() {
        *kinds.entry(n.kind.as_str()).or_insert(0) += 1;
    }
    json!({
        "root": store.root,
        "meta": store.meta,
        "node_kinds": kinds,
        "nodes": counts["nodes"],
        "edges": counts["edges"],
        "edges_contains": counts["edges_contains"],
        "edges_references": counts["edges_references"],
        "edges_duplicate_of": counts["edges_duplicate_of"],
        "edges_similar_to": counts["edges_similar_to"],
    })
}
