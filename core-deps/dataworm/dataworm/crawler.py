"""The worm: recursive downward traversal that builds nodes + `contains` edges.

Given a root directory, this walks *down* through every subfolder and file,
creating a node for each and a structural ``contains`` edge from each directory
to its immediate children. It never climbs above the root.
"""

from __future__ import annotations

import hashlib
import mimetypes
import os
import stat
from pathlib import Path

from dataworm.config import Config
from dataworm.graph import GraphStore
from dataworm.models import Edge, EdgeType, Node, NodeKind

# Hard recursion bound for _walk (mirrors Rust's MAX_DEPTH): the per-entry
# link gate below already bounds real trees, this bounds pathological ones.
MAX_DEPTH = 256


def _is_reparse_link(path: Path) -> bool:
    """True if ``path`` is a symlink or an NTFS reparse point (junction, …).

    Windows junctions are *not* ``is_symlink()``, but they do resolve() to
    their target — so gating descent on symlinks alone let a junction cycle
    recurse forever (and resolve() re-keyed the junction onto its target's
    node id). Reparse points are recorded as nodes but never descended into,
    mirroring the Rust traversal's symlink_metadata gate.
    """
    try:
        if path.is_symlink():
            return True
        st = os.stat(path, follow_symlinks=False)
    except OSError:
        return True  # unreadable: refuse to descend
    attrs = getattr(st, "st_file_attributes", 0)
    return bool(attrs & stat.FILE_ATTRIBUTE_REPARSE_POINT)


def _rel_id(root: Path, path: Path) -> str:
    """Canonical node id: root-relative path with forward slashes ('' for root).

    Raises ValueError if ``path`` is not inside ``root`` — this is the strict
    confinement guard. The worm never mints a node for anything outside the
    crawl root (no silent basename fallback); callers must skip out-of-root
    paths rather than record them.
    """
    rel = path.relative_to(root)   # raises ValueError if not under root
    parts = rel.parts
    return "/".join(parts)


def _sha256(path: Path, limit: int) -> str:
    h = hashlib.sha256()
    try:
        with open(path, "rb") as fh:
            while chunk := fh.read(65536):
                h.update(chunk)
                if limit and fh.tell() > limit:
                    break
    except OSError:
        return ""
    return h.hexdigest()


def _root_id(root: Path) -> str:
    """Globally unique node id for a fragment's crawl root.

    ``"#root:"`` + the root as an absolute forward-slash path (e.g.
    ``"#root:F:/proj/src"``). The empty id of the single-store era collapsed
    every federated fragment's root into one canvas node; this scheme keeps
    them distinct while staying collision-free against file ids (which never
    start with ``#``) and against cross-dir shadow ids (raw backslash paths).
    """
    return "#root:" + str(root).replace("\\", "/")


def crawl(store: GraphStore, config: Config) -> None:
    """Populate ``store`` with the full downward tree from ``config.root``.

    Idempotent per cycle: existing nodes with an unchanged mtime keep their
    cached content hash so repeated cycles stay cheap (incremental by mtime).
    """
    root = Path(config.root).resolve()
    store.root = str(root)
    if not store.roots:
        store.roots.add(str(root))

    # Root node.
    root_stat = root.stat()
    store.add_node(Node(
        id=_root_id(root),
        path=str(root),
        kind=NodeKind.DIR,
        mtime=root_stat.st_mtime,
        root=str(root),
    ))

    _walk(store, config, root, root)


def crawl_shallow(store: GraphStore, config: Config) -> list[str]:
    """Crawl only the files *directly* in ``config.root`` (no recursion into
    subdirs). Records immediate subdir nodes + ``contains`` edges, but does NOT
    descend into them — the caller gives each subdir its own fragment store.

    Returns the list of immediate subdir absolute paths (so the caller can
    crawl each one fully). This is the federated split: the root fragment holds
    only top-level files + subdir markers; each subdir is a separate fragment.
    """
    root = Path(config.root).resolve()
    store.root = str(root)
    if not store.roots:
        store.roots.add(str(root))

    root_stat = root.stat()
    store.add_node(Node(
        id=_root_id(root),
        path=str(root),
        kind=NodeKind.DIR,
        mtime=root_stat.st_mtime,
        root=str(root),
    ))

    subdirs: list[str] = []
    try:
        entries = sorted(os.scandir(root), key=lambda e: e.name)
    except (PermissionError, OSError):
        return subdirs

    for entry in entries:
        if entry.is_dir(follow_symlinks=False):
            if config.should_ignore_dir(entry.name):
                continue
            # Lexical identity + resolved confinement guard (see _walk).
            entry_path = Path(entry.path)
            try:
                entry_path.resolve().relative_to(root.resolve())
            except (ValueError, OSError):
                continue
            try:
                node_id = _rel_id(root, entry_path)
            except ValueError:
                continue
            try:
                st = entry.stat(follow_symlinks=False)
            except OSError:
                continue
            is_link = _is_reparse_link(entry_path)
            store.add_node(Node(
                id=node_id,
                path=str(entry_path),
                kind=NodeKind.DIR,
                mtime=st.st_mtime,
                root=str(root),
            ))
            store.add_edge(Edge(src=_root_id(root), dst=node_id, type=EdgeType.CONTAINS))
            if not is_link:
                # Never hand a symlink/junction subdir to the federated
                # fragment crawler: crawling it would crawl its target,
                # which may be this very tree (or outside it).
                subdirs.append(str(entry_path))

        elif entry.is_file(follow_symlinks=False):
            # Identity comes from the LEXICAL path (see dir branch): resolve()
            # must never re-key a link-file onto its target's path.
            entry_path = Path(entry.path)
            is_link = _is_reparse_link(entry_path)
            if not is_link:
                # Resolved confinement guard for REAL entries only. A link
                # whose TARGET resolves outside the root is still recorded
                # under its lexical id when that lexical path is under root,
                # mirroring Rust's rel_id-only confinement.
                try:
                    entry_path.resolve().relative_to(root.resolve())
                except (ValueError, OSError):
                    continue
            try:
                node_id = _rel_id(root, entry_path)
            except ValueError:
                continue
            if config.should_ignore_file(node_id, entry.name):
                continue
            try:
                st = entry.stat(follow_symlinks=False)
            except OSError:
                continue
            if is_link:
                # Link attributes come from ONE bounded follow-stat (mirrors
                # Rust's fs::metadata + link_meta fallback): a resolvable
                # link records its target's size/mtime; a broken/unreadable
                # link keeps its own lstat values from above. open() below
                # follows the link already, so content_hash sees target bytes.
                try:
                    st = os.stat(entry_path)
                except OSError:
                    pass
            existing = store.get_node(node_id)
            if existing and existing.mtime == st.st_mtime and existing.content_hash:
                content_hash = existing.content_hash
            elif st.st_size <= config.max_content_bytes:
                content_hash = _sha256(Path(entry.path), config.max_content_bytes)
            else:
                content_hash = ""
            mime, _ = mimetypes.guess_type(entry.name)
            store.add_node(Node(
                id=node_id,
                path=str(entry_path),
                kind=NodeKind.FILE,
                size=st.st_size,
                mtime=st.st_mtime,
                mime=mime or "",
                content_hash=content_hash,
                root=str(root),
            ))
            store.add_edge(Edge(src=_root_id(root), dst=node_id, type=EdgeType.CONTAINS))

    return subdirs


def _walk(store: GraphStore, config: Config, root: Path, current: Path,
          depth: int = 0) -> None:
    # Defense-in-depth confinement: never descend into anything outside `root`.
    # The recursion is downward-only and symlinks are not followed, but this
    # explicit guard makes the invariant structural rather than conventional.
    try:
        current.resolve().relative_to(root.resolve())
    except ValueError:
        return  # `current` escaped the root — refuse to walk it
    if depth >= MAX_DEPTH:
        return  # defensive bound (mirrors Rust MAX_DEPTH); pathological trees

    # The crawl root's parent is the fragment's unique root node; deeper dirs
    # key by their root-relative id.
    parent_id = _root_id(root) if current == root else _rel_id(root, current)
    try:
        entries = sorted(os.scandir(current), key=lambda e: e.name)
    except (PermissionError, OSError):
        return

    for entry in entries:
        if entry.is_dir(follow_symlinks=False):
            if config.should_ignore_dir(entry.name):
                continue
            # Identity comes from the LEXICAL path: resolve() would re-key a
            # junction onto its target (a self-referential junction would mint
            # the root's "" id and loop forever). Resolve is used only as the
            # confinement guard below.
            entry_path = Path(entry.path)
            # Confinement guard per entry: a link pointing outside the root
            # can never mint a node or edge.
            try:
                entry_path.resolve().relative_to(root.resolve())
            except (ValueError, OSError):
                continue
            try:
                node_id = _rel_id(root, entry_path)
            except ValueError:
                continue  # out-of-root; never record
            try:
                st = entry.stat(follow_symlinks=False)
            except OSError:
                continue
            store.add_node(Node(
                id=node_id,
                path=str(entry_path),
                kind=NodeKind.DIR,
                mtime=st.st_mtime,
                root=str(root),
            ))
            store.add_edge(Edge(src=parent_id, dst=node_id, type=EdgeType.CONTAINS))
            if _is_reparse_link(entry_path):
                # Symlink/junction dir: recorded above, NEVER descended into.
                # This gate terminates cyclic junction pairs.
                continue
            _walk(store, config, root, entry_path, depth + 1)

        elif entry.is_file(follow_symlinks=False):
            # Identity comes from the LEXICAL path (see dir branch): resolve()
            # must never re-key a link-file onto its target's path.
            entry_path = Path(entry.path)
            is_link = _is_reparse_link(entry_path)
            if not is_link:
                # Resolved confinement guard for REAL entries only. A link
                # whose TARGET resolves outside the root is still recorded
                # under its lexical id when that lexical path is under root,
                # mirroring Rust's rel_id-only confinement.
                try:
                    entry_path.resolve().relative_to(root.resolve())
                except (ValueError, OSError):
                    continue
            try:
                node_id = _rel_id(root, entry_path)
            except ValueError:
                continue
            if config.should_ignore_file(node_id, entry.name):
                continue
            try:
                st = entry.stat(follow_symlinks=False)
            except OSError:
                continue
            if is_link:
                # Link attributes come from ONE bounded follow-stat (mirrors
                # Rust's fs::metadata + link_meta fallback): a resolvable
                # link records its target's size/mtime; a broken/unreadable
                # link keeps its own lstat values from above. open() below
                # follows the link already, so content_hash sees target bytes.
                try:
                    st = os.stat(entry_path)
                except OSError:
                    pass

            # Reuse cached hash when the file is unchanged since last cycle.
            existing = store.get_node(node_id)
            if existing and existing.mtime == st.st_mtime and existing.content_hash:
                content_hash = existing.content_hash
            elif st.st_size <= config.max_content_bytes:
                content_hash = _sha256(Path(entry.path), config.max_content_bytes)
            else:
                content_hash = ""

            mime, _ = mimetypes.guess_type(entry.name)
            store.add_node(Node(
                id=node_id,
                path=str(entry_path),
                kind=NodeKind.FILE,
                size=st.st_size,
                mtime=st.st_mtime,
                mime=mime or "",
                content_hash=content_hash,
                root=str(root),
            ))
            store.add_edge(Edge(src=parent_id, dst=node_id, type=EdgeType.CONTAINS))
