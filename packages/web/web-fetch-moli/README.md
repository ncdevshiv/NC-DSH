# @deepseek-ai/dsh-web-fetch-moli

English | [中文](README.zh.md)

A [moli](https://github.com/lexmount/moli)-backed `WebFetchProvider` for the harness [web capability seam](../web/README.md) (`ctx.web`). It retrieves a concrete URL by rendering it in the local moli headless browser (`fetch --dump markdown`) and returns the rendered markdown, so JavaScript-rendered pages yield real content where a plain HTTP GET sees an empty shell.

This is an **implementation** package: it registers a provider into `ctx.web`, it does not own the `ctx.web` key and it does not register a model-facing tool (that is `@deepseek-ai/dsh-tool-web`). Like `@deepseek-ai/dsh-web-fetch-http`, it is a function/namespace plugin (`inject: ['web']`).

## Responsibility split

The provider owns **safe resource retrieval through an external binary**: URL validation, subprocess transport, abort propagation, the resource-backstop timeout, and the character cap. `@deepseek-ai/dsh-tool-web` owns **presentation**. Availability is a memoized local `moli --version` probe, so a mounted provider stays dormant until the binary resolves.

The provider's `timeoutMs` is a resource backstop for direct `ctx.web.fetch()` callers and misconfigured deployments, not the model-facing tool-call budget. [`dsh-tool-call-timeout-policy`](../../guard/timeout-policy/README.md) owns the tool-call budget; if the outer deadline reaches this provider first it reports `WEB_ABORTED`, and an exhausted backstop reports `WEB_FETCH_TIMEOUT`.

## Transport hygiene

- Accepts only `http:` and `https:` URLs; rejects credentials in URLs (`WEB_BLOCKED_URL`) and over-long/malformed URLs (`WEB_INVALID_URL`, beyond the configured `maxUrlLength`) before any process starts. An already-aborted caller signal also rejects before the subprocess spawns.
- Runs moli shell-free with abort propagation into the subprocess.
- Browser-grade by design: page JavaScript executes, and redirects are followed natively per browser semantics — the same-origin-only redirect rule of [`dsh-web-fetch-http`](../web-fetch-http/README.md) does not apply here.
- Emits rendered markdown classified as the seam's `kind: 'text'` body (markdown IS text; the tool passes text through untouched) until the closed `WebFetchBody` union gains a markdown arm.

## Config

| Key | Default | Meaning |
|---|---|---|
| `binaryPath` | `$MOLI_BINARY` ?? `'moli'` | The moli executable: a PATH name or a path. Unresolved at probe time makes the provider unavailable. |
| `maxUrlLength` | `2048` | Maximum accepted request URL length in characters. |
| `maxBodyChars` | `100_000` | Maximum returned markdown length in characters; a longer body is truncated and flagged. |
| `timeoutMs` | `30_000` | Fetch timeout within Node's timer range — a resource backstop for direct callers, not the model-facing budget. |
| `probeTimeoutMs` | `5_000` | Budget for the one-time `--version` availability probe. |

The numeric limits are validated at plugin construction: every cap must be a positive finite number and `timeoutMs` within Node's timer range. An invalid value throws rather than silently constructing a provider with nonsensical limits.

```yaml
- id: web-fetch-moli
  name: '@deepseek-ai/dsh-web-fetch-moli'
  config:
    binaryPath: !!js process.env.MOLI_BINARY
```

## Model Experience

Indirectly, through [`dsh-tool-web`](../tool-web/README.md), which places this provider's `maxBodyChars`-bounded markdown under its fetch-result wrapper and retains provider failures while redirects, headers, and transport mechanics remain hidden.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

- **Requires an external moli binary** — not bundled; install per the project's releases and point `binaryPath`/`$MOLI_BINARY` at it when it is not on `PATH`.
- **`statusCode` is always 200 on success** — dump mode does not expose the HTTP status; genuinely failed navigations surface as provider errors instead of non-2xx results.
- **SSRF / private-network protection is inherited-deferred** — the seam-level deferral applies, and moli executes page JavaScript that issues its own requests; its `--private-network` policy flag is not yet exposed here. Do not enable fetch where rendered pages can reach sensitive internal targets.
- **Markdown rides `kind: 'text'`** until the closed body union gains a markdown arm.
