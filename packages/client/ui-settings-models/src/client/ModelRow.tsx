/** Shared model fields and actions for both adapter catalog editors. */

import type { ReactNode } from 'react'
import {
  IconChevronDownOutlineRegular, IconChevronRightOutlineRegular, IconLoadingOutlineRegular,
  IconPlayOutlineRegular, IconTrashOutlineRegular, Switch, Tag,
} from '@deepseek-ai/dsh-client-ui-primitives'
import type { DeepSeekModelDraft } from './DeepSeekModelsEditor.tsx'
import type { ModelTestFeedback } from './operations.ts'
import type { ModelsKey } from './locales.ts'
import { ModelInputTypes } from './ModelInputTypes.tsx'
import { ModelReasoningEfforts } from './ModelReasoningEfforts.tsx'
import { formatCapacity } from './DeepSeekModelsEditor.tsx'
import { MODEL_SOURCE_FIELD } from './mutations.ts'
import styles from './ModelsSection.module.css'

/** A capacity's editable text and adapter-specific inherited hint. */
interface CapacityInput {
  value: string
  placeholder: string
  onChange: (value: string) => void
  onBlur?: () => void
}

/**
 * Capability facts a listing disclosed about one model. Every member is
 * optional because a provider listing may disclose any subset; a surface shows
 * only what is present, so a chip never states a fact nobody measured.
 */
export interface ModelCapabilities {
  /** The model exposes selectable reasoning/thinking levels. */
  readonly reasoning?: boolean | undefined
  /** The model accepts image input. */
  readonly image?: boolean | undefined
  /** Disclosed context window in tokens. */
  readonly contextWindow?: number | undefined
}

/** Adapter-owned data and actions for one model row. */
interface ModelRowProps {
  model: DeepSeekModelDraft
  position: number
  inputField: 'inputModalities' | 'input'
  inputFallback?: readonly string[] | undefined
  inputLoading?: boolean
  /** Levels the row's `reasoningEfforts` may declare; absent or empty shows no reasoning control. */
  reasoningLevels?: readonly string[] | undefined
  /**
   * Capability facts this row's listing disclosed, shown as collapsed-row chips.
   * A fact the endpoint did not report is omitted rather than guessed, so a
   * row never advertises a capability nobody measured.
   */
  capabilities?: ModelCapabilities | undefined
  expanded: boolean
  disabled: boolean
  t: (key: ModelsKey) => string
  contextWindow: CapacityInput
  maxTokens: CapacityInput
  /**
   * Whether this row is one the route's installed catalog serves and the user
   * layer only adjusts. Its id is the catalog's own key and is therefore not
   * editable, and there is nothing to remove — withdrawing such a row is what
   * disabling it does.
   */
  catalog?: boolean
  onFieldChange: (field: 'id' | 'name', value: string | undefined) => void
  onIdBlur?: (value: string) => void
  onChange: (model: DeepSeekModelDraft) => void
  onToggle: () => void
  /** Remove this row; absent where the row cannot leave the served list. */
  onRemove?: (() => void) | undefined
  /** Probe one model; absent when the owning card has no test surface. */
  onTest?: ((modelId: string) => void) | undefined
  /** Whether this row's probe is in flight. */
  testing?: boolean | undefined
  /** The settled probe for this row, or undefined. */
  testFeedback?: ModelTestFeedback | undefined
}

/**
 * The probe props one row receives, as both family editors forward them: every
 * one is absent where the card has no test surface or no answer yet, so a row
 * never states a probe the owning card does not hold.
 * @param onTest - the card's probe action; absent when it has none.
 * @param testing - ids whose probes are in flight.
 * @param feedback - each row's settled probe, by model id.
 * @param modelId - the row's model id.
 * @returns the props to spread into that row.
 */
export function modelProbeProps(
  onTest: ((modelId: string) => void) | undefined,
  testing: ReadonlySet<string> | undefined,
  feedback: ReadonlyMap<string, ModelTestFeedback> | undefined,
  modelId: string,
): Pick<ModelRowProps, 'onTest' | 'testing' | 'testFeedback'> {
  const settled = feedback?.get(modelId)
  return {
    ...onTest === undefined ? {} : { onTest },
    ...testing === undefined ? {} : { testing: testing.has(modelId) },
    ...settled === undefined ? {} : { testFeedback: settled },
  }
}

/** A capability fact stated on the collapsed row, with its chip label. */
interface CapabilityChip {
  readonly key: string
  readonly tone: 'outline' | 'info' | 'success' | 'neutral'
  readonly text: string
}

/**
 * The capability facts one drafted row declares by itself, used where the
 * listing disclosed no answer. A field the row leaves unset stays unknown.
 * @param model - the drafted row.
 * @param field - the adapter's input-type field for this row.
 * @returns the facts the row states.
 */
function declaredCapabilities(
  model: DeepSeekModelDraft,
  field: 'inputModalities' | 'input',
): ModelCapabilities {
  const modalities = model[field]
  const contextWindow = model['contextWindow']
  const efforts = model['reasoningEfforts']
  return {
    ...typeof contextWindow === 'number' ? { contextWindow } : {},
    ...Array.isArray(modalities) ? { image: modalities.includes('image') } : {},
    ...efforts === false
      ? { reasoning: false }
      : typeof efforts === 'object' && efforts !== null ? { reasoning: true } : {},
  }
}

/**
 * The capability chips one collapsed row shows. A fact neither the listing nor
 * the row discloses is left out, so a chip never advertises an unmeasured
 * capability.
 * @param capabilities - facts the listing disclosed, which win over the row's own.
 * @param model - the drafted row, for facts it declares itself.
 * @param field - the adapter's input-type field for this row.
 * @param source - the row's stored provenance, when one is recorded.
 * @param t - section copy.
 * @returns the chip row, or nothing when there is no fact to state.
 */
function CapabilityChips({
  capabilities,
  model,
  field,
  source,
  t,
}: {
  capabilities: ModelCapabilities | undefined
  model: DeepSeekModelDraft
  field: 'inputModalities' | 'input'
  source: unknown
  t: (key: ModelsKey) => string
}): ReactNode {
  const facts: ModelCapabilities = { ...declaredCapabilities(model, field), ...capabilities }
  const contextWindow = facts.contextWindow
  const chips: CapabilityChip[] = []
  if (facts.reasoning === true) {
    chips.push({ key: 'thinking', tone: 'info', text: t('modelThinking') })
  }
  if (facts.image === true) {
    chips.push({ key: 'vision', tone: 'info', text: t('modelVision') })
  }
  if (contextWindow !== undefined && contextWindow > 0) {
    chips.push({
      key: 'context',
      tone: 'outline',
      text: t('modelContextChip').replace('{size}', () => formatCapacity(contextWindow)),
    })
  }
  // A hand-added row is named as custom beside its capabilities, so a sync
  // that leaves it in place reads as a choice rather than an omission.
  if (source === 'custom') {
    chips.push({ key: 'custom', tone: 'neutral', text: t('customTag') })
  }
  if (chips.length === 0) return null
  return (
    <div className={styles['modelCapabilities']}>
      {chips.map(chip => (
        <Tag key={chip.key} tone={chip.tone} className={styles['modelCapability']}>{chip.text}</Tag>
      ))}
    </div>
  )
}

/**
 * Render consistent model identity, capacity, and input-type controls.
 * @param props - drafted fields and their owning editor's actions.
 * @returns one expandable model entry.
 */
export function ModelRow(props: ModelRowProps): ReactNode {
  const { model, position, t, disabled } = props
  const modelId = typeof model.id === 'string' ? model.id : ''
  const enabled = model.enabled !== false
  const feedback = props.testFeedback
  return (
    <div className={styles['modelEntry']}>
      <div className={props.catalog === true ? styles['modelRowCatalog'] : styles['modelRow']}>
        {(['id', 'name'] as const).map(field => (
          <input
            key={field}
            className={styles['input']}
            type="text"
            value={typeof model[field] === 'string' ? model[field] : ''}
            placeholder={t(field === 'id' ? 'modelId' : 'modelName')}
            aria-label={`${t(field === 'id' ? 'modelId' : 'modelName')} ${String(position)}`}
            readOnly={field === 'id' && props.catalog === true}
            disabled={disabled}
            onChange={(event) => {
              const value = event.target.value
              props.onFieldChange(field, field === 'name' && value === '' ? undefined : value)
            }}
            onBlur={field === 'id' ? event => props.onIdBlur?.(event.target.value) : undefined}
          />
        ))}
        <button
          type="button"
          className={styles['iconButton']}
          aria-label={`${t('modelAdvanced')} ${String(position)}`}
          aria-expanded={props.expanded}
          title={t('modelAdvanced')}
          onClick={props.onToggle}
        >
          {props.expanded ? <IconChevronDownOutlineRegular /> : <IconChevronRightOutlineRegular />}
        </button>
        {props.onRemove === undefined
          ? null
          : (
            <button
              type="button"
              className={`${styles['iconButton']} ${styles['iconButtonDanger']}`}
              aria-label={`${t('removeModel')} ${String(position)}`}
              title={t('removeModel')}
              disabled={disabled}
              onClick={props.onRemove}
            >
              <IconTrashOutlineRegular size={14} />
            </button>
          )}
      </div>
      <div className={styles['modelMetaRow']}>
        <Switch
          checked={enabled}
          label={`${t('modelEnabled')} ${String(position)}`}
          disabled={disabled}
          onChange={(next) => { props.onChange({ ...model, enabled: next }) }}
        />
        <span className={styles['modelEnabled']}>{t('modelEnabled')}</span>
        {props.onTest === undefined
          ? null
          : (
            <button
              type="button"
              className={styles['iconButton']}
              aria-label={`${t('testConnection')} ${String(position)}`}
              title={t('testConnection')}
              disabled={disabled || modelId.length === 0 || props.testing === true}
              onClick={() => { props.onTest?.(modelId) }}
            >
              {props.testing === true ? <IconLoadingOutlineRegular size={14} /> : <IconPlayOutlineRegular size={14} />}
            </button>
          )}
        {props.testing === true ? <span role="status" className={styles['testPending']}>{t('testingConnection')}</span> : null}
        {feedback === undefined
          ? null
          : (
            <span
              role="status"
              className={feedback.ok ? styles['testSuccess'] : styles['testFailure']}
            >
              {feedback.ok
                ? t('testSucceeded').replace('{latency}', () => String(feedback.latencyMs ?? 0))
                : t('testFailed').replace('{detail}', () => feedback.message !== undefined && feedback.message.length > 0
                  ? feedback.message
                  : t('testFailedUnknown'))}
            </span>
          )}
      </div>
      <CapabilityChips
        capabilities={props.capabilities}
        model={model}
        field={props.inputField}
        source={model[MODEL_SOURCE_FIELD]}
        t={t}
      />
      {props.expanded
        ? (
          <div className={styles['modelAdvanced']}>
            {(['contextWindow', 'maxTokens'] as const).map(field => (
              <label className={styles['modelField']} key={field}>
                <span className={styles['modelFieldLabel']}>{t(field)}</span>
                <input
                  className={styles['input']}
                  type="text"
                  inputMode="numeric"
                  value={props[field].value}
                  placeholder={props[field].placeholder}
                  aria-label={`${t(field)} ${String(position)}`}
                  disabled={disabled}
                  onChange={(event) => { props[field].onChange(event.target.value) }}
                  onBlur={props[field].onBlur}
                />
              </label>
            ))}
            <ModelInputTypes
              model={model} field={props.inputField} position={position}
              fallback={props.inputFallback} disabled={disabled || props.inputLoading === true} t={t} onChange={props.onChange}
            />
            {props.reasoningLevels !== undefined && props.reasoningLevels.length > 0
              ? (
                <ModelReasoningEfforts
                  model={model} levels={props.reasoningLevels} position={position} disabled={disabled} t={t} onChange={props.onChange}
                />
              )
              : null}
          </div>
        )
        : null}
    </div>
  )
}
