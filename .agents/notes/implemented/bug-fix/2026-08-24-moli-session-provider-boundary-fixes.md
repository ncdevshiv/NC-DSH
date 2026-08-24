# Agent Note: moli session teardown and provider-boundary fixes

English | [中文](2026-08-24-moli-session-provider-boundary-fixes.zh.md)

Status: implemented

## Problem

The moli CDP session reported wrong outcomes on ordinary paths: a page whose load event fired while `Page.navigate` was still settling stalled the navigation out to its full budget; `close()` waited out blocked event-waiter deadlines and leaked their timers; an in-page evaluation exception collapsed to an empty result, letting click/type report success while nothing happened; the serialization chain retained one settled promise per operation for the session's lifetime; and navigation forwarded non-http(s) targets such as `file:`/`javascript:` into the protocol although the seam contract assigns scheme rejection to providers. Adjacent provider boundaries shared the pattern: SearXNG credential pairs that HTTP basic auth cannot encode surfaced as per-search `WEB_PROVIDER_ERROR` instead of a load-time misconfiguration, and the moli fetch provider classified a killed run by thrown shape before consulting its signals — a backstop timeout could report `WEB_ABORTED` — and spawned a subprocess even when the caller had already aborted. The packaged Python runtime bundled the browser seam and provider without the consumer tool, leaving the distribution unable to mount any composition that calls `browser_*`.

## Decision

The moli CDP connection rejects outstanding event waiters and pending commands on close with `BROWSER_SESSION_CLOSED`, rejects sends and event waits after close the same way instead of leaking raw transport exceptions, and validates target-discovery status codes and body shape before casting. The session registers the load-event waiter before sending `Page.navigate`; checks `Runtime.evaluate` responses for `exceptionDetails` and throws `BROWSER_EVALUATION_FAILED` carrying the in-page description; serializes over a rolling single-promise chain that retains only the latest operation; and validates navigate targets as absolute http(s) URLs (`BROWSER_INVALID_URL`) before any CDP traffic carries them. SearXNG rejects a configured credential pair that is not Latin-1 encodable at plugin load. The moli fetch provider classifies the caller's aborted signal first, an exhausted backstop second (`WEB_FETCH_TIMEOUT`), and only then abort-shaped thrown values, and it rejects an already-aborted signal before spawning. The packaged runtime manifest adds `@deepseek-ai/dsh-tool-browser`, completing the browser closure it distributes alongside `dsh-browser`/`dsh-browser-moli`.

## Testing

The moli suite pins each fix behaviorally: load-event-during-command resolution, prompt close with an operation blocked on a CDP event, evaluation-failure surfacing through click and snapshot, and scheme rejection with zero frames sent. A Loader-booted test-only cordis.yml mounts seam, provider, and consumer together and asserts both the registered five-tool surface plus guidance section and the structured no-usable-provider failure for an absent binary. SearXNG covers the load-time credential rejection; the fetch provider covers classification precedence and the pre-abort non-spawn guard.

## Alternatives considered

### Why not wait on richer navigation lifecycle events?

Waiting on frame or lifecycle event combinations mirrors Chrome's model more faithfully but couples the deliberately minimal CDP client to more of the protocol surface. Waiter-before-send registration resolves the missed-event race without new protocol dependencies.

### Why not validate navigation URLs at the consumer schema layer?

A schema pattern would duplicate the contract away from the provider that owns it, and direct `ctx.browser` callers never pass through tool schemas; the seam's request type already states "providers reject other schemes", so the provider is the enforcement point.

### Why not drop the browser deps from `python/sdk-runtime`?

That manifest's dependency closure IS the distributed plugin set, and every sibling capability ships its consumer there. Removal would keep the packaged runtime unable to mount compositions using `browser_*`; adding `@deepseek-ai/dsh-tool-browser` completes the intended closure symmetrically with `dsh-tool-web`.

### Why not keep classifying fetch failures by thrown shape alone?

Shape-only classification worked only because the current runner happens to reject plain errors; any transport surfacing kills as DOMExceptions would misreport timeouts as cancellations. Signal-state-first ordering depends on ownership facts (the signals this provider created) rather than runner internals.

## Consequences

Fast pages resolve navigation promptly instead of timing out; teardown completes within connection settlement rather than waiter deadlines; interaction can no longer report success silently, at the cost of one more implementation-owned code (`BROWSER_EVALUATION_FAILED`) that consumers tolerate under the open-string contract. Credential misconfiguration fails once, loudly, at load. The follow-up hardening wave closes the rest of the audit list: `AbortSignal` propagates through every CDP command and event wait (`BROWSER_ABORTED`), startup discovery and the WebSocket open share the startup budget, live serve processes are force-killed during Node's synchronous exit phase, `cdpTimeoutMs`/`settleMs`/`maxUrlLength` replace the hardcoded deadlines and settle delay, and the consumer caps screenshot bytes (`BROWSER_SCREENSHOT_TOO_LARGE`) with temp-file retention now documented as deliberate. Still open: reconciliation of any residual `available()` wording drift in sibling web providers' docs, and end-to-end runs against a real installed moli binary.
