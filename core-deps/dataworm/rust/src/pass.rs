//! The convergence loop, all in Rust.
//!
//! Ports `dataworm/engine.py`'s `run(config, max_cycles)`: crawl → reference_pass
//! → hashing_pass → semantic_pass per cycle, checking the graph signature for a
//! fixed point. The whole loop runs in Rust with zero Python↔Rust crossings per
//! cycle; Python calls this single op, then replays the returned event log so
//! the live dashboard animates (the bus stays Python; the compute stays Rust).
//!
//! Also hosts the stateful `hashing_pass` (the only pass not yet on the Rust
//! store — exact + near duplicates, mirroring `engine.hashing_pass`).

use std::collections::{HashMap, HashSet};
use std::fs;

use serde_json::{json, Value};

use crate::store::GraphStore;
use crate::{NodeData, PassWarning};

// ---- hashing_pass (stateful, parity with engine.hashing_pass) -------------

/// Exact + near duplicate_of pass (parity with `engine.hashing_pass`).
///
/// `warnings` collects {"op":"read"} PassWarnings for files that could not be
/// read_to_string'd (they are skipped exactly as before — outputs unchanged).
pub fn hashing_pass(
    store: &mut GraphStore,
    max_content_bytes: u64,
    text_extensions: &[String],
    max_hashing_nodes: usize,
    warnings: &mut Vec<PassWarning>,
) -> Value {
    let removed = store.clear_edges("duplicate_of");

    // Snapshot file nodes (avoid borrowing store across the loop).
    let file_nodes: Vec<NodeData> = store.nodes(Some("file")).into_iter().cloned().collect();

    // Exact duplicates: group by content_hash.
    let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
    for n in &file_nodes {
        if !n.content_hash.is_empty() {
            by_hash
                .entry(n.content_hash.clone())
                .or_default()
                .push(n.id.clone());
        }
    }
    let mut added: Vec<(String, String)> = Vec::new();
    // Shared exact-edge builder (same core as the stateless lib::hash_pass).
    for edge in crate::exact_duplicate_edges(&by_hash) {
        added.push((edge.src.clone(), edge.dst.clone()));
        store.add_edge(edge);
    }

    // Near duplicates: simhash over text files, compare within hamming distance 3.
    // Fingerprint lookup is memoized by content_hash (parity with
    // engine.hashing_pass): a hit skips the read entirely; a miss computes and
    // stores (only when the hash is known). Empty-text files are skipped
    // WITHOUT a memo entry, exactly like the Python pass.
    let mut fingerprints: Vec<(String, u64)> = Vec::new();
    for n in &file_nodes {
        if !crate::is_text(&n.id, text_extensions) || n.size > max_content_bytes {
            continue;
        }
        let fp = if !n.content_hash.is_empty() {
            if let Some(cached) = store.memo_simhash.get(&n.content_hash) {
                *cached
            } else {
                let text = match fs::read_to_string(&n.path) {
                    Ok(t) if t.len() as u64 <= max_content_bytes => t,
                    Ok(_) => continue, // over the size cap: policy skip, not a failure
                    Err(e) => {
                        warnings.push(PassWarning {
                            path: n.path.clone(),
                            op: "read".to_string(),
                            error: e.to_string(),
                        });
                        continue;
                    }
                };
                if text.is_empty() {
                    continue;
                }
                let fp = crate::simhash(&text);
                store.memo_simhash.insert(n.content_hash.clone(), fp);
                fp
            }
        } else {
            let text = match fs::read_to_string(&n.path) {
                Ok(t) if t.len() as u64 <= max_content_bytes => t,
                Ok(_) => continue, // over the size cap: policy skip, not a failure
                Err(e) => {
                    warnings.push(PassWarning {
                        path: n.path.clone(),
                        op: "read".to_string(),
                        error: e.to_string(),
                    });
                    continue;
                }
            };
            if text.is_empty() {
                continue;
            }
            crate::simhash(&text)
        };
        fingerprints.push((n.id.clone(), fp));
    }

    // Cap the O(n^2) near-duplicate compare: sort by id (deterministic) and
    // truncate to `max_hashing_nodes`. Mirrors the `max_semantic_nodes` cap —
    // at scale, near-duplicate detection becomes approximate rather than
    // blocking the crawl for minutes-to-hours.
    fingerprints.sort_by(|a, b| a.0.cmp(&b.0));
    fingerprints.truncate(max_hashing_nodes);

    // Track pairs already linked (exact) to skip, parity with the Python dedup.
    let linked: HashSet<(String, String)> = added
        .iter()
        .flat_map(|(s, d)| [(s.clone(), d.clone()), (d.clone(), s.clone())])
        .collect();

    // Shared near-edge core — band-indexed candidate generation + exact
    // hamming verification live there (see lib.rs::near_duplicate_edges).
    for edge in crate::near_duplicate_edges(&fingerprints, &linked) {
        added.push((edge.src.clone(), edge.dst.clone()));
        store.add_edge(edge);
    }

    json!({ "removed": removed, "added_edges": added })
}

// ---- the convergence loop -------------------------------------------------

/// Run crawl + reference + hashing + semantic passes until the signature is
/// stable (a fixed point) or `max_cycles` is hit. Mutates `store` **in place**
/// — the live Rust GraphStore — so there is no snapshot round-trip and the
/// convergence edges (refs/dup/similar) are written straight to the store the
/// daemon holds. Returns an event log + final counts so Python can replay
/// every `pass`/`cycle`/`done` bus event in one shot.
///
/// The event log's shapes mirror the engine's bus emissions so the Python
/// replay is a straight translation.
// Argument list mirrors the PyO3 method's kwargs 1:1 (the JSON/wire contract
// with dataworm.engine) — restructuring it would ripple across both sides.
#[allow(clippy::too_many_arguments)]
pub fn run_convergence(
    store: &mut GraphStore,
    max_cycles: usize,
    max_content_bytes: u64,
    text_extensions: Vec<String>,
    max_semantic_nodes: usize,
    similarity_threshold: f64,
    enable_semantic: bool,
    enable_hashing: bool,
    max_hashing_nodes: usize,
) -> Value {
    let root = store.root.clone();
    let mut events: Vec<Value> = Vec::new();
    events.push(json!({ "kind": "start", "root": root, "max_cycles": max_cycles }));
    // Aggregated across ALL passes across ALL cycles; capped once at the end.
    let mut warnings: Vec<PassWarning> = Vec::new();

    let mut prev_sig: Option<String> = None;
    let mut cycles = 0usize;
    let mut converged = false;

    for cycle in 0..max_cycles {
        cycles = cycle + 1;

        // references
        events.push(
            json!({ "kind": "pass", "name": "references", "cycle": cycles, "status": "start" }),
        );
        let refs_result =
            crate::refs::reference_pass(store, max_content_bytes, &text_extensions, &mut warnings);
        events.push(json!({ "kind": "refs_result", "data": refs_result }));
        events.push(
            json!({ "kind": "pass", "name": "references", "cycle": cycles, "status": "end" }),
        );

        // hashing
        if enable_hashing {
            events.push(
                json!({ "kind": "pass", "name": "hashing", "cycle": cycles, "status": "start" }),
            );
            let hash_result = hashing_pass(
                store,
                max_content_bytes,
                &text_extensions,
                max_hashing_nodes,
                &mut warnings,
            );
            events.push(json!({ "kind": "hash_result", "data": hash_result }));
            events.push(
                json!({ "kind": "pass", "name": "hashing", "cycle": cycles, "status": "end" }),
            );
        }

        // semantic
        if enable_semantic {
            events.push(
                json!({ "kind": "pass", "name": "semantic", "cycle": cycles, "status": "start" }),
            );
            let sem_result = crate::semantic::semantic_pass(
                store,
                max_content_bytes,
                &text_extensions,
                max_semantic_nodes,
                similarity_threshold,
                &mut warnings,
            );
            events.push(json!({ "kind": "sem_result", "data": sem_result }));
            events.push(
                json!({ "kind": "pass", "name": "semantic", "cycle": cycles, "status": "end" }),
            );
        }

        let sig = store.signature();
        events.push(json!({ "kind": "cycle", "n": cycles, "signature": sig }));
        if Some(&sig) == prev_sig.as_ref() {
            converged = true;
            break;
        }
        prev_sig = Some(sig);
    }

    let counts = store.counts();
    events.push(json!({
        "kind": "done",
        "converged": converged,
        "cycles": cycles,
        "counts": counts,
        "root": root,
    }));

    // No `final_snapshot`: the store was mutated in place, so Python's handle
    // already sees the post-convergence graph. Returning the whole graph as
    // JSON here would re-introduce the OOM spike we just removed.
    json!({
        "converged": converged,
        "cycles": cycles,
        "root": root,
        "counts": counts,
        "events": events,
        // Every fs failure from every pass of every cycle, oldest-first,
        // capped at 1000 + one synthetic "truncated" tail. ALWAYS present.
        "warnings": crate::cap_warnings(warnings),
    })
}
