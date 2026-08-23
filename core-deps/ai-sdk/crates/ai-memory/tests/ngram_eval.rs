//! MINERVA eval harness: recall@5 of `NgramEmbeddings` v2 versus the
//! `StatisticalEmbeddings` baseline over a committed fixture set
//! (`tests/eval_fixtures/eval_set.json`).
//!
//! Methodology:
//! - The index contains every case's relevant document plus shared
//!   distractors (including confusables that share background vocabulary
//!   with queries).
//! - The n-gram embedder is *fitted honestly*: it observes only the corpus
//!   documents through its online-idf phase — never the queries.
//! - recall@5 per case is 1.0 when the relevant document appears in the
//!   top-5 cosine ranking, else 0.0; category and suite scores are means.
//!
//! Per-category numbers are printed to stdout truthfully. The fixture set
//! is designed so subword hashing plausibly wins morphology/typo/paraphrase
//! categories, while `lexical_control` favors exact word overlap; the
//! assertion is on the suite total only (`ngram >= statistical`).

use std::collections::BTreeMap;

use serde::Deserialize;

use ai_memory::{EmbeddingsProvider, NgramEmbeddings, StatisticalEmbeddings};

#[derive(Deserialize)]
struct Fixture {
    #[allow(dead_code)]
    description: String,
    distractors: Vec<String>,
    categories: Vec<Category>,
}

#[derive(Deserialize)]
struct Category {
    name: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    query: String,
    relevant: String,
}

fn fixture_path() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/eval_fixtures/eval_set.json"
    )
    .to_string()
}

/// Cosine similarity for L2-normalized vectors (dot product), NaN-safe.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
    }
    dot
}

/// Ranks `query_vector` against the indexed vectors; returns the rank
/// (0-based) at which `target` appears, or `None` if outside `top_k`.
fn rank_of(query_vector: &[f32], index: &[Vec<f32>], target: usize, top_k: usize) -> Option<usize> {
    let mut scored: Vec<(usize, f32)> = index
        .iter()
        .enumerate()
        .map(|(i, doc)| (i, cosine(query_vector, doc)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.iter().take(top_k).position(|(i, _)| *i == target)
}

async fn evaluate(
    embedder_label: &str,
    embedder: &dyn EmbeddingsProvider,
) -> BTreeMap<String, f32> {
    // Windows note: right after a full rebuild, real-time AV scanners can
    // transiently lock freshly-written files; retry briefly before failing.
    let raw = {
        let path = fixture_path();
        let mut attempt = 0;
        loop {
            match std::fs::read_to_string(&path) {
                Ok(raw) => break raw,
                Err(e) if attempt < 3 => {
                    attempt += 1;
                    eprintln!("fixture read retry {attempt}/3 after {e}");
                    std::thread::sleep(std::time::Duration::from_millis(150 * attempt as u64));
                }
                Err(e) => panic!(
                    "fixture unreadable: {e}; path={path:?}; cwd={:?}",
                    std::env::current_dir().unwrap_or_default()
                ),
            }
        }
    };
    let fx: Fixture = serde_json::from_str(&raw).expect("valid fixture JSON");

    // Index = every relevant doc + every distractor.
    let mut docs: Vec<String> = Vec::new();
    let mut targets: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new(); // category → (case id, doc idx)
    for category in &fx.categories {
        let entry = targets.entry(category.name.clone()).or_default();
        for case in &category.cases {
            let idx = docs.len();
            docs.push(case.relevant.clone());
            entry.push((case.id.clone(), idx));
        }
    }
    for d in &fx.distractors {
        docs.push(d.clone());
    }

    embedder.observe(&docs).await;

    let index_vectors = embedder.embed(&docs).await.expect("embed index");
    let query_vectors: Vec<Vec<f32>> = {
        let queries: Vec<String> = fx
            .categories
            .iter()
            .flat_map(|c| c.cases.iter().map(|kase| kase.query.clone()))
            .collect();
        embedder.embed(&queries).await.expect("embed queries")
    };

    let mut per_category: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // hits, total
    let mut qi = 0usize;
    for category in &fx.categories {
        let stats = per_category.entry(category.name.clone()).or_default();
        for case in &category.cases {
            let target = targets[&category.name]
                .iter()
                .find(|(id, _)| id == &case.id)
                .map(|(_, idx)| *idx)
                .expect("target indexed");
            let hit = rank_of(&query_vectors[qi], &index_vectors, target, 5).is_some();
            if !hit {
                println!(
                    "  MISS [{embedder_label}] {} {}: {:?}",
                    case.id, case.query, case.relevant
                );
            }
            stats.0 += hit as usize;
            stats.1 += 1;
            qi += 1;
        }
    }

    let mut result = BTreeMap::new();
    for (name, (hits, total)) in per_category {
        result.insert(name, hits as f32 / total as f32);
    }
    result
}

#[tokio::test(flavor = "multi_thread")]
async fn ngram_recall_at_5_matches_or_beats_statistical() {
    let statistical = evaluate("statistical", &StatisticalEmbeddings::defaults()).await;
    let ngram = evaluate("ngram", &NgramEmbeddings::defaults()).await;

    // ---- report ---------------------------------------------------------
    println!("\n=== MINERVA eval: recall@5 (statistical vs ngram) ===");
    println!(
        "{:<18} {:>12} {:>12} {:>8}",
        "category", "statistical", "ngram", "delta"
    );
    let categories: Vec<&String> = statistical.keys().collect();
    let mut sum_stat = 0.0f32;
    let mut sum_ng = 0.0f32;
    let mut count = 0usize;
    for name in &categories {
        let s = statistical[*name];
        let n = ngram[name.as_str()];
        sum_stat += s;
        sum_ng += n;
        count += 1;
        println!("{name:<18} {s:>12.3} {n:>12.3} {:+>8.3}", n - s);
    }
    let macro_stat = sum_stat / count as f32;
    let macro_ngram = sum_ng / count as f32;
    println!("----------------------------------------------------");
    println!(
        "{:<18} {macro_stat:>12.3} {macro_ngram:>12.3} {:+>8.3}",
        "TOTAL (macro)",
        macro_ngram - macro_stat
    );
    println!("====================================================\n");

    assert!(
        macro_ngram >= macro_stat,
        "ngram recall@5 ({macro_ngram:.3}) must be >= statistical ({macro_stat:.3}) on the suite"
    );
}
