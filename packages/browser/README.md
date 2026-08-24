# browser/ — browser-automation capability family

English | [中文](README.zh.md)

This family provides provider-neutral browser automation — launch a headless-browser session, navigate, read page state, interact by CSS selector, and capture screenshots — plus the model-facing tools that consume it.

| Package | Role | ctx key |
|---|---|---|
| [`browser/`](browser/README.md) | Defines browser provider registration, selection, and shared errors | `ctx.browser` |
| [`browser-moli/`](browser-moli/README.md) | Drives the local [moli](https://github.com/lexmount/moli) headless browser (`serve` + CDP), one isolated process per session | registers on `ctx.browser` |
| [`tool-browser/`](tool-browser/README.md) | Exposes browser navigation, inspection, interaction, and screenshots to the model | registers on `ctx.tools` |

Selection semantics mirror the [web family](../web/README.md): providers register into one seam; at launch time a configured provider wins, and without configuration exactly one usable provider auto-selects. Providers stay dormant until their external binary resolves (`$MOLI_BINARY` or `PATH`), so mounting them costs nothing until the deployment opts in.

The model-facing surface is opt-in: mount `@deepseek-ai/dsh-tool-browser` in a profile overlay to add the `browser_*` tools.
