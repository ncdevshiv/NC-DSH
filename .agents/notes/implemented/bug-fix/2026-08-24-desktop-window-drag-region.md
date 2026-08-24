# Agent Note: Desktop window caption-band drag region

Status: implemented

English | [中文](2026-08-24-desktop-window-drag-region.zh.md)

## Problem

The Electron desktop shell (`apps/desktop`) renders the web GUI in a frameless window (`titleBarStyle: 'hidden'` plus `titleBarOverlay`). Outside the three native caption buttons, such a window has no draggable chrome of its own: dragging works only where the page marks elements with `-webkit-app-region: drag`. The client declared no such region anywhere, so once the shell adopted the hidden title bar the window could no longer be moved at all. Resize borders and the caption buttons kept working, which made the defect look like a partial glitch instead of a missing contract.

## Decision

The web client owns the caption band. `apps/web/index.html` renders an inert strip element (`#dsh-titlebar-drag`, `display: none` by default). `apps/web/src/main.ts` sets `html[data-dsh-desktop]` when the shell's preload bridge (`window.dshDesktop`) is present — the same feature-detect `ThemePresenter` and the boot theme script already use. `packages/client/web/src/base.css` keys the real rules on that attribute: the strip becomes a fixed drag handle sized by the Window Controls Overlay environment variables (`env(titlebar-area-x/y/width/height)`), and the band is reserved from layout (`body` switches to `border-box` with `padding-top: env(titlebar-area-height, 32px)`, while `#root`'s viewport min-heights are re-based by the same amount so they cannot push content back under the band).

The behavior is pinned at the OS level: `WM_NCHITTEST` on the live window answers `HTCAPTION` across the band except the native button zone and `HTCLIENT` below it, and a synthetic mouse drag moves the window. Browsers never set the attribute and render exactly as before.

## Alternatives considered

- **Gate on `@media (display-mode: window-controls-overlay)`**, the installed-PWA pattern: measured false in Electron even with an active overlay, while `env(titlebar-area-*)` resolves correctly — the strip would have stayed dead in the one context that needs it. The manifest also ships `display: fullscreen` without `display_override`, so no installed-PWA consumer exists today.
- **Overlay the strip on unshifted content**: rejected because the top band holds interactive chrome (sidebar brand, conversation hero controls); reserving the band keeps every control clickable.
- **Revert to the OS title bar**: rejects the theme-synced overlay chrome the shell intentionally added.
- **Read geometry from `navigator.windowControlsOverlay.getTitlebarAreaRect()`**: redundant — CSS `env()` carries the geometry; the one-shot attribute gate is the only JavaScript.

## Consequences

Inside the desktop shell the whole top band is drag-only: dragging moves the window, double-clicking toggles maximize, and app content starts one band lower. Native caption buttons keep hit-test precedence over the page region, so min/max/close work even where the strip spans them. An installed-PWA WCO context would need the standard media-query twin added alongside the attribute gate; none exists today.
