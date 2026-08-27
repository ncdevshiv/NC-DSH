import { Context, Service } from '@deepseek-ai/cordis'
import z from '@deepseek-ai/schemastery'
import type {
  GenerationId,
  GenerationKind,
  GenerationPhase,
  GenerationRecord,
  PromotionResult,
  QuiesceOptions,
  SupervisorSnapshot,
} from './types.ts'

export type {
  GenerationId,
  GenerationKind,
  GenerationPhase,
  GenerationRecord,
  PromotionResult,
  QuiesceOptions,
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

export class RuntimeSupervisor extends Service {
  static Config: z<Config> = z.object({
    deadlineMs: z.number().step(1).min(100).max(120_000),
    requiredProbes: z.number().step(1).min(1).max(20),
  })

  private readonly kinds = new Map<GenerationKind, KindState>()
  private readonly listeners = new Set<(snapshot: SupervisorSnapshot) => void>()
  private readonly config: Required<Config>

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

  nextId(kind: GenerationKind, buildRev: string, pid?: number): GenerationRecord {
    return { id: nextGenerationId(), kind, buildRev, pid, phase: 'active' }
  }

  onSnapshot(listener: (snapshot: SupervisorSnapshot) => void): () => void {
    this.listeners.add(listener)
    return () => { this.listeners.delete(listener) }
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
