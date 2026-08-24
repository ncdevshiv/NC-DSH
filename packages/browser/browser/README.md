# @deepseek-ai/dsh-browser

English | [中文](README.zh.md)

Service Definition for the browser-automation capability seam (`ctx.browser`): a provider registry and provider-selecting session launch, with the shared request/result vocabulary and `BrowserError` taxonomy. Selection mirrors [`dsh-web`](../../web/web/README.md): a configured provider wins; without configuration exactly one usable provider auto-selects, so selection never depends on registration order.

- [`browser-moli/`](../browser-moli/README.md) drives the local [moli](https://github.com/lexmount/moli) headless browser and registers on this seam.
- [`tool-browser/`](../tool-browser/README.md) owns the stable model-facing `browser_*` tools.

## Model Experience

Indirectly, through `dsh-tool-browser`, which owns every schema, prompt section, and rendered result while provider failures surface as structured `BrowserError`s.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

- The seam exposes CSS-selector interaction only after a provider supplies page state; ref-based element handles and multi-tab session multiplexing are named deferred work in the consumer.
- Screenshot delivery returns PNG bytes to the consumer; inline model-visible pixels wait on attachment-backed image blocks in `dsh-tool-browser`.

The subsystem reference — session/page requests and results, availability, `BrowserError` — lives in this package's [src/types.ts](src/types.ts); the [web capability decision](../../../.agents/notes/implemented/architecture/2026-06-24-web-capability-seam.md) records the shared-seam precedent both registries follow.
