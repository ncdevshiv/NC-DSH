"""Semantic similarity: pluggable embedders + cosine comparison.

The default :class:`TfidfEmbedder` is pure-python (no heavy deps) so the
semantic dimension works out of the box. Swap in :class:`SentenceTransformerEmbedder`
(install the ``semantic`` extra) for real embeddings — the interface is identical.

All vectors are sparse ``{term_index: weight}`` dicts, kept unit-normalised so
cosine similarity is just a dot product.
"""

from __future__ import annotations

import math
import re
from typing import Protocol

from dataworm.config import Config

Vector = dict[int, float]

_TOKEN = re.compile(r"[a-zA-Z_][a-zA-Z0-9_]+")
_STOPWORDS = {
    "the", "and", "for", "with", "import", "from", "return", "def", "class",
    "this", "that", "self", "function", "const", "let", "var", "null", "true",
    "false", "none", "not", "are", "was", "were", "into", "out", "int", "str",
}


class Embedder(Protocol):
    def embed(self, texts: list[str]) -> list[Vector]: ...


class TfidfEmbedder:
    """Dependency-free TF-IDF vectoriser over a capped vocabulary."""

    def __init__(self, max_features: int = 4096) -> None:
        self.max_features = max_features

    def embed(self, texts: list[str]) -> list[Vector]:
        tokenized = [self._tokens(t) for t in texts]
        df: dict[str, int] = {}
        for tokens in tokenized:
            for term in set(tokens):
                df[term] = df.get(term, 0) + 1

        # Keep the most informative terms (drop singletons, cap vocabulary).
        vocab_terms = sorted(
            (t for t, c in df.items() if c > 1),
            key=lambda t: (-df[t], t),
        )[: self.max_features]
        vocab = {term: i for i, term in enumerate(vocab_terms)}

        n = max(len(tokenized), 1)
        vectors: list[Vector] = []
        for tokens in tokenized:
            tf: dict[str, int] = {}
            for tok in tokens:
                if tok in vocab:
                    tf[tok] = tf.get(tok, 0) + 1
            vec: Vector = {}
            for term, count in tf.items():
                idf = math.log((1 + n) / (1 + df[term])) + 1.0
                vec[vocab[term]] = count * idf
            vectors.append(_normalise(vec))
        return vectors

    @staticmethod
    def _tokens(text: str) -> list[str]:
        return [
            t for t in (m.group(0).lower() for m in _TOKEN.finditer(text))
            if t not in _STOPWORDS and len(t) > 2
        ]


class SentenceTransformerEmbedder:
    """Real embeddings via sentence-transformers (requires the ``semantic`` extra)."""

    def __init__(self, model_name: str = "all-MiniLM-L6-v2") -> None:
        from sentence_transformers import SentenceTransformer  # lazy import

        self._model = SentenceTransformer(model_name)

    def embed(self, texts: list[str]) -> list[Vector]:
        dense = self._model.encode(texts, normalize_embeddings=True)
        return [
            {i: float(v) for i, v in enumerate(row) if v != 0.0}
            for row in dense
        ]


def get_embedder(config: Config) -> Embedder:
    """Pick the best available embedder for the config."""
    try:
        return SentenceTransformerEmbedder()
    except Exception:
        return TfidfEmbedder()


def cosine(a: Vector, b: Vector) -> float:
    """Cosine similarity between two unit-normalised sparse vectors."""
    if not a or not b:
        return 0.0
    if len(b) < len(a):
        a, b = b, a
    dot = sum(w * b.get(i, 0.0) for i, w in a.items())
    return dot  # vectors are unit length, so dot == cosine


def _normalise(vec: Vector) -> Vector:
    norm = math.sqrt(sum(w * w for w in vec.values()))
    if norm == 0:
        return {}
    return {i: w / norm for i, w in vec.items()}
