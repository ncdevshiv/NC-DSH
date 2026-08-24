# Browser Automation

English | [中文](browser.zh.md)

The browser-automation seam — a capability seam on one `ctx.browser` service, split across packages: Service Definition ([dsh-browser](../../packages/browser/browser), `ctx.browser` + the provider registry), Service Provider ([dsh-browser-moli](../../packages/browser/browser-moli), which drives the local [moli](https://github.com/lexmount/moli) headless browser over CDP), and Consumer ([dsh-tool-browser](../../packages/browser/tool-browser), the `browser_*` tool schemas). Browser is one optional capability: its vocabulary lives here, not in [core.md](core.md). A provider swap does not change how the model asks for navigation, interaction, or a screenshot.

Source: [`packages/browser/browser/src/types.ts`](../../packages/browser/browser/src/types.ts)

## Sessions are stateful and sequential

A `BrowserSession` persists across calls within an agent run: navigation, cookies, and storage carry over, so `browser_snapshot` sees what the last action produced. The consumer (tool-browser) owns one shared session per context, launches it lazily through `ctx.browser.launch()`, serializes every operation behind the previous one, and closes it when its fiber is disposed. Provider implementations own the process or connection underneath; each moli launch spawns an isolated `moli serve` child.

## Requests and results

Interaction requests carry CSS selectors (`click`, `type` with optional Enter submission); `navigate` takes an absolute HTTP(S) URL; `screenshot` captures the viewport or, with `fullPage`, the whole scrollable page as PNG bytes. Every interaction resolves to a `BrowserPageState`: the final URL plus optional title and bounded text content. Providers bound content length; consumers bound rendered output.

## Availability and selection

A provider's `available()` is a cheap local check (for moli, a memoized `--version` probe) and never spawns a long-lived process. Selection mirrors [web](web.md): a configured id wins; without configuration exactly one usable provider auto-selects; ambiguity and unavailability reject with structured codes rather than order-dependent picks.

## Errors

`BrowserError extends HarnessError` carries an open-string `code`, like `WebError`. Seam-neutral codes come from the shared runtime contract: `BROWSER_PROVIDER_UNAVAILABLE`, `BROWSER_PROVIDER_CONFIGURED_MISSING`, `BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE`, `BROWSER_PROVIDER_AMBIGUOUS`, `BROWSER_DUPLICATE_PROVIDER`, `BROWSER_ABORTED`, `BROWSER_SESSION_CLOSED`. Implementation-owned codes cover startup timeout, navigation timeout, invalid navigation targets, missing elements, capture failure, oversized captures, and failed in-page evaluation; consumers must tolerate unknown codes.

<!-- BEGIN GENERATED cordis-surface (gen-cordis-catalog.ts) — do not edit between markers -->

<a id="cordis-surface"></a>

## Cordis API

Generated from source by `scripts/gen-cordis-catalog.ts` (verified fresh by `bun run verify-cordis-catalog` in doc-sync; regenerate with `bun run gen-cordis-catalog`) — this section is byte-identical in both language sides of the page. Signature blocks use a `ts cordis-catalog` fence and keep the original source JSDoc; dispatch modes are defined in the [primer](../cordis-primer.md#dispatch-modes), and the framework-inherited `ctx` API lives in [cordis-api/inherited.md](../cordis-api/inherited.md).

<a id="ctxbrowser--browserruntime"></a>

### `ctx.browser` — `BrowserRuntime`

The browser-automation service. Registered as `ctx.browser` (one instance per context).

Selection semantics (resolved at launch time, never order-dependent):

- A configured id that is registered and `available()` → that provider.
- A configured id not registered → `BROWSER_PROVIDER_CONFIGURED_MISSING`.
- A configured id registered but unavailable → `BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE`.
- No id configured, exactly one registered usable provider → that provider.
- No id configured, multiple usable providers → `BROWSER_PROVIDER_AMBIGUOUS`.
- No id configured, no usable provider → `BROWSER_PROVIDER_UNAVAILABLE`.

```ts cordis-catalog
/**
 * Register a browser provider. Throws {@link BrowserError} `BROWSER_DUPLICATE_PROVIDER`
 * if its id is already registered. Returns a disposer; disposed with the calling fiber.
 * @param provider - the provider; its `id` is the registry key.
 * @returns the disposer that unregisters the provider.
 */
registerProvider(provider: BrowserProvider): () => void

/**
 * Launch a session through the selected provider. Resolves the provider at call time with the
 * selection rules above; throws {@link BrowserError} when no provider can run. The caller owns
 * the returned session and must eventually call its `close()`.
 * @param signal - optional cancellation signal for the startup phase.
 * @returns the launched session.
 */
async launch(signal?: AbortSignal): Promise<BrowserSession>
```

Source: [`packages/browser/browser/src/index.ts:68`](../../packages/browser/browser/src/index.ts)
<!-- END GENERATED cordis-surface -->
