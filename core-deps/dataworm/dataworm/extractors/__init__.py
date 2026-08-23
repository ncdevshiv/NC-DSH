"""Extractor registry: maps file extensions to reference-extraction strategies."""

from __future__ import annotations

from dataworm.extractors import references  # noqa: F401  (registers nothing; module API)

__all__ = ["references"]
