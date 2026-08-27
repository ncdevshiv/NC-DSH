# Agent Note: Fix custom provider and model-fetch user flows

Status: implemented

English | [中文](2026-08-27-user-flow-custom-provider-and-model-fetch.zh.md)

## Problem

Tracing every user flow end to end — custom provider addition, model fetch, workspace grouping, and the surrounding settings surfaces — surfaced four user-visible breakages that were fixed together in this PR:

1. **Vision toggle never persisted for custom providers.** `ModelListEditor` read and wrote `model['input']` while `DeepSeekModelsEditor`, `llm-ai-sdk`'s `catalogModel` schema, and `AiSdkAdapter` all read `inputModalities`. A user checking vision on a custom provider left `input: ['text','image']` on disk, which the adapter ignored — every custom model stayed text-only. Adopted discovery candidates had the same hole, writing `input`.

2. **Custom provider creation accepted whitespace-padded values.** `CustomProviderCard` validated `route` and `baseURL` without trimming. `route = " acme "` failed `ROUTE_PATTERN` but `route = "acme "` with trailing space also failed while `baseURL = "   "` passed `ready`. The profile persisted raw strings with surrounding spaces, producing opaque `CONFIG`/`network` failures from the sidecar.

3. **Sidecar `childExitOrTimeout` leaked timers and listeners on every request.** `sidecar.ts:childExitOrTimeout` attached `child.once('exit'|'error')` plus a 120s `setTimeout` per `request()` and never cleared them on success. Burst discovery or streaming accumulated unbounded handlers and held timers for the full ceiling.

4. **Ungrouped current blank session disappeared.** `tree.ts:groupByWorkspace` built the `stray` (ungrouped) set with an extra `&& !s.blank`, so a `current` session that was blank and ungrouped was dropped even though `sessionVisible` already allows the current blank. Workspace-member blanks were shown; ungrouped blanks were not — breaking `connectChat` reuse UX.

Related notes: [whole-section provider reset framing](2026-08-23-whole-section-provider-reset-framing.md), [workspace UI product flow](../feature/2026-07-25-workspace-ui-product-flow.md), [moli session/provider boundary fixes](2026-08-24-moli-session-provider-boundary-fixes.md).

## Decision

- **Unified vision field on `inputModalities`.** `ModelListEditor.declaresVision` now checks `inputModalities` first with `input` as legacy fallback; `setVision` writes `inputModalities` and drops legacy `input`; `adopt` writes `inputModalities`. Traces from UI to `resolveAdapterOptions` → `AiSdkAdapter` → `catalogModel` schema now agree. Tests in `provider-form.client.spec.tsx` and `components.client.spec.tsx` were updated from `input` to `inputModalities`.
- **Trim at the validation and persistence boundary.** `CustomProviderCard` introduces `trimmedRoute`/`trimmedBaseURL` used for `routeInvalid`, `routeTaken`, `ready`, hint text, `deriveKeyRef`, `displayName`, `baseURL`, and the `providers.<route>` settings path. Raw input still renders so the user sees what they typed until it is judged.
- **`CustomProviderCard` now surfaces `settings-conflict` as the localized `conflict` copy** instead of the raw host message when the revision check races.
- **`AiSidecarClient.request` accepts an optional `AbortSignal` and `discoverModels` forwards it.** `childExitOrTimeout` captures `child` once, installs named `onExit`/`onError` handlers, clears the timer on any exit/error, and removes handlers on timeout. The `model.discover` path now carries the caller's signal through `index.ts:registerModelDiscovery` → `sidecar.discoverModels`. Malformed discovery rows without a non-empty `id` are dropped at the sidecar boundary rather than cast through.
- **`groupByWorkspace` no longer filters `stray` blanks.** The current blank session is now visible in the Ungrouped bucket, symmetric with workspace-member blanks. `tree.client.spec.ts` gains coverage for that case.

## Alternatives considered

**Migrating stored `input` values in `resolveAdapterOptions`.** Rejected: the on-disk `settings.yaml` is user-editable YAML; silently rewriting it during resolve would change a file the user owns without a settings write, and the transient mismatch is already repaired on next vision toggle. A future settings migration can normalize lingering `input` fields when it touches that layer.

**Deferring the sidecar listener fix to a native-side pagination/capacity overhaul.** Rejected: the leak is independent and cheap to fix; pagination and hard-coded `ModelInfo(128_000, 8192)` in `openai_compat.rs` remain follow-ups with distinct Rust scope.

## Consequences

- Vision choice for custom providers persists and streams gate on the same field. Legacy profiles with `input` still render correctly until edited.
- Whitespace-padded routes and endpoints are rejected in the form and never land trimmed-spaced in `settings.yaml`.
- Sidecar timeout/exit handlers no longer accumulate; aborted discovery requests settle promptly with `SidecarProtocolError(kind='cancelled')`.
- Ungrouped blank current sessions appear in the browser, consistent with workspace-member blanks.
- Verification: `bun run typecheck` clean; `bun run test --run packages/client/ui-settings-models packages/client/ui-workspace packages/llm/llm-ai-sdk` green (383 + 155 + 228 tests); `tree.client.spec.ts` added `shows the current ungrouped blank session in the Ungrouped bucket`.

## Verification

- `bun run typecheck` — zero errors
- `bun run test --run packages/client/ui-settings-models` — 10 files, 228 tests passed
- `bun run test --run packages/llm/llm-ai-sdk packages/client/ui-workspace` — 11 files, 155 tests passed
- New spec case: `tree.client.spec.ts:shows the current ungrouped blank session in the Ungrouped bucket`
