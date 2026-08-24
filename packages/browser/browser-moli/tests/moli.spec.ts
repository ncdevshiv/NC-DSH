import { describe, expect, it, vi } from 'vitest'
import { Context } from '@deepseek-ai/cordis'
import BrowserRuntime from '@deepseek-ai/dsh-browser'
import {
  MOLI_BROWSER_PROVIDER_ID,
  MoliBrowserProvider,
  buildServeArgv,
} from '@deepseek-ai/dsh-browser-moli'
import * as moliPlugin from '@deepseek-ai/dsh-browser-moli'

/** A scripted WebSocket whose sends are answered by the test's handler. */
class FakeWebSocket {
  private readonly listeners = new Map<string, Array<(event: { data?: string }) => void>>()
  readonly sent: string[] = []

  constructor(private readonly handler: (message: { id?: number; method?: string; params?: Record<string, unknown> }) => unknown) {}

  addEventListener(type: string, listener: (event: { data?: string }) => void): void {
    const queue = this.listeners.get(type) ?? []
    queue.push(listener)
    this.listeners.set(type, queue)
    if (type === 'open') queueMicrotask(() => { listener({}) })
  }

  removeEventListener(): void {}

  send(raw: string): void {
    this.sent.push(raw)
    const message = JSON.parse(raw) as { id?: number }
    const result = this.handler(message)
    queueMicrotask(() => {
      this.emit('message', { data: JSON.stringify({ id: message.id, result }) })
    })
  }

  close(): void {}

  /** Deliver one raw protocol frame from the endpoint side. */
  deliverEvent(frame: Record<string, unknown>): void {
    this.emit('message', { data: JSON.stringify(frame) })
  }

  private emit(type: string, event: { data?: string }): void {
    for (const listener of [...this.listeners.get(type) ?? []]) listener(event)
  }
}

interface Harness {
  provider: MoliBrowserProvider
  kill: ReturnType<typeof vi.fn>
  socket: FakeWebSocket
  evaluateValues: string[]
}

/** Wire a provider whose process/HTTP/WebSocket boundaries are all fakes. */
function makeHarness(overrides: Partial<ConstructorParameters<typeof MoliBrowserProvider>[0]> = {}): Harness {
  const kill = vi.fn()
  const evaluateValues = ['https://example.com/', 'Page title', 'Body text']
  let socket!: FakeWebSocket
  const provider = new MoliBrowserProvider({
    binaryPath: 'moli-fake',
    startupTimeoutMs: 500,
    navigationTimeoutMs: 5_000,
    cdpTimeoutMs: 30_000,
    maxContentChars: 100_000,
    settleMs: 150,
    probeTimeoutMs: 5_000,
    pollEveryMs: 1,
    extraServeArgs: [],
    prober: () => ({ status: 0, error: null }),
    spawnFn: () => ({ pid: 4321, kill }),
    fetchFn: overrides.fetchFn ?? (async (url: string) => {
      if (url.endsWith('/json/version')) return { ok: true } as Response
      if (url.endsWith('/json/list')) {
        return { ok: true, json: async () => [{ type: 'page', webSocketDebuggerUrl: 'ws://cdp' }] } as unknown as Response
      }
      throw new Error(`unexpected fetch ${url}`)
    }),
    wsFactory: () => {
      socket = new FakeWebSocket((message) => {
        if (message.method === 'Runtime.evaluate') return { result: { value: evaluateValues.shift() ?? '' } }
        if (message.method === 'Page.captureScreenshot') return { data: 'aGk=' }
        return {}
      })
      return socket as unknown as WebSocket
    },
    ...overrides,
  })
  return {
    provider,
    kill,
    get socket(): FakeWebSocket {
      return socket
    },
    evaluateValues,
  }
}

/** Launch through the harness and yield past the real loopback port bind. */
async function launchWithSocket(harness: ReturnType<typeof makeHarness>): Promise<Awaited<ReturnType<typeof harness.provider.launch>>> {
  const launching = harness.provider.launch()
  await new Promise(resolve => setTimeout(resolve, 10))
  return launching
}

describe('MoliBrowserProvider availability', () => {
  it('probes once and memoizes the result', () => {
    const prober = vi.fn(() => ({ status: 0, error: null }))
    const { provider } = makeHarness({ prober })
    expect(provider.available()).toBe(true)
    expect(provider.available()).toBe(true)
    expect(prober).toHaveBeenCalledTimes(1)
    expect(prober).toHaveBeenCalledWith('moli-fake', 5_000)
  })

  it('is unavailable when the probe fails and launch refuses without probing again', async () => {
    const prober = vi.fn(() => ({ status: null, error: new Error('ENOENT') }))
    const { provider } = makeHarness({ prober })
    expect(provider.available()).toBe(false)
    await expect(provider.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_UNAVAILABLE' }))
    expect(prober).toHaveBeenCalledTimes(1)
  })
})

describe('MoliBrowserProvider launch lifecycle', () => {
  it('spawns serve and hands out a working session whose close kills the child', async () => {
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    const page = await session.snapshot()
    expect(page).toEqual({ url: 'https://example.com/', title: 'Page title', content: 'Body text' })
    await session.close()
    expect(harness.kill).toHaveBeenCalled()
  })

  it('navigates, waits for load, and reads the resulting state', async () => {
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    harness.evaluateValues.length = 0
    harness.evaluateValues.push('https://example.com/after', 'After', 'After body')
    const navigating = session.navigate({ url: 'https://example.com/' })
    await new Promise(resolve => setTimeout(resolve, 5))
    harness.socket.deliverEvent({ method: 'Page.loadEventFired' })
    await expect(navigating).resolves.toEqual({
      url: 'https://example.com/after',
      title: 'After',
      content: 'After body',
    })
    await session.close()
  })

  it('resolves when the load event fires while the navigate command is in flight', async () => {
    // Ordering guard: the load-event waiter must be registered before the
    // navigate command goes out. An endpoint that answers and fires the event
    // within the same turn must not stall the navigation out to its deadline.
    const evaluateValues = ['https://example.com/after', 'After', 'After body']
    let socket!: FakeWebSocket
    const harness = makeHarness({
      navigationTimeoutMs: 250,
      wsFactory: () => {
        socket = new FakeWebSocket((message) => {
          if (message.method === 'Page.navigate') {
            socket.deliverEvent({ method: 'Page.loadEventFired' })
            return {}
          }
          if (message.method === 'Runtime.evaluate') return { result: { value: evaluateValues.shift() ?? '' } }
          return {}
        })
        return socket as unknown as WebSocket
      },
    })
    const session = await launchWithSocket(harness)
    await expect(session.navigate({ url: 'https://example.com/' })).resolves.toEqual({
      url: 'https://example.com/after',
      title: 'After',
      content: 'After body',
    })
    await session.close()
  })

  it('kills the spawned child when startup times out', async () => {
    const kill = vi.fn()
    const { provider } = makeHarness({
      fetchFn: async () => {
        throw new Error('connection refused')
      },
      startupTimeoutMs: 20,
      spawnFn: () => ({ pid: 4321, kill }),
    })
    await expect(provider.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_STARTUP_TIMEOUT' }))
    expect(kill).toHaveBeenCalled()
  })

  it('kills the spawned child and reports BROWSER_ABORTED when startup is aborted', async () => {
    const kill = vi.fn()
    const controller = new AbortController()
    const { provider } = makeHarness({
      fetchFn: () => new Promise<Response>(() => {}),
      startupTimeoutMs: 5_000,
      spawnFn: () => ({ pid: 4321, kill }),
    })
    const launching = provider.launch(controller.signal)
    await new Promise(resolve => setTimeout(resolve, 10))
    controller.abort()
    await expect(launching).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_ABORTED' }))
    expect(kill).toHaveBeenCalled()
  })

  it('rolls the spawned child back when target discovery fails', async () => {
    const kill = vi.fn()
    const { provider } = makeHarness({
      fetchFn: async (url: string) => {
        if (url.endsWith('/json/version')) return { ok: true } as Response
        throw new Error('discovery down')
      },
      spawnFn: () => ({ pid: 4321, kill }),
    })
    await expect(provider.launch()).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_ERROR' }))
    expect(kill).toHaveBeenCalled()
  })
})

describe('MoliBrowserSession interaction', () => {
  it('rejects a click whose selector matches nothing', async () => {
    const harness = makeHarness()
    harness.evaluateValues.unshift('missing')
    const session = await launchWithSocket(harness)
    await expect(session.click({ selector: '#absent' }))
      .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_ELEMENT_NOT_FOUND' }))
    await session.close()
  })

  it('decodes screenshot bytes from base64', async () => {
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    const shot = await session.screenshot({ fullPage: true })
    expect(shot.mediaType).toBe('image/png')
    expect([...shot.data]).toEqual([0x68, 0x69])
    await session.close()
  })

  it('serializes concurrent operations and rejects later ones after close', async () => {
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    await Promise.all([session.snapshot(), session.snapshot()])
    await session.close()
    await session.close()
    await expect(session.snapshot())
      .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_SESSION_CLOSED' }))
  })

  it('settles close promptly by rejecting an operation blocked on a CDP event', async () => {
    // Teardown guard: closing the connection must settle a navigation that is
    // waiting for its load event, instead of leaving close() to wait out the
    // waiter deadline.
    const harness = makeHarness({ navigationTimeoutMs: 1_000 })
    const session = await launchWithSocket(harness)
    const navigating = session.navigate({ url: 'https://example.com/' })
    await new Promise(resolve => setTimeout(resolve, 5))
    const closing = session.close()
    await expect(navigating).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_SESSION_CLOSED' }))
    await closing
    expect(harness.kill).toHaveBeenCalled()
  })

  it('surfaces a failed in-page evaluation instead of reporting success', async () => {
    let socket!: FakeWebSocket
    const harness = makeHarness({
      wsFactory: () => {
        socket = new FakeWebSocket((message) => {
          if (message.method === 'Runtime.evaluate') {
            return {
              result: {},
              exceptionDetails: { text: 'Uncaught', exception: { description: 'SyntaxError: foo is not defined' } },
            }
          }
          return {}
        })
        return socket as unknown as WebSocket
      },
    })
    const session = await launchWithSocket(harness)
    await expect(session.click({ selector: '#anything' })).rejects.toThrow(expect.objectContaining({
      code: 'BROWSER_EVALUATION_FAILED',
      message: expect.stringContaining('foo is not defined'),
    }))
    await expect(session.snapshot())
      .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_EVALUATION_FAILED' }))
    await session.close()
  })

  it('rejects non-http(s) navigation targets without sending CDP traffic', async () => {
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    harness.socket.sent.length = 0
    for (const url of ['file:///etc/hosts', 'javascript:void(0)', 'data:text/html,x', 'not a url']) {
      await expect(session.navigate({ url }))
        .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_INVALID_URL' }))
    }
    expect(harness.socket.sent).toEqual([])
    await session.close()
  })

  it('reports BROWSER_ABORTED when a caller aborts an operation blocked on a CDP event', async () => {
    const controller = new AbortController()
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    const navigating = session.navigate({ url: 'https://example.com/' }, controller.signal)
    await new Promise(resolve => setTimeout(resolve, 5))
    controller.abort()
    await expect(navigating).rejects.toThrow(expect.objectContaining({ code: 'BROWSER_ABORTED' }))
    // The aborted operation must not wedge the serialization chain.
    await expect(session.snapshot()).resolves.toMatchObject({ url: 'https://example.com/' })
    await session.close()
  })

  it('rejects an already-aborted signal before sending any CDP frame', async () => {
    const controller = new AbortController()
    controller.abort()
    const harness = makeHarness()
    const session = await launchWithSocket(harness)
    harness.socket.sent.length = 0
    await expect(session.snapshot(controller.signal))
      .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_ABORTED' }))
    expect(harness.socket.sent).toEqual([])
    await session.close()
  })
})

describe('MoliBrowserProvider live-session finalization', () => {
  it('force-kills every unclosed serve process at host exit and clears the set', async () => {
    const kill = vi.fn()
    const { provider } = makeHarness({
      spawnFn: () => ({ pid: 4321, kill }),
    })
    await provider.launch()
    await provider.launch()
    provider.terminateForHostExit()
    expect(kill).toHaveBeenCalledTimes(2)
    // A second pass is a no-op: the set emptied after the first sweep.
    provider.terminateForHostExit()
    expect(kill).toHaveBeenCalledTimes(2)
  })

  it('stops tracking a session once its close() runs', async () => {
    const kill = vi.fn()
    const harness = makeHarness({
      spawnFn: () => ({ pid: 4321, kill }),
    })
    const session = await harness.provider.launch()
    await session.close()
    // The graceful path already killed it; host-exit finalization finds nothing left.
    harness.provider.terminateForHostExit()
    expect(kill).toHaveBeenCalledTimes(1)
  })

  it('detaches the exit listener through the finalization disposer', () => {
    const prependSpy = vi.spyOn(process, 'prependListener')
    const offSpy = vi.spyOn(process, 'off')
    try {
      const { provider } = makeHarness()
      const dispose = provider.installHostExitFinalization()
      expect(prependSpy).toHaveBeenCalledWith('exit', expect.any(Function))
      dispose()
      expect(offSpy).toHaveBeenCalledWith('exit', expect.any(Function))
    } finally {
      prependSpy.mockRestore()
      offSpy.mockRestore()
    }
  })
})

describe('moli serve argv', () => {
  it('places the cdp port flag and appends extra argv verbatim', () => {
    expect(buildServeArgv({ port: 1234 })).toEqual(['serve', '--cdp-port', '1234'])
    expect(buildServeArgv({ port: 1234, extraServeArgs: ['--layout'] })).toEqual(['serve', '--cdp-port', '1234', '--layout'])
  })
})

describe('browser-moli plugin registration', () => {
  it('registers the provider into ctx.browser and disposes with the fiber (HMR-safe)', async () => {
    const ctx = new Context()
    await ctx.plugin(BrowserRuntime, { provider: MOLI_BROWSER_PROVIDER_ID })
    // A nonexistent binary keeps the registered provider unavailable without
    // launching anything: the probe fails fast at path lookup.
    const fiber = await ctx.plugin(moliPlugin, { binaryPath: 'dsh-test-no-such-moli-binary' })
    await expect(ctx.browser.launch())
      .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_CONFIGURED_UNAVAILABLE' }))
    await fiber.dispose()
    await expect(ctx.browser.launch())
      .rejects.toThrow(expect.objectContaining({ code: 'BROWSER_PROVIDER_CONFIGURED_MISSING' }))
  })

  it('has no default export (namespace plugin export shape)', () => {
    expect('default' in moliPlugin).toBe(false)
  })

  it('rejects non-positive caps at construction', async () => {
    const ctx = new Context()
    await ctx.plugin(BrowserRuntime)
    await expect(ctx.plugin(moliPlugin, { startupTimeoutMs: 0 }))
      .rejects.toThrow(/startupTimeoutMs must be a positive finite number/)
    await expect(ctx.plugin(moliPlugin, { maxContentChars: -1 }))
      .rejects.toThrow(/maxContentChars must be a positive finite number/)
    await expect(ctx.plugin(moliPlugin, { cdpTimeoutMs: 0 }))
      .rejects.toThrow(/cdpTimeoutMs must be a positive finite number/)
    await expect(ctx.plugin(moliPlugin, { settleMs: -5 }))
      .rejects.toThrow(/settleMs must be a positive finite number/)
  })
})
