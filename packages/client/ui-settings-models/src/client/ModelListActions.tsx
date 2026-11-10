/**
 * The actions above one adapter's model list, shared by both family editors so
 * the list's header cannot drift between them: probe every listed model at
 * once, return the served rows to the adapter's own catalog, and whatever the
 * owning family adds beside them — a fetch action, where that family can ask an
 * endpoint at all.
 */

import type { ReactNode } from 'react'
import type { ModelsKey } from './locales.ts'
import styles from './ModelsSection.module.css'

/** Props of {@link ModelListActions}. */
export interface ModelListActionsProps {
  /** Section copy. */
  t: (key: ModelsKey) => string
  /** Disable every action (read-only deployment or a pending write). */
  disabled: boolean
  /** Whether a probe is already in flight anywhere in this list. */
  testing: boolean
  /** Model ids the test-all action probes; empty hides that action. */
  testIds: readonly string[]
  /** Probe every listed model; absent when the card has no test surface. */
  onTestAll?: ((modelIds: readonly string[]) => void) | undefined
  /** Return the served rows to the adapter's own catalog; absent when the user layer owns none. */
  onReset?: (() => void) | undefined
  /** Family-specific actions, rendered after the shared ones. */
  children?: ReactNode
}

/**
 * Render the model list's header actions.
 * @param props - copy, the hold state, and the actions this family offers.
 * @returns the action row.
 */
export function ModelListActions(props: ModelListActionsProps): ReactNode {
  const { t, disabled, testing, testIds, onTestAll, onReset, children } = props
  return (
    <div className={styles['modelListActions']}>
      {onTestAll === undefined || testIds.length === 0
        ? null
        : (
          <button
            type="button"
            className={styles['linkButton']}
            disabled={disabled || testing}
            title={t('testAll')}
            onClick={() => { onTestAll(testIds) }}
          >
            {t('testAll')}
          </button>
        )}
      {onReset === undefined
        ? null
        : (
          <button
            type="button"
            className={styles['linkButton']}
            disabled={disabled}
            onClick={onReset}
          >
            {t('resetModels')}
          </button>
        )}
      {children}
    </div>
  )
}
