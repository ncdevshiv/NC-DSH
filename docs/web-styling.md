# Web UI style reference

English | [中文](web-styling.zh.md)

This reference defines styling ownership and component rules for browser client packages. The current token values live in [`packages/client/ui-theme/src/styles/`](../packages/client/ui-theme/src/styles/); this document does not duplicate that generated-by-source inventory.

## Ownership

[`ui-theme`](../packages/client/ui-theme/README.md) owns the `--dsw-*` static scale, semantic aliases, typography, motion, gradients, shadows, scrollbar styles, and light/dark preference. [`ui-layout`](../packages/client/ui-layout/README.md) applies the resolved theme snapshot to the document. Feature packages consume semantic aliases and do not define another global theme.

[`ui-primitives`](../packages/client/ui-primitives/README.md) owns the Spectrum component layer: `SpectrumSurface` mounts the single root Provider, and its DeepSeek theme restates Spectrum variables as `var(--dsw-*)` references so the token sheets stay the color authority. Light/dark keeps flipping via `body[data-ds-dark-theme]`; the Provider's own scheme classes carry identical values in both slots.

Global style sheets belong in `ui-theme/src/styles/`. Component styles live beside their component as CSS Modules. A component may define a local custom property when its value is part of that component's layout or presentation contract; shared colors, typography, elevation, and motion belong to the theme package.

## Component rules

- Interactive atoms come from `ui-primitives`, whose controls render Adobe React Spectrum. Feature packages do not import `@adobe/react-spectrum` directly; they consume Spectrum behavior through the primitive's API and register no module-table row for it.
- Use CSS Modules and `clsx` for component styling; do not add Tailwind and do not style `spectrum-*` classes from feature CSS.
- Use `--dsw-alias-*` semantic tokens in feature components. Do not copy static palette values or write literal colors there.
- Keep theme selectors out of feature component CSS. Light/dark overrides belong to the theme owner; Spectrum-variable remapping belongs to `ui-primitives`' override module.
- Pair font sizes with line heights and use the theme typography variables when an existing role matches.
- Keep source text, terminal output, and diff lines unwrapped when their component contract requires column preservation; use the shared scrollbar styles rather than component-specific scrollbar selectors.
- Put presentation in CSS. Inline React styles may pass component-local custom-property values but must not encode theme branches.
- Preserve keyboard focus visibility and reduced-motion behavior when adding transitions or hover-only controls.

## Changing the system

Add or change a shared token in the owning `ui-theme` sheet, then consume its semantic alias from feature packages; Spectrum components follow automatically through the `var(--dsw-*)` mapping in `ui-primitives`. Update the owning package reference when a public styling contract changes. Visual behavior follows the [testing policy](testing.md); the [styling-system Agent Note](../.agents/notes/implemented/process/2026-07-19-web-styling-system.md) records token-system rationale and the [Spectrum port note](../.agents/notes/proposed/architecture/2026-08-22-spectrum-port-foundation.md) records the component-library adoption.
