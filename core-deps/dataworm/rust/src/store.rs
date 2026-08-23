//! Stateful GraphStore: the worm's in-memory graph, owned by Rust.
//!
//! This is the Rust-side source of truth for nodes + edges once a crawl has
//! ingested them. Python holds a thin PyO3 handle (`dataworm/graph.py`) and
//! crosses the boundary once per mutation/query — the whole graph never
//! materialises in Python. Bus-event emission stays on the Python side (the
//! Python wrapper emits `node`/`edge`/`reset_dim` after each Rust call so the
//! live dashboard keeps animating).
//!
//! Semantics mirror `dataworm/graph.py`'s `GraphStore` exactly so the parity
//! tests (Rust store vs the pure-Python fallback) hold:
//!   - nodes keyed by `id` (root-relative, forward slashes)
//!   - edges keyed by `(src, dst, type)` so multiple dimensions coexist
//!   - `signature()` is sha256 over (num_nodes | sorted rounded edge tuples)
//!   - `merge(other)` re-keys `other`'s ids into our namespace by absolute path

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{json, Value};

use crate::semantic::Vector;
use crate::{repr_float, round6, EdgeData, NodeData};

/// The graph store: nodes + edges + per-root provenance + content-addressed
/// pass memos, all in Rust memory.
///
/// The memo maps mirror `graph.py`'s Python-side `store.memo` exactly (same
/// shapes, same keys) and are transported across the PyO3 boundary in bulk by
/// `PyGraphStore::set_memos` / `get_memos`. They are keyed by sha256
/// ``content_hash`` — the crawler's dirty marker — so a hit means "this exact
/// content was already extracted/embedded":
///
///   - `memo_refs`: "<content_hash>|<ext>" -> raw reference strings; the
///     extension rides along because it selects which regex family extracts
///     (parity with engine._memo_ref_key)
///   - `memo_simhash`: content_hash -> near-duplicate fingerprint
///   - `memo_embed`: content_hash -> sparse unit-normalised TF-IDF vector
///   - `memo_vocab`: content_hash -> fingerprint of the vocabulary the cached
///     embed vector was built against (semantic.rs). Entries without a matching
///     fingerprint entry are treated as stale and re-embedded: TF-IDF indices
///     are positional, so vectors from different vocabularies must never be
///     cosine-compared together. Deliberately NOT persisted across restarts
///     (`set_memos`/`get_memos` don't carry it) — a fresh process re-embeds
///     once, which also invalidates any cross-restart vintage mixing.
pub struct GraphStore {
    pub root: String,
    pub roots: HashSet<String>,
    pub meta: HashMap<String, Value>,
    pub memo_refs: HashMap<String, Vec<String>>,
    pub memo_simhash: HashMap<String, u64>,
    pub memo_embed: HashMap<String, Vector>,
    pub memo_vocab: HashMap<String, u64>,
    nodes: HashMap<String, NodeData>,
    /// Keyed by (src, dst, edge_type) — mirrors networkx MultiDiGraph keyed edges.
    edges: HashMap<(String, String, String), EdgeData>,
    /// Endpoint index: node id -> edges where it's the source / destination.
    /// Maintained on add_edge/clear_edges/remove_node so out_edges/in_edges are
    /// O(degree) instead of O(E) — critical for /api/context, /api/impact,
    /// /api/neighbors at scale (was a full edge scan per click).
    out_index: HashMap<String, Vec<(String, String, String)>>, // node -> [(src,dst,type)]
    in_index: HashMap<String, Vec<(String, String, String)>>,
}

impl Default for GraphStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphStore {
    pub fn new() -> Self {
        GraphStore {
            root: String::new(),
            roots: HashSet::new(),
            meta: HashMap::new(),
            memo_refs: HashMap::new(),
            memo_simhash: HashMap::new(),
            memo_embed: HashMap::new(),
            memo_vocab: HashMap::new(),
            nodes: HashMap::new(),
            edges: HashMap::new(),
            out_index: HashMap::new(),
            in_index: HashMap::new(),
        }
    }

    pub fn with_root(root: String) -> Self {
        let mut s = GraphStore::new();
        s.root = root.clone();
        if !root.is_empty() {
            s.roots.insert(root);
        }
        s
    }

    // ---- nodes -----------------------------------------------------------

    /// Insert/replace a node. Returns true if it was newly added (so the Python
    /// wrapper can emit a `node` bus event only on first discovery).
    pub fn add_node(&mut self, node: NodeData) -> bool {
        let is_new = !self.nodes.contains_key(&node.id);
        self.nodes.insert(node.id.clone(), node);
        is_new
    }

    pub fn has_node(&self, id: &str) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn get_node(&self, id: &str) -> Option<&NodeData> {
        self.nodes.get(id)
    }

    /// Every node, optionally filtered by kind ("dir" / "file").
    pub fn nodes(&self, kind: Option<&str>) -> Vec<&NodeData> {
        self.nodes
            .values()
            .filter(|n| kind.is_none_or(|k| n.kind == k))
            .collect()
    }

    pub fn node_ids(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    pub fn all_nodes(&self) -> Vec<&NodeData> {
        self.nodes.values().collect()
    }

    /// Remove a node (and any edges touching it). Returns true if removed.
    pub fn remove_node(&mut self, id: &str) -> bool {
        let existed = self.nodes.remove(id).is_some();
        if existed {
            self.edges.retain(|(s, d, _), _| s != id && d != id);
            // Drop the node's index entries; other nodes' entries that pointed
            // at it are cleaned lazily by the retain in clear_edges, or on the
            // next out_edges/in_edges lookup (which checks edges.contains_key).
            self.out_index.remove(id);
            self.in_index.remove(id);
            for v in self.out_index.values_mut() {
                v.retain(|k| self.edges.contains_key(k));
            }
            for v in self.in_index.values_mut() {
                v.retain(|k| self.edges.contains_key(k));
            }
        }
        existed
    }

    /// Remove many nodes at once (e.g. every stale node of an incremental
    /// re-crawl). Returns the count removed. The endpoint-index cleanup runs
    /// ONCE for the whole batch — per-node `remove_node` costs an O(V) index
    /// retain each call, which goes quadratic on mass deletes.
    pub fn remove_nodes_batch(&mut self, ids: &[String]) -> usize {
        let dead: HashSet<&String> = ids.iter().collect();
        let mut removed = 0usize;
        let mut any = false;
        for id in &dead {
            if self.nodes.remove(*id).is_some() {
                removed += 1;
                any = true;
            }
        }
        if any {
            self.edges
                .retain(|(s, d, _), _| !dead.contains(s) && !dead.contains(d));
            let retain = |keys: &mut Vec<(String, String, String)>| {
                keys.retain(|k| self.edges.contains_key(k));
            };
            for v in self.out_index.values_mut() {
                retain(v);
            }
            for v in self.in_index.values_mut() {
                retain(v);
            }
            for id in &dead {
                self.out_index.remove(*id);
                self.in_index.remove(*id);
            }
        }
        removed
    }

    // ---- edges -----------------------------------------------------------

    pub fn add_edge(&mut self, edge: EdgeData) {
        let key = (edge.src.clone(), edge.dst.clone(), edge.edge_type.clone());
        // Maintain the endpoint index — but only for genuinely NEW keys.
        // Re-adding an existing edge replaces its EdgeData in `self.edges`;
        // pushing again would leave N copies of the key here, so
        // out_edges/in_edges would yield the same edge N times and downstream
        // counters (plan_edit dependents, snapshot refs lists) double-count.
        if !self.edges.contains_key(&key) {
            self.out_index
                .entry(edge.src.clone())
                .or_default()
                .push(key.clone());
            self.in_index
                .entry(edge.dst.clone())
                .or_default()
                .push(key.clone());
        }
        self.edges.insert(key, edge);
    }

    pub fn get_edge(&self, src: &str, dst: &str, edge_type: &str) -> Option<&EdgeData> {
        self.edges
            .get(&(src.to_string(), dst.to_string(), edge_type.to_string()))
    }

    /// Every edge, optionally filtered by type.
    pub fn edges(&self, edge_type: Option<&str>) -> Vec<&EdgeData> {
        self.edges
            .values()
            .filter(|e| edge_type.is_none_or(|t| e.edge_type == t))
            .collect()
    }

    pub fn all_edges(&self) -> Vec<&EdgeData> {
        self.edges.values().collect()
    }

    /// O(degree) via the endpoint index (was a full O(E) scan per call).
    pub fn out_edges(&self, node_id: &str, edge_type: Option<&str>) -> Vec<&EdgeData> {
        self.out_index
            .get(node_id)
            .map(|keys| {
                keys.iter()
                    .filter_map(|k| self.edges.get(k))
                    .filter(|e| edge_type.is_none_or(|t| e.edge_type == t))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// O(degree) via the endpoint index (was a full O(E) scan per call).
    pub fn in_edges(&self, node_id: &str, edge_type: Option<&str>) -> Vec<&EdgeData> {
        self.in_index
            .get(node_id)
            .map(|keys| {
                keys.iter()
                    .filter_map(|k| self.edges.get(k))
                    .filter(|e| edge_type.is_none_or(|t| e.edge_type == t))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Remove every edge of a dimension. Returns the count removed (so the
    /// Python wrapper can emit `reset_dim` with the right number).
    pub fn clear_edges(&mut self, edge_type: &str) -> usize {
        let before = self.edges.len();
        self.edges.retain(|(_, _, t), _| t != edge_type);
        let removed = before - self.edges.len();
        // Rebuild the index cheaply: drop index entries whose key was removed.
        // (A full rebuild would be O(E); this is O(removed) amortized.)
        let retain = |keys: &mut Vec<(String, String, String)>| {
            keys.retain(|k| self.edges.contains_key(k));
        };
        for v in self.out_index.values_mut() {
            retain(v);
        }
        for v in self.in_index.values_mut() {
            retain(v);
        }
        removed
    }

    // ---- stats / convergence -------------------------------------------

    pub fn counts(&self) -> Value {
        let mut by_type: HashMap<&str, usize> = [
            ("contains", 0),
            ("references", 0),
            ("duplicate_of", 0),
            ("similar_to", 0),
        ]
        .into_iter()
        .collect();
        for e in self.edges.values() {
            *by_type.entry(e.edge_type.as_str()).or_insert(0) += 1;
        }
        json!({
            "nodes": self.nodes.len(),
            "edges": self.edges.len(),
            "edges_contains": by_type["contains"],
            "edges_references": by_type["references"],
            "edges_duplicate_of": by_type["duplicate_of"],
            "edges_similar_to": by_type["similar_to"],
        })
    }

    /// Deterministic fingerprint. Parity with `graph.py`'s `signature()`:
    /// sha256 over (num_nodes | sorted (src,dst,type,round(weight,6)) tuples),
    /// each tuple fed as Python `repr`-style bytes + b";".
    pub fn signature(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(self.nodes.len().to_string().as_bytes());
        h.update(b"|");
        let mut edge_tuples: Vec<(&str, &str, &str, f64)> = self
            .edges
            .values()
            .map(|e| {
                (
                    e.src.as_str(),
                    e.dst.as_str(),
                    e.edge_type.as_str(),
                    round6(e.weight),
                )
            })
            .collect();
        edge_tuples.sort_by(|a, b| {
            a.0.cmp(b.0)
                .then_with(|| a.1.cmp(b.1))
                .then_with(|| a.2.cmp(b.2))
                .then_with(|| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        });
        for t in &edge_tuples {
            // Parity: Python does h.update(repr(tup).encode()) per edge + b";".
            // repr of a 4-tuple (str,str,str,float) -> ('a','b','contains',1.0)
            // with a space after each comma (Python's tuple repr).
            h.update(format!("('{}', '{}', '{}', {})", t.0, t.1, t.2, repr_float(t.3)).as_bytes());
            h.update(b";");
        }
        let bytes = h.finalize();
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    // ---- multi-root: attach + merge --------------------------------------

    pub fn attach_root(&mut self, root: &str) {
        if !root.is_empty() {
            self.roots.insert(root.to_string());
        }
    }

    /// Absorb `other` into this store, re-keying ids into our namespace by
    /// absolute path. Parity with `graph.py`'s `merge()`.
    ///
    /// Nodes whose absolute path is NOT under our root are skipped. Edge
    /// endpoints are rewritten. Returns a summary JSON dict.
    pub fn merge(&mut self, other: &GraphStore) -> Value {
        if self.root.is_empty() {
            // Nothing to re-key against; copy verbatim.
            let mut absorbed_nodes = 0usize;
            for n in &other.nodes {
                self.add_node(n.1.clone());
                absorbed_nodes += 1;
            }
            let absorbed_edges = other.edges.len();
            for e in &other.edges {
                self.add_edge(e.1.clone());
            }
            return json!({
                "absorbed_nodes": absorbed_nodes,
                "absorbed_edges": absorbed_edges,
                "rekeyed": 0,
                "skipped": 0,
            });
        }

        let my_root = match Path::new(&self.root).canonicalize() {
            Ok(p) => p,
            Err(_) => Path::new(&self.root).to_path_buf(),
        };

        // Map: old id -> new id, by absolute path under our root.
        let mut rekey: HashMap<String, String> = HashMap::new();
        let mut skipped = 0usize;
        for n in other.nodes.values() {
            let p = Path::new(&n.path);
            // Resolve without following symlinks (parity with Python's .resolve()
            // on the path string; here we just canonicalize the parent chain).
            let canon = match p.canonicalize() {
                Ok(c) => c,
                Err(_) => p.to_path_buf(),
            };
            match canon.strip_prefix(&my_root) {
                Ok(rel) => {
                    let new_id: String = rel
                        .components()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("/");
                    rekey.insert(n.id.clone(), new_id);
                }
                Err(_) => {
                    skipped += 1;
                }
            }
        }

        let mut absorbed_nodes = 0usize;
        for n in other.nodes.values() {
            let new_id = match rekey.get(&n.id) {
                Some(id) => id.clone(),
                None => continue,
            };
            if !self.nodes.contains_key(&new_id) {
                let mut merged = n.clone();
                merged.id = new_id;
                if n.root.is_empty() && !other.root.is_empty() {
                    merged.root = other.root.clone();
                }
                self.add_node(merged);
                absorbed_nodes += 1;
            } else if !n.root.is_empty() {
                self.attach_root(&n.root);
            }
        }

        let mut absorbed_edges = 0usize;
        for e in other.edges.values() {
            let new_src = match rekey.get(&e.src) {
                Some(id) => id.clone(),
                None => continue,
            };
            let new_dst = match rekey.get(&e.dst) {
                Some(id) => id.clone(),
                None => continue,
            };
            // Skip duplicates (parent already has the same edge).
            if self.get_edge(&new_src, &new_dst, &e.edge_type).is_some() {
                continue;
            }
            let mut new_edge = e.clone();
            new_edge.src = new_src;
            new_edge.dst = new_dst;
            self.add_edge(new_edge);
            absorbed_edges += 1;
        }

        if !other.root.is_empty() {
            self.attach_root(&other.root);
        }
        for r in &other.roots {
            self.attach_root(r);
        }

        json!({
            "absorbed_nodes": absorbed_nodes,
            "absorbed_edges": absorbed_edges,
            "rekeyed": rekey.len(),
            "skipped": skipped,
        })
    }

    // ---- (de)serialization helpers --------------------------------------

    /// Snapshot the whole store as a GraphSnapshot (for dispatch ops that
    /// expect the snapshot shape, and for save/load).
    pub fn to_snapshot(&self) -> crate::GraphSnapshot {
        crate::GraphSnapshot {
            root: self.root.clone(),
            nodes: self.nodes.values().cloned().collect(),
            edges: self.edges.values().cloned().collect(),
            meta: self.meta.clone(),
            // A store round-trip performs no fs ops of its own; pass-level
            // warnings are reported by the passes / run_convergence instead.
            warnings: Vec::new(),
        }
    }

    /// Load a snapshot into a fresh store (used by Python to hydrate from
    /// a Rust crawl result or from a persisted snapshot).
    pub fn from_snapshot(snap: &crate::GraphSnapshot) -> Self {
        let mut s = GraphStore::with_root(snap.root.clone());
        for n in &snap.nodes {
            s.add_node(n.clone());
        }
        for e in &snap.edges {
            s.add_edge(e.clone());
        }
        for (k, v) in &snap.meta {
            s.meta.insert(k.clone(), v.clone());
        }
        s
    }
}

// round6/repr_float live in lib.rs (crate-shared); the private copies that
// used to sit here were folded into those.

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn edge(src: &str, dst: &str, edge_type: &str, weight: f64) -> EdgeData {
        EdgeData {
            src: src.to_string(),
            dst: dst.to_string(),
            edge_type: edge_type.to_string(),
            weight,
            attrs: HashMap::new(),
        }
    }

    /// FINDING B regression: re-adding an existing (src,dst,type) key must
    /// replace the edge in place, NOT leave duplicate keys in the endpoint
    /// indexes (which made out_edges/in_edges yield N copies and downstream
    /// counters double-count).
    #[test]
    fn add_edge_twice_yields_single_index_entry() {
        let mut s = GraphStore::new();
        s.add_edge(edge("a.py", "b.py", "references", 1.0));
        s.add_edge(edge("a.py", "b.py", "references", 0.5)); // re-add, new weight

        // Exactly one entry from each endpoint's perspective.
        let out = s.out_edges("a.py", None);
        assert_eq!(out.len(), 1, "out_index must hold exactly one key");
        assert_eq!(out[0].weight, 0.5, "re-add must replace the payload");
        let inn = s.in_edges("b.py", None);
        assert_eq!(inn.len(), 1, "in_index must hold exactly one key");

        // The canonical maps agree: one edge total, counted once.
        assert_eq!(s.edges(Some("references")).len(), 1);
        assert_eq!(s.counts()["edges_references"], 1);
        assert_eq!(s.get_edge("a.py", "b.py", "references").unwrap().weight, 0.5);

        // A genuinely new edge still appends normally.
        s.add_edge(edge("a.py", "c.py", "references", 1.0));
        assert_eq!(s.out_edges("a.py", None).len(), 2);
        assert_eq!(s.in_edges("c.py", None).len(), 1);
        // ...and re-adding THAT one doesn't duplicate either.
        s.add_edge(edge("a.py", "c.py", "references", 0.25));
        assert_eq!(s.out_edges("a.py", None).len(), 2);
        assert_eq!(s.edges(Some("references")).len(), 2);
    }

    /// Same-key guard must be per-(src,dst,type): same pair under another type
    /// is a distinct key and gets its own index entries.
    #[test]
    fn add_edge_same_pair_different_type_both_indexed_once() {
        let mut s = GraphStore::new();
        s.add_edge(edge("a.py", "b.py", "references", 1.0));
        s.add_edge(edge("a.py", "b.py", "contains", 1.0));
        s.add_edge(edge("a.py", "b.py", "references", 1.0)); // dup of the first
        assert_eq!(s.out_edges("a.py", Some("references")).len(), 1);
        assert_eq!(s.out_edges("a.py", Some("contains")).len(), 1);
        assert_eq!(s.in_edges("b.py", Some("references")).len(), 1);
        assert_eq!(s.in_edges("b.py", Some("contains")).len(), 1);
    }

    /// clear_edges keeps working on top of the guarded index: removed keys are
    /// dropped from both endpoint indexes, surviving types stay queryable.
    #[test]
    fn clear_edges_after_readd_keeps_index_consistent() {
        let mut s = GraphStore::new();
        s.add_edge(edge("a.py", "b.py", "similar_to", 0.9));
        s.add_edge(edge("a.py", "b.py", "similar_to", 0.8)); // re-add pre-clear
        s.add_edge(edge("a.py", "b.py", "references", 1.0));
        assert_eq!(s.clear_edges("similar_to"), 1, "one logical edge removed");
        assert_eq!(s.out_edges("a.py", Some("similar_to")).len(), 0);
        assert_eq!(s.out_edges("a.py", Some("references")).len(), 1);
        assert_eq!(s.in_edges("b.py", Some("references")).len(), 1);
    }
}
