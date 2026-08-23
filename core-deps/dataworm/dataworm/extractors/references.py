"""Content-reference extraction and resolution.

Extracts *raw* reference strings from a file's text (imports, relative path
literals, markdown links) in a language-aware way, then resolves them to real
node ids in the graph. Resolved targets become ``references`` edges; unresolved
ones are surfaced to the agent as dangling references.
"""

from __future__ import annotations

import posixpath
import re
from pathlib import PurePosixPath

from dataworm.graph import GraphStore
from dataworm.models import Node

# ---- raw reference patterns ---------------------------------------------

# import foo.bar / import a, b as x  — capture the whole (possibly comma
# separated) list; `_split_import_list` breaks it into module names.
_PY_IMPORT = re.compile(r"^\s*import\s+(.+)$", re.MULTILINE)
# from foo.bar import x  |  from .rel import x  |  from .. import x
_PY_FROM = re.compile(r"^\s*from\s+(\.{0,3}[\w\.]*)\s+import", re.MULTILINE)

# import ... from './x'  |  export ... from './x'
_JS_FROM = re.compile(r"""(?:import|export)[^'"]*?from\s*['"]([^'"]+)['"]""")
# require('./x')  |  import('./x')
_JS_CALL = re.compile(r"""(?:require|import)\s*\(\s*['"]([^'"]+)['"]\s*\)""")

# [text](target) — markdown links/images; strip #anchors and <> wrappers
_MD_LINK = re.compile(r"\]\(\s*<?([^)>\s]+)>?(?:\s+[^)]*)?\)")

# Generic: quoted relative path literals like "./foo/bar" or "../baz"
_GENERIC_REL = re.compile(r"""['"](\.{1,2}/[^'"\s]+)['"]""")

# Extensions tried when a reference has none (module/bare specifier resolution).
_RESOLVE_EXTS = (
    ".py", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
    ".md", ".json", ".html", ".txt", ".rst",
)
_INDEX_FILES = ("index.js", "index.ts", "index.jsx", "index.tsx", "__init__.py")


def extract_raw_references(node: Node, text: str) -> list[str]:
    """Return raw reference strings found in ``text`` based on the file type."""
    suffix = PurePosixPath(node.id).suffix.lower()
    refs: list[str] = []

    if suffix == ".py":
        refs += _py_references(node, text)
    elif suffix in {".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"}:
        refs += _JS_FROM.findall(text)
        refs += _JS_CALL.findall(text)
    elif suffix in {".md", ".markdown", ".rst"}:
        refs += _MD_LINK.findall(text)
        refs += _GENERIC_REL.findall(text)
    else:
        refs += _GENERIC_REL.findall(text)

    # De-duplicate while preserving order; drop noise.
    seen: set[str] = set()
    out: list[str] = []
    for raw in refs:
        raw = raw.strip()
        if not raw or raw in seen:
            continue
        if raw.startswith(("http://", "https://", "mailto:", "#", "data:")):
            continue
        seen.add(raw)
        out.append(raw)
    return out


def _split_import_list(stmt: str) -> list[str]:
    """Break an ``import`` statement body into module names.

    ``"a, b as x, c.d  # comment"`` -> ``["a", "b", "c.d"]``: drop the comment,
    split on commas, and keep each part's leading ``[A-Za-z0-9_.]`` token (the
    module, ignoring any ``as`` alias). Mirrors Rust's ``split_import_list``.
    """
    stmt = stmt.split("#", 1)[0]
    out: list[str] = []
    for part in stmt.split(","):
        m = re.match(r"^([A-Za-z0-9_\.]+)", part.strip())
        if m:
            out.append(m.group(1))
    return out


def _py_references(node: Node, text: str) -> list[str]:
    refs: list[str] = []
    for stmt in _PY_IMPORT.findall(text):
        refs += _split_import_list(stmt)
    for mod in _PY_FROM.findall(text):
        refs.append(mod)
    return refs


def _resolve_py(node: Node, raw: str) -> list[str]:
    """Resolve a python module specifier to candidate root-relative node ids.

    Handles three shapes:
      - ``from . import x`` / ``from ..pkg import y``  -> relative, climb (dots-1) levels
      - ``import pkg.mod`` / ``from pkg.mod import x`` -> absolute (root-relative)
      - ``import helper`` (bare, single segment)       -> try sibling first, then absolute

    The bare-single-segment case matters once a sub-network is merged into a
    parent: ``import helper`` from ``lib/core.py`` resolves to its sibling
    ``lib/helper.py`` in the parent namespace (it was ``helper.py`` when crawled
    standalone under ``lib/``). Without the sibling candidate, the merged
    reference would dangle.
    """
    dots = len(raw) - len(raw.lstrip("."))
    module = raw[dots:]
    base_dir = posixpath.dirname(node.id)

    if dots > 0:
        # Relative import: climb (dots - 1) package levels from the file's dir.
        start = base_dir
        for _ in range(dots - 1):
            start = posixpath.dirname(start)
        rel_module = module.replace(".", "/")
        prefix = posixpath.join(start, rel_module) if rel_module else start
    else:
        prefix = module.replace(".", "/")

    if not prefix:
        return []

    candidates = [prefix + ".py", posixpath.join(prefix, "__init__.py")]
    # Bare single-segment import (e.g. ``import helper``): also try the sibling
    # in the file's own directory. This is how Python resolves a module that
    # lives next to the importer inside a package.
    if dots == 0 and "/" not in module and base_dir:
        sibling = posixpath.join(base_dir, module)
        candidates += [sibling + ".py", posixpath.join(sibling, "__init__.py")]
    return candidates


def _resolve_pathlike(node: Node, raw: str) -> list[str]:
    """Resolve a relative-path literal (./x, ../x, x.md) to candidate ids."""
    raw = raw.split("#", 1)[0].split("?", 1)[0]
    if not raw:
        return []
    base_dir = posixpath.dirname(node.id)
    joined = posixpath.normpath(posixpath.join(base_dir, raw))
    if joined in (".", "") or joined.startswith(".."):
        return []

    candidates = [joined]
    if not PurePosixPath(joined).suffix:
        candidates += [joined + ext for ext in _RESOLVE_EXTS]
        candidates += [posixpath.join(joined, idx) for idx in _INDEX_FILES]
    return candidates


def resolve_reference(store: GraphStore, node: Node, raw: str) -> str | None:
    """Return the node id a raw reference points to, or None if unresolved."""
    suffix = PurePosixPath(node.id).suffix.lower()

    if suffix == ".py":
        candidates = _resolve_py(node, raw)
        # A bare relative path in a .py string literal can still be a file.
        if raw.startswith("."):
            candidates += _resolve_pathlike(node, raw)
    else:
        candidates = _resolve_pathlike(node, raw)

    for cand in candidates:
        if cand and store.has_node(cand):
            return cand
    return None
