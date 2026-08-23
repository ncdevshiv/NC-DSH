"""Configuration: ignore rules, size limits, and per-extension behaviour."""

from __future__ import annotations

from dataclasses import dataclass, field
from fnmatch import fnmatch
from pathlib import PurePosixPath

# Directory names that are never worth traversing (VCS, caches, build output).
DEFAULT_IGNORE_DIRS: set[str] = {
    ".git", ".hg", ".svn",
    "__pycache__", ".mypy_cache", ".pytest_cache", ".ruff_cache",
    "node_modules", ".venv", "venv", "env",
    ".idea", ".vscode",
    "dist", "build", ".next", ".nuxt", "target",
    ".dataworm",  # the worm's own output; never crawl it
}

# Glob patterns (matched against the relative id) that are skipped entirely.
DEFAULT_IGNORE_GLOBS: tuple[str, ...] = (
    "*.pyc", "*.pyo", "*.class", "*.o", "*.so", "*.dll", "*.exe",
    "*.png", "*.jpg", "*.jpeg", "*.gif", "*.ico", "*.webp",
    "*.mp3", "*.mp4", "*.avi", "*.mov", "*.wav",
    "*.zip", "*.tar", "*.gz", "*.7z", "*.rar",
    "*.pdf", "*.lock",
)

# Extensions we treat as text and attempt reference extraction on.
TEXT_EXTENSIONS: set[str] = {
    ".py", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs",
    ".md", ".markdown", ".rst", ".txt",
    ".json", ".yaml", ".yml", ".toml", ".ini", ".cfg",
    ".html", ".css", ".sh", ".bat", ".sql", ".go", ".rs", ".java", ".c", ".h", ".cpp",
}

# Files larger than this are not read for content (references/semantic/hashing
# still record the node, but skip expensive content work).
MAX_CONTENT_BYTES: int = 2 * 1024 * 1024  # 2 MiB


@dataclass
class Config:
    """Tunable knobs for a crawl. Sensible defaults cover the common case."""

    root: str = ""
    ignore_dirs: set[str] = field(default_factory=lambda: set(DEFAULT_IGNORE_DIRS))
    ignore_globs: tuple[str, ...] = DEFAULT_IGNORE_GLOBS
    text_extensions: set[str] = field(default_factory=lambda: set(TEXT_EXTENSIONS))
    max_content_bytes: int = MAX_CONTENT_BYTES

    # semantic pass
    enable_semantic: bool = True
    similarity_threshold: float = 0.35   # cosine similarity cutoff for similar_to
    # Memory safety valve only — NOT a correctness limit: the inverted-index
    # candidate generation is exact at any scale (every pair clearing a positive
    # threshold is recalled and scored). The cap bounds how many vectors are
    # held/embedded in one pass, nothing more.
    max_semantic_nodes: int = 50000

    # hashing pass
    enable_hashing: bool = True
    # Memory safety valve only — NOT a correctness limit: simhash banding
    # recalls every hamming<=3 pair exactly at any scale (pigeonhole over
    # 4x16-bit bands). The cap bounds the fingerprint list held in memory.
    max_hashing_nodes: int = 100000

    def should_ignore_dir(self, name: str) -> bool:
        return name in self.ignore_dirs

    def should_ignore_file(self, rel_id: str, name: str) -> bool:
        for pattern in self.ignore_globs:
            if fnmatch(name, pattern) or fnmatch(rel_id, pattern):
                return True
        return False

    def is_text(self, name: str) -> bool:
        suffix = PurePosixPath(name).suffix.lower()
        return suffix in self.text_extensions
