/**
 * The NC-AG Settings page: one live monitor over the host-plane platform and
 * the configuration form that owns its model overrides, prices, and limits.
 * Every monitor panel renders an existing platform record; the page performs no
 * platform work of its own.
 */

import { useEffect, useId, useState } from 'react'
import type { InjectFace, PropsLocale, PropsRuntime } from '@deepseek-ai/dsh-client-ui-slots'
import type { SnapshotStore } from '@deepseek-ai/dsh-client-store'
import {
  Button, SettingsForm, type SettingsFormLabels,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { NcagDecision, NcagProjectView } from '@deepseek-ai/dsh-api-ncag-controller/types'
import type { NcagCardFace, NcagCardState } from './ncag-card-controller.ts'
import type { NcagMonitorState } from './monitor.ts'
import { NcagFields } from './NcagFields.tsx'
import css from './NcagSection.module.css'

/** One page tab. */
type Tab = 'overview' | 'projects' | 'timeline' | 'configure'

/** Actions and observable state bound by the slot renderer. */
export interface NcagSectionInjected extends Pick<NcagCardFace, 'edit' | 'resetField' | 'save' | 'discard'> {
  hooks: {
    ncagCard: SnapshotStore<NcagCardState>
    ncagMonitor: SnapshotStore<NcagMonitorState>
  }
  start: () => void
  refresh: () => void
  selectProject: (projectId: string | undefined) => void
  loadTrace: (correlationId: string) => void
  closeTrace: () => void
  createProject: (input: { name: string; description: string; repositoryPath?: string }) => void
  submitRequest: (text: string) => void
  setPaused: (projectId: string, paused: boolean) => void
  cancelTask: (taskId: string) => void
  reassignTask: (taskId: string, capability: string) => void
  cancelAgent: (agentId: string) => void
  instructAgent: (agentId: string, text: string) => void
  resolveDecision: (decisionId: string, outcome: string) => void
}

/** Framework-derived props for the NC-AG settings page. */
export type NcagSectionProps = PropsRuntime<'settings.section'>
  & PropsLocale<'settings.ncag'> & InjectFace<NcagSectionInjected>

/**
 * Render the NC-AG monitor and configuration page.
 * @param props - locale, the monitor snapshot, and its actions.
 * @returns the page with its tab bar.
 */
export function NcagSettingsSection(props: NcagSectionProps) {
  const { t } = props
  const [tab, setTab] = useState<Tab>('overview')
  const headingId = useId()
  const monitor = props.useNcagMonitor(snapshot => snapshot)
  const card = props.useNcagCard(snapshot => snapshot)
  const { start } = props
  useEffect(() => { start() }, [start])

  const tabs: readonly { id: Tab; label: string }[] = [
    { id: 'overview', label: t('tabOverview') },
    { id: 'projects', label: t('tabProjects') },
    { id: 'timeline', label: t('tabTimeline') },
    { id: 'configure', label: t('tabConfigure') },
  ]

  return (
    <section className={css.page} aria-labelledby={headingId}>
      <header className={css.head}>
        <div>
          <h2 className={css.title} id={headingId}>{t('pageTitle')}</h2>
          <p className={css.intro}>{t('pageIntro')}</p>
        </div>
        <div className={css.headActions}>
          <StatusChip monitor={monitor} t={t} />
          <Button size="sm" variant="outline" onClick={props.refresh}>{t('refresh')}</Button>
        </div>
      </header>
      {monitor.error !== null && <p className={css.error} role="alert">{monitor.error}</p>}
      <div className={css.tabs} role="tablist" aria-label={t('pageTitle')}>
        {tabs.map(item => (
          <button
            key={item.id}
            type="button"
            role="tab"
            aria-selected={tab === item.id}
            className={tab === item.id ? `${css.tab} ${css.tabActive}` : css.tab}
            onClick={() => { setTab(item.id) }}
          >
            {item.label}
          </button>
        ))}
      </div>
      {tab === 'overview' && <Overview props={props} monitor={monitor} />}
      {tab === 'projects' && <Projects props={props} monitor={monitor} />}
      {tab === 'timeline' && <Timeline props={props} monitor={monitor} />}
      {tab === 'configure' && <Configure props={props} card={card} />}
    </section>
  )
}

/** Health and counters reported by the latest snapshot. */
function StatusChip({ monitor, t }: { monitor: NcagMonitorState; t: NcagSectionProps['t'] }) {
  if (monitor.status === 'unavailable') return <span className={`${css.chip} ${css.chipWarn}`}>{t('statusUnavailable')}</span>
  if (monitor.status === 'error') return <span className={`${css.chip} ${css.chipError}`}>{t('statusError')}</span>
  if (monitor.status !== 'ready' || monitor.snapshot === undefined) return <span className={css.chip}>{t('statusLoading')}</span>
  const broken = monitor.snapshot.audit.brokenAt !== null
  return (
    <span className={broken ? `${css.chip} ${css.chipError}` : `${css.chip} ${css.chipOk}`}>
      {broken ? t('auditBroken', { at: String(monitor.snapshot.audit.brokenAt) }) : t('auditVerified', { records: String(monitor.snapshot.audit.records) })}
    </span>
  )
}

/** Platform overview: usage, counts, effective routes, and pending decisions. */
function Overview({ props, monitor }: { props: NcagSectionProps; monitor: NcagMonitorState }) {
  const { t } = props
  const snapshot = monitor.snapshot
  if (snapshot === undefined) return <p className={css.empty}>{t('statusLoading')}</p>
  return (
    <div className={css.stack}>
      <div className={css.grid}>
        <Metric label={t('metricProjects')} value={String(snapshot.projects.length)} />
        <Metric label={t('metricLiveAgents')} value={String(snapshot.liveAgents)} />
        <Metric label={t('metricPendingDecisions')} value={String(snapshot.pendingDecisions)} />
        <Metric label={t('metricProfiles')} value={String(snapshot.profiles)} />
        <Metric label={t('metricCalls')} value={String(snapshot.usage.calls)} />
        <Metric label={t('metricTokens')} value={`${formatNumber(snapshot.usage.inputTokens)} / ${formatNumber(snapshot.usage.outputTokens)}`} />
        <Metric label={t('metricCost')} value={`$${snapshot.usage.costUsd.toFixed(4)}`} />
        <Metric label={t('metricGeneratedAt')} value={snapshot.generatedAt} />
      </div>

      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('routesTitle')}</h3>
        {snapshot.routes.length === 0
          ? <p className={css.empty}>{t('routesEmpty')}</p>
          : (
            <table className={css.table}>
              <thead>
                <tr>
                  <th>{t('columnTier')}</th>
                  <th>{t('columnRoute')}</th>
                  <th>{t('columnPrice')}</th>
                </tr>
              </thead>
              <tbody>
                {snapshot.routes.map(route => (
                  <tr key={`${route.tier}:${route.provider}/${route.model}`}>
                    <td>{route.tier}</td>
                    <td>{route.provider}/{route.model}</td>
                    <td>{route.inputPerMillion === null || route.outputPerMillion === null
                      ? t('routeUnpriced')
                      : `$${route.inputPerMillion} / $${route.outputPerMillion}`}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
      </section>

      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('profilesTitle')}</h3>
        {monitor.profiles.length === 0
          ? <p className={css.empty}>{t('profilesEmpty')}</p>
          : (
            <ul className={css.list}>
              {monitor.profiles.map(profile => (
                <li key={profile.id} className={css.row}>
                  <span className={css.rowTitle}>{profile.key}</span>
                  <span className={css.muted}>{profile.role}</span>
                  <span className={css.muted}>{profile.modelTier}</span>
                  <span className={css.muted}>{profile.capabilities.join(', ')}</span>
                </li>
              ))}
            </ul>
          )}
      </section>

      <Decisions props={props} monitor={monitor} />
    </div>
  )
}

/** Projects list, creation form, and the selected project's controls. */
function Projects({ props, monitor }: { props: NcagSectionProps; monitor: NcagMonitorState }) {
  const { t } = props
  const snapshot = monitor.snapshot
  const selected = snapshot?.projects.find(entry => entry.project.id === monitor.selectedProjectId)
  return (
    <div className={css.stack}>
      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('projectsTitle')}</h3>
        {snapshot === undefined || snapshot.projects.length === 0
          ? <p className={css.empty}>{t('projectsEmpty')}</p>
          : (
            <ul className={css.list}>
              {snapshot.projects.map(entry => (
                <ProjectRow key={entry.project.id} entry={entry} props={props} selected={entry.project.id === monitor.selectedProjectId} />
              ))}
            </ul>
          )}
      </section>
      <NewProjectForm props={props} />
      {selected !== undefined && <ProjectDetail props={props} entry={selected} monitor={monitor} />}
    </div>
  )
}

/** One project row with its status and selection control. */
function ProjectRow({ entry, props, selected }: { entry: NcagProjectView; props: NcagSectionProps; selected: boolean }) {
  const { t } = props
  const project = entry.project
  return (
    <li className={selected ? `${css.row} ${css.rowSelected}` : css.row}>
      <button type="button" className={css.rowButton} onClick={() => { props.selectProject(project.id) }}>
        <span className={css.rowTitle}>{project.name}</span>
        <span className={css.muted}>{project.status}</span>
        <span className={css.muted}>{t('projectCounts', {
          tasks: String(entry.tasks.length),
          agents: String(entry.agents.length),
          pending: String(entry.decisionsPending),
        })}</span>
        <span className={css.muted}>${entry.costUsd.toFixed(4)}</span>
      </button>
      <Button size="sm" variant="outline" onClick={() => { props.setPaused(project.id, project.status !== 'PAUSED') }}>
        {project.status === 'PAUSED' ? t('resume') : t('pause')}
      </Button>
    </li>
  )
}

/** Inline project creation. */
function NewProjectForm({ props }: { props: NcagSectionProps }) {
  const { t } = props
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [repositoryPath, setRepositoryPath] = useState('')
  return (
    <section className={css.section}>
      <h3 className={css.sectionTitle}>{t('createProjectTitle')}</h3>
      <div className={css.form}>
        <label className={css.field}>
          <span>{t('createProjectName')}</span>
          <input className={css.input} value={name} onChange={(event) => { setName(event.target.value) }} />
        </label>
        <label className={css.field}>
          <span>{t('createProjectDescription')}</span>
          <input className={css.input} value={description} onChange={(event) => { setDescription(event.target.value) }} />
        </label>
        <label className={css.field}>
          <span>{t('createProjectRepository')}</span>
          <input className={css.input} value={repositoryPath} onChange={(event) => { setRepositoryPath(event.target.value) }} placeholder={t('createProjectRepositoryHint')} />
        </label>
        <Button
          size="sm"
          variant="primary"
          disabled={name.trim() === '' || description.trim() === ''}
          onClick={() => {
            props.createProject({ name, description, ...repositoryPath.trim() === '' ? {} : { repositoryPath } })
            setName('')
            setDescription('')
            setRepositoryPath('')
          }}
        >
          {t('createProjectAction')}
        </Button>
      </div>
    </section>
  )
}

/** Tasks, agents, and decisions of the selected project. */
function ProjectDetail({ props, entry, monitor }: { props: NcagSectionProps; entry: NcagProjectView; monitor: NcagMonitorState }) {
  const { t } = props
  const [requestText, setRequestText] = useState('')
  return (
    <div className={css.stack}>
      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('submitRequestTitle')}</h3>
        <div className={css.form}>
          <label className={css.field}>
            <span>{t('submitRequestLabel')}</span>
            <textarea className={css.textarea} value={requestText} onChange={(event) => { setRequestText(event.target.value) }} />
          </label>
          <Button
            size="sm"
            variant="primary"
            disabled={requestText.trim() === ''}
            onClick={() => {
              props.submitRequest(requestText)
              setRequestText('')
            }}
          >
            {t('submitRequestAction')}
          </Button>
        </div>
      </section>

      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('tasksTitle')}</h3>
        {entry.tasks.length === 0 ? <p className={css.empty}>{t('tasksEmpty')}</p> : (
          <table className={css.table}>
            <thead>
              <tr>
                <th>{t('columnTask')}</th>
                <th>{t('columnStatus')}</th>
                <th>{t('columnCapability')}</th>
                <th>{t('columnAttempts')}</th>
                <th>{t('columnActions')}</th>
              </tr>
            </thead>
            <tbody>
              {entry.tasks.map(task => (
                <TaskRow key={task.id} task={task} props={props} />
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('agentsTitle')}</h3>
        {entry.agents.length === 0 ? <p className={css.empty}>{t('agentsEmpty')}</p> : (
          <ul className={css.list}>
            {entry.agents.map(agent => (
              <AgentRow key={agent.id} agent={agent} props={props} />
            ))}
          </ul>
        )}
      </section>

      <Decisions props={props} monitor={monitor} />
    </div>
  )
}

/** One task row with its cancel and reassign controls. */
function TaskRow({ task, props }: { task: NcagProjectView['tasks'][number]; props: NcagSectionProps }) {
  const { t } = props
  const [capability, setCapability] = useState('')
  return (
    <tr>
      <td>{task.title}</td>
      <td>{task.status}</td>
      <td>{task.requiredCapability}</td>
      <td>{String(task.attempts)}</td>
      <td className={css.actions}>
        <Button size="sm" variant="ghost" onClick={() => { props.cancelTask(task.id) }}>{t('cancelTask')}</Button>
        <input
          className={css.inlineInput}
          value={capability}
          placeholder={t('capabilityPlaceholder')}
          onChange={(event) => { setCapability(event.target.value) }}
        />
        <Button
          size="sm"
          variant="outline"
          disabled={capability.trim() === ''}
          onClick={() => {
            props.reassignTask(task.id, capability.trim())
            setCapability('')
          }}
        >
          {t('reassignTask')}
        </Button>
      </td>
    </tr>
  )
}

/** One agent row with its instruction and cancel controls. */
function AgentRow({ agent, props }: { agent: NcagProjectView['agents'][number]; props: NcagSectionProps }) {
  const { t } = props
  const [text, setText] = useState('')
  return (
    <li className={css.row}>
      <span className={css.rowTitle}>{agent.profileKey}</span>
      <span className={css.muted}>{agent.status}</span>
      <span className={css.muted}>{agent.model.provider}/{agent.model.model}</span>
      <span className={css.muted}>{t('agentSteps', { steps: String(agent.steps) })}</span>
      <span className={css.muted}>{t('agentUsage', {
        input: formatNumber(agent.usage.inputTokens),
        output: formatNumber(agent.usage.outputTokens),
      })}</span>
      <input
        className={css.inlineInput}
        value={text}
        placeholder={t('instructPlaceholder')}
        onChange={(event) => { setText(event.target.value) }}
      />
      <Button
        size="sm"
        variant="outline"
        disabled={text.trim() === ''}
        onClick={() => {
          props.instructAgent(agent.id, text)
          setText('')
        }}
      >
        {t('instruct')}
      </Button>
      <Button size="sm" variant="ghost" onClick={() => { props.cancelAgent(agent.id) }}>{t('cancelAgent')}</Button>
    </li>
  )
}

/** Pending (or selected-project) decisions with their resolution control. */
function Decisions({ props, monitor }: { props: NcagSectionProps; monitor: NcagMonitorState }) {
  const { t } = props
  return (
    <section className={css.section}>
      <h3 className={css.sectionTitle}>{t('decisionsTitle')}</h3>
      {monitor.decisions.length === 0 ? <p className={css.empty}>{t('decisionsEmpty')}</p> : (
        <ul className={css.list}>
          {monitor.decisions.map(decision => (
            <DecisionRow key={decision.id} decision={decision} props={props} />
          ))}
        </ul>
      )}
    </section>
  )
}

/** One decision with its recommendation and an answer field. */
function DecisionRow({ decision, props }: { decision: NcagDecision; props: NcagSectionProps }) {
  const { t } = props
  const recommendation = decision.recommendation
  const question = decision.debate?.question ?? decision.problem
  const [outcome, setOutcome] = useState(recommendation)
  return (
    <li className={css.row}>
      <span className={css.rowTitle}>{question}</span>
      <span className={css.muted}>{decision.status}</span>
      {recommendation !== ''
        && (
          <Button size="sm" variant="ghost" onClick={() => { props.resolveDecision(decision.id, recommendation) }}>
            {t('acceptRecommendation')}
          </Button>
        )}
      <input
        className={css.inlineInput}
        value={outcome}
        placeholder={t('decisionOutcomePlaceholder')}
        onChange={(event) => { setOutcome(event.target.value) }}
      />
      <Button
        size="sm"
        variant="outline"
        disabled={outcome.trim() === ''}
        onClick={() => { props.resolveDecision(decision.id, outcome.trim()) }}
      >
        {t('resolveDecision')}
      </Button>
    </li>
  )
}

/** Timeline of platform events with the correlation trace reader. */
function Timeline({ props, monitor }: { props: NcagSectionProps; monitor: NcagMonitorState }) {
  const { t } = props
  const [correlationId, setCorrelationId] = useState('')
  const events = monitor.snapshot?.events ?? []
  return (
    <div className={css.stack}>
      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('traceTitle')}</h3>
        <div className={css.form}>
          <label className={css.field}>
            <span>{t('traceLabel')}</span>
            <input className={css.input} value={correlationId} onChange={(event) => { setCorrelationId(event.target.value) }} />
          </label>
          <Button
            size="sm"
            variant="outline"
            disabled={correlationId.trim() === '' || monitor.selectedProjectId === undefined}
            onClick={() => { props.loadTrace(correlationId.trim()) }}
          >
            {t('traceAction')}
          </Button>
          {monitor.trace !== undefined && <Button size="sm" variant="ghost" onClick={props.closeTrace}>{t('traceClose')}</Button>}
        </div>
        {monitor.traceError !== null && <p className={css.error}>{monitor.traceError}</p>}
        {monitor.trace !== undefined && (
          <div className={css.trace}>
            <p className={css.muted}>{t('traceSummary', {
              audit: String(monitor.trace.audit.length),
              events: String(monitor.trace.events.length),
              toolCalls: String(monitor.trace.toolCalls.length),
              messages: String(monitor.trace.messages.length),
            })}</p>
            <ul className={css.list}>
              {monitor.trace.toolCalls.map(call => (
                <li key={`${call.sequence}`} className={css.row}>
                  <span className={css.muted}>{call.at}</span>
                  <span className={css.rowTitle}>{call.tool}</span>
                  <span className={css.muted}>{call.ok ? t('toolOk') : t('toolFailed')}</span>
                  <span className={css.muted}>{`${String(call.durationMs)}ms`}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>

      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('timelineTitle')}</h3>
        {events.length === 0 ? <p className={css.empty}>{t('timelineEmpty')}</p> : (
          <ul className={css.list}>
            {[...events].reverse().map(event => (
              <li key={`${String(event.sequence)}`} className={css.row}>
                <span className={css.muted}>{event.at}</span>
                <span className={css.kind}>{event.kind}</span>
                <span>{event.summary}</span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}

/** The configuration form: model overrides, prices, and limits. */
function Configure({ props, card }: { props: NcagSectionProps; card: NcagCardState }) {
  const { t } = props
  const labels: SettingsFormLabels = {
    unavailable: t('unavailable'),
    readOnly: t('readOnly'),
    saveFailed: t('saveFailed'),
    save: t('save'),
    saving: t('saving'),
  }
  return (
    <SettingsForm labels={labels} state={card} onSave={props.save} onDiscard={props.discard}>
      <section className={css.section}>
        <h3 className={css.sectionTitle}>{t('configureTitle')}</h3>
        <p className={css.muted}>{t('configureHelp')}</p>
        <NcagFields t={t} state={card} edit={props.edit} resetField={props.resetField} />
      </section>
    </SettingsForm>
  )
}

/** One headline number. */
function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className={css.metric}>
      <span className={css.metricLabel}>{label}</span>
      <span className={css.metricValue}>{value}</span>
    </div>
  )
}

/** Compact integer formatting for token counts. */
function formatNumber(value: number): string {
  return value.toLocaleString('en-US')
}
