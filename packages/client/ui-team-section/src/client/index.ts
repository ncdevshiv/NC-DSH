/**
 * Team section plugin, browser half. Registers the members-first team view
 * into the sidebar shell's keyed `sidebar.section` slot under the `team`
 * key. The preset roster rides one snapshot store fed by the agentPresets
 * wire face (refreshed on reconnect); members derive from the standard
 * session-list hook. The registration waits on the shell's declaration
 * through `slots.inject()` and leaves with the caller's fiber.
 */
import type { ConnectionHandle, IApiClient } from '@deepseek-ai/dsh-api-remotes/client'
// Type-only: pulls the locale plugin's Context merge (ctx.locale).
import type {} from '@deepseek-ai/dsh-client-locale/client'
import type { ClientContext } from '@deepseek-ai/dsh-client-runtime/client'
import { createSnapshotStore } from '@deepseek-ai/dsh-client-runtime/client'
import type { TeamRosterState, TeamSectionInjected } from './TeamSection.tsx'
import { TeamSection } from './TeamSection.tsx'
import { en, zh, type TeamKey } from './locales.ts'

export type { TeamPreset, TeamRosterState, TeamSectionInjected, TeamSectionProps } from './TeamSection.tsx'
export type { TeamKey } from './locales.ts'

declare module '@deepseek-ai/dsh-client-ui-slots' {
  interface LocaleNamespaceMap {
    /** The Team section's members-and-roster copy. */
    team: TeamKey
  }
}

/** Dictionary namespace owned by this plugin. */
const NS = 'team'

/** Services required by the Team section plugin. */
export const inject = ['slots', 'sessions', 'workspaces', 'locale', 'connection']

/**
 * Register the Team section once the sidebar's keyed declaration is on the
 * ledger.
 * @param ctx - client root context.
 */
export function apply(ctx: ClientContext): void {
  const { api } = ctx.get('connection') as ConnectionHandle
  ctx.effect(() => ctx.locale.register(NS, { zh, en }), 'ui-team-section: dictionaries')

  // The roster is a live directory (files on disk can change between
  // connects), so a reconnect re-reads it; nothing else on the wire
  // announces preset authoring.
  const roster = createSnapshotStore<TeamRosterState>({ status: 'loading' })
  const loadRoster = async (): Promise<void> => {
    try {
      const response = await (api as IApiClient).agentPresets.list({})
      if (!response.result.ok) {
        roster.set({ status: 'error', error: response.result.error.message })
        return
      }
      roster.set({
        status: 'ready',
        presets: response.result.value.presets.map(preset => ({
          id: preset.id,
          trust: preset.trust,
          ...(preset.name === undefined ? {} : { name: preset.name }),
          ...(preset.description === undefined ? {} : { description: preset.description }),
          ...(preset.broken === undefined ? {} : { broken: preset.broken }),
        })),
      })
    } catch (error) {
      roster.set({ status: 'error', error: error instanceof Error ? error.message : String(error) })
    }
  }
  void loadRoster()
  ctx.effect(() => ctx.on('connection/reset', () => {
    roster.set({ status: 'loading' })
    void loadRoster()
  }), 'ui-team-section: roster refresh')

  const injected = (): TeamSectionInjected => ({
    hooks: { roster },
    openMember: (sessionId) => {
      // The subagent address routes the conversation shell into the child's
      // read-only continuation view; a session without an address (an edge
      // of the list projection that has none) opens plainly.
      const address = ctx.sessions.subagentAddress(sessionId)
      if (address !== undefined) ctx.sessions.openSubagent(address)
      else ctx.sessions.open(sessionId)
    },
    startWith: async (presetId) => {
      // The session must be blank when the preset lands on it (the host
      // refuses to recompose a started session), and startSession returns
      // exactly that fresh blank session.
      const sessionId = await ctx.workspaces.startSession()
      const response = await (api as IApiClient).agentPresets.select({ sessionId, agentPreset: presetId })
      if (!response.result.ok) throw new Error(response.result.error.message)
    },
  })
  ctx.slots.inject('sidebar.section', () => ctx.slots.register(
    {
      name: 'sidebar.section',
      key: 'team',
      inject: injected,
      locale: NS,
    },
    TeamSection,
  ))
}
