import { describe, expect, it } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import BrowserRuntime, {
  BrowserError,
  type BrowserProvider,
  type BrowserSession,
} from '@deepseek-ai/dsh-browser'

/** A scripted session whose methods record calls and return canned state. */
function makeSession(marker: string): BrowserSession {
  return {
    navigate: () => Promise.resolve({ url: `https://example.com/${marker}` }),
    snapshot: () => Promise.resolve({ url: `https://example.com/${marker}`, title: marker }),
    click: () => Promise.resolve({ url: `https://example.com/${marker}`, content: marker }),
    type: () => Promise.resolve({ url: `https://example.com/${marker}` }),
    screenshot: () => Promise.resolve({ mediaType: 'image/png', data: new Uint8Array([1]) }),
    close: () => Promise.resolve(),
  }
}

/** A scripted provider that hands out one shared scripted session. */
function makeProvider(
  id: string,
  usable: boolean,
  session: BrowserSession = makeSession(id),
): BrowserProvider {
  return {
    id,
    available: () => usable,
    launch: () => Promise.resolve(session),
  }
}

const usable = true
const unusable = false

/** Mount a BrowserRuntime on a fresh root context with the given config. */
async function mountBrowser(
  config: ConstructorParameters<typeof BrowserRuntime>[1] = {},
): Promise<{ ctx: Context; browser: BrowserRuntime }> {
  const ctx = new Context()
  await ctx.plugin(BrowserRuntime, config)
  return { ctx, browser: ctx.browser }
}

describe('BrowserRuntime registration', () => {
  it('registers a provider and unregisters it via the returned disposer', async () => {
    const { browser } = await mountBrowser()
    const session = makeSession('moli')

    const dispose = browser.registerProvider(makeProvider('moli', usable, session))
    await expect(browser.launch()).resolves.toBe(session)

    dispose()
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_UNAVAILABLE' }))
  })

  it('throws BROWSER_DUPLICATE_PROVIDER on a duplicate id', async () => {
    const { browser } = await mountBrowser()
    browser.registerProvider(makeProvider('moli', usable))
    expect(() => browser.registerProvider(makeProvider('moli', usable)))
      .toThrow(expect.objectContaining({ code: 'BROWSER_DUPLICATE_PROVIDER' }))
  })

  it('disposes provider registrations when the contributing fiber is disposed (HMR safety)', async () => {
    const { ctx, browser } = await mountBrowser()
    const session = makeSession('moli')
    const fiber = await ctx.plugin(Object.assign((inner: Context) => {
      inner.browser.registerProvider(makeProvider('moli', usable, session))
    }, { inject: ['browser'] }))
    await expect(browser.launch()).resolves.toBe(session)
    await fiber.dispose()
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_UNAVAILABLE' }))
  })
})

describe('BrowserRuntime launch-time resolution', () => {
  it('throws BROWSER_PROVIDER_UNAVAILABLE when nothing is registered', async () => {
    const { browser } = await mountBrowser()
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_UNAVAILABLE' }))
  })

  it('throws BROWSER_PROVIDER_UNAVAILABLE when providers exist but none are usable', async () => {
    const { browser } = await mountBrowser()
    browser.registerProvider(makeProvider('moli', unusable))
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_UNAVAILABLE' }))
  })

  it('throws BROWSER_PROVIDER_CONFIGURED_MISSING for an unregistered configured id', async () => {
    const { browser } = await mountBrowser({ provider: 'other' })
    browser.registerProvider(makeProvider('moli', usable))
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_CONFIGURED_MISSING' }))
  })

  it('throws BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE for an unusable configured id', async () => {
    const { browser } = await mountBrowser({ provider: 'moli' })
    browser.registerProvider(makeProvider('moli', unusable))
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE' }))
  })

  it('throws BROWSER_PROVIDER_AMBIGUOUS rather than picking by order', async () => {
    const { browser } = await mountBrowser()
    browser.registerProvider(makeProvider('moli', usable))
    browser.registerProvider(makeProvider('other', usable))
    await expect(browser.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_AMBIGUOUS' }))
  })

  it('launches the configured provider even when another usable provider is registered', async () => {
    const { browser } = await mountBrowser({ provider: 'other' })
    browser.registerProvider(makeProvider('moli', usable))
    const other = makeSession('other')
    browser.registerProvider(makeProvider('other', usable, other))
    await expect(browser.launch()).resolves.toBe(other)
  })

  it('ignores unusable providers when auto-selecting', async () => {
    const { browser } = await mountBrowser()
    const moliSession = makeSession('moli')
    browser.registerProvider(makeProvider('moli', usable, moliSession))
    browser.registerProvider(makeProvider('broken', unusable))
    await expect(browser.launch()).resolves.toBe(moliSession)
  })

  it('does not let registration order change auto-selection', async () => {
    const otherSession = makeSession('other')
    const first = await mountBrowser()
    first.browser.registerProvider(makeProvider('moli', unusable))
    first.browser.registerProvider(makeProvider('other', usable, otherSession))
    await expect(first.browser.launch()).resolves.toBe(otherSession)

    const second = await mountBrowser()
    second.browser.registerProvider(makeProvider('other', usable, otherSession))
    second.browser.registerProvider(makeProvider('moli', unusable))
    await expect(second.browser.launch()).resolves.toBe(otherSession)
  })

  it('returns the selected provider session and forwards the abort signal', async () => {
    const { browser } = await mountBrowser()
    const seen: (AbortSignal | undefined)[] = []
    const session = makeSession('moli')
    browser.registerProvider({
      id: 'moli',
      available: () => usable,
      launch: (signal) => {
        seen.push(signal)
        return Promise.resolve(session)
      },
    })
    const controller = new AbortController()
    await expect(browser.launch(controller.signal)).resolves.toBe(session)
    expect(seen[0]).toBe(controller.signal)
  })
})

describe('BrowserError', () => {
  it('is a HarnessError carrying its code', () => {
    const error = new BrowserError('boom', 'BROWSER_INVALID_URL')
    expect(error.code).toBe('BROWSER_INVALID_URL')
    expect(error.name).toBe('BrowserError')
  })
})
