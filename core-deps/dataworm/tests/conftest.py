"""Shared fixtures: builds a small sample tree that exercises every edge type."""

from __future__ import annotations

from pathlib import Path

import pytest

from dataworm.config import Config
from dataworm.engine import run

# Rich, distinctive shared text so TF-IDF / simhash see real overlap.
_SHARED_PARA = (
    "quantum flux capacitor resonance harmonic oscillator lattice "
    "eigenvalue manifold tensor gradient descent optimization convergence "
    "trajectory manifold embedding vector semantic discovery linkage graph "
) * 4


def build_sample_tree(root: Path) -> Path:
    """Create a sample project tree under ``root`` and return its path."""
    root = root / "sample"
    (root / "utils").mkdir(parents=True)
    (root / "docs").mkdir(parents=True)

    # Reference chain: a -> b -> c  (clean blast-radius test).
    (root / "a.py").write_text("import b\n\nprint('a')\n", encoding="utf-8")
    (root / "b.py").write_text("import c\n\nprint('b')\n", encoding="utf-8")
    (root / "c.py").write_text("print('c')\n", encoding="utf-8")

    # A helper referenced across the tree.
    (root / "utils" / "helper.py").write_text(
        "def help():\n    return 42\n", encoding="utf-8"
    )

    # Markdown links: relative file link + parent-dir link.
    (root / "docs" / "readme.md").write_text(
        "# Docs\n\nSee [the guide](guide.md) and [entry](../a.py).\n",
        encoding="utf-8",
    )
    (root / "docs" / "guide.md").write_text("# Guide\n\ncontent here\n", encoding="utf-8")

    # Exact duplicates (identical bytes).
    (root / "dup1.txt").write_text(_SHARED_PARA, encoding="utf-8")
    (root / "dup2.txt").write_text(_SHARED_PARA, encoding="utf-8")

    # A directory that must be ignored.
    (root / "__pycache__").mkdir()
    (root / "__pycache__" / "junk.pyc").write_bytes(b"\x00\x01")

    return root


@pytest.fixture
def sample_root(tmp_path: Path) -> Path:
    return build_sample_tree(tmp_path)


@pytest.fixture
def sample_config(sample_root: Path) -> Config:
    return Config(root=str(sample_root))


@pytest.fixture
def sample_store(sample_config: Config):
    return run(sample_config, max_cycles=5)
