# Agent Note: Capture provider response metadata in the pi-ai adapter

Status: implemented

English | [中文](2026-08-23-pi-ai-response-metadata-capture.zh.md)

## Problem

Diagnosing a retried pi-ai-path failure — the transport-truncation family both [classifying pi-ai transport truncations](2026-07-22-pi-ai-transport-truncation-classification.md) and [tracking pi-ai's transient-error wording table](2026-08-23-pi-ai-transient-wording-parity.md) route to retryable codes — stopped at the harness code: the failure carried no HTTP status and no provider request id, while the direct DeepSeek adapter's failures retain `x-request-id` / `x-deepseek-request-id` for support correlation. That asymmetry between the two adapters of one seam had no owner. pi-ai error events themselves expose neither fact: the terminal event delivers a flattened message string only.

## Decision

- Each `PiAiAdapter.stream()` call captures the status and request id through pi-ai's `onResponse` stream option, which every shipped assistant protocol invokes when response headers arrive — before body consumption — so the facts exist even when the body then dies mid-stream.
- The captured facts fill absent fields on error and aborted finish chunks (`LlmFailure.status`, `LlmFailure.requestId`) and on the idle-timeout `LlmError`. A caller abort stays bare, and successful finishes pass through unchanged.
- Request-id lookup mirrors the DeepSeek adapter's precedence (`x-request-id` first, then `x-deepseek-request-id`) with case-insensitive header names; already-mapped failure fields are never overwritten.
- The package READMEs replace the "Provider HTTP status is unavailable" limitation with the accurate residual: status comes only from this boundary capture, so a failure thrown before any response arrives exposes codes without status.

## Alternatives considered

**Parse ids out of the flattened error text.** Rejected: ids live in response headers, not in message text; the truncation wordings carry nothing to parse.

**Observe responses below pi-ai via a fetch/dispatcher/client hook.** Rejected for the reason the truncation-classification note already recorded: pi-ai exposes no such hook. `onResponse` is its sanctioned observation point, and firing at header time makes it sufficient despite arriving before the body.

**Attach the metadata to every streamed chunk.** Rejected: it would duplicate one boundary fact across hundreds of durably logged chunks with no consumer reading it per chunk.

## Consequences

- `llm/retry` events and turn failures on pi-ai routes now carry request ids like direct DeepSeek routes, so a gateway operator can correlate a reported session with server-side logs even when retries already recovered the turn.
- The enriched finish chunk is what the loop logs, so the transcript, the derived failure, and the retry event all show identical facts.
- A protocol that never fires `onResponse` leaves failures bare — an observable absence rather than a misattributed value.
