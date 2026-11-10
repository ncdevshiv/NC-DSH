/**
 * The model list's own search field. Both adapter family editors share it, so
 * the field, its label, and its placement cannot drift between them: the typed
 * text filters the rows as it is written, and the editor keeps the term.
 */

import type { ReactNode } from 'react'
import type { ModelsKey } from './locales.ts'
import styles from './ModelsSection.module.css'

/** Props of {@link ModelCatalogSearch}. */
export interface ModelCatalogSearchProps {
  /** The search text as typed. */
  value: string
  /** Section copy. */
  t: (key: ModelsKey) => string
  /** Record the typed text. */
  onChange: (value: string) => void
}

/**
 * Render the model list's search field.
 * @param props - the typed text, the copy, and the action that records it.
 * @returns the search field.
 */
export function ModelCatalogSearch(props: ModelCatalogSearchProps): ReactNode {
  return (
    <div className={styles['modelSearch']}>
      <input
        className={styles['input']}
        type="search"
        value={props.value}
        placeholder={props.t('searchModels')}
        aria-label={props.t('searchModels')}
        onChange={(event) => { props.onChange(event.target.value) }}
      />
    </div>
  )
}
