"""Ingest dsh session logs into the openmem memory store.

Dev-local NIO tool (BLUEPRINT.md Phase 3): walks dsh JSONL session roots,
extracts human prompts and assistant final text, and stores each as an openmem
memory through the same write path the MCP server uses (`get_vector_db()`
honoring `OPENMEM_DB_PATH`). Memory content embeds the session id and event
seq, so ids are content-hash stable and re-runs are idempotent.

Usage (from core-deps/openmem so `memory_store` imports resolve):

    ../core-deps/openmem/.venv/Scripts/python.exe ingest-openmem.py \
        --sessions ~/.dsh/sessions \
        --db F:/deepseek-harness-master/nio-graphs/openmem-lancedb

Skipped by design: reasoning blocks (internal), packed delta-chunk rows
(assistant/message carries the assembled text), plugin-sourced user/message
notices, and sessions whose transcript fails to decode (counted, not fatal).
"""

from __future__ import annotations

import argparse
import io
import json
import os
import sys
import zstandard as zstd

MAX_MEMORY_CHARS = 2000


def iter_log_lines(path: str):
    """Yield decoded logical lines from one transcript file."""
    with io.open(path, "rb") as fh:
        if path.endswith(".zstd"):
            reader = zstd.ZstdDecompressor().stream_reader(fh, read_across_frames=True)
            raw = reader.read()
        else:
            raw = fh.read()
    for line in raw.decode("utf-8", "replace").splitlines():
        line = line.strip()
        if line:
            yield line


def block_text(blocks) -> str:
    """Join text content blocks; ignore reasoning and other block types."""
    parts = []
    for block in blocks or []:
        if isinstance(block, dict) and block.get("type") == "text":
            text = (block.get("text") or "").strip()
            if text:
                parts.append(text)
    return "\n".join(parts).strip()


def truncate(text: str) -> str:
    if len(text) <= MAX_MEMORY_CHARS:
        return text
    return text[: MAX_MEMORY_CHARS - 20] + " …[truncated]"


def extract_memories(path: str):
    """Yield (content, importance, tags_base, seq) per eligible event."""
    session_id = None
    project = None
    preset = None
    for line in iter_log_lines(path):
        try:
            record = json.loads(line)
        except ValueError:
            continue
        rtype = record.get("type")
        if rtype == "session":
            session_id = record.get("id")
            cwd = record.get("cwd")
            project = os.path.basename(cwd) if cwd else None
            preset = record.get("agentPreset")
            continue
        if rtype == "user/message":
            source = (record.get("data") or {}).get("source") or {}
            if source.get("kind") == "plugin":
                continue
            text = block_text((record.get("data") or {}).get("content"))
            if text:
                yield text, 0.8, "user", record.get("seq"), session_id, project, preset
        elif rtype == "assistant/message":
            text = block_text(((record.get("data") or {}).get("message") or {}).get("content"))
            if text:
                yield text, 0.5, "assistant", record.get("seq"), session_id, project, preset


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--sessions", action="append", required=True,
                        help="session root to scan (<root>/<project>/<id>/session.jsonl[.zstd]); repeatable")
    parser.add_argument("--db", default=os.environ.get("OPENMEM_DB_PATH"),
                        help="openmem LanceDB directory (defaults to OPENMEM_DB_PATH)")
    parser.add_argument("--min-chars", type=int, default=40,
                        help="skip texts shorter than this many characters")
    parser.add_argument("--no-assistant", action="store_true",
                        help="ingest human prompts only")
    parser.add_argument("--dry-run", action="store_true",
                        help="report what would be stored without writing")
    args = parser.parse_args()

    if args.db:
        os.environ["OPENMEM_DB_PATH"] = args.db

    here = os.path.dirname(os.path.abspath(__file__))
    sys.path.insert(0, here)
    sys.path.insert(0, os.path.join(here, "..", "..", "..", "..", "..", "core-deps", "openmem"))

    files = []
    for root in args.sessions:
        root = os.path.expanduser(root)
        for dirpath, _dirnames, filenames in os.walk(root):
            for name in filenames:
                if name in ("session.jsonl", "session.jsonl.zstd"):
                    files.append(os.path.join(dirpath, name))

    if args.dry_run:
        count = 0
        for path in files:
            for text, _imp, role, _seq, _sid, _proj, _preset in extract_memories(path):
                if len(text) >= args.min_chars:
                    count += 1
        print(f"dry-run: would ingest {count} memories from {len(files)} transcripts")
        return 0

    from memory_store.vector_db import get_vector_db

    store = get_vector_db()
    stored = skipped_short = failed_files = 0
    for path in files:
        try:
            batch = list(extract_memories(path))
        except Exception as error:  # noqa: BLE001 - one bad transcript must not stop the run
            print(f"  ! decode failed, skipping: {os.path.basename(path)}: {error}")
            failed_files += 1
            continue
        for text, importance, role, seq, session_id, project, preset in batch:
            if len(text) < args.min_chars:
                skipped_short += 1
                continue
            where = f"{project or 'unknown-project'}"
            content = truncate(f"[dsh-session {session_id} seq {seq}] ({where}, {role}) {text}")
            tags = ["dsh-session", role]
            if project:
                tags.append(project)
            if preset:
                tags.append(f"preset:{preset}")
            memory_id = store.add_memory(
                content=content,
                importance=importance,
                tags=tags,
                metadata={"source": "dsh-session-ingest", "session": session_id, "seq": seq},
            )
            if memory_id:
                stored += 1
            else:
                print(f"  ! add_memory returned no id (seq {seq} of {session_id})")

    print(f"ingested {stored} memories from {len(files)} transcripts "
          f"({skipped_short} too-short skipped, {failed_files} transcripts undecodable)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
