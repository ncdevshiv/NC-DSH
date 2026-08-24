# Agent Note: Track pi-ai's transient-error wording table in classifyPiAiError

Status: implemented

English | [中文](2026-08-23-pi-ai-transient-wording-parity.zh.md)

## Problem

[classifying pi-ai transport truncations](2026-07-22-pi-ai-transport-truncation-classification.md) taught `classifyPiAiError` two transport wordings; comparing the classifier against pi-ai's own maintained transient-error pattern table (`dist/utils/retry.ts`, most entries citing an upstream issue) shows it still misses most of that table: gateway cutoffs (`upstream connect`, `reset before headers`), OpenRouter's wrapped `Provider returned error`, bare `Service Unavailable` with no numeric status, websocket drop variants beyond `closed unexpectedly`, DNS failures (`getaddrinfo`, `ENOTFOUND`, `EAI_AGAIN`), explicit mid-stream provider retry guidance (the "you can retry your request" family), and gRPC `ResourceExhausted` throttling. Each miss falls through to non-retryable `PI_AI_ERROR`, so a recoverable failure permanently ends the turn — the same defect that note fixed for two wordings only.

The drift is structural, not incidental: the adapter pins pi-ai's internal retries off (`maxRetries: 0`; one adapter call is one wire attempt), so pi-ai's table decides nothing at runtime here. This classifier alone routes those wordings to retryability, yet nothing tied its coverage to the upstream list it effectively replaces.

## Decision

- `classifyPiAiError` mirrors pi-ai's transient pattern table:
  - `TRANSPORT`: `upstream connect`, `reset before headers`, `provider returned error`, generalized `websocket closed/error`, DNS failures (`getaddrinfo`, `ENOTFOUND`, `EAI_AGAIN`), and the retry-guidance phrases OpenAI Responses and Bedrock emit mid-stream when the stream dies.
  - `SERVER`: bare `service unavailable` alongside the existing numeric-status arm.
  - `RATE_LIMIT`: gRPC `ResourceExhausted` / `RESOURCE_EXHAUSTED` throttling.
- Precedence is unchanged: the specific arms (auth digits, quota text, 429, 413/400, 5xx digits, timeout) run first, so a wrapped message carrying status digits or quota wording keeps its precise code, and pi-ai's generic `Provider returned an error stop reason` fallback remains unclassified.
- Both READMEs of `llm-pi-ai` state the tracking relationship and its reason (`maxRetries: 0`).

## Alternatives considered

**Call pi-ai's exported `isRetryableAssistantError` instead of copying wordings.** Rejected: it answers only "retryable, yes or no", while the harness needs the specific code (`TRANSPORT` vs `RATE_LIMIT` vs `QUOTA`) that llm-retry eligibility, UI rendering, and diagnostics route on; code attribution requires our own matching regardless. Its non-retryable subscription-limit wording is already covered here by `isQuotaExceededError` mapping to terminal `QUOTA`.

**Make `PI_AI_ERROR` retryable to absorb future wording drift.** Rejected in the earlier note and still rejected: the catch-all holds genuinely permanent failures, which a default-retryable code would repeat pointlessly.

**Fix the root enabler upstream (preserve the original Error past flattening).** That remains the durable end state named by the classifier's `XXX(pi-ai upstream)` note and is not actionable in this repository; mirroring the wording table narrows the blast radius until it lands.

## Consequences

- Every transient family in pi-ai's table now recovers under default retry policy instead of failing the turn.
- Parity is maintained by hand: a new upstream wording still needs a harness pattern update, but the tracked surface is now the whole maintained table rather than incident-driven single additions.
- Over-matching stays bounded: the new arms only ever see terminal error messages (never model output), run after the specific-code checks, and name transport conditions rather than response judgments.
