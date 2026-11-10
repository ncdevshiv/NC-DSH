/**
 * The NC-AG settings page, browser half: one Settings section, directly below
 * Agent presets, that monitors the host-plane platform and edits the models,
 * prices, and limits its agents use. The page reads the platform over the
 * generated `ncag` Remote namespace and the configuration over the
 * `ncag-settings` settings namespace.
 */

// Type-only: pulls the locale plugin's Context merge (ctx.locale).
import type {} from '@deepseek-ai/dsh-client-locale/client'
// Type-only: the ctx.configForms Context merge.
import type {} from '@deepseek-ai/dsh-client-ui-settings/client'
// Type-only: the settings shell's SlotMap merge (the 'settings.section' entry).
import type {} from '@deepseek-ai/dsh-client-ui-renderer/client'
// Type-only: pulls the ctx.remote merge and the `ncag` namespace face.
import type {} from '@deepseek-ai/dsh-api-remotes/client'
import type { Context as ClientContext } from '@deepseek-ai/cordis'
import { NcagSettingsSection } from './NcagSection.tsx'
import type { NcagSectionInjected } from './NcagSection.tsx'
import { NcagCardController } from './ncag-card-controller.ts'
import type { NcagSettings } from './ncag-card-controller.ts'
import { NcagMonitorController } from './monitor.ts'
import { en, zh, type NcagSettingsLocaleKey } from './locales.ts'

export type { NcagSectionProps, NcagSectionInjected } from './NcagSection.tsx'
export type { NcagCardFace, NcagCardState, NcagSettings } from './ncag-card-controller.ts'
export type { NcagActionOutcome, NcagMonitorState, NcagMonitorStatus } from './monitor.ts'
export type { NcagSettingsLocaleKey } from './locales.ts'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap {
    /** NC-AG settings page copy. */
    'settings.ncag': NcagSettingsLocaleKey
  }
}

/** Dictionary namespace owned by this plugin. */
export const NS = 'settings.ncag'

/**
 * Namespace of the platform settings. Spelled here rather than imported: a
 * client package must not depend on a Host package.
 */
export const NCAG_NS = 'ncag-settings'

/** Required services (cordis fiber inject). */
export const inject = ['slots', 'locale', 'configForms', 'remote']

/**
 * Mount the NC-AG Settings section.
 * @param ctx - the browser plugin context.
 */
export function apply(ctx: ClientContext): void {
  const t = ctx.locale.bind(NS)
  ctx.effect(() => ctx.locale.register(NS, { zh, en }), 'ui-settings-ncag: dictionaries')
  const card = new NcagCardController(ctx.configForms.get<NcagSettings>(NCAG_NS))
  ctx.effect(() => () => { card.dispose() }, 'ui-settings-ncag: form subscription')
  const monitor = new NcagMonitorController(ctx)
  ctx.effect(() => () => { monitor.dispose() }, 'ui-settings-ncag: monitor')
  const face = card.inject()
  const injected = (): NcagSectionInjected => ({
    edit: face.edit,
    resetField: face.resetField,
    save: face.save,
    discard: face.discard,
    hooks: { ncagCard: face.hooks.ncagCard, ncagMonitor: monitor.store },
    start: () => { monitor.start() },
    refresh: () => { void monitor.refresh() },
    selectProject: (projectId) => { monitor.selectProject(projectId) },
    loadTrace: (correlationId) => { void monitor.loadTrace(correlationId) },
    closeTrace: () => { monitor.closeTrace() },
    createProject: (input) => { void monitor.createProject(input) },
    submitRequest: (text) => { void monitor.submitRequest(text) },
    setPaused: (projectId, paused) => { void monitor.setPaused(projectId, paused) },
    cancelTask: (taskId) => { void monitor.cancelTask(taskId) },
    reassignTask: (taskId, capability) => { void monitor.reassignTask(taskId, capability) },
    cancelAgent: (agentId) => { void monitor.cancelAgent(agentId) },
    instructAgent: (agentId, text) => { void monitor.instructAgent(agentId, text) },
    resolveDecision: (decisionId, outcome) => { void monitor.resolveDecision(decisionId, outcome) },
  })
  // Directly below Agent presets (order 20): the platform settings belong with
  // the mode that runs it.
  ctx.slots.inject('settings.section', () => ctx.slots.register({
    name: 'settings.section',
    id: 'ncag',
    order: 25,
    label: () => t('nav'),
    locale: NS,
    inject: injected,
  }, NcagSettingsSection))
}
