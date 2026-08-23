//! Property-based tests for the self-hosted statistical embeddings.
//!
//! Guarantees: embeddings are deterministic, whitespace-insensitive, and
//! unit-norm (L2) for any tokenizable text — the invariants that make them
//! usable as a drop-in semantic similarity source.

#![cfg(test)]

use proptest::prelude::*;

use crate::embeddings::EmbeddingsProvider;
use crate::statistical::StatisticalEmbeddings;

fn rt() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime builds")
    })
}

fn embed(text: &str) -> Vec<f32> {
    let embeddings = StatisticalEmbeddings::defaults();
    rt().block_on(embeddings.embed(&[text.to_string()]))
        .expect("statistical embeddings are infallible")
        .pop()
        .expect("exactly one embedding returned")
}

proptest! {
    /// The same text always embeds to the exact same vector.
    #[test]
    fn embeddings_are_deterministic(text in ".*") {
        prop_assert_eq!(embed(&text), embed(&text));
    }

    /// Non-empty texts embed to unit-length vectors (L2 norm 1), unless the
    /// text has no tokenizable content (punctuation-only, etc.).
    #[test]
    fn embeddings_are_unit_norm(text in ".+") {
        let v = embed(&text);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            prop_assert!((norm - 1.0).abs() < 1e-5, "norm {norm} for {text:?}");
        }
    }

    /// Whitespace never changes the embedding (the tokenizer ignores it).
    #[test]
    fn whitespace_does_not_change_embedding(text in "[a-zA-Z0-9 ]{0,60}") {
        let a = embed(&text);
        let b = embed(&format!("  \n{text}  \t"));
        prop_assert_eq!(a, b);
    }
}
