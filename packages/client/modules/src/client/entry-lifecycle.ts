/** Loader lifecycle operations shared by live graph reconciliation and code replacement. */
import type { Entry } from '@deepseek-ai/cordis-plugin-loader'

/**
 * Release a runtime before clearing its entry fiber so Loader refresh can import new code.
 * Registry deletion prevents Loader from treating replacement as a user disable.
 * @param entry - Entry retained for code replacement.
 */
export async function tearDownEntryFiber(entry: Entry): Promise<void> {
  const fiber = entry.fiber
  if (fiber === undefined) return
  const runtime = fiber.runtime
  /* v8 ignore next -- Loader entries own plugin fibers; only the root context has a null runtime. */
  if (runtime !== null) entry.ctx.registry.delete(runtime.callback)
  while (fiber.inertia !== undefined) await fiber.inertia
  delete entry.fiber
}

/**
 * Remove styles after their plugin's effect cleanup has settled.
 * @param id - Package whose factory owns the style tags.
 */
export function removeOwnedStyles(id: string): void {
  if (typeof document === 'undefined') return
  for (const el of document.querySelectorAll('style[data-plugin]')) {
    if (el.getAttribute('data-plugin') === id) el.remove()
  }
}

/**
 * Detach a package's style tags and return them, so a failed replacement can
 * re-attach the still-active plugin's styles.
 * @param id - Package whose factory owns the style tags.
 * @returns the detached elements, in document order.
 */
export function detachOwnedStyles(id: string): Element[] {
  if (typeof document === 'undefined') return []
  const detached: Element[] = []
  for (const el of document.querySelectorAll('style[data-plugin]')) {
    if (el.getAttribute('data-plugin') !== id) continue
    detached.push(el)
    el.remove()
  }
  return detached
}

/**
 * Re-attach style elements detached from a running plugin before a replacement.
 * @param elements - elements returned by {@link detachOwnedStyles}.
 */
export function restoreOwnedStyles(elements: readonly Element[]): void {
  if (typeof document === 'undefined') return
  for (const el of elements) document.head.append(el)
}
