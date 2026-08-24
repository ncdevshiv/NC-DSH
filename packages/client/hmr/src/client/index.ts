/**
 * client-hmr, browser half: hot-reload driver for client plugin entries.
 *
 * Listens on the host's system SSE channel (`GET /plugins/events`); on a
 * `rebuilt` frame it reloads the entry's bundle and swaps the cordis
 * fiber in place. Every graph entry is a plugin bundle
 * — `immediately` rows differ only in stage-one prefetch (a boot
 * optimization), so all rostered plugin packages share these reload semantics;
 * normal packages (react family, cordis, shell, pure libs) are not entries
 * and shell changes still mean a page reload. Cascade is zero-touch:
 * downstream fibers key their activation epoch on provider fiber uids
 * (vendor/cordis/src/fiber.ts `_refresh`), so replacing a provider fiber
 * re-cascades natively — reloading a data-layer plugin (connection/runtime)
 * cascades into its UI dependents with no HMR-side bookkeeping.
 *
 * Reload order (lazy CJS table): invalidate (drop the stale factory and
 * materialized record) → prefetch (load and register the fresh
 * factory) → registry-first teardown → drain old fiber unload → remove
 * owned `<style data-plugin>` tags → `entry.refresh()` materializes the new
 * factory. Invalidate MUST precede prefetch: a live factory makes prefetch
 * a no-op, and re-executing a bundle over an undeleted registration is a
 * loud duplicate. The swap is safe because execution is pure registration
 * under the lazy model — every module side effect (CSS injection included)
 * lives in the factory closure and runs at materialization, inside
 * refresh(). That also keeps the CSS ordering guarantee: owned styles are
 * removed after the old fiber's disposers drained (SlotCore one-owner
 * unregister) and before materialization re-injects tags under the same
 * stable tag ids.
 *
 * Failure window: if prefetch rejects after invalidate, the module is left
 * unregistered while the OLD fiber keeps running untouched (teardown never
 * started) — degraded but recoverable, the next rebuilt frame retries from
 * scratch. Consistent with the no-rollback policy below. Known dev-only
 * race: a rebuilt frame overlapping a still-in-flight boot arrival shares
 * that arrival's task and may materialize the pre-rebuild bytes; the next
 * rebuilt frame self-heals.
 *
 * Why not the naive `entry.fiber.dispose()` → `entry.refresh()` path:
 * 1. `Entry.fiber` is never cleared on dispose (vendor/loader/src/config/
 *    entry.ts assigns it only in `_init`), so `refresh()` hits its
 *    `if (this.fiber) return` guard and no-ops.
 * 2. A bare `fiber.dispose()` lands in Loader's self-dispose branch
 *    (vendor/loader/src/index.ts `internal/plugin` case 4: the registry
 *    still holds the runtime at emit time), which flags the entry
 *    `disabled: true` — permanently.
 * vendor/hmr's reload skeleton documents the fix: delete the runtime record
 * FIRST (`registry.delete` → case 4 returns early, the entry stays enabled),
 * then rebuild. `entry.fiber` is additionally cleared so
 * `entry.refresh()` re-imports and re-plugins through the Loader's own
 * `_init` (entry-resolved config, automatic `fiber.entry` rebinding) instead
 * of hand-rolling `registry.plugin`. Client entries have exactly one fiber
 * per runtime, so `registry.delete` never collaterally disposes siblings.
 *
 * Self-reload: this plugin is itself a graph entry, so a rebuilt frame may
 * name it. The in-flight reload keeps running in the old bundle's closure
 * (its EventSource closes with the old fiber's effects); the new bundle's
 * apply opens a fresh channel. Frames arriving during the gap are lost —
 * acceptable for the dev channel, the next rebuild renotifies.
 *
 * Failure policy: bounded self-recovery, no rollback. A failed swap retries
 * with backoff (0/500/2000ms) before giving up until the next rebuilt frame;
 * an import failure leaves the entry fiberless (retryable the same way); an
 * apply failure leaves a FAILED fiber for the shell's status projection.
 * Every settled swap — success or final failure — announces `dsh:hmr-swapped`
 * on window so crashed slot boundaries reset against the live generation.
 * All failures log loudly.
 */
import type { Context } from '@deepseek-ai/cordis'
import type { Entry, Loader } from '@deepseek-ai/cordis-plugin-loader'
import type { PluginsEventFrame } from '../events.ts'
import { EVENTS_ENDPOINT } from '../events.ts'

export type { PluginsEventFrame } from '../events.ts'
export { EVENTS_ENDPOINT } from '../events.ts'

/** Cordis plugin name. */
export const name = 'client-hmr'

/** Required services: the vendored Loader (entry governance) and the client module system (boot provide, service name `modules`). */
export const inject = ['loader', 'modules']

/** Find the loader entry whose module specifier is `id` (entry tree ids are random; the package name lives in `options.name`). */
function findEntry(loader: Loader, id: string): Entry | undefined {
  for (const entry of loader.entries()) {
    if (entry.options.name === id) return entry
  }
  return undefined
}

/** Remove every `<style data-plugin>` tag owned by `id` (attribute compared verbatim — no CSS-selector escaping pitfalls). */
function removeOwnedStyles(id: string): void {
  for (const el of document.querySelectorAll('style[data-plugin]')) {
    if (el.getAttribute('data-plugin') === id) el.remove()
  }
}

/**
 * Quiesce point before the destructive phase: let in-flight browser work
 * settle so no React commit or microtask is mid-render while registrations
 * are absent. Without it, a render landing in the teardown→rematerialize gap
 * throws "service unavailable" inside an entry boundary and abdicates that
 * entry — recoverable since the boundary self-heals on the swap signal, but
 * avoidable churn. Two animation frames straddle one full commit cycle.
 */
function settle(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => { requestAnimationFrame(() => { setTimeout(resolve, 0) }) })
  })
}

/** Swap-settled announcement: crashed slot boundaries reset on this signal. */
function announceSwapped(id: string): void {
  window.dispatchEvent(new CustomEvent('dsh:hmr-swapped', { detail: { id } }))
}

// ── white-screen watchdog ───────────────────────────────────────────────────
//
// A swap (or boot) failure above the slot layer — an uncaught throw while a
// shell-critical entry applies — leaves the renderer blank, and every heal
// below that layer is unreachable because there is no mounted tree left to
// heal. The watchdog is the top of the recovery ladder: after boot and after
// every settled swap it checks for real rendered content and, finding none,
// performs exactly ONE automatic page reload per episode. A sessionStorage
// stamp (survives the reload, cleared on success) makes the second blank
// window in a row log loudly and stop — a reload loop would hide the very
// failure this exists to surface.

const WATCHDOG_DELAY_MS = 8_000
const WATCHDOG_MIN_TEXT_CHARS = 40
const WATCHDOG_GUARD_KEY = 'dsh:hmr-watchdog-reloaded'
const WATCHDOG_GUARD_WINDOW_MS = 60_000

/** Heuristic liveness: the shell always renders chrome text; a truly blank renderer does not. */
function rendererLooksAlive(): boolean {
  const body = document.body
  if (body === null) return false
  if (document.querySelector('canvas, video') !== null) return true
  return body.innerText.trim().length >= WATCHDOG_MIN_TEXT_CHARS
}

function armWatchdog(ctx: Context, delayMs: number = WATCHDOG_DELAY_MS): void {
  window.setTimeout(() => {
    if (rendererLooksAlive()) {
      sessionStorage.removeItem(WATCHDOG_GUARD_KEY)
      return
    }
    const stamped = Number(sessionStorage.getItem(WATCHDOG_GUARD_KEY) ?? '0')
    if (Number.isFinite(stamped) && stamped > 0 && Date.now() - stamped < WATCHDOG_GUARD_WINDOW_MS) {
      ctx.logger.error('client-hmr: renderer blank even after the watchdog reload; not reloading again — inspect the renderer console for the uncaught error')
      return
    }
    ctx.logger.error(`client-hmr: renderer blank ${String(delayMs)}ms after boot/swap; forcing one page reload`)
    sessionStorage.setItem(WATCHDOG_GUARD_KEY, String(Date.now()))
    window.location.reload()
  }, delayMs)
}

/**
 * Mount the HMR driver: subscribe to the system SSE channel and hot-swap
 * rebuilt entries.
 * @param ctx - plugin context with `loader` and `modules` available.
 */
export function apply(ctx: Context): void {
  // Both are declared injections (typed Context merges: `modules` from the
  // client module loader package, `loader` from the vendored Loader).
  const modLoader = ctx.modules
  const loader: Loader = ctx.loader

  async function reload(id: string): Promise<void> {
    const entry = findEntry(loader, id)
    if (entry === undefined) {
      ctx.logger.warn(`client-hmr: rebuilt frame for unknown entry "${id}" (not in the loader tree)`)
      return
    }
    // Invalidate first (drop stale factory + record — a live factory makes
    // prefetch a no-op and re-registration a loud duplicate), then run the
    // async half while the old fiber still serves: script loading registers
    // the fresh factory with zero side effects (lazy CJS — module bodies run
    // at materialization, not execution).
    modLoader.invalidate(id)
    await modLoader.prefetch(id)

    const oldFiber = entry.fiber
    if (oldFiber !== undefined) {
      // Quiesce first: in-flight commits drain against the OLD, fully
      // registered world (see settle()).
      await settle()
      // Registry-first teardown (see module comment): the runtime record must
      // be gone before the fiber's disposer emits internal/plugin, or the
      // Loader flags the entry disabled.
      const runtime = oldFiber.runtime
      if (runtime !== null) entry.ctx.registry.delete(runtime.callback)
      // Drain the unload: effect disposers (slots, subscriptions) must finish
      // before the new bundle executes and the new apply re-registers.
      while (oldFiber.inertia !== undefined) await oldFiber.inertia
      delete entry.fiber
    }
    // Old owned styles go before materialization re-injects them (the CSS
    // idempotency guard keys on stable tag ids).
    removeOwnedStyles(id)
    // Re-init through the entry: fiber cleared above, so refresh() re-imports
    // — materializing the prefetched factory (CSS injects here) — and
    // re-plugins under the entry context. Import failures are logged by
    // Entry._init and leave the entry fiberless (retryable).
    await entry.refresh()
    // Surface apply failures loudly (no rollback, FAILED state stays).
    await entry.fiber?.await()
  }

  /**
   * True for cordis's inactive-context rejection — the signature of applying
   * or continuing work on a fiber whose uid was cleared. A swap that produced
   * one has left the entry half-applied; an immediate retry would re-apply
   * onto that dead context and throw again, so this failure ends the retry
   * budget and waits for the next rebuilt frame (fresh bytes, fresh state).
   */
  function isInactiveContextError(error: unknown): boolean {
    return error instanceof Error && error.message.includes('cannot create effect on inactive context')
  }

  // Serialize reloads: frames can arrive faster than a swap completes, and
  // interleaved dispose/execute chains would corrupt the single-slot handoff.
  // A failed swap retries with backoff (bounded): transient import/apply
  // failures used to strand an entry fiberless until the NEXT rebuild, which
  // for a shell-critical plugin meant a dead window. After the final attempt
  // (or success) the swap signal still fires so crashed slot boundaries get
  // their one chance to reset against whatever generation is live.
  let queue: Promise<void> = Promise.resolve()
  const RELOAD_DELAYS_MS = [0, 500, 2000] as const
  const handle = (frame: PluginsEventFrame): void => {
    switch (frame.type) {
      case 'rebuilt':
        queue = queue.then(async () => {
          for (let attempt = 0; attempt < RELOAD_DELAYS_MS.length; attempt++) {
            try {
              if (RELOAD_DELAYS_MS[attempt] !== 0) {
                await new Promise((resolve) => { setTimeout(resolve, RELOAD_DELAYS_MS[attempt]) })
              }
              await reload(frame.id)
              announceSwapped(frame.id); armWatchdog(ctx)
              return
            } catch (error: unknown) {
              const last = attempt === RELOAD_DELAYS_MS.length - 1
              ctx.logger.error(
                `client-hmr: reload of "${frame.id}" failed (attempt ${String(attempt + 1)})${last ? ' — giving up until the next rebuilt frame' : ''}`,
              )
              ctx.logger.error(error)
              if (last || isInactiveContextError(error)) {
                announceSwapped(frame.id); armWatchdog(ctx)
                return
              }
            }
          }
        })
        break
      case 'graph':
        // Connect-time snapshot, unused. The loader's cached graph rev
        // goes stale after rebuilds — harmless, since prefetch hits the
        // network anyway (host serves bundles no-cache); graph rev refresh
        // lands with the reconnect-handshake mechanism.
        break
      default:
        // Merge-extensible frame union: unknown frame types from newer hosts
        // are ignored by design.
        break
    }
  }

  ctx.effect(() => {
    const source = new EventSource(EVENTS_ENDPOINT)
    source.addEventListener('message', (event: MessageEvent<string>) => {
      let frame: PluginsEventFrame
      try {
        frame = JSON.parse(event.data) as PluginsEventFrame
      } catch {
        // Wire boundary: a malformed dev-channel frame is dropped loudly.
        ctx.logger.warn(`client-hmr: unparseable event frame: ${event.data}`)
        return
      }
      handle(frame)
    })
    return () => { source.close() }
  }, 'client-hmr: event source')

  // Boot arm: a renderer that never painted gets one automatic reload.
  armWatchdog(ctx)
}
