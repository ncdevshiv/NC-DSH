# Agent Note: The desktop dev loop got a three-layer recovery stack

Status: implemented

English | [中文](2026-08-24-desktop-recovery-stack.zh.md)

## Problem

One failure class dominated desktop development: a renderer-side fault left a permanently dead window, and every recovery path required a human. Four concrete manifestations, each root-caused separately:

1. **Lossy hot-swap** — editing a client plugin crashed slot entries mid-swap (services are absent during the teardown→rematerialize window), and the crashed entries never came back: the outlet renders the fresh registration under the SAME `SlotErrorBoundary` instance, whose `failed` state stayed true — a permanent crash face hiding live content.
2. **White screen** — a swap failure above the slot layer (uncaught `cannot create effect on inactive context` while a shell-critical entry applied) killed the mounted tree; every heal below that layer became unreachable, and no later rebuild existed to trigger recovery.
3. **Black screen** — one failed navigation in Electron's main process meant a permanent `backgroundColor`-colored window: `did-fail-load` logged and did nothing.
4. **Zombie stacks** — a launcher whose window died kept its node tree and the launch lock alive; two concurrent stacks then raced `vite build --watch` over one `apps/web/dist` (Windows EBUSY kills the watcher outright), and the stale-artifact chain produced misleading renderer protocol errors.

## Decision

Recovery now exists at three layers, each owned by the process that can actually execute it:

**Renderer (`packages/client/hmr`, `packages/client/ui-renderer`)** — `SlotErrorBoundary` subscribes to a revival channel; after every settled swap client-hmr dispatches `dsh:hmr-swapped` on window and crashed boundaries reset their crash face so children remount with inject factories re-run against current services. client-hmr additionally quiesces (double-rAF + macrotask) before teardown, retries failed swaps with backoff (0/500/2000ms), and treats cordis's inactive-context rejection as terminal for the retry budget — a half-applied generation must never be re-applied onto.

**Electron main (`apps/desktop/main.mjs`)** — supervision deliberately lives in the one process a broken renderer cannot kill: `did-fail-load` retries the navigation with bounded exponential backoff (main-frame only, first-of-burst logging), `render-process-gone` reloads bounded, and a blank-pixel watchdog samples `capturePage()` every 10s — three consecutive uniformly-colored frames (one flat color means dead; rendered chrome always varies) force a reload, budgeted to 3 per 10 minutes. Minimized/hidden windows are skipped: their captures are empty and indistinguishable from a dead renderer.

**Launcher (`scripts/dev-desktop.mjs`)** — a lockfile single-instance guard refuses a second stack (`--replace` kills the previous tree and takes over; `--force` starts alongside at the caller's risk), and `scripts/dev-web.ts` supervises each watch stage with bounded restarts instead of dying on the first EBUSY. `apps/web/vite.config.ts` skips the public-dir copy in watch mode as a third EBUSY defense layer.

**Authoring-time gate** — the dangling-plugin class (rows referencing nonexistent packages that passed self-consistency checks and exploded only at host boot) is now caught by `bun run verify-composition-references`, wired into hygiene: every `name: '@deepseek-ai/…'` across composition YAML must resolve to a workspace package, installed package, or declared subpath export.

## Testing

Verified live on Windows: `--replace` takeover, clean boot to rendered frame (pixel capture via PrintWindow), composition gate passing and failing on an injected dangling reference, full client typecheck. A unit test for the boundary-reset path remains open.

## Alternatives considered

**Auto page-reload from the renderer watchdog alone.** Rejected as insufficient: the bootstrap gap — a renderer whose swap driver or JS context died cannot run its own repair. Only main-process supervision covers that class, which is why the pixel watchdog lives there.

**Per-instance dist directories instead of the single-instance guard.** Rejected: doubles disk and build time to enable a workflow (two concurrent dev stacks) that has no known use; `--replace` covers the real case.

**Vite dev-server HMR instead of the build + stat-poll chain.** Rejected for now: the host serves built artifacts by design (plugin bundles arrive through the client module system); switching the shell to a dev server is a much larger change than hardening what exists.

## Consequences

Renderer faults heal without human intervention at all three layers, the dangling-plugin failure class fails at authoring time instead of host boot, and an EBUSY no longer ends the watch session. The watchdog cannot see minimized or hidden windows (their empty captures are indistinguishable from death), reload budgets bound how much repair each layer attempts before giving up, and the single-instance guard protects only the dev launcher — shipped-app lifecycle stays unsupervised.
