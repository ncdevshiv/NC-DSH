# 2026-08-22 — Spectrum port foundation: Provider seam, DeepSeek theme, Button

## Decision

Port the web client to Adobe React Spectrum (`@adobe/react-spectrum` ^3.47.2), chosen over the unstyled `react-aria-components` flavors after an explicit pros/cons review. The port goes through a **wrapper seam**: `ui-primitives` keeps its exported component APIs (including native-shaped props like `onClick`) while internals render Spectrum components; consumer packages keep compiling and migrate to native Spectrum props per-package afterward.

This supersedes the framework ruling in [2026-07-19-web-styling-system](../implemented/process/2026-07-19-web-styling-system.md): "no component library" no longer holds. What survives from that note unchanged: CSS Modules + clsx for all custom styling, tokens-only colors, and the `--dsw-*` sheets in `ui-theme/src/styles/` as the single color authority.

## Structure

- **Theme**: `ui-primitives/src/spectrum/theme.ts` builds `deepseekTheme` by spreading stock `defaultTheme` and adding one override class (`spectrum-vars.module.css`) to both scheme slots. Every override value is a `var(--dsw-*)` reference, so light/dark keeps flipping via `body[data-ds-dark-theme]`; both slots carry identical overrides on purpose, making the Provider's own scheme class visually inert.
- **Provider mount**: `SpectrumSurface` (`ui-primitives/src/spectrum/Surface.tsx`) wraps ui-layout's AppFrame registration — one `<Provider>` at the root, in the static channel so no dynamic bundle requests `@adobe/react-spectrum` through the module table. Feature packages consume Spectrum only through primitives atoms; if that changes, add the specifier to `PLATFORM_MODULES`.
- **Button**: rebuilt on `useButton` from `@react-aria/button` rather than the styled Spectrum component. The styled `SpectrumButton` drops native attributes (verified: `title` never reaches the DOM) and hands refs a focus-handle instead of the element, which broke tooltip contracts across consumer suites. The hook layer supplies press semantics, disabled handling, focus wiring, and keyboard activation while this package owns the element, so every native attribute and handler passes through unchanged and the DOM shape matches the pre-port contract.
- **Test lane**: jsdom suites (`*.client.spec.*`) moved to a dedicated `client-dom` project running `pool: 'vmForks'`. Only the vm pools intercept external-module CSS; under plain forks, any suite importing the primitives barrel died on Spectrum's side-effect `.css` imports with ERR_UNKNOWN_FILE_EXTENSION, and Vitest 4.1.8's inline patterns did not match on Windows paths. Node-half suites that redefine `document`/`window` (modules loader, locale boot) stay in the forks project — VM-context globals are non-configurable.

## Consequences / deferred

- Button forwards the complete native attribute bag (the hook-level surface has no whitelist); exotic handlers keep working because the element is ours.
- Deferred ports, in planned order: Input (TextField needs label/error wiring; its onChange(value) contract breaks 42 call sites and deserves its own pass), Menu→ActionMenu+MenuTrigger, Modal→Dialog+DialogTrigger, Tooltip, Toast→toast queue. Content renderers (ANSI terminal, diff, JSON tree, markdown) stay bespoke permanently — Spectrum has no equivalent DOM.
- Bundle size grows by the react-aria layer (rides the index chunk; it imports react/jsx-runtime so it must never join VENDOR_PACKAGES).
- Visual parity is unverified until `DSH_SNAPSHOT=replay pnpm run test:web` runs against a built frontend; expect follow-up token-mapping fixes for spectrum-css-temp variables this override set does not yet cover.

## Environment caveat

Verified on a Windows host whose only reachable registry is `registry.npmmirror.com` (npmjs is firewalled): `pnpm install`, full-repo typecheck, and `pnpm run test:gui` all pass (275 files / 3,870 tests). `DSH_SNAPSHOT=replay pnpm run test:web` did NOT run: its build step embeds a git commit hash and this checkout has no `.git`. Run that replay plus the browser GIF pass on a normal checkout before merging.
