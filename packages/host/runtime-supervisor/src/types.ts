import type { Branded } from '@deepseek-ai/dsh-brand'

export type GenerationId = Branded<'RuntimeGenerationId'>
export type GenerationKind = 'host' | 'sidecar' | 'desktop'
export type GenerationPhase = 'active' | 'shadow' | 'draining' | 'retired'
export interface GenerationRecord {
  readonly id: GenerationId
  readonly kind: GenerationKind
  readonly buildRev: string
  readonly pid?: number | undefined
  readonly phase: GenerationPhase
  readonly promotedAt?: string | undefined
}
export interface SupervisorSnapshot {
  readonly generations: readonly GenerationRecord[]
}
export interface QuiesceOptions {
  readonly deadlineMs: number
  readonly requiredProbes: number
}
export interface PromotionResult {
  readonly ok: boolean
  readonly from?: GenerationId | undefined
  readonly to: GenerationId
  readonly reason?: string | undefined
}
export interface SpawnShadowOptions {
  readonly kind: GenerationKind
  readonly buildRev: string
  readonly pid?: number | undefined
}
export interface HealthProbeResult {
  readonly ok: boolean
  readonly reason?: string | undefined
}
