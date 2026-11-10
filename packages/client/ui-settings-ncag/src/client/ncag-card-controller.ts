/** Staged NC-AG settings backed by the Host's `ncag` namespace. */

import type { SnapshotStore } from '@deepseek-ai/dsh-client-store'
import {
  SettingsFormModel, settingsNumberField, settingsTextField,
  type SettingsFieldSpec, type SettingsFieldState, type SettingsFormActions, type SettingsFormScope, type SettingsFormShell,
} from '@deepseek-ai/dsh-client-ui-primitives'

/** Host-owned NC-AG defaults and overrides. */
export interface NcagSettings {
  fastModel: string
  standardModel: string
  reasoningModel: string
  pricingOverrides: string
  uiMaxConcurrentAgents: number
  uiMaxAgentsPerTask: number
  uiMaxStepsPerTask: number
  uiMaxOutputTokens: number
  uiMaxCostUsd: number
}

/** Effective values and drafts presented by the card. */
export interface NcagCardState extends SettingsFormShell {
  fastModel: SettingsFieldState
  standardModel: SettingsFieldState
  reasoningModel: SettingsFieldState
  pricingOverrides: SettingsFieldState
  uiMaxConcurrentAgents: SettingsFieldState
  uiMaxAgentsPerTask: SettingsFieldState
  uiMaxStepsPerTask: SettingsFieldState
  uiMaxOutputTokens: SettingsFieldState
  uiMaxCostUsd: SettingsFieldState
}

/** Actions and observable state bound by the slot renderer. */
export interface NcagCardFace extends SettingsFormActions {
  hooks: {
    ncagCard: SnapshotStore<NcagCardState>
  }
}

/** Fields in display order. */
export const NCAG_FIELDS = [
  'fastModel', 'standardModel', 'reasoningModel', 'pricingOverrides',
  'uiMaxConcurrentAgents', 'uiMaxAgentsPerTask', 'uiMaxStepsPerTask', 'uiMaxOutputTokens', 'uiMaxCostUsd',
] as const

/** One model-route field, validated as `provider/model`. */
function routeField(field: string): SettingsFieldSpec {
  const base = settingsTextField(field)
  return {
    ...base,
    parse: (text) => {
      const write = base.parse(text)
      if (write?.kind !== 'set') return write
      const value = String(write.value)
      const slash = value.indexOf('/')
      return slash > 0 && slash < value.length - 1 ? write : undefined
    },
  }
}

/** One whole-number limit at or above zero. */
function limitField(field: string): SettingsFieldSpec {
  const numeric = settingsNumberField(field)
  return {
    ...numeric,
    parse: (text) => {
      const write = numeric.parse(text)
      if (write?.kind !== 'set') return write
      const value = write.value as number
      return Number.isSafeInteger(value) && value >= 0 && !Object.is(value, -0) ? write : undefined
    },
  }
}

/** Bind the whole NC-AG namespace to one staged settings form. */
export class NcagCardController {
  private readonly form: SettingsFormModel<NcagSettings>
  private readonly store: SnapshotStore<NcagCardState>

  /** @param scope - The Host's `ncag` settings section. */
  constructor(scope: SettingsFormScope<NcagSettings>) {
    this.form = new SettingsFormModel(scope, [
      routeField('fastModel'), routeField('standardModel'), routeField('reasoningModel'),
      settingsTextField('pricingOverrides'),
      limitField('uiMaxConcurrentAgents'), limitField('uiMaxAgentsPerTask'),
      limitField('uiMaxStepsPerTask'), limitField('uiMaxOutputTokens'), limitField('uiMaxCostUsd'),
    ])
    this.store = this.form.bind(() => ({
      ...this.form.shell(),
      fastModel: this.form.field('fastModel'),
      standardModel: this.form.field('standardModel'),
      reasoningModel: this.form.field('reasoningModel'),
      pricingOverrides: this.form.field('pricingOverrides'),
      uiMaxConcurrentAgents: this.form.field('uiMaxConcurrentAgents'),
      uiMaxAgentsPerTask: this.form.field('uiMaxAgentsPerTask'),
      uiMaxStepsPerTask: this.form.field('uiMaxStepsPerTask'),
      uiMaxOutputTokens: this.form.field('uiMaxOutputTokens'),
      uiMaxCostUsd: this.form.field('uiMaxCostUsd'),
    }))
  }

  /**
   * Bind the card to the slot renderer.
   * @returns The card snapshot and staged write actions.
   */
  inject(): NcagCardFace {
    return { hooks: { ncagCard: this.store }, ...this.form.actions() }
  }

  /** Release accepted-value subscriptions. */
  dispose(): void { this.form.dispose() }
}
