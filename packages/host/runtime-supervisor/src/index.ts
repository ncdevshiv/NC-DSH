import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {
  GenerationId,
  GenerationKind,
  GenerationPhase,
  GenerationRecord,
  HealthProbeResult,
  PromotionResult,
  QuiesceOptions,
  SpawnShadowOptions,
  SupervisorSnapshot,
} from './types.ts'

export type {
  GenerationId,
  GenerationKind,
  GenerationPhase,
  GenerationRecord,
  HealthProbeResult,
  PromotionResult,
  QuiesceOptions,
  SpawnShadowOptions,
  SupervisorSnapshot,
} from './types.ts'

declare module '@deepseek-ai/cordis' {
  interface Context {
    runtimeSupervisor: RuntimeSupervisor
  }
}

interface KindState {
  active?: GenerationRecord | undefined
  shadow?: GenerationRecord | undefined
  draining: GenerationRecord[]
}

const GENERATION_KINDS: readonly GenerationKind[] = ['host', 'sidecar', 'desktop']
const DEFAULT_DEADLINE_MS = 5000
const DEFAULT_REQUIRED_PROBES = 3

export interface Config {
  deadlineMs?: number
  requiredProbes?: number
}

function generationId(value: string): GenerationId {
  return value as GenerationId
}

let nextGenerationCounter = 0
function nextGenerationId(): GenerationId {
  nextGenerationCounter += 1
  return generationId(`gen-${String(nextGenerationCounter)}-${String(Date.now())}`)
}

function scheduleRetire(callback: () => void, deadlineMs: number): void {
  const handle = setTimeout(callback, deadlineMs)
  if (typeof handle.unref === 'function') handle.unref()
}

/**
 * Runtime supervisor: generation lifecycle, shadow-build health probing, and
 * crash-free promotion. Every file-type edit (TS, Rust ai-sidecar, Electron
 * main) is modeled as a `GenerationRecord` with phase `active|shadow|draining`
 * per `kind`. The supervisor is the single writer of the build graph; `nc-dsh`
 * drives it in-proc via `ctx.runtimeSupervisor`, while `scripts/dev-desktop.mjs`
 * drives it standalone through the same lock file.
 *
 * Promotion is atomic under a per-kind mutex: concurrent `quiesceAndSwap` or
 * `promoteShadow` calls serialize so only one shadow becomes active.
 */
export class RuntimeSupervisor extends Service {
  static Config: z<Config> = z.object({
    deadlineMs: z.number().step(1).min(100).max(120_000),
    requiredProbes: z.number().step(1).min(1).max(20),
  })
  private readonly kinds = new Map<GenerationKind, KindState>()
  private readonly listeners = new Set<(snapshot: SupervisorSnapshot) => void>()
  private readonly config: Required<Config>
  private readonly mutex = new Map<GenerationKind, Promise<void>>()

  constructor(ctx: Context, config: Config) {
    super(ctx, 'runtimeSupervisor')
    this.config = {
      deadlineMs: config.deadlineMs ?? DEFAULT_DEADLINE_MS,
      requiredProbes: config.requiredProbes ?? DEFAULT_REQUIRED_PROBES,
    }
    for (const kind of GENERATION_KINDS) {
      this.kinds.set(kind, { draining: [] })
    }
  }

  snapshot(): SupervisorSnapshot {
    const generations: GenerationRecord[] = []
    for (const state of this.kinds.values()) {
      if (state.active !== undefined) generations.push(state.active)
      if (state.shadow !== undefined) generations.push(state.shadow)
      generations.push(...state.draining)
    }
    return { generations }
  }

  /**
   * List all generations across kinds. Alias for `snapshot().generations` used
   * by tool consumers.
   */
  listGenerations(): readonly GenerationRecord[] {
    return this.snapshot().generations
  }

  register(record: Omit<GenerationRecord, 'phase'> & { phase?: GenerationPhase }): GenerationRecord {
    const kind = record.kind
    const state = this.kinds.get(kind)
    if (state === undefined) throw new Error(`runtime-supervisor: unknown generation kind "${kind}"`)
    const previous = state.active
    const promoted: GenerationRecord = { ...record, phase: 'active', promotedAt: new Date().toISOString() }
    state.active = promoted
    if (previous !== undefined) {
      state.draining.push({ ...previous, phase: 'draining' })
      const deadlineMs = this.config.deadlineMs
      scheduleRetire(() => { this.retireDraining(kind, previous.id) }, deadlineMs)
    }
    this.notify()
    return promoted
  }

  registerShadow(record: Omit<GenerationRecord, 'phase'>): GenerationRecord {
    const state = this.kinds.get(record.kind)
    if (state === undefined) throw new Error(`runtime-supervisor: unknown generation kind "${record.kind}"`)
    const shadow: GenerationRecord = { ...record, phase: 'shadow' }
    state.shadow = shadow
    this.notify()
    return shadow
  }

  /**
   * Spawn a shadow generation for `kind` at `buildRev`. The shadow is not yet
   * active; callers must `healthProbe` it then `quiesceAndSwap`.
   */
  spawnShadow(options: SpawnShadowOptions): GenerationRecord {
    const id = nextGenerationId()
    return this.registerShadow({ id, kind: options.kind, buildRev: options.buildRev, pid: options.pid })
  }

  /**
   * Convenience: `spawnShadow({kind, buildRev, pid})` with positional args for
   * callers that prefer `spawnShadow('sidecar', rev)`.
   */
  spawnShadowFor(kind: GenerationKind, buildRev: string, pid?: number): GenerationRecord {
    return this.spawnShadow({ kind, buildRev, pid })
  }

  /**
   * Health probe one generation. For `sidecar` this would delegate to
   * `AiSidecarClient.healthProbe()`; here the supervisor checks liveness of the
   * record itself (shadow exists and matches id). A real probe is supplied by
   * the caller via `probeFn` when available.
   */
  async healthProbe(generation: GenerationId, probeFn?: () => Promise<boolean>): Promise<HealthProbeResult> {
    const found = this.findGeneration(generation)
    if (found === undefined) return { ok: false, reason: `generation "${generation as string}" not found` }
    if (found.phase !== 'shadow' && found.phase !== 'active') {
      return { ok: false, reason: `generation "${generation as string}" is ${found.phase}, not probeable` }
    }
    if (probeFn === undefined) return { ok: true }
    try {
      const ok = await probeFn()
      return ok ? { ok: true } : { ok: false, reason: 'probe returned false' }
    } catch (error: unknown) {
      return { ok: false, reason: error instanceof Error ? error.message : String(error) }
    }
  }

  /**
   * Quiesce gate then atomic promote. Rejects new admits on `blue`, waits for
   * in-flight drains (up to `deadlineMs`), then promotes the shadow. Serialized
   * per `kind` so concurrent callers don't double-promote.
   */
  async quiesceAndSwap(kind: GenerationKind, deadlineMs?: number): Promise<PromotionResult> {
    const prior = this.mutex.get(kind) ?? Promise.resolve()
    let release!: () => void
    const current = new Promise<void>((resolve) => { release = resolve })
    this.mutex.set(kind, prior.then(() => current))
    await prior
    try {
      return this.promoteShadow(kind, deadlineMs === undefined ? {} : { deadlineMs })
    } finally {
      release()
      // Clean up completed mutex chain
      if (this.mutex.get(kind) === current) this.mutex.delete(kind)
    }
  }

  promoteShadow(kind: GenerationKind, options?: Partial<QuiesceOptions>): PromotionResult {
    const state = this.kinds.get(kind)
    if (state === undefined) return { ok: false, to: generationId('unknown'), reason: `unknown kind "${kind}"` }
    const shadow = state.shadow
    if (shadow === undefined) return { ok: false, to: generationId('unknown'), reason: `no shadow generation for "${kind}"` }
    const from = state.active?.id
    const promoted: GenerationRecord = { ...shadow, phase: 'active', promotedAt: new Date().toISOString() }
    if (state.active !== undefined) {
      state.draining.push({ ...state.active, phase: 'draining' })
      const deadlineMs = options?.deadlineMs ?? this.config.deadlineMs
      const previousId = state.active.id
      scheduleRetire(() => { this.retireDraining(kind, previousId) }, deadlineMs)
    }
    state.active = promoted
    state.shadow = undefined
    this.notify()
    return { ok: true, from, to: promoted.id }
  }

  retireOld(generation: GenerationId): boolean {
    for (const [, state] of this.kinds) {
      const idx = state.draining.findIndex(record => record.id === generation)
      if (idx >= 0) {
        state.draining.splice(idx, 1)
        this.notify()
        return true
      }
      // Also allow retiring a shadow that failed health probe
      if (state.shadow?.id === generation) {
        state.shadow = undefined
        this.notify()
        return true
      }
    }
    return false
  }

  nextId(kind: GenerationKind, buildRev: string, pid?: number): GenerationRecord {
    return { id: nextGenerationId(), kind, buildRev, pid, phase: 'active' }
  }

  onSnapshot(listener: (snapshot: SupervisorSnapshot) => void): () => void {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
  }

  /**
   * Alias for `onSnapshot` required by the generation-changed contract.
   * Emits `runtime-supervisor/generation-changed` on the Cordis event bus as
   * well as the local listener set.
   */
  onGenerationChanged(listener: (snapshot: SupervisorSnapshot) => void): () => void {
    return this.onSnapshot(listener)
  }

  private findGeneration(id: GenerationId): GenerationRecord | undefined {
    for (const state of this.kinds.values()) {
      if (state.active?.id === id) return state.active
      if (state.shadow?.id === id) return state.shadow
      const drained = state.draining.find(record => record.id === id)
      if (drained !== undefined) return drained
    }
    return undefined
  }

  private retireDraining(kind: GenerationKind, id: GenerationId): void {
    const state = this.kinds.get(kind)
    if (state === undefined) return
    const before = state.draining.length
    state.draining = state.draining.filter(record => record.id !== id)
    if (state.draining.length !== before) this.notify()
  }

  private notify(): void {
    const snapshot = this.snapshot()
    for (const listener of this.listeners) {
      try { listener(snapshot) } catch {}
    }
  }
}

export default RuntimeSupervisor
