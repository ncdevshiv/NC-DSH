# @deepseek-ai/dsh-web-search-searxng

English | [中文](README.zh.md)

A [SearXNG](https://docs.searxng.org)-backed `WebSearchProvider` for the harness [web capability seam](../web/README.md) (`ctx.web`). It calls a SearXNG instance's JSON search API (`GET /search?q=<query>&format=json`) and maps the aggregated `results[]` into the seam's normalized `WebSearchResult`.

This is an **implementation** package: it registers a provider into `ctx.web`, it does not own the `ctx.web` key and it does not register a model-facing tool (that is `@deepseek-ai/dsh-tool-web`). Like `@deepseek-ai/dsh-llm-deepseek`, it is a function/namespace plugin (`inject: ['web']`) that registers its backend, not a default-export service.

## Config

| Key | Default | Meaning |
|---|---|---|
| `baseURL` | `$SEARXNG_BASE_URL` | Instance base URL; `/search` is appended. There is no built-in default because instances are self-hosted; empty or unparseable makes the provider unavailable. |
| `username` | (empty) | Basic-auth username for instances behind an authenticated reverse proxy. |
| `password` | (empty) | Basic-auth password; set together with `username` — a half-configured pair makes the provider unavailable. |

A configured credential pair must be Latin-1 encodable, because HTTP basic auth transmits `user:pass` through `btoa`; anything wider fails at plugin load instead of surfacing as a per-search provider error.

```yaml
- id: web-search-searxng
  name: '@deepseek-ai/dsh-web-search-searxng'
  config:
    baseURL: !!js process.env.SEARXNG_BASE_URL
```

## Mapping

SearXNG aggregates upstream engines into a flat `results[]`. Each entry with a non-blank `url` maps to a `WebSearchSource`: `url` ← `url`, `title` ← `title`, `snippet` ← `content` (omitted when blank), `publishedAt` ← `publishedDate`; url-less entries are skipped. Non-blank `answers[]` instant answers join with `\n\n` into `content`; SearXNG generates nothing beyond them, so otherwise `content` is omitted. SearXNG's JSON API has no result-count control, so `maxResults` is enforced solely by the seam (truncating `sources[]` and setting `truncated`). Provider failures (HTTP errors such as a disabled JSON format, network failure, unparseable or wrong-shape bodies) surface as `WebError` `WEB_PROVIDER_ERROR`; an aborted request surfaces as `WEB_ABORTED`. HTTP redirects are rejected before the `Location` target is contacted and surface as `WEB_PROVIDER_ERROR`.

## Model Experience

Indirectly, through [`dsh-tool-web`](../tool-web/README.md), which retains this provider's `maxResults`-bounded URLs, titles, snippets, and publication dates plus any joined instant answers or its exact `SearXNG search aborted`, `SearXNG search request failed: <error>`, and `SearXNG returned an unprocessable response body: <error>` failures under the consumer's error wrapper while provider-private fields (engine names, infoboxes) remain outside context.

#### KV Cache effect

No direct invalidation; the named consumer owns any request-prefix changes.

## Known Limitations and Deferred Work

- **A private instance is expected** — public SearXNG instances usually disable `format=json` (HTTP 403), so point `baseURL` at an instance you control.
- **Upstream-engine failures surface verbatim** — engine CAPTCHAs and rate limits reach the tool result as `WEB_PROVIDER_ERROR` messages; SearXNG aggregates engines but cannot hide their challenges from the JSON payload.
- **No result-count control** — SearXNG's JSON API has no count parameter, so `maxResults` is enforced only post-hoc by seam truncation.
- **No generated answer beyond instant answers** — `content` carries only SearXNG's instant answers; there is no LLM-generated summary to map.
- **Only `baseURL`/`username`/`password` are exposed** — SearXNG's query controls (categories, language, time range, safe search) wait on provider-neutral Service Definition fields ([seam Agent Note](../../../.agents/notes/implemented/architecture/2026-06-24-web-capability-seam.md)).
- **Abort classification is error-shape-based** — only a `DOMException` named `AbortError` maps to `WEB_ABORTED`; an abort carrying a custom reason (e.g. `dsh-timeout`'s `TimeoutReason`) surfaces as `WEB_PROVIDER_ERROR`.
