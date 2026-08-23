"""Core data model: nodes, edges, and the edge types that form graph dimensions."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class NodeKind(str, Enum):
    """Whether a node is a directory or a file."""

    DIR = "dir"
    FILE = "file"


class EdgeType(str, Enum):
    """The dimension/layer an edge belongs to.

    Multiple edge types may connect the same pair of nodes, which is what makes
    the graph "higher-dimensional" (a layered/multiplex graph).
    """

    CONTAINS = "contains"          # structural: parent dir -> child (dir or file)
    REFERENCES = "references"      # content: file -> file it imports/links to
    DUPLICATE_OF = "duplicate_of"  # identical / near-identical content
    SIMILAR_TO = "similar_to"      # semantic similarity above a threshold


@dataclass
class Node:
    """A single file or directory in the traversed tree.

    ``id`` is the path relative to the crawl root, using forward slashes, so it
    is stable across platforms and acts as the canonical graph key.

    ``root`` is the absolute path of the crawl root this node was discovered
    from (provenance). It is empty for legacy/loaded nodes. Two nodes with the
    same absolute ``path`` but different ``root`` values refer to the same file
    on disk — the merge pass uses this to re-key subdir nodes into a parent's
    namespace and link their sub-networks.
    """

    id: str
    path: str                       # absolute path on disk
    kind: NodeKind
    size: int = 0
    mtime: float = 0.0
    mime: str = ""
    content_hash: str = ""          # sha256 of file bytes ("" for dirs)
    root: str = ""                  # absolute crawl root this node came from
    attrs: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "path": self.path,
            "kind": self.kind.value,
            "size": self.size,
            "mtime": self.mtime,
            "mime": self.mime,
            "content_hash": self.content_hash,
            "root": self.root,
            "attrs": self.attrs,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Node":
        return cls(
            id=data["id"],
            path=data["path"],
            kind=NodeKind(data["kind"]),
            size=data.get("size", 0),
            mtime=data.get("mtime", 0.0),
            mime=data.get("mime", ""),
            content_hash=data.get("content_hash", ""),
            root=data.get("root", ""),
            attrs=data.get("attrs", {}),
        )


@dataclass
class Edge:
    """A directed, typed, weighted link between two nodes."""

    src: str
    dst: str
    type: EdgeType
    weight: float = 1.0
    attrs: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return {
            "src": self.src,
            "dst": self.dst,
            "type": self.type.value,
            "weight": self.weight,
            "attrs": self.attrs,
        }

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "Edge":
        return cls(
            src=data["src"],
            dst=data["dst"],
            type=EdgeType(data["type"]),
            weight=data.get("weight", 1.0),
            attrs=data.get("attrs", {}),
        )
