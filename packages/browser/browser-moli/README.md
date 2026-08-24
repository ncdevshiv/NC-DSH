# @deepseek-ai/dsh-browser-moli

English | [中文](README.zh.md)

A [moli](https://github.com/lexmount/moli)-backed `BrowserProvider` for the harness [browser capability seam](../browser/README.md) (`ctx.browser`). Each launch spawns an isolated `moli serve` process on a reserved ephemeral port, waits for its HTTP CDP endpoint to answer, attaches to a page target over WebSocket, and hands out a [`BrowserSession`](../browser/README.md) that navigates pages, reads rendered state, interacts by CSS selector, and captures PNG screenshots.

This is an **implementation** package: it registers a provider into `ctx.browser`, it does not own the key and it does not register a model-facing tool (that is `@deepseek-ai/dsh-tool-browser`). Like the web providers, it is a function/namespace plugin (`inject: ['browser']`).

## How it works

- Availability is a memoized local `moli --version` probe — a mounted provider stays dormant until `$MOLI_BINARY` (or `PATH`) resolves.
- One session = one serve process: state isolation between launches, one lifecycle controller per session owning child process plus WebSocket, torn down by `close()` from any path. Closing the connection settles every blocked command and event waiter immediately, so `close()` never waits out a deadline timer.
- Navigation validates its target against the seam contract (absolute `http:`/`https:` only, `BROWSER_INVALID_URL` otherwise) before any CDP traffic carries it; a failed in-page evaluation surfaces as `BROWSER_EVALUATION_FAILED` rather than an empty page state.
- The CDP client is minimal and in-package (id-correlated commands, event waiters) over Node's global WebSocket; no automation dependency is added.
- Operations serialize per session; page text rides bounded `Runtime.evaluate` reads; interaction runs small DOM scripts; screenshots ride `Page.captureScreenshot`.

## Config

| Key | Default | Meaning |
|---|---|---|
| `binaryPath` | `$MOLI_BINARY` ?? `'moli'` | The moli executable. Unresolved at probe time makes the provider unavailable. |
| `startupTimeoutMs` | `15_000` | Budget for one session's server startup readiness polling. |
| `navigationTimeoutMs` | `30_000` | Budget for one page navigation. |
| `maxContentChars` | `100_000` | Character cap on returned page text content. |
| `probeTimeoutMs` | `5_000` | Budget for the one-time availability probe. |
| `pollEveryMs` | `100` | Interval between readiness polls. |
| `extraServeArgs` | `[]` | Extra argv appended to the `moli serve` invocation verbatim (flag overrides). |

```yaml
- id: browser-moli
  name: '@deepseek-ai/dsh-browser-moli'
  config:
    binaryPath: !!js process.env.MOLI_BINARY
```

## Model Experience

Indirectly, through [`dsh-tool-browser`](../tool-browser/README.md), which owns the stable model-facing names, schemas, prompt guidance, and presentation while provider failures surface as structured `BrowserError`s.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

- **Requires an external moli binary** — not bundled; install per the project's releases.
- **The default `--cdp-port` flag spelling is an assumption** — moli's serve flags are not pinned by a contract here; deployments correct the invocation via `extraServeArgs` without code changes.
- **CSS-selector interaction only** — ref-based element handles wait on a snapshot vocabulary; clicks are DOM-dispatched rather than coordinate-based input.
- **Software rendering only** — screenshots follow moli's layout policy (on-demand software paint); pixel parity with Chrome is not pursued upstream either.
