//! Property-based tests for RAG scoring and chunking.
//!
//! These properties pin down the mathematical invariants of the real
//! retrieval components: cosine similarity is a true metric (symmetric,
//! bounded, self-similar), BM25 scores are non-negative with exact zeros on
//! disjoint vocabularies, and chunking never loses document content — on
//! arbitrary text, including multi-byte UTF-8 and whitespace gaps.

#![cfg(test)]

use std::collections::HashSet;

use proptest::prelude::*;

use crate::chunking::{ChunkingStrategy, chunk_document};
use crate::hybrid::{bm25_score, cosine, tokenize};

/// Two equal-length f32 vectors: the second is generated at exactly the
/// first vector's length, so length is shared by construction instead of
/// filtered with `prop_assume!` (independent 1..32 sizes reject ~97% of
/// cases and trip proptest's global reject limit).
fn equal_len_vecs() -> impl Strategy<Value = (Vec<f32>, Vec<f32>)> {
    prop::collection::vec(-100.0f32..100.0, 1usize..32).prop_flat_map(|a| {
        let len = a.len();
        (Just(a), prop::collection::vec(-100.0f32..100.0, len..=len))
    })
}

proptest! {
    /// Cosine similarity is symmetric and bounded by 1.
    #[test]
    fn cosine_is_symmetric_and_bounded((a, b) in equal_len_vecs()) {
        prop_assume!(a.iter().any(|x| *x != 0.0));
        prop_assume!(b.iter().any(|x| *x != 0.0));
        let ab = cosine(&a, &b).unwrap();
        let ba = cosine(&b, &a).unwrap();
        prop_assert!((ab - ba).abs() < 1e-4, "symmetric: {ab} vs {ba}");
        prop_assert!(ab.abs() <= 1.0 + 1e-4, "bounded: {ab}");
    }

    /// A vector is similar to itself with score 1.
    #[test]
    fn cosine_self_is_one(v in prop::collection::vec(-100.0f32..100.0, 1..32)) {
        prop_assume!(v.iter().any(|x| *x != 0.0));
        let s = cosine(&v, &v).unwrap();
        prop_assert!((s - 1.0).abs() < 1e-4, "self similarity: {s}");
    }

    /// Cosine is only defined for equal-length vectors.
    #[test]
    fn cosine_requires_equal_lengths(
        a in prop::collection::vec(-10.0f32..10.0, 1..16),
        b in prop::collection::vec(-10.0f32..10.0, 1..16),
    ) {
        if a.len() != b.len() {
            prop_assert!(cosine(&a, &b).is_none());
        }
    }

    /// BM25 scores are never negative, and are exactly zero when the query
    /// shares no terms with the document.
    #[test]
    fn bm25_is_nonnegative_and_zero_without_overlap(
        query in "[a-z ]{0,40}",
        document in "[a-z ]{0,60}",
    ) {
        let score = bm25_score(&query, &document, 1.2, 0.75);
        prop_assert!(score >= 0.0, "negative score {score}");
        let query_terms: HashSet<String> = tokenize(&query).into_iter().collect();
        let doc_terms: HashSet<String> = tokenize(&document).into_iter().collect();
        if query_terms.is_disjoint(&doc_terms) {
            prop_assert_eq!(score, 0.0);
        }
    }

    /// Repeating a query term doubles its contribution to the score.
    #[test]
    fn bm25_repeated_query_term_doubles_score(query in "[a-z]{1,6}", document in "[a-z ]{0,30}") {
        let single = bm25_score(&query, &document, 1.2, 0.75);
        let doubled_query = format!("{query} {query}");
        let doubled = bm25_score(&doubled_query, &document, 1.2, 0.75);
        prop_assert!((doubled - 2.0 * single).abs() < 1e-3, "{doubled} vs 2*{single}");
    }

    /// The tokenizer emits only non-empty lowercase alphanumeric runs.
    #[test]
    fn tokenize_produces_clean_tokens(text in ".*") {
        for token in tokenize(&text) {
            prop_assert!(
                token.chars().all(|c| c.is_alphanumeric() && !c.is_uppercase()),
                "dirty token {token:?} from {text:?}"
            );
            prop_assert!(!token.is_empty());
        }
    }

    /// Fixed-size chunking must not lose any non-whitespace content, must
    /// never panic on multi-byte UTF-8, and chunk starts must point at the
    /// real position in the document.
    #[test]
    fn fixed_chunking_covers_all_content(
        document in ".*",
        size in 1usize..=32,
        overlap in 0usize..=31,
    ) {
        let chunks = chunk_document(&document, ChunkingStrategy::Fixed { size, overlap });
        let bytes = document.as_bytes();
        for chunk in &chunks {
            prop_assert!(!chunk.text.is_empty());
            prop_assert!(document.is_char_boundary(chunk.start));
            prop_assert!(
                document[chunk.start..].starts_with(&chunk.text),
                "chunk text not at its start offset"
            );
        }
        for pair in chunks.windows(2) {
            prop_assert!(pair[0].start < pair[1].start, "starts must strictly increase");
        }
        let mut covered = vec![false; document.len()];
        for chunk in &chunks {
            for slot in &mut covered[chunk.start..chunk.start + chunk.text.len()] {
                *slot = true;
            }
        }
        for (i, byte) in bytes.iter().enumerate() {
            if !byte.is_ascii_whitespace() {
                prop_assert!(covered[i], "byte {i} ({byte:?}) not covered by any chunk");
            }
        }
    }

    /// Sentence chunking reassembles the document exactly and chunk starts
    /// point at the true source offsets.
    #[test]
    fn sentence_chunking_reassembles_document(document in ".*\\.", max_size in 1usize..=200) {
        let chunks = chunk_document(&document, ChunkingStrategy::Sentence { max_size });
        let reassembled: String = chunks.iter().map(|c| c.text.as_str()).collect();
        prop_assert_eq!(&reassembled, &document);
        for chunk in &chunks {
            prop_assert!(!chunk.text.is_empty());
            prop_assert!(document.is_char_boundary(chunk.start));
            prop_assert!(
                document[chunk.start..].starts_with(&chunk.text),
                "chunk text not at its start offset"
            );
        }
    }
}
