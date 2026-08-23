//! TF-IDF embedding + pairwise cosine semantic_pass, in Rust.
//!
//! Ports `dataworm/extractors/semantic.py`'s `TfidfEmbedder` (tokenize with
//! the same regex + stopwords, build a capped vocabulary dropping singletons,
//! TF-IDF weight, unit-normalise) and `cosine` (sparse dot product), then
//! `dataworm/engine.py`'s `semantic_pass`: clear `similar_to` edges, select
//! candidates (sorted by id, capped at `max_semantic_nodes`), embed, score
//! pairs from the exact-recall posting-list inverted index (`similar_pairs`,
//! parity with engine._similar_pairs), add edges.
//!
//! `sentence-transformers` (the `semantic` extra) stays in Python by nature;
//! when it's active, the Python embedder produces vectors which are fed to
//! `semantic_pass_from_vectors` here so the O(n^2) compare is always Rust.

use std::collections::HashMap;
use std::fs;

use regex::Regex;
use serde_json::{json, Value};

use crate::store::GraphStore;
use crate::{EdgeData, NodeData, PassWarning};

// Same token regex + stopwords as semantic.py.
static TOKEN_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
fn token_re() -> &'static Regex {
    TOKEN_RE.get_or_init(|| Regex::new(r"[a-zA-Z_][a-zA-Z0-9_]+").unwrap())
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "import", "from", "return", "def", "class", "this", "that",
    "self", "function", "const", "let", "var", "null", "true", "false", "none", "not", "are",
    "was", "were", "into", "out", "int", "str",
];

fn is_stopword(t: &str) -> bool {
    STOPWORDS.contains(&t)
}

fn tokenize(text: &str) -> Vec<String> {
    token_re()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|t| !is_stopword(t) && t.len() > 2)
        .collect()
}

/// A sparse vector: term_index -> weight, unit-normalised.
pub type Vector = HashMap<usize, f64>;

/// TF-IDF embedder with a capped vocabulary, parity with TfidfEmbedder.
struct TfidfEmbedder {
    vocab: HashMap<String, usize>,
    df: HashMap<String, usize>,
    n: usize,
    max_features: usize,
}

impl TfidfEmbedder {
    fn new(max_features: usize) -> Self {
        TfidfEmbedder {
            vocab: HashMap::new(),
            df: HashMap::new(),
            n: 0,
            max_features,
        }
    }

    fn fit(&mut self, texts: &[String]) {
        self.n = texts.len().max(1);
        let tokenized: Vec<Vec<String>> = texts.iter().map(|t| tokenize(t)).collect();
        // Document frequency.
        for tokens in &tokenized {
            let seen: std::collections::HashSet<&String> = tokens.iter().collect();
            for term in seen {
                *self.df.entry(term.clone()).or_insert(0) += 1;
            }
        }
        // Keep most informative terms: drop singletons, sort by (-df, term), cap.
        let mut vocab_terms: Vec<String> = self
            .df
            .iter()
            .filter(|(_, c)| **c > 1)
            .map(|(t, _)| t.clone())
            .collect();
        vocab_terms.sort_by(|a, b| self.df[b].cmp(&self.df[a]).then_with(|| a.cmp(b)));
        vocab_terms.truncate(self.max_features);
        self.vocab = vocab_terms
            .iter()
            .enumerate()
            .map(|(i, t)| (t.clone(), i))
            .collect();
    }

    fn embed(&self, texts: &[String]) -> Vec<Vector> {
        // Build a parallel df-by-index map so we don't reverse-lookup vocab.
        let df_by_idx: HashMap<usize, usize> = self
            .vocab
            .iter()
            .map(|(term, idx)| (*idx, *self.df.get(term).unwrap_or(&0)))
            .collect();
        texts
            .iter()
            .map(|t| {
                let tokens = tokenize(t);
                let mut tf: HashMap<usize, usize> = HashMap::new();
                for tok in &tokens {
                    if let Some(&idx) = self.vocab.get(tok) {
                        *tf.entry(idx).or_insert(0) += 1;
                    }
                }
                let mut vec: Vector = HashMap::new();
                for (idx, count) in tf {
                    let df = *df_by_idx.get(&idx).unwrap_or(&0) as f64;
                    let idf = ((1.0 + self.n as f64) / (1.0 + df)).ln() + 1.0;
                    vec.insert(idx, count as f64 * idf);
                }
                normalise(&vec)
            })
            .collect()
    }
}

fn normalise(vec: &Vector) -> Vector {
    let norm: f64 = vec.values().map(|w| w * w).sum::<f64>().sqrt();
    if norm == 0.0 {
        return HashMap::new();
    }
    vec.iter().map(|(i, w)| (*i, w / norm)).collect()
}

/// FNV-1a over the vocabulary's terms in index order. TF-IDF indices are
/// positional, so both membership and ordering matter: any change yields a
/// different fingerprint. Used to stamp cached embed vectors (store.memo_vocab)
/// so vectors from different vocabularies are never cosine-compared together —
/// comparing across vintages produced numerically meaningless similarities.
fn vocab_fingerprint(vocab: &HashMap<String, usize>) -> u64 {
    let mut ordered: Vec<&str> = vec![""; vocab.len()];
    for (term, &idx) in vocab {
        if idx < ordered.len() {
            ordered[idx] = term.as_str();
        }
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for t in &ordered {
        for b in t.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0u64; // term separator
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Cosine between two unit-normalised sparse vectors = dot product.
fn cosine(a: &Vector, b: &Vector) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    let mut dot = 0.0;
    for (i, w) in small {
        if let Some(w2) = large.get(i) {
            dot += w * w2;
        }
    }
    dot
}

/// The historical O(n^2) sweep: every pair through `cosine`. Parity with
/// engine._similar_pairs_full_sweep.
fn cosine_pairs_full_sweep(vectors: &[&Vector], threshold: f64) -> Vec<(usize, usize, f64)> {
    let mut out: Vec<(usize, usize, f64)> = Vec::new();
    for i in 0..vectors.len() {
        for j in (i + 1)..vectors.len() {
            let score = cosine(vectors[i], vectors[j]);
            if score >= threshold {
                out.push((i, j, score));
            }
        }
    }
    out
}

/// All pairs with `cosine >= threshold` as `(i, j, score)`, i < j, ascending
/// by (i, j) — byte-identical to the full nested loop. Parity with
/// engine._similar_pairs.
///
/// Exactness: vectors are unit-normalised, so a pair clearing a positive
/// threshold must share at least one nonzero-weight dimension; co-occurrence
/// in a dimension's posting list is therefore an exact-recall candidate
/// filter and each pair is still scored by the existing `cosine`. For
/// `threshold <= 0` no pruning is valid (two empty vectors score 0.0 and
/// qualify), so the full sweep runs. When posting lists are so dense that
/// candidates approach the full pairwise block (e.g. dense embeddings), the
/// plain sweep streams with less memory and identical output.
///
/// The density heuristic below mirrors Python EXACTLY so both backends take
/// identical code paths on identical inputs.
pub(crate) fn similar_pairs(vectors: &[&Vector], threshold: f64) -> Vec<(usize, usize, f64)> {
    if vectors.len() < 2 {
        return Vec::new();
    }
    if threshold <= 0.0 {
        return cosine_pairs_full_sweep(vectors, threshold);
    }

    let mut postings: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut non_empty = 0usize;
    for (i, vec) in vectors.iter().enumerate() {
        if !vec.is_empty() {
            non_empty += 1;
            for dim in vec.keys() {
                postings.entry(*dim).or_default().push(i);
            }
        }
    }
    if non_empty < 2 {
        return Vec::new();
    }

    // u64 arithmetic so the products can't overflow regardless of n.
    let brute_pairs = (non_empty as u64) * ((non_empty - 1) as u64) / 2;
    let co_occurrences: u64 = postings
        .values()
        .map(|m| (m.len() as u64) * ((m.len() - 1) as u64) / 2)
        .sum();
    if 2 * co_occurrences >= brute_pairs {
        // Index would visit >= half of all pairs anyway — sweep instead.
        return cosine_pairs_full_sweep(vectors, threshold);
    }

    // A pair whose score misses the threshold is re-scored when another shared
    // dim surfaces it again (harmless; mirrors Python's dict-membership guard).
    let mut checked: HashMap<(usize, usize), f64> = HashMap::new();
    for members in postings.values() {
        if members.len() < 2 {
            continue;
        }
        for a in 0..(members.len() - 1) {
            let i = members[a];
            for b in (a + 1)..members.len() {
                let key = (i, members[b]);
                if checked.contains_key(&key) {
                    continue;
                }
                let score = cosine(vectors[i], vectors[members[b]]);
                if score >= threshold {
                    checked.insert(key, score);
                }
            }
        }
    }
    let mut out: Vec<(usize, usize, f64)> =
        checked.into_iter().map(|(k, s)| (k.0, k.1, s)).collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    out
}

/// The full semantic_pass using the built-in TF-IDF embedder.
/// >= threshold, adds edges. Returns the added edges so Python can emit events.
///
/// `warnings` collects {"op":"read"} PassWarnings for files that could not be
/// read_to_string'd; those candidates still embed from "" exactly as before
/// (unwrap_or_default semantics preserved — outputs unchanged).
pub fn semantic_pass(
    store: &mut GraphStore,
    max_content_bytes: u64,
    text_extensions: &[String],
    max_semantic_nodes: usize,
    threshold: f64,
    warnings: &mut Vec<PassWarning>,
) -> Value {
    store.clear_edges("similar_to");

    // Select candidates: file nodes that are text and within size cap, sorted by id.
    let mut candidates: Vec<NodeData> = store
        .nodes(Some("file"))
        .into_iter()
        .filter(|n| crate::is_text(&n.id, text_extensions) && n.size <= max_content_bytes)
        .cloned()
        .collect();
    candidates.sort_by(|a, b| a.id.cmp(&b.id));
    candidates.truncate(max_semantic_nodes);
    if candidates.len() < 2 {
        return json!({ "removed": 0, "added_edges": [] });
    }

    // Content-addressed embedding reuse (parity with engine._embed_candidates)
    // WITH vocabulary-fingerprint validation. TF-IDF indices are positional: a
    // cached vector is only mutually comparable with vectors built against the
    // SAME fitted vocabulary. Because that vocabulary is only known after
    // `fit` over the miss slice, cached hits are grouped by their stored
    // fingerprint and later scored only within their group — cross-vintage
    // pairs (the old silent garbage above threshold 0.35) are never formed.
    // Entries lacking a fingerprint (legacy persisted memos, or any fresh
    // process — memo_vocab is deliberately not persisted across restarts) are
    // treated as misses and re-embedded.
    let mut vectors: Vec<Option<Vector>> = vec![None; candidates.len()];
    let mut miss_idx: Vec<usize> = Vec::new();
    let mut miss_texts: Vec<String> = Vec::new();
    // fingerprint -> candidate indices sharing that cached vocabulary
    let mut hit_groups: std::collections::BTreeMap<u64, Vec<usize>> = Default::default();
    for (i, n) in candidates.iter().enumerate() {
        let cached = if !n.content_hash.is_empty() {
            store.memo_embed.get(&n.content_hash).cloned()
        } else {
            None
        };
        match (cached, store.memo_vocab.get(&n.content_hash)) {
            (Some(vec), Some(&fp)) => {
                vectors[i] = Some(vec);
                hit_groups.entry(fp).or_default().push(i);
            }
            _ => {
                let text = match fs::read_to_string(&n.path) {
                    Ok(t) => t,
                    Err(e) => {
                        // Record, then embed from "" exactly like the old
                        // unwrap_or_default — byte-identical vectors.
                        warnings.push(PassWarning {
                            path: n.path.clone(),
                            op: "read".to_string(),
                            error: e.to_string(),
                        });
                        String::new()
                    }
                };
                miss_idx.push(i);
                miss_texts.push(text);
            }
        }
    }
    if !miss_texts.is_empty() {
        let mut embedder = TfidfEmbedder::new(4096);
        embedder.fit(&miss_texts);
        let fp_new = vocab_fingerprint(&embedder.vocab);
        let fresh = embedder.embed(&miss_texts);
        for (pos, vec) in fresh.into_iter().enumerate() {
            let i = miss_idx[pos];
            if !candidates[i].content_hash.is_empty() {
                store
                    .memo_embed
                    .insert(candidates[i].content_hash.clone(), vec.clone());
                store
                    .memo_vocab
                    .insert(candidates[i].content_hash.clone(), fp_new);
            }
            vectors[i] = Some(vec);
        }
        // The freshly embedded slice is one same-vocabulary group.
        hit_groups.insert(fp_new, miss_idx);
    }
    // Every slot is filled by construction: hits and fresh vectors merged back
    // into candidate order. (unwrap_or_default only guards the impossible case.)
    let vectors: Vec<Vector> = vectors.into_iter().map(|v| v.unwrap_or_default()).collect();

    // Score pairs WITHIN each same-vocabulary group only (see above), then
    // emit globally ordered by (i, j). A cold store produces exactly one group
    // covering every candidate in original order — byte-identical to the old
    // single sweep.
    let mut accepted: Vec<(usize, usize, f64)> = Vec::new();
    for members in hit_groups.values() {
        let slice: Vec<&Vector> = members.iter().map(|&i| &vectors[i]).collect();
        for (a, b, score) in similar_pairs(&slice, threshold) {
            accepted.push((members[a], members[b], score));
        }
    }
    accepted.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut added: Vec<(String, String, f64)> = Vec::new();
    for (i, j, score) in accepted {
        let w = crate::round6(score);
        store.add_edge(EdgeData {
            src: candidates[i].id.clone(),
            dst: candidates[j].id.clone(),
            edge_type: "similar_to".to_string(),
            weight: w,
            attrs: HashMap::new(),
        });
        added.push((candidates[i].id.clone(), candidates[j].id.clone(), w));
    }
    json!({ "removed": 0, "added_edges": added })
}

/// Pairwise compare over pre-computed vectors (for the sentence-transformers
/// path: embedding happens in Python, the O(n^2) compare is Rust). `vectors`
/// is a list of {node_id, vec} where vec is a {index: weight} dict.
pub fn semantic_pass_from_vectors(
    store: &mut GraphStore,
    items: &[(String, Vector)],
    threshold: f64,
) -> Value {
    store.clear_edges("similar_to");
    // Same exact-recall candidate core as the TF-IDF path (see similar_pairs).
    let vrefs: Vec<&Vector> = items.iter().map(|it| &it.1).collect();
    let mut added: Vec<(String, String, f64)> = Vec::new();
    for (i, j, score) in similar_pairs(&vrefs, threshold) {
        let w = crate::round6(score);
        store.add_edge(EdgeData {
            src: items[i].0.clone(),
            dst: items[j].0.clone(),
            edge_type: "similar_to".to_string(),
            weight: w,
            attrs: HashMap::new(),
        });
        added.push((items[i].0.clone(), items[j].0.clone(), w));
    }
    json!({ "removed": 0, "added_edges": added })
}

// The shared text filter is crate::is_text (lib.rs); the per-module copy that
// used to live here was removed with the is_text dedup.

/// Parse a Python dict {index: weight} into a Vector.
pub fn parse_vector(d: &Value) -> Vector {
    let mut v = HashMap::new();
    if let Some(obj) = d.as_object() {
        for (k, w) in obj {
            if let (Ok(idx), Some(weight)) = (k.parse::<usize>(), w.as_f64()) {
                v.insert(idx, weight);
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::GraphStore;
    use std::collections::HashMap as Map;

    fn node(id: &str, path: &str, hash: &str) -> NodeData {
        NodeData {
            id: id.to_string(),
            path: path.to_string(),
            kind: "file".to_string(),
            size: 32,
            mtime: 0.0,
            mime: "text/plain".to_string(),
            content_hash: hash.to_string(),
            root: String::new(),
            attrs: HashMap::new(),
        }
    }

    fn store_with(dir: &std::path::Path) -> GraphStore {
        let mut s = GraphStore::with_root(dir.to_string_lossy().to_string());
        let files = [
            ("a.py", "alpha beta gamma", "h-a"),
            ("b.py", "alpha beta gamma", "h-b"),
            ("c.py", "alpha beta zeta", "h-c"),
        ];
        for (name, body, hash) in files {
            let p = dir.join(name);
            fs::write(&p, body).unwrap();
            s.add_node(node(name, &p.to_string_lossy(), hash));
        }
        s
    }

    fn exts() -> Vec<String> {
        vec![".py".to_string()]
    }

    fn sim_edges(s: &GraphStore) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = s
            .edges(Some("similar_to"))
            .into_iter()
            .map(|e| (e.src.clone(), e.dst.clone()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn cold_then_warm_produce_identical_edges() {
        let dir = std::env::temp_dir().join(format!("dw_sem_cold_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mut s = store_with(&dir);
        let mut warns: Vec<PassWarning> = Vec::new();
        let r1 = semantic_pass(&mut s, 4096, &exts(), 50_000, 0.35, &mut warns);
        let e1 = sim_edges(&s);
        assert!(warns.is_empty(), "unexpected read warnings: {warns:?}");
        assert_eq!(e1.len(), 3, "a-b, a-c, b-c all mutually similar: {e1:?} | r1={r1}");

        // Warm rerun over the SAME store: every candidate is a fingerprinted
        // hit — grouping must not change the outcome.
        let r2 = semantic_pass(&mut s, 4096, &exts(), 50_000, 0.35, &mut Vec::new());
        let e2 = sim_edges(&s);
        assert_eq!(r1, r2);
        assert_eq!(e1, e2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cross_vintage_vectors_are_never_compared() {
        // THE regression test for the memo-vocabulary mixing bug: poison ONE
        // cached vector's vintage marker; it must be excluded from pairs with
        // the still-consistent group instead of producing garbage similarity.
        let dir = std::env::temp_dir().join(format!("dw_sem_vintage_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mut s = store_with(&dir);
        semantic_pass(&mut s, 4096, &exts(), 50_000, 0.35, &mut Vec::new());
        assert_eq!(sim_edges(&s).len(), 3);

        // Re-vintage ONLY a.py: move its cache entry to a different fp.
        let fp_a = *s.memo_vocab.get("h-a").unwrap();
        s.memo_vocab.insert("h-a".to_string(), fp_a ^ 0xdead_beef);

        semantic_pass(&mut s, 4096, &exts(), 50_000, 0.35, &mut Vec::new());
        let edges = sim_edges(&s);
        assert!(
            edges.iter().all(|(x, y)| x != "a.py" && y != "a.py"),
            "a.py must not pair against the other vintage group: {edges:?}"
        );
        assert!(
            edges.contains(&("b.py".to_string(), "c.py".to_string())),
            "consistent group b-c must still pair: {edges:?}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_entries_without_fingerprint_are_stale() {
        let dir = std::env::temp_dir().join(format!("dw_sem_legacy_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let mut s = store_with(&dir);
        // Pre-seed legacy-style memos: embed vectors present, no fingerprints
        // (what persisted state looks like across an upgrade).
        s.memo_embed.insert(
            "h-a".to_string(),
            [(0usize, 1.0)].into_iter().collect::<Map<_, _>>(),
        );
        semantic_pass(&mut s, 4096, &exts(), 50_000, 0.35, &mut Vec::new());
        // The legacy entry must have been re-embedded under a stamped fp...
        assert!(
            s.memo_vocab.contains_key("h-a"),
            "legacy entry must gain a fingerprint after re-embed"
        );
        // ...and produce the full mutual-similarity triangle like a cold run.
        assert_eq!(sim_edges(&s).len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }
}
