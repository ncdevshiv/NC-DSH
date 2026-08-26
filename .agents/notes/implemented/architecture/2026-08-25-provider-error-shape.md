# Agent Note: Provider error shape — structured facts survive SDK flattening

Status: implemented

English | [中文](2026-08-25-provider-error-shape.zh.md)

## Problem

Provider SDKs collapse HTTP failures into one display string before any harness layer sees structured facts. The Anthropic SDK's `APIError.makeMessage` renders `${status} ${error.message}`, and pi-ai's shared formatter renders `${status}: ${JSON.stringify(body)}`; pi-ai then flattens every caught error into its assistant message's `errorMessage`, discarding the SDK's `status`, structural `type`, parsed body, and request id. A gateway answering `{"error":{"type":"error","message":"Internal server error"}}` with status 500 therefore reached the harness retry UI as the raw flatten `500: {"type":"error","message":"Internal server error"}` — the user had to parse JSON out of a failure label to learn what happened, and no consumer could branch on status or provider type without regex-matching display text.

## Decision

**Adapters own structured-fact recovery; `LlmFailure` carries it verbatim; `message` stays human-readable.** Three rules:

1. **Recover at the earliest owned boundary.** Where a library flattens errors before we see them, the owning adapter parses the deterministic flatten back into facts rather than forwarding the flat string. `dsh-llm-pi-ai`'s `flat-error.ts` recognizes both flattened shapes (`<status> <payload>` and `<status>: <payload>`), JSON-parses the payload when it is one, and extracts `message`/`type`/`code` from either envelope nesting. Unrecoverable text passes through unchanged — nothing guesses beyond what the string states.
2. **Classify through the seam's single classifier.** `classifyHttpStatus(status, detail)` in `dsh-llm` is the only status-to-code mapping; adapters feed it recovered facts instead of keeping local copies. Wording-based classification remains only as the fallback for failures with no recoverable status.
3. **Display fields are separate, never concatenated.** The chat retry disclosure renders delay, reason, HTTP status, provider type, and request id as distinct rows; a failure message never embeds its own status prefix or body JSON.

The same discipline applies on the web seam: `dsh-web`'s shared `readErrorBody`/`parseErrorBody`/`throwProviderHttpError` replace four per-provider body-parse dances, bound the read at 16 KB, quote only the first line of a non-JSON body, and attach `status`/`providerType` to `WebError`.

## Consequences

`LlmFailure` gained optional `providerType` (validated ≤128 chars at `LlmError`, durable-invariant, and normalization boundaries; additive, so no session-format bump). DeepSeek direct-fetch reads both OpenAI-wrapped and top-level envelope shapes and appends a bounded fragment for non-JSON bodies. The regression tests pin the reported case end to end: an adapter test proves the flatten recovers to `{message: 'Internal server error', code: 'SERVER', status: 500, providerType: 'error'}`, the retry spec pins the full fact set in the durable `llm/retry` event, and the loop spec pins terminal `turn/end` retention. No separate log line was added: the durable events are the queryable record, and a parallel log channel would duplicate them.

## Alternatives considered

- **Fixing the flatten inside pi-ai** — rejected: vendored upstream owns that line; the workaround must live where the harness compiles.
- **Regex-classifying richer wording tables** — rejected: wording changes across SDK versions; structure parsed from the stated status is provable, words are not.
- **Putting the raw body JSON on the failure** — rejected: hostile or oversized bodies would reach durable storage and UI labels; bounded fragments cover the diagnostic need.

Related: [bounded LLM request recovery](2026-06-21-bounded-llm-request-recovery.md) (the `LlmFailure` contract this extends), [pi-ai response metadata capture](../bug-fix/2026-08-23-pi-ai-response-metadata-capture.md) (the response-boundary facts this complements).
