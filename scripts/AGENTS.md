# AGENTS.md — Repository scripts

Gate scripts invoke package scripts and local binaries shell-free, normalize repository-relative glob paths to `/` at ingestion, and keep platform adaptation in the gate that needs it instead of a shared platform layer.
