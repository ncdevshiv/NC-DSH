from dataworm.extractors import hashing, semantic


def test_simhash_identical_texts_distance_zero():
    text = "alpha beta gamma delta epsilon " * 10
    assert hashing.hamming_distance(hashing.simhash(text), hashing.simhash(text)) == 0


def test_simhash_near_duplicate_detected():
    base = "quantum flux capacitor resonance harmonic oscillator lattice " * 10
    tweaked = base + "extra"
    assert hashing.is_near_duplicate(hashing.simhash(base), hashing.simhash(tweaked))


def test_simhash_different_texts_far_apart():
    a = "quantum flux capacitor resonance harmonic oscillator " * 10
    b = "banana pancake recipe cooking kitchen butter flour sugar " * 10
    assert not hashing.is_near_duplicate(hashing.simhash(a), hashing.simhash(b))


def test_tfidf_cosine_identical_is_one():
    embedder = semantic.TfidfEmbedder()
    docs = [
        "graph node edge link discovery context semantic vector",
        "graph node edge link discovery context semantic vector",
        "cooking recipe banana pancake kitchen flour butter",
    ]
    vecs = embedder.embed(docs)
    assert semantic.cosine(vecs[0], vecs[1]) > 0.99
    assert semantic.cosine(vecs[0], vecs[2]) < 0.2


def test_cosine_empty_vectors_is_zero():
    assert semantic.cosine({}, {1: 0.5}) == 0.0
