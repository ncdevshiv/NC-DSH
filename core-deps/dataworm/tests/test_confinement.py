"""Stage 1.1: the worm never mints nodes outside the crawl root.

The crawler is downward-only and never follows symlinks, but we also defend in
depth: an out-of-root path must never produce a node id. These tests pin that.
"""

from __future__ import annotations

from pathlib import Path

from dataworm.crawler import crawl
from dataworm.config import Config
from dataworm.graph import GraphStore


def test_crawl_confines_to_root(sample_config, tmp_path: Path) -> None:
    """No node id may resolve to a path outside the crawl root."""
    store = GraphStore()
    crawl(store, sample_config)

    root = Path(sample_config.root).resolve()
    for node in store.all_nodes():
        # node.path is absolute on disk; it must live under root.
        assert Path(node.path).resolve().is_relative_to(root), node.path


def test_out_of_root_path_rejected_by_rel_id(tmp_path: Path) -> None:
    """_rel_id raises for a path outside root rather than falling back to a basename."""
    from dataworm.crawler import _rel_id

    root = tmp_path / "root"
    outside = tmp_path / "elsewhere" / "sneaky.py"
    root.mkdir(parents=True)
    outside.parent.mkdir(parents=True)
    outside.touch()

    import pytest
    with pytest.raises(ValueError):
        _rel_id(root, outside)


def test_symlinked_dir_outside_root_is_not_crawled(tmp_path: Path) -> None:
    """A symlink pointing outside the root must not leak nodes into the graph."""
    root = tmp_path / "root"
    outside = tmp_path / "outside"
    outside.mkdir()
    (outside / "secret.py").write_text("print('hidden')\n", encoding="utf-8")
    root.mkdir()
    # Symlink inside root pointing to the outside dir.
    link = root / "link"
    try:
        link.symlink_to(outside, target_is_directory=True)
    except OSError:
        # Windows without dev mode / admin can't create symlinks; skip gracefully.
        import pytest
        pytest.skip("cannot create symlinks on this platform")

    store = GraphStore()
    crawl(store, Config(root=str(root)))

    # The symlink dir itself: os.scandir reports it but follow_symlinks=False
    # means is_dir() is False for a symlink -> it is skipped, never recorded.
    for node in store.all_nodes():
        assert "secret.py" not in node.path, node.path
        assert "outside" not in node.path, node.path
