"""Duplicate detection: exact matches via sha256, near-duplicates via simhash."""

from __future__ import annotations

import hashlib
import re
from collections import defaultdict
from typing import Iterator, Sequence

_TOKEN = re.compile(r"\w+")
_HASHBITS = 64

# Simhash banding: a 64-bit fingerprint splits into BANDS disjoint slices of
# BAND_BITS bits. Pigeonhole guarantee for exact recall of hamming <= 3: with
# 4 bands, at most 3 differing bits can touch at most 3 bands, so any true
# near-duplicate pair agrees on at least one full band value. Indexing
# fingerprints by band value therefore recalls EVERY qualifying pair; the
# exact hamming check stays as the verifier.
BANDS = 4
BAND_BITS = 16


def _token_hash(token: str) -> int:
    digest = hashlib.md5(token.encode("utf-8", "ignore")).digest()
    return int.from_bytes(digest[:8], "big")


def simhash(text: str, hashbits: int = _HASHBITS) -> int:
    """64-bit locality-sensitive fingerprint of ``text``.

    Similar documents yield fingerprints with a small Hamming distance, which
    lets us detect near-duplicates without comparing full contents.
    """
    tokens = _TOKEN.findall(text.lower())
    if not tokens:
        return 0
    v = [0] * hashbits
    for token in tokens:
        h = _token_hash(token)
        for i in range(hashbits):
            if h & (1 << i):
                v[i] += 1
            else:
                v[i] -= 1
    fingerprint = 0
    for i in range(hashbits):
        if v[i] > 0:
            fingerprint |= (1 << i)
    return fingerprint


def hamming_distance(a: int, b: int) -> int:
    return bin(a ^ b).count("1")


def is_near_duplicate(a: int, b: int, max_distance: int = 3) -> bool:
    """Two fingerprints within ``max_distance`` bits are near-duplicates."""
    if a == 0 or b == 0:
        return False
    return hamming_distance(a, b) <= max_distance


def band_value(fp: int, band: int) -> int:
    """The 16-bit slice of ``fp`` for ``band`` in ``range(BANDS)``."""
    return (fp >> (band * BAND_BITS)) & 0xFFFF


def near_duplicate_candidates(fps: Sequence[int]) -> Iterator[tuple[int, int]]:
    """Yield index pairs ``(i, j)``, ``i < j``, sharing at least one band value.

    Exact-recall candidate generation for the O(n^2) near-duplicate compare
    (see the banding note above): every pair with ``hamming_distance <= 3``
    co-occurs in at least one bucket, so nothing qualifying is missed. Pairs
    may be yielded more than once (once per shared band) and the order is
    grouped-by-band — consumers dedupe and re-sort before emitting edges.
    """
    for band in range(BANDS):
        shift = band * BAND_BITS
        buckets: defaultdict[int, list[int]] = defaultdict(list)
        for idx, fp in enumerate(fps):
            buckets[(fp >> shift) & 0xFFFF].append(idx)
        for members in buckets.values():
            if len(members) < 2:
                continue
            for a in range(len(members) - 1):
                ia = members[a]
                for b in range(a + 1, len(members)):
                    yield ia, members[b]
