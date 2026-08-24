# @deepseek-ai/dsh-tool-browser

English | [中文](README.zh.md)

The model-facing browser tool suite — `browser_navigate`, `browser_snapshot`, `browser_click`, `browser_type`, and `browser_screenshot` — over the [browser capability seam](../browser/README.md) (`ctx.browser`). It owns model-facing concerns only: tool names, JSON schemas, prompt guidance, per-tool budgets, and output rendering. All browser interaction goes through one shared, serialized session that this package launches lazily on first use and closes when its fiber is disposed; the package never imports a concrete provider.

## Tools

| Tool | Args | Behavior |
|---|---|---|
| `browser_navigate` | `url` (required) | Loads the URL and returns the page state (`url`, `title`, text content). |
| `browser_snapshot` | — | Reads the current page state without navigating. |
| `browser_click` | `selector` (required) | Clicks the first CSS-selector match and returns the resulting page state. |
| `browser_type` | `selector`, `text` (required), `submit?` | Fills the first match after clearing it, optionally pressing Enter; returns the resulting page state. |
| `browser_screenshot` | `full_page?` | Captures a PNG of the current page to a temp file; returns the path and byte size. |

Each tool is registered independently; a product disables an action via config (`{ click: false }`, …). The suite contributes ONE shared guidance section regardless of which tools are enabled, so toggling a single action never rewrites prompt text.

## Config

| Key | Default | Meaning |
|---|---|---|
| `navigate` / `snapshot` / `click` / `typing` / `screenshot` | `true` | Register each tool. |
| `navigationTimeoutMs` | `30000` | Cooperative budget for `browser_navigate`. |
| `actionTimeoutMs` | `15000` | Cooperative budget for in-page actions and screenshots. |
| `maxOutputChars` | `20000` | Cap on one tool's complete rendered output. |

Timeouts attach as `ToolDefinition.timeoutMs` and are enforced by [`dsh-tool-call-timeout-policy`](../../guard/timeout-policy/README.md); no model-facing timeout argument exists.

```yaml
- id: tool-browser
  name: '@deepseek-ai/dsh-tool-browser'
```

## Stable registration

Enabled tools stay visible when no browser provider is usable; execution resolves through `ctx.browser` at call time and fails with the structured `BrowserError` (e.g. `BROWSER_PROVIDER_UNAVAILABLE`) as a readable error result. Provider selection lives entirely inside the seam.

## Model Experience

### System prompt

#### What the model sees

One shared guidance section is registered whenever any browser tool is enabled, and its text does not change with the enabled set.

##### Browser tools guidance

```markdown
Use the browser_* tools to drive a real headless browser: browser_navigate loads a URL, browser_snapshot reads the current page state, browser_click and browser_type act on elements matched by CSS selector (use browser_snapshot first to find selectors), and browser_screenshot saves a PNG of the current page and returns its path. The session persists across calls within a conversation.
```

#### Token effect

Fixed section cost per request while any browser tool is enabled.

#### KV Cache effect

Prefix-stable while enablement is unchanged; toggling any tool does not rewrite the section text, so reuse survives single-action toggles from the first changed schema token only.

### Tool schemas

#### What the model sees

The model sees the generated [`browser_*` schemas](../../../docs/tool-catalog.md#deepseek-aidsh-tool-browser). Timeout budgets are deployment settings, not model arguments.

#### Token effect

Five schemas when everything is enabled; each disabled toggle removes exactly its schema.

#### KV Cache effect

Prefix-stable while definitions are unchanged.

### Results

#### What the model sees

Page-state tools render `Navigated to <url>`-style headers followed by optional title and content, capped at `maxOutputChars` with a truncation notice; failures become `Error: <message>`. Screenshots render a `<path>` envelope naming the saved PNG.

#### Token effect

Data-dependent results are resent until compaction.

#### KV Cache effect

Append-only.

## Known Limitations and Deferred Work

- **Screenshots return a file path, not inline pixels** — attachment-backed image blocks (the `read_image` mechanism) are the named promotion path; today a model with image input composes via `read_image` on the returned path.
- **CSS-selector interaction only** — ref-based element handles wait on a snapshot vocabulary in the seam.
- **One sequential session per context** — concurrent calls serialize; parallel tab work needs a session-multiplexing design first.
- **No web-specific permission policy** — like the web tools, actions execute without `ctx.approval`; confirm-first deployments add a `tools/pre-execute` policy.
