from __future__ import annotations

import hashlib
from pathlib import Path

from moli_frontend_smoke.config import sha256_fixture_tree


def test_fixture_tree_hash_uses_relative_paths_bytes_and_excludes_metadata(
    tmp_path: Path,
) -> None:
    (tmp_path / "nested").mkdir()
    (tmp_path / "a.txt").write_bytes(b"alpha")
    (tmp_path / "nested" / "b.bin").write_bytes(b"\x00beta")
    (tmp_path / "manifest.json").write_text("ignored manifest")
    (tmp_path / "metafile.json").write_text("ignored metafile")

    expected = hashlib.sha256()
    for relative, content in (
        ("a.txt", b"alpha"),
        ("nested/b.bin", b"\x00beta"),
    ):
        expected.update(relative.encode("utf-8"))
        expected.update(b"\0")
        expected.update(content)
        expected.update(b"\0")

    assert sha256_fixture_tree(tmp_path) == expected.hexdigest()
