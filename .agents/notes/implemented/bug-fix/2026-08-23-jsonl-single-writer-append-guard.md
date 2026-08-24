# Agent Note: Refuse JSONL appends over foreign durable growth

Status: implemented

English | [中文](2026-08-23-jsonl-single-writer-append-guard.zh.md)

## Problem

The JSONL backend checked append contiguity only against the owning coordinator's in-memory cursor, and cold recovery committed closers derived from a stored read that another writer could invalidate at any moment. When two harness processes shared one storage root and opened the same session — one live, one performing a cold history or resume load — the cold side appended synthetic interrupted-turn closers computed from its snapshot while the live owner kept numbering events from its own counter. Both sides' appends passed validation, the log acquired overlapping sequence numbers plus a stray seeded end-seed marker, and every later read rejected the artifact as `corrupt session log: seq gap in committed region`, surfacing to users as failed history loads with no recovery path short of byte surgery.

## Decision

`JsonlSessionPersistence` keeps a per-session durable byte-size baseline: set at lazy materialization, advanced after every fsync-committed append, reset at repair truncation, and seeded by every revision-stable adoption read (`readPrefix`). Before writing, each append compares the file's current size against the baseline and rejects divergence with an error naming the session, both sizes, and the concurrent-writer cause. A fresh instance always seeds through its adoption read, so legitimate resume never false-positives. The SQLite peer already enforced the same single-writer invariant logically (`append starts at seq X, stored next seq is Y`); the two storage backends now agree that foreign growth is a loud refusal, not silent interleaving. Regression coverage drives a real external append between backend writes in `packages/session/session-persistence-jsonl/tests/jsonl.spec.ts` alongside a fresh-instance resume control.

## Alternatives considered

**Cross-process advisory locks per session directory.** Rejected: hand-rolled locking across Windows and POSIX brings stale-holder recovery and ownership-lifecycle problems, and the persistence seam would need a new exclusive-access protocol; size verification detects the conflict with primitives the backend already uses.

**Deferring cold-load repair until resume publication.** Rejected for now: balancing an interrupted turn is legitimate work for history inspection and prepared-session reuse, and changing when `commitRepair` runs reshapes resume semantics well beyond this defect. With append-time refusal, a concurrently live owner makes the repair side's next write fail instead of corrupting.

**Re-validating logical sequence numbers before each append.** Rejected: JSONL is sequential media, so reading and decoding the compressed tail per append defeats the streaming design; the byte size is the cheap durable-tail proxy this format affords.

**Enforcing the check in the coordinator.** Rejected: physical layout belongs to each backend, and the SQLite backend already validates inside the store; a seam-level cursor check would still be self-referential memory rather than durable fact.

## Consequences

A second process opening a live session now fails loudly on its next durable append instead of producing an unreadable log; the failing side's buffered events are retained with the existing background-write warning, and neither file bytes nor in-memory state are silently rewritten. The residual gap is an instance's first contact with a session it has never read or written — there is no baseline yet — which stored-session adoption reads and self-materialized new logs already cover. Operators who see the refusal must decide which writer owns the session before retrying; the error names both sizes to make that diagnosis direct.
