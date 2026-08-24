# Agent Note: Spectrum port foundation: Provider seam, DeepSeek theme, Button

Status: proposed

English | [中文](2026-08-22-spectrum-port-foundation.zh.md)

## Problem

The web client's interactive atoms need focus, press, and keyboard semantics plus a maintained visual system, and the styling ruling in [2026-07-19-web-styling-system](../../implemented/process/2026-07-19-web-styling-system.md) — "no component library" — no longer holds against that need. The port must not break consumer packages compiling against the current `ui-primitives` APIs, and it must keep tokens-only theming (`--dsw-*`) as the single color authority.

## Proposal

Port the web client to Adobe React Spectrum (`@adobe/react-spectrum` ^3.47.2) through a wrapper seam: `ui-primitives` keeps its exported component APIs (including native-shaped props like `onClick`) while internals render Spectrum components; consumer packages keep compiling and migrate to native Spectrum props per-package afterward.

What survives from [2026-07-19-web-styling-system](../../implemented/process/2026-07-19-web-styling-system.md) unchanged: CSS Modules + clsx for all custom styling, tokens-only colors, and the `--dsw-*` sheets in `ui-theme/src/styles/` as the single color authority.

## Structure

- **Theme**: `ui-primitives/src/spectrum/theme.ts` builds `deepseekTheme` by spreading stock `defaultTheme` and adding one override class (`spectrum-vars.module.css`) to both scheme slots. Every override value is a `var(--dsw-*)` reference, so light/dark keeps flipping via `body[data-ds-dark-theme]`; both slots carry identical overrides on purpose, making the Provider's own scheme class visually inert.
- **Provider mount**: `SpectrumSurface` (`ui-primitives/src/spectrum/Surface.tsx`) wraps ui-layout's AppFrame registration — one `<Provider>` at the root, in the static channel so no dynamic bundle requests `@adobe/react-spectrum` through the module table. Feature packages consume Spectrum only through primitives atoms; if that changes, add the specifier to `PLATFORM_MODULES`.
- **Button**: rebuilt on `useButton` from `@react-aria/button` rather than the styled Spectrum component, so every native attribute and handler passes through unchanged and the DOM shape matches the pre-port contract.
- **Test lane**: jsdom suites (`*.client.spec.*`) move to a dedicated `client-dom` project running `pool: 'vmForks'`. Only the vm pools intercept external-module CSS; under plain forks, any suite importing the primitives barrel dies on Spectrum's side-effect `.css` imports with ERR_UNKNOWN_FILE_EXTENSION, and Vitest 4.1.8's inline patterns did not match on Windows paths. Node-half suites that redefine `document`/`window` (modules loader, locale boot) stay in the forks project — VM-context globals are non-configurable.
- **Deferred ports, in planned order**: Input (TextField needs label/error wiring; its onChange(value) contract breaks 42 call sites and deserves its own pass), Menu→ActionMenu+MenuTrigger, Modal→Dialog+DialogTrigger, Tooltip, Toast→toast queue. Content renderers (ANSI terminal, diff, JSON tree, markdown) stay bespoke permanently — Spectrum has no equivalent DOM.

## Alternatives considered

**Keep the bespoke unstyled primitives.** What the old ruling mandated; lost because each atom reimplements and re-tests focus, press, disabled, and keyboard behavior Spectrum maintains upstream, and the duplication was already showing in tooltip and attribute-pass-through gaps.

**react-aria-components unstyled flavors.** Lost the explicit pros/cons review against Spectrum: they supply behavior without visuals, leaving every visual decision in-house, which is the cost this port exists to shed.

**The styled `SpectrumButton` component.** Rejected for Button: it drops native attributes (verified: `title` never reaches the DOM) and hands refs a focus handle instead of the element, which broke tooltip contracts across consumer suites; the `useButton` hook supplies press semantics, disabled handling, focus wiring, and keyboard activation while this package owns the element.

## Acceptance criteria

`bun install`, full-repo typecheck, and `bun run test:gui` pass on the mirror-registry Windows host used for development. Before merge, `DSH_SNAPSHOT=replay bun run test:web` and the browser GIF pass run against a built frontend on a normal checkout — the build step embeds a git commit hash, so they cannot run where `.git` is absent.

## Risks

Bundle size grows by the react-aria layer; it rides the index chunk and imports react/jsx-runtime, so it must never join VENDOR_PACKAGES. Visual parity is unverified until the replay run — expect follow-up token-mapping fixes for spectrum-css-temp variables this override set does not yet cover. Only Button ports in this slice; each deferred atom carries its own call-site migration cost, largest for Input.
